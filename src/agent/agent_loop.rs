use crate::agent::{Action, AgentState};
use crate::config::{Config, Language};
use crate::languages::LanguageExpert;
use crate::llm::LlmBackend;
use crate::brain::ContextBrain;
use crate::rag::RagPipeline;
use crate::tools;
use anyhow::Result;
use regex::Regex;
use std::sync::LazyLock;

const MAX_PARSE_RETRIES: u32 = 3;
const MAX_DIAG_CHARS: usize = 1500;
const MAX_IDENTICAL_FAILURES: u32 = 2;

/// Short outputs (plans) — the model only emits a numbered list.
const TOKENS_PLAN: u32 = 1024;
/// Full-file outputs (code generation and fixes).
const TOKENS_FILES: u32 = 4096;

// Compiled once, not per call.
static XML_FILE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<file\s+path=["']([^"']+)["']\s*>(.*?)</file>"#).unwrap()
});
static MD_FENCE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)```([^\n]+)\n(.*?)```").unwrap()
});
static DONE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<done>(.*?)</done>").unwrap()
});
static NUMBERED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*\d+[.)]\s*(.+)$").unwrap()
});

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
        if plan.is_empty() {
            anyhow::bail!(
                "Model returned an empty plan (no numbered steps found). \
                 Rephrase your prompt, or the model may not be instruction-following."
            );
        }
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

        let mut checks_passed = false;
        let mut last_filtered: Option<String> = None;
        let mut identical_failures: u32 = 0;
        let mut stuck_reason: Option<String> = None;

        for _ in 0..self.config.max_iterations {
            let diagnostics = self.expert.check_project(&self.config.project_dir).await?;
            let filtered = filter_errors(&diagnostics, &*self.expert);

            if filtered.is_empty() {
                println!("✅ Project passes all checks.");
                for err in &state.diagnostics {
                    self.brain.mark_resolved(err);
                }
                checks_passed = true;
                break;
            }

            println!("⚠️ Diag:\n{}", filtered);

            if last_filtered.as_deref() == Some(filtered.as_str()) {
                identical_failures += 1;
                if identical_failures >= MAX_IDENTICAL_FAILURES {
                    stuck_reason = Some(format!(
                        "Fix loop is stuck: identical compiler errors after {} consecutive attempts. \
                         Last errors:\n{}",
                        identical_failures + 1,
                        filtered
                    ));
                    break;
                }
            } else {
                identical_failures = 0;
            }
            last_filtered = Some(filtered.clone());

            let repeat_note = self.brain.record_error_attempt(&filtered, "pending");
            state.diagnostics = vec![filtered.clone()];

            let fix = self.generate_fix(&state, repeat_note.as_deref()).await?;
            self.apply_action(&fix, &mut state).await?;
        }

        self.brain.save(&self.config.project_dir);
        self.llm.reset_context().await;

        if let Some(reason) = stuck_reason {
            anyhow::bail!("{}", reason);
        }
        if !checks_passed && self.config.max_iterations > 0 {
            anyhow::bail!(
                "Project still has errors after {} fix iteration(s). Last errors:\n{}",
                self.config.max_iterations,
                last_filtered.unwrap_or_default()
            );
        }

        Ok(state)
    }

    async fn plan(&self, prompt: &str) -> Result<Vec<String>> {
        let plan_prompt = self.expert.plan_prompt(prompt);
        let full = format!(
            "{}\n\nReply with ONLY a numbered list of steps. No preamble, no explanation.",
            plan_prompt
        );
        let response = self.llm.complete_with_budget(&full, TOKENS_PLAN).await?;
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
        self.complete_with_retry(&prompt, TOKENS_FILES).await
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
        self.complete_with_retry(&prompt, TOKENS_FILES).await
    }

    async fn complete_with_retry(&self, delta: &str, max_tokens: u32) -> Result<Action> {
        let mut last_err = String::new();

        for attempt in 0..MAX_PARSE_RETRIES {
            let turn = if attempt == 0 {
                delta.to_string()
            } else {
                self.llm.pop_last().await;
                format!(
                    "{}\n\nIMPORTANT: your previous reply was rejected: {}.\n\
                     Respond with ONLY one or more \
                     <file path=\"relative/path.ext\">complete file contents</file> blocks. \
                     No markdown fences, no prose, no explanations. \
                     Every <file> block MUST end with </file>. \
                     To signal completion instead, reply with <done>summary</done>.",
                    delta, last_err
                )
            };

            let response = self.llm.complete_with_budget(&turn, max_tokens).await?;

            match parse_action(&response) {
                Ok(action) => return Ok(action),
                Err(e) => {
                    last_err = e;
                    if attempt == MAX_PARSE_RETRIES - 1 {
                        anyhow::bail!(
                            "Failed to parse model response after {} attempts: {}",
                            MAX_PARSE_RETRIES, last_err
                        );
                    }
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
    NUMBERED_RE
        .captures_iter(text)
        .filter_map(|c| c.get(1).map(|m| m.as_str().trim().to_string()))
        .collect()
}

fn parse_action(text: &str) -> Result<Action, String> {
    let opens = text.matches("<file").count();
    let closes = text.matches("</file>").count();
    if opens > closes {
        return Err(
            "response was cut off mid-<file> block (token limit); resend complete file(s)"
                .to_string(),
        );
    }

    let mut changes: Vec<(String, String)> = XML_FILE_RE
        .captures_iter(text)
        .filter_map(|c| {
            let path = c.get(1)?.as_str().trim().to_string();
            let raw = c.get(2)?.as_str();
            let content = raw.strip_prefix('\n').unwrap_or(raw).to_string();
            if path.is_empty() || content.trim().is_empty() {
                None
            } else {
                Some((path, content))
            }
        })
        .collect();

    if changes.is_empty() {
        changes = MD_FENCE_RE
            .captures_iter(text)
            .filter_map(|c| {
                let path = c.get(1)?.as_str().trim().to_string();
                let raw = c.get(2)?.as_str();
                let content = raw.strip_suffix('\n').unwrap_or(raw).to_string();
                if path.is_empty() || !looks_like_path(&path) || content.trim().is_empty() {
                    None
                } else {
                    Some((path, content))
                }
            })
            .collect();
    }

    if !changes.is_empty() {
        return Ok(Action::Fix { explanation: String::new(), changes });
    }

    if let Some(caps) = DONE_RE.captures(text) {
        let summary = caps
            .get(1)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "done".to_string());
        return Ok(Action::Done { summary });
    }

    if text.trim().is_empty() {
        Err("empty response".to_string())
    } else {
        Err("no <file> blocks and no <done> tag in non-empty reply".to_string())
    }
}

fn looks_like_path(s: &str) -> bool {
    s.contains('/') || s.ends_with(".rs") || s.ends_with(".odin")
}

fn filter_errors(diagnostics: &[String], expert: &dyn LanguageExpert) -> String {
    let joined = diagnostics.join("\n");
    let mut out = String::new();
    let mut capture = false;
    let mut lines_after = 0;
    let patterns = expert.error_patterns();

    for line in joined.lines() {
        let trimmed = line.trim();
        let is_error = patterns.iter().any(|p| trimmed.contains(p));
        if is_error {
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

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyExpert;

    #[async_trait]
    impl LanguageExpert for DummyExpert {
        fn name(&self) -> &'static str { "Dummy" }
        fn system_prompt(&self) -> String { String::new() }
        fn plan_prompt(&self, _task: &str) -> String { String::new() }
        fn code_prompt(&self, _task: &str, _step: &str, _ctx: &str) -> String { String::new() }
        fn fix_prompt(&self, _state: &AgentState, _ctx: &str) -> String { String::new() }
        async fn check_project(&self, _dir: &Path) -> Result<Vec<String>> { Ok(vec![]) }
        fn error_patterns(&self) -> &[&str] { &[") Error:", "error"] }
    }

    #[test]
    fn parses_xml_file_block_preserving_content() {
        let r = parse_action("<file path=\"src/main.rs\">\nfn main() {\n    x();\n}\n</file>");
        match r.unwrap() {
            Action::Fix { changes, .. } => {
                assert_eq!(changes[0].0, "src/main.rs");
                assert_eq!(changes[0].1, "fn main() {\n    x();\n}\n");
            }
            _ => panic!("expected Fix"),
        }
    }

    #[test]
    fn truncated_file_block_is_an_error_not_a_write() {
        let r = parse_action("<file path=\"src/lib.rs\">\nfn incomplete() {");
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("cut off"));
    }

    #[test]
    fn bare_prose_is_an_error_not_done() {
        let r = parse_action("I will now create the parser module for you.");
        assert!(r.is_err());
    }

    #[test]
    fn explicit_done_tag_completes() {
        let r = parse_action("<done>parser finished</done>");
        match r.unwrap() {
            Action::Done { summary } => assert_eq!(summary, "parser finished"),
            _ => panic!("expected Done"),
        }
    }

    #[test]
    fn markdown_fence_with_path_parses_multiline() {
        let r = parse_action("```src/util.rs\npub fn f() {\n    g();\n}\n```");
        match r.unwrap() {
            Action::Fix { changes, .. } => {
                assert_eq!(changes[0].0, "src/util.rs");
                assert_eq!(changes[0].1, "pub fn f() {\n    g();\n}");
            }
            _ => panic!("expected Fix"),
        }
    }

    #[test]
    fn bare_language_fence_is_rejected() {
        let r = parse_action("```rust\nfn main() {}\n```");
        assert!(r.is_err());
    }

    #[test]
    fn odin_error_format_is_captured() {
        let expert = DummyExpert;
        let diags = vec!["/proj/main.odin(12:5) Error: undeclared name: foo".to_string()];
        let filtered = filter_errors(&diags, &expert);
        assert!(filtered.contains("undeclared name: foo"));
    }

    #[test]
    fn rust_error_format_is_captured() {
        let expert = DummyExpert;
        let diags = vec!["error[E0308]: mismatched types\n  --> src/main.rs:4:5".to_string()];
        let filtered = filter_errors(&diags, &expert);
        assert!(filtered.contains("error[E0308]"));
    }
}

