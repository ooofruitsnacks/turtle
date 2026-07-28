use crate::agent::{Action, AgentState};
use crate::config::{Config, Language};
use crate::languages::LanguageExpert;
use crate::llm::LlmBackend;
use crate::brain::ContextBrain;
use crate::rag::RagPipeline;
use crate::tools;
use anyhow::Result;
use regex::Regex;

const MAX_PARSE_RETRIES: u32 = 2;
const MAX_DIAG_CHARS: usize = 1500;

pub struct Agent<'a> {
    llm: &'a dyn LlmBackend,
    config: &'a Config,
    expert: Box<dyn LanguageExpert>,
    brain: ContextBrain,
    rag: RagPipeline,
}

impl<'a> Agent<'a> {
    pub fn new(llm: &'a dyn LlmBackend, config: &'a Config) -> Self {
        let expert: Box<dyn LanguageExpert> = match config.language {
            Language::Rust => Box::new(crate::languages::rust::RustExpert),
            Language::Odin => Box::new(crate::languages::odin::OdinExpert),
        };
        let brain = ContextBrain::load(&config.project_dir);
        let ollama_url = std::env::var("OLLAMA_HOST")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());
        let rag = RagPipeline::new(&ollama_url, "nomic-embed-text", &config.project_dir);
        Self { llm, config, expert, brain, rag }
    }

    pub async fn run(&mut self, prompt: &str) -> Result<AgentState> {
        self.llm.set_system(&self.expert.system_prompt()).await;

        let mut state = AgentState {
            task: prompt.to_string(),
            language: self.config.language,
            ..Default::default()
        };

        let plan = self.plan(prompt).await?;
        for step in &plan {
            self.brain.record_decision(step);
        }
        println!("Plan:");
        for (i, step) in plan.iter().enumerate() {
            println!("  {}. {}", i + 1, step);
        }

        for step in &plan {
            if state.done { break; }
            let action = self.execute_step(step, &state).await?;
            self.apply_action(&action, &mut state).await?;
        }

        for _ in 0..self.config.max_iterations {
            let diagnostics = self.expert.check_project(&self.config.project_dir).await?;
            let filtered = filter_errors(&diagnostics);

            if filtered.is_empty() {
                println!("✅ Project passes all checks.");
                for err in &state.diagnostics {
                    self.brain.mark_resolved(err);
                }
                break;
            }

            println!("⚠️ Diag:\n{}", filtered);
            let repeat_note = self.brain.record_error_attempt(&filtered, "pending");
            state.diagnostics = vec![filtered.clone()];

            let fix = self.generate_fix(&state, repeat_note.as_deref()).await?;
            self.apply_action(&fix, &mut state).await?;
        }

        self.brain.save(&self.config.project_dir);

        self.llm.reset_context().await;

        Ok(state)
    }

    async fn plan(&self, prompt: &str) -> Result<Vec<String>> {
        let plan_prompt = self.expert.plan_prompt(prompt);
        let full = format!(
            "{}\n\nReply with ONLY a numbered list of steps. No preamble, no explanation.",
            plan_prompt
        );
        let response = self.llm.complete(&full).await?;
        Ok(parse_numbered_list(&response))
    }

    async fn execute_step(&mut self, step: &str, state: &AgentState) -> Result<Action> {
        let hits = self.rag.retrieve(step, 4).await?;
        let rag_context = RagPipeline::render_context(&hits);
        let project_context = format!(
            "{}\n\nProject files:\n{}\n\nRelevant retrieved context:\n{}",
            self.brain.decisions_block(),
            self.brain.project_map(),
            rag_context
        );
        let prompt = self.expert.code_prompt(&state.task, step, &project_context);
        self.complete_with_retry(&prompt).await
    }

    async fn generate_fix(&mut self, state: &AgentState, repeat_note: Option<&str>) -> Result<Action> {
        let note = repeat_note.map(|n| format!("\n\nIMPORTANT: {}", n)).unwrap_or_default();
        let query = state.diagnostics.join(" ");
        let hits = self.rag.retrieve(&query, 4).await?;
        let rag_context = RagPipeline::render_context(&hits);
        let project_context = format!(
            "Project files:\n{}\n\nRelevant retrieved context:\n{}",
            self.brain.project_map(),
            rag_context
        );
        let prompt = format!(
            "{}{}",
            self.expert.fix_prompt(state, &project_context),
            note
        );
        self.complete_with_retry(&prompt).await
    }
    async fn complete_with_retry(&self, delta: &str) -> Result<Action> {
        let mut turn = delta.to_string();

        for attempt in 0..MAX_PARSE_RETRIES {
            let response = self.llm.complete(&turn).await?;

            match parse_action(&response) {
                Ok(action) => return Ok(action),
                Err(e) => {
                    if attempt == MAX_PARSE_RETRIES - 1 {
                        anyhow::bail!(
                            "Failed to parse model response after {} attempts: {}",
                            MAX_PARSE_RETRIES, e
                        );
                    }
                    turn = format!(
                        "Previous reply was unparsable: {}. Use ONLY <file path=\"...\">...</file> tags, no markdown fences, no prose.",
                        e
                    );
                }
            }
        }
        unreachable!()
    }

    async fn apply_action(&mut self, action: &Action, state: &mut AgentState) -> Result<()> {
        match action {
            Action::WriteFile { path, content } => {
                tools::write_file(&self.config.project_dir, path, content).await?;
                state.files.insert(path.clone(), content.clone());
                self.brain.record_file(path, content, state.iteration);
                if let Err(e) = self.rag.ingest(path, content, None).await {
                    eprintln!("⚠️ RAG ingest failed for {}: {}", path, e);
                }
                println!("✍️  Wrote {}", path);
            }
            Action::Fix { changes, .. } => {
                for (path, content) in changes {
                    tools::write_file(&self.config.project_dir, path, content).await?;
                    state.files.insert(path.clone(), content.clone());
                    self.brain.record_file(path, content, state.iteration);
                    if let Err(e) = self.rag.ingest(path, content, None).await {
                        eprintln!("⚠️ RAG ingest failed for {}: {}", path, e);
                    }
                    println!("🔧 Fixed {}", path);
                }
            }
            Action::RunCommand { command } => {
                let result = tools::run_shell(command, &self.config.project_dir).await?;
                state.diagnostics.push(result.stdout);
                state.diagnostics.push(result.stderr);
                println!("🔧 Ran command: {}", command);
            }
            Action::Done { summary } => {
                println!("🏁 Done: {}", summary);
                state.done = true;
            }
            Action::Plan { .. } => {}
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

fn parse_action(text: &str) -> Result<Action, String> {
    let xml_re = Regex::new(r#"(?s)<file path="(.*?)">\s*(.*?)\s*</file>"#).unwrap();
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

