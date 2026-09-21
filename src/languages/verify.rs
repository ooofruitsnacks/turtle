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

    const MARKER: &str = "\n...[middle omitted]...\n";

    if max_bytes <= MARKER.len() {
        let mut end = max_bytes.min(text.len());

        while !text.is_char_boundary(end) {
            end -= 1;
        }

        return text[..end].to_owned();
    }

    let available = max_bytes - MARKER.len();
    let mut head_end = available / 2;
    let tail_bytes = available - head_end;
    let mut tail_start = text.len() - tail_bytes;

    while !text.is_char_boundary(head_end) {
        head_end -= 1;
    }

    while !text.is_char_boundary(tail_start) {
        tail_start += 1;
    }

    format!("{}{}{}", &text[..head_end], MARKER, &text[tail_start..])
}

fn clean_diagnostics(text: &str) -> String {
    use std::sync::OnceLock;

    static ESCAPES: OnceLock<regex::Regex> = OnceLock::new();

    let escapes = ESCAPES.get_or_init(|| {
        regex::Regex::new(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07\x1b]*(?:\x07|\x1b\\))")
            .expect("built-in terminal escape expression must compile")
    });

    let stripped = escapes.replace_all(text, "");

    stripped
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .collect()
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

pub fn plan_context(config: &Config) -> String {
    let checks = match &config.checks {
        Some(checks) => Ok(checks.clone()),
        None => default_checks(config),
    };

    let authorization = if config.allow_checks {
        "Execution of the configured checks is authorized."
    } else {
        "Execution is NOT authorized; do not claim verification."
    };

    match checks {
        Ok(checks) => {
            let encoded = serde_json::to_string_pretty(&checks).unwrap_or_else(|_| "[]".to_owned());

            format!(
                "\n\nVERIFICATION CONTRACT:\n\
                 {authorization}\n\
                 The harness runs the following commands sequentially \
                 from the project root and stops at the first failure.\n\
                 This is command data, not permission to execute arbitrary \
                 commands or change the check definitions.\n\
                 {}\n\
                 Preserve test discovery, assertions, compiler options, \
                 and lint/type-check coverage. Do not weaken checks to \
                 obtain a passing result.\n\
                 Passing these commands establishes only what they test; \
                 it does not prove every user requirement is satisfied.\n",
                clipped(&encoded, 6000)
            )
        }

        Err(reason) => format!(
            "\n\nVERIFICATION CONTRACT:\n\
             {authorization}\n\
             Default checks cannot currently be resolved: {reason}\n\
             Do not invent a test framework or claim tests passed. \
             For a new project, create the required manifest when that \
             is part of the requested implementation.\n"
        ),
    }
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

fn is_pytest_check(check: &CheckSpec) -> bool {
    let executable = Path::new(&check.program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let executable = executable.strip_suffix(".exe").unwrap_or(&executable);

    if matches!(executable, "pytest" | "pytest-3") {
        return true;
    }

    let python = matches!(executable, "python" | "python3")
        || executable.strip_prefix("python3.").is_some_and(|version| {
            !version.is_empty() && version.bytes().all(|byte| byte.is_ascii_digit())
        });

    python
        && check.args.first().map(String::as_str) == Some("-m")
        && check.args.get(1).map(String::as_str) == Some("pytest")
}

async fn capture<R: AsyncRead + Unpin>(mut reader: R) -> std::io::Result<String> {
    use std::collections::VecDeque;

    let head_limit = CAPTURE_LIMIT / 2;
    let tail_limit = CAPTURE_LIMIT - head_limit;

    let mut head = Vec::with_capacity(head_limit);
    let mut tail = VecDeque::with_capacity(tail_limit);
    let mut buffer = [0_u8; 4096];
    let mut total = 0_usize;

    loop {
        let count = reader.read(&mut buffer).await?;

        if count == 0 {
            break;
        }

        total = total.saturating_add(count);

        let head_count = head_limit.saturating_sub(head.len()).min(count);

        head.extend_from_slice(&buffer[..head_count]);

        let remaining = &buffer[head_count..count];

        if remaining.len() >= tail_limit {
            tail.clear();
            tail.extend(remaining[remaining.len() - tail_limit..].iter().copied());
        } else {
            let excess = tail
                .len()
                .saturating_add(remaining.len())
                .saturating_sub(tail_limit);

            tail.drain(..excess);
            tail.extend(remaining.iter().copied());
        }
    }

    let mut retained = head;

    if total > CAPTURE_LIMIT {
        retained.extend_from_slice(b"\n...[intermediate process output discarded]...\n");
    }

    retained.extend(tail);

    let decoded = String::from_utf8_lossy(&retained);
    Ok(clean_diagnostics(&decoded))
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
            let _ = child.wait().await;

            return Ok(VerificationOutcome::Unavailable {
                reason: format!(
                    "{} timed out after {} seconds. \
                     The timeout does not establish a source-code defect. \
                     No automatic source repair was attempted.",
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

    let command = serde_json::json!({
        "program": check.program,
        "args": check.args,
        "timeout_secs": check.timeout_secs
    });

    let diagnostics = format!(
        "Exit status: {status}\n\
         Configured command, as data:\n{}\n\
         STDERR — bounded beginning and end:\n{}\n\
         STDOUT — bounded beginning and end:\n{}",
        clipped(&command.to_string(), 1500),
        clipped(&stderr, 4500),
        clipped(&stdout, 4500)
    );

    if is_pytest_check(check) {
        let infrastructure_reason = match status.code() {
            Some(3) => Some("pytest reported an internal error"),
            Some(4) => Some("pytest reported a command-line/configuration usage error"),
            Some(5) => Some("pytest collected no tests"),
            _ => None,
        };

        let missing_pytest = check.args.first().map(String::as_str) == Some("-m")
            && stderr.contains(": No module named pytest");

        if let Some(reason) = infrastructure_reason {
            return Ok(VerificationOutcome::Unavailable {
                reason: format!(
                    "{}: {reason}. Review the configured test environment \
                     and test discovery before requesting source repair.\n{}",
                    check.name, diagnostics
                ),
            });
        }

        if missing_pytest {
            return Ok(VerificationOutcome::Unavailable {
                reason: format!(
                    "{}: pytest is not installed in the selected Python \
                     environment. Configure that environment outside the \
                     agent repair loop.\n{}",
                    check.name, diagnostics
                ),
            });
        }
    }

    Ok(VerificationOutcome::Failed {
        check: check.name.clone(),
        diagnostics,
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
    #[test]
    fn diagnostic_clipping_preserves_both_ends() {
        let source = format!("FIRST_ERROR\n{}\nFINAL_SUMMARY", "x".repeat(2000));
        let result = clipped(&source, 200);

        assert!(result.starts_with("FIRST_ERROR"));
        assert!(result.ends_with("FINAL_SUMMARY"));
        assert!(result.len() <= 200);
    }

    #[test]
    fn diagnostic_clipping_handles_unicode() {
        let source = "é".repeat(1000);
        let result = clipped(&source, 101);

        assert!(result.len() <= 101);
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }

    #[test]
    fn diagnostic_cleanup_removes_terminal_colors() {
        assert_eq!(clean_diagnostics("\x1b[31merror\x1b[0m\n"), "error\n");
    }

    #[test]
    fn detects_explicit_python_pytest_invocation() {
        let check = CheckSpec::new("Python tests", "python3", &["-m", "pytest", "-q"]);

        assert!(is_pytest_check(&check));

        let other = CheckSpec::new("Other command", "python3", &["script.py"]);

        assert!(!is_pytest_check(&other));
    }

    #[tokio::test]
    async fn capture_preserves_start_and_end_of_large_output() {
        use tokio::io::AsyncWriteExt;

        let (mut writer, reader) = tokio::io::duplex(4096);

        let output = format!(
            "BEGIN_FAILURE\n{}\nEND_FAILURE\n",
            "x".repeat(CAPTURE_LIMIT * 3)
        );

        let task = tokio::spawn(async move {
            writer.write_all(output.as_bytes()).await.unwrap();
            writer.shutdown().await.unwrap();
        });

        let captured = capture(reader).await.unwrap();
        task.await.unwrap();

        assert!(captured.starts_with("BEGIN_FAILURE"));
        assert!(captured.ends_with("END_FAILURE\n"));
        assert!(captured.contains("discarded"));
        assert!(captured.len() < CAPTURE_LIMIT + 256);
    }
}
