use super::LanguageExpert;
use crate::agent::AgentState;
use crate::tools;
use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;

pub struct RustExpert;

#[async_trait]
impl LanguageExpert for RustExpert {
    fn name(&self) -> &'static str {
        "Rust"
    }

    fn system_prompt(&self) -> String {
        include_str!("rust_system.md").to_string()
    }

    fn plan_prompt(&self, task: &str) -> String {
        format!(
            "You are an expert Rust engineer. Break the following task into concrete implementation steps.\n\nTask: {}\n\nUse idiomatic Rust (2021 edition). Prefer `std` when possible. Use `anyhow`/`thiserror` for errors, `tokio` for async, and `clap` for CLI. Avoid `unsafe` unless justified and documented. Every public item must have a doc comment.",
            task
        )
    }

    fn code_prompt(&self, task: &str, step: &str, project_context: &str) -> String {
        format!(
            "Implement this step for a Rust project.\n\nTask: {}\nStep: {}\n\nProject context:\n{}\n\nWrite the complete file contents using <file path=\"...\">...</file> tags. Only output files that need to be created or modified. No prose outside the tags.",
            task, step, project_context
        )
    }

    fn fix_prompt(&self, state: &AgentState, project_context: &str) -> String {
        let diag = state.diagnostics.join("\n");
        format!(
            "The Rust project has compiler errors. Fix them.\n\nDiagnostics:\n{}\n\nProject context:\n{}\n\nReturn the corrected file contents using <file path=\"...\">...</file> tags. Do not change the file paths. No prose outside the tags.",
            diag, project_context
        )
    }
    fn error_patterns(&self) -> &[&str] {
        &["error"]
    }


    async fn check_project(&self, project_dir: &Path) -> Result<Vec<String>> {
        let result = tools::run_shell("cargo check --message-format=short", project_dir).await?;
        let mut out = Vec::new();
        if !result.stdout.is_empty() {
            out.push(result.stdout);
        }
        if !result.stderr.is_empty() {
            out.push(result.stderr);
        }
        Ok(out)
    }
}

