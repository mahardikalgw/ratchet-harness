use crate::{ToolContext, error::ToolResult};
use serde_json::Value;

/// Build a command that runs `command` through the platform's shell.
///
/// POSIX `sh -c` does not exist on Windows, so shell execution would fail
/// there; `cmd /C` is the equivalent.
pub fn shell_command(command: &str) -> tokio::process::Command {
    #[cfg(windows)]
    {
        let mut cmd = tokio::process::Command::new("cmd");
        cmd.arg("/C").arg(command);
        cmd
    }
    #[cfg(not(windows))]
    {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(command);
        cmd
    }
}

pub struct ShellExec;

impl ShellExec {
    pub async fn execute(
        &self,
        ctx: &ToolContext,
        command: &str,
        timeout: Option<std::time::Duration>,
    ) -> ToolResult<Value> {
        ctx.sandbox
            .check_shell(command)
            .map_err(|e| crate::error::ToolError::SandboxViolation(e.to_string()))?;

        let timeout = timeout.unwrap_or(std::time::Duration::from_secs(60));

        let output = tokio::time::timeout(timeout, {
            let mut cmd = shell_command(command);
            cmd.current_dir(&ctx.cwd);
            cmd.output()
        })
        .await
        .map_err(|_| crate::error::ToolError::Execution("shell command timed out".into()))??;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(serde_json::json!({
            "stdout": stdout,
            "stderr": stderr,
            "exit_code": output.status.code(),
            "success": output.status.success(),
        }))
    }
}
