use super::LanguageExpert;
use crate::agent::AgentState;
use crate::tools;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
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

    fn code_prompt(&self, task: &str, step: &str, files: &HashMap<String, String>) -> String {
        let context = files
            .iter()
            .map(|(p, c)| format!("--- {} ---\n{}", p, c))
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            "Implement this step for a Rust project.\n\nTask: {}\nStep: {}\n\nExisting files:\n{}\n\nWrite the complete file contents in fenced code blocks with the relative file path as the language tag, e.g.:\n\n```src/main.rs\nfn main() {{}}\n```\n\nOnly output files that need to be created or modified. Do not output explanations outside the code blocks unless necessary.",
            task, step, context
        )
    }

    fn fix_prompt(&self, state: &AgentState) -> String {
        let files = state
            .files
            .iter()
            .map(|(p, c)| format!("--- {} ---\n{}", p, c))
            .collect::<Vec<_>>()
            .join("\n");

        let diag = state.diagnostics.join("\n");

        format!(
            "The Rust project has compiler errors. Fix them.\n\nDiagnostics:\n{}\n\nFiles:\n{}\n\nReturn the corrected file contents in fenced code blocks with the relative path as the language tag. Do not change the file paths.",
            diag, files
        )
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

