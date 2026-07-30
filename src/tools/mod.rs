use anyhow::Result;
use std::path::{Component, Path, PathBuf};
use tokio::process::Command;

pub struct ToolResult {
    pub stdout: String,
    pub stderr: String,
    pub status: i32,
}
fn resolve_safe(base: &Path, path: &str) -> Result<PathBuf> {
    anyhow::ensure!(!path.is_empty(), "empty path rejected");

    let rel = Path::new(path);
    anyhow::ensure!(!rel.is_absolute(), "absolute path rejected: {path}");
    anyhow::ensure!(
        !rel.components().any(|c| matches!(c, Component::ParentDir)),
        "path traversal rejected: {path}"
    );

    let full = base.join(rel);
    let canon_base = base.canonicalize()?;
    let mut ancestor = full.as_path();
    loop {
        match ancestor.canonicalize() {
            Ok(canon) => {
                anyhow::ensure!(
                    canon.starts_with(&canon_base),
                    "path escapes project root: {path}"
                );
                break;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {
                match ancestor.parent() {
                    Some(parent) => ancestor = parent,
                    None => break,
                }
            }
            Err(e) => return Err(e.into()),
        }
    }

    Ok(full)
}

pub async fn write_file(base: &Path, path: &str, content: &str) -> Result<()> {
    let full = resolve_safe(base, path)?;
    if let Some(parent) = full.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&full, content).await?;
    Ok(())
}

pub async fn read_file(base: &Path, path: &str) -> Result<String> {
    let full = resolve_safe(base, path)?;
    let content = tokio::fs::read_to_string(&full).await?;
    Ok(content)
}

pub async fn run_shell(command: &str, cwd: &Path) -> Result<ToolResult> {
    let output = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", command])
            .current_dir(cwd)
            .output()
            .await?
    } else {
        Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(cwd)
            .output()
            .await?
    };

    Ok(ToolResult {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        status: output.status.code().unwrap_or(-1),
    })
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_relative_path() {
        let base = std::env::temp_dir();
        assert!(resolve_safe(&base, "src/main.rs").is_ok());
    }

    #[test]
    fn rejects_parent_traversal() {
        let base = std::env::temp_dir();
        assert!(resolve_safe(&base, "../outside.rs").is_err());
        assert!(resolve_safe(&base, "src/../../outside.rs").is_err());
    }

    #[test]
    fn rejects_absolute_path() {
        let base = std::env::temp_dir();
        assert!(resolve_safe(&base, "/etc/passwd").is_err());
    }

    #[test]
    fn rejects_empty_path() {
        let base = std::env::temp_dir();
        assert!(resolve_safe(&base, "").is_err());
    }
}


