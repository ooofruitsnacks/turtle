use crate::config::{CheckSpec, Config, Language};
use anyhow::{Context, Result};
use std::io::ErrorKind;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

const CAPTURE_LIMIT: usize = 16 * 1024;

#[derive(Debug)]
pub enum VerificationOutcome {
    Passed { checks: Vec<String> },
    Failed { check: String, diagnostics: String },
    Unavailable { reason: String },
}

fn clipped(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_owned();
    }

    let mut end = max_bytes;

    while !text.is_char_boundary(end) {
        end -= 1;
    }

    format!("{}\n[truncated]", &text[..end])
}

fn default_checks(config: &Config) -> Result<Vec<CheckSpec>, String> {
    let mut checks = Vec::new();

    for language in config.languages() {
        match language {
            Language::Rust => {
                if !config.project_dir.join("Cargo.toml").is_file() {
                    return Err("Rust defaults require a Cargo.toml in the project root.".into());
                }

                checks.push(CheckSpec::new(
                    "Rust check",
                    "cargo",
                    &["check", "--all-targets", "--message-format=short"],
                ));

                checks.push(CheckSpec::new(
                    "Rust tests",
                    "cargo",
                    &["test", "--all-targets", "--quiet"],
                ));
            }
            Language::Odin => {
                checks.push(CheckSpec::new("Odin check", "odin", &["check", "."]));
            }
            Language::Go => {
                if !config.project_dir.join("go.mod").is_file() {
                    return Err("Go defaults require a go.mod in the project root.".into());
                }

                checks.push(CheckSpec::new("Go vet", "go", &["vet", "./..."]));
                checks.push(CheckSpec::new("Go tests", "go", &["test", "./..."]));
            }
            other => {
                return Err(format!(
                    "{} requires project-specific checks supplied with --checks. \
                     No verification command was guessed.",
                    other.label()
                ));
            }
        }
    }

    Ok(checks)
}

pub async fn verify(config: &Config) -> Result<VerificationOutcome> {
    if !config.allow_checks {
        return Ok(VerificationOutcome::Unavailable {
            reason: "Checks were not authorized. Use --allow-checks only \
                     for code you trust or inside an isolated environment."
                .into(),
        });
    }

    let checks = match &config.checks {
        Some(checks) => checks.clone(),
        None => match default_checks(config) {
            Ok(checks) => checks,
            Err(reason) => {
                return Ok(VerificationOutcome::Unavailable { reason });
            }
        },
    };

    if checks.is_empty() {
        return Ok(VerificationOutcome::Unavailable {
            reason: "No verification checks are configured.".into(),
        });
    }

    let mut passed = Vec::new();

    for check in checks {
        check.validate()?;

        match run_check(&config.project_dir, &check).await? {
            VerificationOutcome::Passed { .. } => passed.push(check.name),
            other => return Ok(other),
        }
    }

    Ok(VerificationOutcome::Passed { checks: passed })
}

async fn capture<R: AsyncRead + Unpin>(mut reader: R) -> std::io::Result<String> {
    let mut retained = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut truncated = false;

    loop {
        let count = reader.read(&mut buffer).await?;

        if count == 0 {
            break;
        }

        let keep = CAPTURE_LIMIT.saturating_sub(retained.len()).min(count);

        retained.extend_from_slice(&buffer[..keep]);
        truncated |= keep < count;

        // Keep draining discarded output to avoid blocking the child.
    }

    let mut text = String::from_utf8_lossy(&retained).into_owned();

    if truncated {
        text.push_str("\n[additional output discarded]");
    }

    Ok(text)
}

async fn run_check(directory: &Path, check: &CheckSpec) -> Result<VerificationOutcome> {
    let child = Command::new(&check.program)
        .args(&check.args)
        .current_dir(directory)
        .env("NO_COLOR", "1")
        .env("CARGO_TERM_COLOR", "never")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn();

    let mut child = match child {
        Ok(child) => child,
        Err(error) => {
            let category = if error.kind() == ErrorKind::NotFound {
                "executable not found"
            } else {
                "could not start executable"
            };

            return Ok(VerificationOutcome::Unavailable {
                reason: format!("{}: {} ({category}): {error}", check.name, check.program),
            });
        }
    };

    let stdout = child.stdout.take().context("missing verifier stdout")?;
    let stderr = child.stderr.take().context("missing verifier stderr")?;

    let work = async {
        let (status, stdout, stderr) =
            tokio::try_join!(child.wait(), capture(stdout), capture(stderr))?;

        Ok::<_, std::io::Error>((status, stdout, stderr))
    };

    let result = tokio::time::timeout(Duration::from_secs(check.timeout_secs), work).await;

    let (status, stdout, stderr) = match result {
        Ok(result) => result?,
        Err(_) => {
            let _ = child.kill().await;

            return Ok(VerificationOutcome::Unavailable {
                reason: format!(
                    "{} timed out after {} seconds. \
                     No automatic source repair was attempted for the timeout.",
                    check.name, check.timeout_secs
                ),
            });
        }
    };

    if status.success() {
        return Ok(VerificationOutcome::Passed {
            checks: vec![check.name.clone()],
        });
    }

    Ok(VerificationOutcome::Failed {
        check: check.name.clone(),
        diagnostics: format!(
            "Exit status: {status}\nSTDERR:\n{}\nSTDOUT:\n{}",
            clipped(&stderr, 1800),
            clipped(&stdout, 600)
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_tool_is_unavailable() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("definitely-missing-verifier");

        let check = CheckSpec::new("Missing tool", missing.to_str().unwrap(), &[]);

        let result = run_check(directory.path(), &check).await.unwrap();

        assert!(matches!(result, VerificationOutcome::Unavailable { .. }));
    }

    #[tokio::test]
    async fn no_authorization_does_not_run_checks() {
        let config = Config::default();
        let result = verify(&config).await.unwrap();

        assert!(matches!(result, VerificationOutcome::Unavailable { .. }));
    }

    #[tokio::test]
    async fn empty_checks_cannot_pass() {
        let config = Config {
            allow_checks: true,
            checks: Some(Vec::new()),
            ..Config::default()
        };

        assert!(matches!(
            verify(&config).await.unwrap(),
            VerificationOutcome::Unavailable { .. }
        ));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn nonzero_exit_is_failure_even_without_error_text() {
        let directory = tempfile::tempdir().unwrap();
        let check = CheckSpec::new("Failure", "sh", &["-c", "exit 7"]);

        assert!(matches!(
            run_check(directory.path(), &check).await.unwrap(),
            VerificationOutcome::Failed { .. }
        ));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn arguments_are_not_split_or_interpolated() {
        let directory = tempfile::tempdir().unwrap();

        let check = CheckSpec::new(
            "Literal argument",
            "sh",
            &["-c", "test \"$1\" = 'a b; c'", "test", "a b; c"],
        );

        assert!(matches!(
            run_check(directory.path(), &check).await.unwrap(),
            VerificationOutcome::Passed { .. }
        ));
    }
}
