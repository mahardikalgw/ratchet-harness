use crate::{error::ToolResult, ToolContext};
use serde_json::Value;

pub struct GitDiff;

impl GitDiff {
    pub async fn execute(&self, ctx: &ToolContext, staged: bool) -> ToolResult<Value> {
        let mut command = "git diff".to_string();
        if staged {
            command.push_str(" --staged");
        }
        let shell = crate::shell::ShellExec;
        shell.execute(ctx, &command, Some(std::time::Duration::from_secs(30))).await
    }
}

pub struct GitStatus;

impl GitStatus {
    pub async fn execute(&self, ctx: &ToolContext) -> ToolResult<Value> {
        let shell = crate::shell::ShellExec;
        shell.execute(ctx, "git status --short", Some(std::time::Duration::from_secs(30))).await
    }
}

pub struct GitCommit;

impl GitCommit {
    pub async fn execute(
        &self,
        ctx: &ToolContext,
        message: &str,
        files: &[String],
    ) -> ToolResult<Value> {
        // Stage files first
        if !files.is_empty() {
            let files_str = files.join(" ");
            let add_cmd = format!("git add {}", files_str);
            let shell = crate::shell::ShellExec;
            shell.execute(ctx, &add_cmd, Some(std::time::Duration::from_secs(30))).await?;
        }

        let commit_cmd = format!("git commit -m '{}'", message.replace('\'', "'\"'\"'"));
        let shell = crate::shell::ShellExec;
        shell.execute(ctx, &commit_cmd, Some(std::time::Duration::from_secs(30))).await
    }
}
