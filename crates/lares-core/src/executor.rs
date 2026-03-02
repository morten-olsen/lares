use anyhow::{Context, Result};
use std::path::Path;
use tokio::process::Command;

pub async fn read_file(path: &str) -> Result<String> {
    tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("reading file: {path}"))
}

pub async fn write_file(path: &str, content: &str) -> Result<()> {
    if let Some(parent) = Path::new(path).parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, content)
        .await
        .with_context(|| format!("writing file: {path}"))
}

pub async fn run_command(command: &str, working_dir: Option<&str>) -> Result<CommandOutput> {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(command);
    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    let output = cmd
        .output()
        .await
        .with_context(|| format!("executing: {command}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let success = output.status.success();

    Ok(CommandOutput {
        stdout,
        stderr,
        success,
        exit_code: output.status.code().unwrap_or(-1),
    })
}

#[derive(Debug)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    pub exit_code: i32,
}

impl std::fmt::Display for CommandOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.stdout.is_empty() {
            write!(f, "{}", self.stdout)?;
        }
        if !self.stderr.is_empty() {
            if !self.stdout.is_empty() {
                writeln!(f)?;
            }
            write!(f, "stderr: {}", self.stderr)?;
        }
        if !self.success {
            write!(f, "\n(exit code: {})", self.exit_code)?;
        }
        Ok(())
    }
}
