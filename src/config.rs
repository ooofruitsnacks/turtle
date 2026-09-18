use anyhow::{ensure, Result};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
pub const MIN_CONTEXT_TOKENS: u32 = 4_096;
pub const MAX_CONTEXT_TOKENS: u32 = 262_144;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, ValueEnum)]
pub enum Language {
    #[default]
    #[serde(alias = "rust")]
    Rust,

    #[serde(alias = "odin")]
    Odin,

    #[serde(alias = "c")]
    C,

    #[value(name = "cpp", alias = "c++")]
    #[serde(alias = "cpp", alias = "c++")]
    Cpp,

    #[serde(alias = "python")]
    Python,

    #[serde(alias = "ruby")]
    Ruby,

    #[serde(alias = "go")]
    Go,

    #[serde(alias = "jai")]
    Jai,

    #[serde(alias = "zig")]
    Zig,

    #[value(name = "javascript", alias = "js")]
    #[serde(alias = "javascript", alias = "js")]
    JavaScript,

    #[value(name = "typescript", alias = "ts")]
    #[serde(alias = "typescript", alias = "ts")]
    TypeScript,

    #[value(name = "html")]
    #[serde(alias = "html")]
    Html,

    #[value(name = "markdown", alias = "md")]
    #[serde(alias = "markdown", alias = "md")]
    Markdown,
}

impl Language {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Odin => "Odin",
            Self::C => "C",
            Self::Cpp => "C++",
            Self::Python => "Python",
            Self::Ruby => "Ruby",
            Self::Go => "Go",
            Self::Jai => "Jai",
            Self::Zig => "Zig",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Html => "HTML",
            Self::Markdown => "Markdown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    #[default]
    Auto,
    Node,
    Bun,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckSpec {
    pub name: String,
    pub program: String,

    #[serde(default)]
    pub args: Vec<String>,

    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    180
}

impl CheckSpec {
    pub fn new(name: &str, program: &str, args: &[&str]) -> Self {
        Self {
            name: name.to_owned(),
            program: program.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            timeout_secs: default_timeout(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.name.trim().is_empty() && self.name.len() <= 120,
            "check names must contain 1–120 bytes"
        );
        ensure!(
            !self.program.trim().is_empty() && self.program.len() <= 4096,
            "invalid check executable"
        );
        ensure!(
            !self.program.contains('\0'),
            "check executable contains a NUL byte"
        );
        ensure!(self.args.len() <= 128, "too many check arguments");
        ensure!(
            self.args
                .iter()
                .all(|arg| arg.len() <= 8192 && !arg.contains('\0')),
            "invalid or oversized check argument"
        );
        ensure!(
            (1..=1800).contains(&self.timeout_secs),
            "check timeout must be between 1 and 1800 seconds"
        );
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChecksFile {
    pub checks: Vec<CheckSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub model_path: PathBuf,
    pub chat_template: PathBuf,
    pub context_size: u32,
    pub max_iterations: u32,
    pub project_dir: PathBuf,
    pub language: Language,
    pub debug: bool,

    #[serde(default)]
    pub additional_languages: Vec<Language>,

    #[serde(default)]
    pub runtime: Runtime,

    #[serde(default)]
    pub allow_checks: bool,

    #[serde(default)]
    pub checks: Option<Vec<CheckSpec>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model_path: PathBuf::new(),
            chat_template: PathBuf::new(),
            context_size: 8192,
            max_iterations: 3,
            project_dir: PathBuf::from("."),
            language: Language::Rust,
            debug: false,
            additional_languages: Vec::new(),
            runtime: Runtime::Auto,
            allow_checks: false,
            checks: None,
        }
    }
}

impl Config {
    pub fn languages(&self) -> Vec<Language> {
        let mut languages = vec![self.language];

        for language in &self.additional_languages {
            if !languages.contains(language) {
                languages.push(*language);
            }
        }

        languages
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            (MIN_CONTEXT_TOKENS..=MAX_CONTEXT_TOKENS).contains(&self.context_size),
            "context size must be between {} and {} tokens; \
             the selected model and available memory may impose lower limits",
            MIN_CONTEXT_TOKENS,
            MAX_CONTEXT_TOKENS
        );
        ensure!(self.max_iterations <= 12, "maximum repair iterations is 12");

        let languages = self.languages();

        if self.runtime != Runtime::Auto {
            ensure!(
                languages.contains(&Language::JavaScript)
                    || languages.contains(&Language::TypeScript),
                "--runtime node/bun requires JavaScript or TypeScript \
                 in --language"
            );
        }

        if let Some(checks) = &self.checks {
            ensure!(
                !checks.is_empty() && checks.len() <= 16,
                "configure between 1 and 16 checks"
            );

            for check in checks {
                check.validate()?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_requested_languages_parse() {
        for name in [
            "rust",
            "odin",
            "c",
            "cpp",
            "c++",
            "python",
            "ruby",
            "go",
            "jai",
            "zig",
            "javascript",
            "typescript",
            "html",
            "markdown",
        ] {
            assert!(Language::from_str(name, false).is_ok(), "{name}");
        }
    }

    #[test]
    fn bun_is_not_a_language() {
        assert!(Language::from_str("bun", false).is_err());
        assert!(Runtime::from_str("bun", false).is_ok());
    }

    #[test]
    fn runtime_requires_javascript_or_typescript() {
        let config = Config {
            runtime: Runtime::Bun,
            ..Config::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn languages_are_deduplicated() {
        let config = Config {
            language: Language::TypeScript,
            additional_languages: vec![Language::Html, Language::TypeScript],
            ..Config::default()
        };

        assert_eq!(
            config.languages(),
            vec![Language::TypeScript, Language::Html]
        );
    }

    #[test]
    fn empty_check_list_is_rejected() {
        let config = Config {
            checks: Some(Vec::new()),
            ..Config::default()
        };

        assert!(config.validate().is_err());
    }
}
