use super::LanguageExpert;
use crate::agent::AgentState;
use crate::tools;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::Path;

pub struct OdinExpert;

#[async_trait]
impl LanguageExpert for OdinExpert {
    fn name(&self) -> &'static str {
        "Odin"
    }

    fn system_prompt(&self) -> String {
        include_str!("odin_system.md").to_string()
    }

    fn plan_prompt(&self, task: &str) -> String {
        format!(
            "You are an expert Odin engineer. Break the following task into concrete implementation steps.\n\nTask: {}\n\nUse idiomatic Odin: explicit memory management, `defer` for cleanup, `using` for qualified imports, slices over fixed arrays, and `context` for allocators. Organize code into packages. Avoid raw pointers unless interfacing with C.",
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
            "Implement this step for an Odin project.\n\nTask: {}\nStep: {}\n\nExisting files:\n{}\n\nWrite the complete file contents in fenced code blocks with the relative file path as the language tag, e.g.:\n\n```src/main.odin\npackage main\n\nmain :: proc() {{}}\n```\n\nOnly output files that need to be created or modified. Do not output explanations outside the code blocks unless necessary.",
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
            "The Odin project has compiler errors. Fix them.\n\nDiagnostics:\n{}\n\nFiles:\n{}\n\nReturn the corrected file contents in fenced code blocks with the relative path as the language tag. Do not change the file paths.",
            diag, files
        )
    }

    async fn check_project(&self, project_dir: &Path) -> Result<Vec<String>> {
        let result = tools::run_shell("odin check .", project_dir).await?;
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

