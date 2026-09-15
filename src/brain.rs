use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileSummary {
    pub path: String,
    pub signatures: Vec<String>,
    pub last_touched_iteration: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRecord {
    pub signature: String,
    pub attempts: u32,
    pub last_fix_summary: String,
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextBrain {
    pub files: HashMap<String, FileSummary>,
    pub errors: HashMap<String, ErrorRecord>,
    pub decisions: Vec<String>,
}

impl ContextBrain {
    pub fn load(project_dir: &Path) -> Self {
        let path = project_dir.join(".turtle_brain.json");
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, project_dir: &Path) {
        let path = project_dir.join(".turtle_brain.json");
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn record_file(&mut self, path: &str, content: &str, iteration: u32) {
        let signatures = extract_signatures(content);
        self.files.insert(
            path.to_string(),
            FileSummary {
                path: path.to_string(),
                signatures,
                last_touched_iteration: iteration,
            },
        );
    }

    pub fn record_decision(&mut self, decision: &str) {
        if !self.decisions.iter().any(|d| d == decision) {
            self.decisions.push(decision.to_string());
        }
    }

    pub fn record_error_attempt(&mut self, raw_error: &str, fix_summary: &str) -> Option<String> {
        let sig = normalize_error(raw_error);
        let entry = self.errors.entry(sig.clone()).or_insert(ErrorRecord {
            signature: sig.clone(),
            attempts: 0,
            last_fix_summary: String::new(),
            resolved: false,
        });

        let note = if entry.attempts > 0 {
            Some(format!(
                "This error was already attempted {} time(s). Last fix tried: \"{}\". It did not resolve it — try a different approach.",
                entry.attempts, entry.last_fix_summary
            ))
        } else {
            None
        };

        entry.attempts += 1;
        entry.last_fix_summary = fix_summary.to_string();
        note
    }

    pub fn mark_resolved(&mut self, raw_error: &str) {
        let sig = normalize_error(raw_error);
        if let Some(entry) = self.errors.get_mut(&sig) {
            entry.resolved = true;
        }
    }

    pub fn project_map(&self) -> String {
        if self.files.is_empty() {
            return "No files yet.".to_string();
        }
        let mut paths: Vec<&FileSummary> = self.files.values().collect();
        paths.sort_by(|a, b| a.path.cmp(&b.path));

        paths
            .iter()
            .map(|f| {
                if f.signatures.is_empty() {
                    f.path.clone()
                } else {
                    format!("{}:\n  {}", f.path, f.signatures.join("\n  "))
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn decisions_block(&self) -> String {
        if self.decisions.is_empty() {
            String::new()
        } else {
            format!("Decided so far:\n- {}", self.decisions.join("\n- "))
        }
    }
}

fn extract_signatures(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            let t = line.trim();
            if t.starts_with("pub fn ")
                || t.starts_with("fn ")
                || t.starts_with("pub struct ")
                || t.starts_with("struct ")
                || t.starts_with("pub enum ")
                || t.starts_with("enum ")
                || t.starts_with("pub trait ")
                || t.starts_with("trait ")
            {
                Some(t.trim_end_matches('{').trim().to_string())
            } else {
                None
            }
        })
        .collect()
}

fn normalize_error(raw: &str) -> String {
    let re = regex::Regex::new(r"[0-9]+|/[\w./-]+").unwrap();
    let stripped = re.replace_all(raw, "");
    let first_line = stripped.lines().next().unwrap_or("").trim().to_string();
    first_line
}

