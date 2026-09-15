use crate::{error::ToolResult, ToolContext};
use serde_json::Value;

pub struct ShellExec;

impl ShellExec {
    pub async fn execute(
        &self,
        ctx: &ToolContext,
        command: &str,
        timeout: Option<std::time::Duration>,
    ) -> ToolResult<Value> {
        ctx.sandbox.check_shell(command)
            .map_err(|e| crate::error::ToolError::SandboxViolation(e.to_string()))?;

        let timeout = timeout.unwrap_or(std::time::Duration::from_secs(60));

        let output = tokio::time::timeout(timeout, tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&ctx.cwd)
            .output())
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
