use anyhow::Result;
use std::path::Path;
use tokio::process::Command;

pub struct ToolResult {
    pub stdout: String,
    pub stderr: String,
    pub status: i32,
}

pub async fn write_file(base: &Path, path: &str, content: &str) -> Result<()> {
    let full = base.join(path);
    if let Some(parent) = full.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&full, content).await?;
    Ok(())
}

pub async fn read_file(base: &Path, path: &str) -> Result<String> {
    let full = base.join(path);
    Ok(tokio::fs::read_to_string(&full).await?)
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

