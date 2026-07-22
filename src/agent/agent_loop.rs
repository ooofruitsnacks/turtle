use crate::agent::{Action, AgentState};
use crate::config::{Config, Language};
use crate::languages::LanguageExpert;
use crate::llm::LlmBackend;
use crate::tools;
use anyhow::Result;
use regex::Regex;

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

        for step in plan {
            if state.done {
                break;
            }
            let action = self.execute_step(&step, &state).await?;
            self.apply_action(&action, &mut state).await?;
        }

        // Verification / fix loop
        for _ in 0..self.config.max_iterations {
            let diagnostics = self.expert.check_project(&self.config.project_dir).await?;
            let has_errors = diagnostics.iter().any(|d| !d.trim().is_empty());

            if !has_errors {
                println!("✅ Project passes all checks.");
                break;
            }

            state.diagnostics = diagnostics.clone();
            println!("⚠️ Diag:\n{}", diagnostics.join("\n"));

            let fix = self.generate_fix(&state).await?;
            self.apply_action(&fix, &mut state).await?;
        }

        Ok(state)
    }

    async fn plan(&self, prompt: &str) -> Result<Vec<String>> {
        let system = self.expert.system_prompt();
        let plan_prompt = self.expert.plan_prompt(prompt);
        let full = format!(
            "{}\n\n{}\n\nReturn only a numbered list of concrete implementation steps. Be specific.",
            system, plan_prompt
        );
        let response = self.llm.complete(&full).await?;
        Ok(parse_numbered_list(&response))
    }

    async fn execute_step(&self, step: &str, state: &AgentState) -> Result<Action> {
        let system = self.expert.system_prompt();
        let prompt = self.expert.code_prompt(&state.task, step, &state.files);
        let response = self.llm.complete(&format!("{}\n\n{}", system, prompt)).await?;
        Ok(parse_action(&response))
    }

    async fn generate_fix(&self, state: &AgentState) -> Result<Action> {
        let system = self.expert.system_prompt();
        let prompt = self.expert.fix_prompt(state);
        let response = self.llm.complete(&format!("{}\n\n{}", system, prompt)).await?;
        Ok(parse_action(&response))
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
    let re = Regex::new(r"(?m)^\s*\d+[\.\)]\s*(.+)$").unwrap();
    re.captures_iter(text)
        .filter_map(|c| c.get(1).map(|m| m.as_str().trim().to_string()))
        .collect()
}

fn parse_action(text: &str) -> Action {
    let file_re = Regex::new(r"```([^\n]+)\n(.*?)```").unwrap();
    let mut changes = Vec::new();

    for cap in file_re.captures_iter(text) {
        let path = cap.get(1).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
        let content = cap.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
        if !path.is_empty() {
            changes.push((path, content));
        }
    }

    if changes.is_empty() {
        Action::Done {
            summary: text.trim().to_string(),
        }
    } else {
        Action::Fix {
            explanation: text.trim().to_string(),
            changes,
        }
    }
}



