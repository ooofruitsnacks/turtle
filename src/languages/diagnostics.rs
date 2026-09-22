use anyhow::{Context, Result};
use clap::Args;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Args, Debug, Clone)]
pub struct DiagnosticOptions {
    #[arg(
        long,
        help = "Disable the structured diagnostic summary added to repair prompts"
    )]
    pub no_structured_diagnostics: bool,

    #[arg(
        long,
        value_name = "PATH",
        help = "JSON file of additional diagnostic regex patterns"
    )]
    pub diagnostic_patterns: Option<PathBuf>,

    #[arg(
        long,
        help = "Use only the operator-supplied patterns, without the built-ins"
    )]
    pub diagnostic_patterns_only: bool,

    #[arg(
        long,
        default_value_t = 24,
        value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(1..=500),
        help = "Maximum structured diagnostics summarized"
    )]
    pub max_diagnostics: usize,
}

impl Default for DiagnosticOptions {
    fn default() -> Self {
        Self {
            no_structured_diagnostics: false,
            diagnostic_patterns: None,
            diagnostic_patterns_only: false,
            max_diagnostics: 24,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatternSpec {
    pub name: String,
    pub regex: String,

    #[serde(default)]
    pub tool: Option<String>,

    #[serde(default)]
    pub severity: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatternFile {
    pub patterns: Vec<PatternSpec>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub tool: String,
    pub severity: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub code: Option<String>,
    pub message: String,
}

impl Diagnostic {
    pub fn key(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.file.clone().unwrap_or_default(),
            self.line.unwrap_or(0),
            self.code.clone().unwrap_or_default(),
            head(&self.message, 120)
        )
    }

    pub fn stable_key(&self) -> String {
        format!(
            "{}|{}|{}",
            self.file.clone().unwrap_or_default(),
            self.code.clone().unwrap_or_default(),
            head(&self.message, 120)
        )
    }

    pub fn render(&self) -> String {
        let location = match (&self.file, self.line, self.column) {
            (Some(file), Some(line), Some(column)) => format!("{file}:{line}:{column}"),
            (Some(file), Some(line), None) => format!("{file}:{line}"),
            (Some(file), None, _) => file.clone(),
            _ => "(location not reported)".to_owned(),
        };

        format!(
            "{} {} {} — {} [{}]",
            self.severity,
            location,
            self.code.clone().unwrap_or_else(|| "-".to_owned()),
            head(&self.message, 240),
            self.tool
        )
    }
}

fn head(text: &str, max: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");

    if collapsed.len() <= max {
        return collapsed;
    }

    let mut end = max;

    while end > 0 && !collapsed.is_char_boundary(end) {
        end -= 1;
    }

    collapsed[..end].to_owned()
}

struct Compiled {
    name: String,
    tool: String,
    severity: Option<String>,
    regex: Regex,
}

pub struct Extractor {
    patterns: Vec<Compiled>,
    max: usize,
    enabled: bool,
}

fn builtin() -> Vec<(
    &'static str,
    &'static str,
    Option<&'static str>,
    &'static str,
)> {
    vec![
        (
            "caret_compiler",
            "compiler",
            None,
            r"(?m)^\s*(?P<file>[A-Za-z0-9_./\\+-]+\.[A-Za-z0-9]+):(?P<line>\d+):(?P<column>\d+):\s*(?P<severity>error|warning|note)(?:\[(?P<code>[A-Za-z0-9_]+)\])?:\s*(?P<message>.+)$",
        ),
        (
            "rustc_summary",
            "rustc",
            None,
            r"(?m)^(?P<severity>error|warning)(?:\[(?P<code>E\d+)\])?:\s*(?P<message>.+)$",
        ),
        (
            "typescript_parenthesized",
            "tsc",
            None,
            r"(?m)^(?P<file>[^\s(][^(\n]*)\((?P<line>\d+),(?P<column>\d+)\):\s*(?P<severity>error|warning)\s+(?P<code>TS\d+):\s*(?P<message>.+)$",
        ),
        (
            "typescript_dashed",
            "tsc",
            None,
            r"(?m)^(?P<file>[^\s:][^:\n]*):(?P<line>\d+):(?P<column>\d+)\s*-\s*(?P<severity>error|warning)\s+(?P<code>TS\d+):\s*(?P<message>.+)$",
        ),
        (
            "go_build",
            "go",
            Some("error"),
            r"(?m)^(?P<file>[A-Za-z0-9_./\\-]+\.go):(?P<line>\d+)(?::(?P<column>\d+))?:\s*(?P<message>.+)$",
        ),
        (
            "go_test_failure",
            "go test",
            Some("error"),
            r"(?m)^\s*---\s+FAIL:\s+(?P<message>\S+)",
        ),
        (
            "python_traceback_frame",
            "python",
            Some("note"),
            r#"(?m)^\s*File "(?P<file>[^"]+)", line (?P<line>\d+)(?:, in (?P<message>.+))?$"#,
        ),
        (
            "python_exception",
            "python",
            Some("error"),
            r"(?m)^(?P<code>[A-Z][A-Za-z0-9_]*(?:Error|Exception|Warning)):\s*(?P<message>.+)$",
        ),
        (
            "pytest_failed_line",
            "pytest",
            Some("error"),
            r"(?m)^(?:FAILED|ERROR)\s+(?P<file>[^\s:]+)::(?P<message>\S+)",
        ),
        (
            "pytest_short_summary",
            "pytest",
            Some("error"),
            r"(?m)^(?P<file>[A-Za-z0-9_./\\-]+\.py):(?P<line>\d+):\s*(?P<code>[A-Za-z]*Error|assert)\b(?P<message>.*)$",
        ),
        (
            "ruby_backtrace",
            "ruby",
            Some("error"),
            r"(?m)^(?P<file>[A-Za-z0-9_./\\-]+\.rb):(?P<line>\d+):in\s+[`'](?P<message>[^']*)'",
        ),
        (
            "rspec_failure_location",
            "rspec",
            Some("error"),
            r"(?m)^rspec\s+(?P<file>[^\s:]+):(?P<line>\d+)",
        ),
        (
            "eslint_stylish",
            "eslint",
            None,
            r"(?m)^\s+(?P<line>\d+):(?P<column>\d+)\s+(?P<severity>error|warning)\s+(?P<message>.+?)\s\s+(?P<code>[@A-Za-z0-9_/-]+)\s*$",
        ),
    ]
}

impl Extractor {
    pub fn builtin() -> Self {
        Self::new(&DiagnosticOptions::default()).expect("built-in patterns must compile")
    }

    pub fn new(options: &DiagnosticOptions) -> Result<Self> {
        let mut patterns = Vec::new();

        if !options.diagnostic_patterns_only {
            for (name, tool, severity, expression) in builtin() {
                patterns.push(Compiled {
                    name: name.to_owned(),
                    tool: tool.to_owned(),
                    severity: severity.map(str::to_owned),
                    regex: Regex::new(expression).with_context(|| {
                        format!("built-in diagnostic pattern failed to compile: {name}")
                    })?,
                });
            }
        }

        if let Some(path) = options.diagnostic_patterns.as_ref() {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("cannot read {}", path.display()))?;

            let file: PatternFile = serde_json::from_str(&text)
                .with_context(|| format!("invalid pattern JSON: {}", path.display()))?;

            for spec in file.patterns {
                let regex = Regex::new(&spec.regex).with_context(|| {
                    format!("operator pattern failed to compile: {}", spec.name)
                })?;

                patterns.push(Compiled {
                    tool: spec.tool.unwrap_or_else(|| spec.name.clone()),
                    name: spec.name,
                    severity: spec.severity,
                    regex,
                });
            }
        }

        Ok(Self {
            patterns,
            max: options.max_diagnostics,
            enabled: !options.no_structured_diagnostics,
        })
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn extract(&self, text: &str) -> Vec<Diagnostic> {
        let mut found = Vec::new();
        let mut seen = BTreeSet::new();

        for diagnostic in self.from_json_lines(text) {
            if seen.insert(diagnostic.key()) {
                found.push(diagnostic);
            }
        }

        for pattern in &self.patterns {
            for capture in pattern.regex.captures_iter(text) {
                let group = |name: &str| {
                    capture
                        .name(name)
                        .map(|value| value.as_str().trim().to_owned())
                        .filter(|value| !value.is_empty())
                };

                let message = group("message")
                    .or_else(|| group("code"))
                    .unwrap_or_else(|| pattern.name.clone());

                let diagnostic = Diagnostic {
                    tool: pattern.tool.clone(),
                    severity: group("severity")
                        .or_else(|| pattern.severity.clone())
                        .unwrap_or_else(|| "error".to_owned()),
                    file: group("file"),
                    line: group("line").and_then(|value| value.parse().ok()),
                    column: group("column").and_then(|value| value.parse().ok()),
                    code: group("code"),
                    message,
                };

                if seen.insert(diagnostic.key()) {
                    found.push(diagnostic);
                }

                if found.len() >= self.max * 4 {
                    break;
                }
            }
        }

        // Errors first; otherwise preserve discovery order.
        found.sort_by_key(|diagnostic| match diagnostic.severity.as_str() {
            "error" => 0,
            "warning" => 1,
            _ => 2,
        });

        found.truncate(self.max);
        found
    }

    fn from_json_lines(&self, text: &str) -> Vec<Diagnostic> {
        let mut found = Vec::new();

        for line in text.lines() {
            let line = line.trim();

            if !line.starts_with('{') || !line.contains("\"reason\"") {
                continue;
            }

            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };

            if value.get("reason").and_then(Value::as_str) != Some("compiler-message") {
                continue;
            }

            let message = &value["message"];

            let severity = message
                .get("level")
                .and_then(Value::as_str)
                .unwrap_or("error")
                .to_owned();

            let code = message
                .get("code")
                .and_then(|code| code.get("code"))
                .and_then(Value::as_str)
                .map(str::to_owned);

            let text_message = message
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();

            let primary = message
                .get("spans")
                .and_then(Value::as_array)
                .and_then(|spans| {
                    spans
                        .iter()
                        .find(|span| span.get("is_primary").and_then(Value::as_bool) == Some(true))
                        .or_else(|| spans.first())
                });

            found.push(Diagnostic {
                tool: "cargo json".to_owned(),
                severity,
                file: primary
                    .and_then(|span| span.get("file_name"))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                line: primary
                    .and_then(|span| span.get("line_start"))
                    .and_then(Value::as_u64)
                    .map(|value| value as u32),
                column: primary
                    .and_then(|span| span.get("column_start"))
                    .and_then(Value::as_u64)
                    .map(|value| value as u32),
                code,
                message: text_message,
            });
        }

        found
    }

    pub fn summary(&self, text: &str) -> String {
        if !self.enabled {
            return String::new();
        }

        let diagnostics = self.extract(text);

        if diagnostics.is_empty() {
            return String::from(
                "\n\nSTRUCTURED DIAGNOSTICS: none recognized. \
                 Read the raw check output directly and do not guess a \
                 file or line that was not reported.\n",
            );
        }

        let mut summary = String::from(
            "\n\nSTRUCTURED DIAGNOSTICS — extracted from the same untrusted \
             output, for orientation only. The raw output above remains \
             authoritative, and this list may be incomplete or mis-parsed.\n",
        );

        for diagnostic in &diagnostics {
            summary.push_str("- ");
            summary.push_str(&diagnostic.render());
            summary.push('\n');
        }

        summary
    }

    pub fn stable_keys(&self, text: &str) -> Vec<String> {
        self.extract(text)
            .iter()
            .map(Diagnostic::stable_key)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_rust_short_format() {
        let extractor = Extractor::builtin();

        let diagnostics = extractor.extract("src/main.rs:10:5: error[E0308]: mismatched types");

        let first = diagnostics.first().expect("diagnostic");

        assert_eq!(first.file.as_deref(), Some("src/main.rs"));
        assert_eq!(first.line, Some(10));
        assert_eq!(first.column, Some(5));
        assert_eq!(first.code.as_deref(), Some("E0308"));
        assert_eq!(first.severity, "error");
    }

    #[test]
    fn extracts_typescript_and_python() {
        let extractor = Extractor::builtin();

        let typescript = extractor.extract("src/a.ts(3,17): error TS2345: bad argument");
        assert!(typescript
            .iter()
            .any(|value| value.code.as_deref() == Some("TS2345")));

        let python = extractor
            .extract("  File \"app/main.py\", line 42, in handler\nValueError: bad input\n");

        assert!(python
            .iter()
            .any(|value| value.file.as_deref() == Some("app/main.py")));

        assert!(python
            .iter()
            .any(|value| value.code.as_deref() == Some("ValueError")));
    }

    #[test]
    fn prefers_cargo_json_messages() {
        let extractor = Extractor::builtin();

        let line = r#"{"reason":"compiler-message","message":{"level":"error","message":"cannot find value x","code":{"code":"E0425"},"spans":[{"is_primary":true,"file_name":"src/lib.rs","line_start":7,"column_start":9}]}}"#;

        let diagnostics = extractor.extract(line);
        let first = diagnostics.first().expect("diagnostic");

        assert_eq!(first.tool, "cargo json");
        assert_eq!(first.file.as_deref(), Some("src/lib.rs"));
        assert_eq!(first.line, Some(7));
        assert_eq!(first.code.as_deref(), Some("E0425"));
    }

    #[test]
    fn stable_key_ignores_line_shifts() {
        let base = Diagnostic {
            tool: "t".into(),
            severity: "error".into(),
            file: Some("a.rs".into()),
            line: Some(10),
            column: None,
            code: Some("E1".into()),
            message: "same message".into(),
        };

        let shifted = Diagnostic {
            line: Some(97),
            ..base.clone()
        };

        assert_eq!(base.stable_key(), shifted.stable_key());
        assert_ne!(base.key(), shifted.key());
    }

    #[test]
    fn summary_can_be_disabled() {
        let extractor = Extractor::new(&DiagnosticOptions {
            no_structured_diagnostics: true,
            ..DiagnosticOptions::default()
        })
        .expect("extractor");

        assert!(extractor.summary("src/main.rs:1:1: error: x").is_empty());
    }
}
