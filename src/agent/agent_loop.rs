use crate::agent::{Action, AgentState};
use crate::config::{Config, Language};
use crate::languages::LanguageExpert;
use crate::llm::LlmBackend;
use crate::tools;
use anyhow::Result;
use regex::Regex;

const MAX_PARSE_RETRIES: u32 = 2;
const MAX_DIAG_CHARS: usize = 1500;

pub struct Agent<'a> {
    llm: &'a dyn LlmBackend,
    config: &'a Config,
    expert: Box<dyn LanguageExpert>,
}

impl<'a> Agent<'a> {
    pub fn new(llm: &'a dyn LlmBackend, config: &'a Config) -> Self {
        let expert: Box<dyn LanguageExpert> = match config.language {
            Language::Rust => Box::new(crate::languages::rust::RustExpert),
            Language::Odin => Box::new(crate::languages::odin::OdinExpert),
        };
        Self { llm, config, expert }
    }

    pub async fn run(&mut self, prompt: &str) -> Result<AgentState> {
        let mut state = AgentState {
            task: prompt.to_string(),
            language: self.config.language,
            ..Default::default()
        };

        let plan = self.plan(prompt).await?;
        println!("Plan:");
        for (i, step) in plan.iter().enumerate() {
            println!("  {}. {}", i + 1, step);
        }

        for step in &plan {
            if state.done {
                break;
            }
            let action = self.execute_step(step, &state).await?;
            self.apply_action(&action, &mut state).await?;
        }

        for _ in 0..self.config.max_iterations {
            let diagnostics = self.expert.check_project(&self.config.project_dir).await?;
            let filtered = filter_errors(&diagnostics);

            if filtered.is_empty() {
                println!("✅ Project passes all checks.");
                break;
            }

            println!("⚠️ Diag:\n{}", filtered);
            state.diagnostics = vec![filtered.clone()];

            let fix = self.generate_fix(&state).await?;
            self.apply_action(&fix, &mut state).await?;
        }

        Ok(state)
    }

    async fn plan(&self, prompt: &str) -> Result<Vec<String>> {
        let system = self.expert.system_prompt();
        let plan_prompt = self.expert.plan_prompt(prompt);
        let full = format!(
            "{}\n\n{}\n\nReply with ONLY a numbered list of steps. No preamble, no explanation.",
            system, plan_prompt
        );
        let response = self.llm.complete(&full).await?;
        Ok(parse_numbered_list(&response))
    }

    async fn execute_step(&self, step: &str, state: &AgentState) -> Result<Action> {
        let system = self.expert.system_prompt();
        let base = format!(
            "{}\n\n{}",
            system,
            self.expert.code_prompt(&state.task, step, &state.files)
        );
        self.complete_with_retry(&base).await
    }

    async fn generate_fix(&self, state: &AgentState) -> Result<Action> {
        let system = self.expert.system_prompt();
        let base = format!("{}\n\n{}", system, self.expert.fix_prompt(state));
        self.complete_with_retry(&base).await
    }

    /// Sends `base`, and on a parse failure retries with ONLY the base prompt
    /// plus the latest error appended — never stacking prior errors, which
    /// keeps retry prompts from growing every attempt.
    async fn complete_with_retry(&self, base: &str) -> Result<Action> {
        let mut last_error: Option<String> = None;

        for attempt in 0..MAX_PARSE_RETRIES {
            let full = match &last_error {
                None => base.to_string(),
                Some(err) => format!(
                    "{}\n\n[Previous reply was unparsable: {}. Use ONLY <file path=\"...\">...</file> tags, no markdown fences, no prose.]",
                    base, err
                ),
            };

            let response = self.llm.complete(&full).await?;

            match parse_action(&response) {
                Ok(action) => return Ok(action),
                Err(e) => {
                    if attempt == MAX_PARSE_RETRIES - 1 {
                        anyhow::bail!("Failed to parse model response after {} attempts: {}", MAX_PARSE_RETRIES, e);
                    }
                    last_error = Some(e);
                }
            }
        }
        unreachable!()
    }

    async fn apply_action(&self, action: &Action, state: &mut AgentState) -> Result<()> {
        match action {
            Action::Plan { .. } => {}
            Action::WriteFile { path, content } => {
                tools::write_file(&self.config.project_dir, path, content).await?;
                state.files.insert(path.clone(), content.clone());
                println!("✍️  Wrote {}", path);
            }
            Action::RunCommand { command } => {
                let result = tools::run_shell(command, &self.config.project_dir).await?;
                state.diagnostics.push(result.stdout);
                state.diagnostics.push(result.stderr);
                println!("🔧 Ran command: {}", command);
            }
            Action::Fix { changes, .. } => {
                for (path, content) in changes {
                    tools::write_file(&self.config.project_dir, path, content).await?;
                    state.files.insert(path.clone(), content.clone());
                    println!("🔧 Fixed {}", path);
                }
            }
            Action::Done { summary } => {
                println!("🏁 Done: {}", summary);
                state.done = true;
            }
        }
        state.iteration += 1;
        Ok(())
    }
}

fn parse_numbered_list(text: &str) -> Vec<String> {
    let re = Regex::new(r"(?m)^\s*\d+[.)]\s*(.+)$").unwrap();
    re.captures_iter(text)
        .filter_map(|c| c.get(1).map(|m| m.as_str().trim().to_string()))
        .collect()
}

/// Primary parser: XML `<file path="...">...</file>` tags, matching the
/// system prompt's mandated output format. Falls back to markdown fences
/// only if no XML tags are found, for resilience against non-compliant models.
fn parse_action(text: &str) -> Result<Action, String> {
    let xml_re = Regex::new(r#"(?s)<file path="([^"]+)">\s*(.*?)\s*</file>"#).unwrap();
    let mut changes: Vec<(String, String)> = xml_re
        .captures_iter(text)
        .filter_map(|c| {
            let path = c.get(1)?.as_str().trim().to_string();
            let content = c.get(2)?.as_str().to_string();
            if path.is_empty() { None } else { Some((path, content)) }
        })
        .collect();

    if changes.is_empty() {
        let md_re = Regex::new(r"```([^\n]+)\n(.*?)```").unwrap();
        changes = md_re
            .captures_iter(text)
            .filter_map(|c| {
                let path = c.get(1)?.as_str().trim().to_string();
                let content = c.get(2)?.as_str().to_string();
                if path.is_empty() { None } else { Some((path, content)) }
            })
            .collect();
    }

    if changes.is_empty() {
        if text.trim().is_empty() {
            Err("empty response".to_string())
        } else {
            Ok(Action::Done { summary: text.trim().to_string() })
        }
    } else {
        Ok(Action::Fix {
            explanation: String::new(),
            changes,
        })
    }
}

fn filter_errors(diagnostics: &[String]) -> String {
    let joined = diagnostics.join("\n");
    let mut out = String::new();
    let mut capture = false;
    let mut lines_after = 0;

    for line in joined.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("error") {
            capture = true;
            lines_after = 4;
        }
        if capture {
            out.push_str(line);
            out.push('\n');
            if lines_after == 0 {
                capture = false;
            } else {
                lines_after -= 1;
            }
        }
        if out.len() > MAX_DIAG_CHARS {
            out.truncate(MAX_DIAG_CHARS);
            out.push_str("\n...(truncated)");
            break;
        }
    }

    out.trim().to_string()
}

