use super::LanguageExpert;
use crate::agent::AgentState;
use crate::tools;
use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;

const MAX_FILE_CHARS: usize = 4000;

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
            "Expert Odin engineer. Break this task into concrete implementation steps.\n\nTask: {}\n\n\
            Idiomatic Odin: explicit memory management, `defer` for cleanup, `using` for qualified \
            imports, slices over fixed arrays, `context` for allocators, small packages, no raw \
            pointers unless interfacing with C.",
            task
        )
    }

    fn code_prompt(&self, task: &str, step: &str, files: &HashMap<String, String>) -> String {
        let listing = file_listing(files);
        let relevant = relevant_by_name(files, step);
        let context = render_context(&listing, &relevant);

        format!(
            "Implement this step for an Odin project.\n\nTask: {}\nStep: {}\n\n{}\n\n\
            Write full file contents using <file path=\"...\">...</file> tags. \
            Only output files that need to be created or modified. No prose outside tags.",
            task, step, context
        )
    }

    fn fix_prompt(&self, state: &AgentState) -> String {
        let diag = state.diagnostics.join("\n");
        let error_paths = extract_error_paths(&diag);
        let listing = file_listing(&state.files);
        let relevant = relevant_by_paths(&state.files, &error_paths);
        let context = render_context(&listing, &relevant);

        format!(
            "Odin compiler errors to fix.\n\nDiagnostics:\n{}\n\n{}\n\n\
            Return corrected full file contents using <file path=\"...\">...</file> tags. \
            Do not change file paths. No prose outside tags.",
            diag, context
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

fn file_listing(files: &HashMap<String, String>) -> String {
    let mut paths: Vec<&String> = files.keys().collect();
    paths.sort();
    paths.iter().map(|p| p.as_str()).collect::<Vec<_>>().join(", ")
}

fn relevant_by_name(files: &HashMap<String, String>, step: &str) -> Vec<(String, String)> {
    let step_lower = step.to_lowercase();
    files
        .iter()
        .filter(|(path, _)| {
            let stem = Path::new(path.as_str())
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            !stem.is_empty() && step_lower.contains(&stem)
        })
        .map(|(p, c)| (p.clone(), truncate(c)))
        .collect()
}

fn relevant_by_paths(files: &HashMap<String, String>, paths: &[String]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = paths
        .iter()
        .filter_map(|p| files.get(p).map(|c| (p.clone(), truncate(c))))
        .collect();

    if out.is_empty() {
        out = files.iter().map(|(p, c)| (p.clone(), truncate(c))).collect();
    }
    out
}

fn truncate(content: &str) -> String {
    if content.len() > MAX_FILE_CHARS {
        format!("{}\n... (truncated, {} chars total)", &content[..MAX_FILE_CHARS], content.len())
    } else {
        content.to_string()
    }
}

fn render_context(listing: &str, relevant: &[(String, String)]) -> String {
    let relevant_block = relevant
        .iter()
        .map(|(p, c)| format!("--- {} ---\n{}", p, c))
        .collect::<Vec<_>>()
        .join("\n");

    format!("Project files: {}\n\nRelevant file contents:\n{}", listing, relevant_block)
}

/// Parses `path.odin(line:col)` from `odin check` output.
fn extract_error_paths(diagnostics: &str) -> Vec<String> {
    let re = Regex::new(r"([^\s]+\.odin)\(\d+:\d+\)").unwrap();
    let mut paths: Vec<String> = re
        .captures_iter(diagnostics)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

