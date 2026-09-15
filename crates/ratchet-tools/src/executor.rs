use crate::{
    error::{ToolError, ToolResult},
    fs::{FilePatch, FileRead, FileWrite},
    git::{GitCommit, GitDiff, GitStatus},
    registry::ToolRegistry,
    shell::ShellExec,
    test_runner::TestRunner,
};
use ratchet_sandbox::SandboxGuard;
use crate::path::string_arg;
use serde_json::Value;
use std::path::PathBuf;

/// Context passed to every tool execution.
pub struct ToolContext {
    pub cwd: PathBuf,
    pub sandbox: SandboxGuard,
    pub registry: ToolRegistry,
}

/// Executes tool calls from the agent.
pub struct ToolExecutor;

impl ToolExecutor {
    pub fn new() -> Self {
        Self
    }

    pub async fn execute(&self, ctx: &ToolContext, name: &str, args: Value) -> ToolResult<Value> {
        match name {
            "file_read" => {
                let path = string_arg(&args, PATH_KEYS).ok_or_else(|| {
                    ToolError::InvalidArguments(path_error(&args))
                })?;
                let limit = args["limit"].as_u64().map(|v| v as usize);
                let offset = args["offset"].as_u64().map(|v| v as usize);
                let tool = FileRead;
                tool.execute(ctx, path, limit, offset).await
            }
            "file_write" => {
                let path = string_arg(&args, PATH_KEYS).ok_or_else(|| {
                    ToolError::InvalidArguments(path_error(&args))
                })?;
                let content = string_arg(&args, &["content", "text", "data", "body"])
                    .ok_or_else(|| {
                        ToolError::InvalidArguments(
                            "content required (expected a `content` string)".into(),
                        )
                    })?;
                let tool = FileWrite;
                tool.execute(ctx, path, content).await
            }
            "file_patch" => {
                let path = string_arg(&args, PATH_KEYS).ok_or_else(|| {
                    ToolError::InvalidArguments(path_error(&args))
                })?;
                let old_text = args["old_text"].as_str().ok_or_else(|| {
                    ToolError::InvalidArguments("old_text required".into())
                })?;
                let new_text = args["new_text"].as_str().ok_or_else(|| {
                    ToolError::InvalidArguments("new_text required".into())
                })?;
                let tool = FilePatch;
                tool.execute(ctx, path, old_text, new_text).await
            }
            "list_dir" => {
                let path = string_arg(&args, PATH_KEYS).unwrap_or(".");
                let tool = crate::search::ListDir;
                tool.execute(ctx, path).await
            }
            "grep" => {
                let pattern = string_arg(&args, &["pattern", "query", "regex"])
                    .ok_or_else(|| {
                        ToolError::InvalidArguments("pattern required".into())
                    })?;
                let path = string_arg(&args, PATH_KEYS);
                let max = args["max_results"].as_u64().map(|v| v as usize);
                let tool = crate::search::Grep;
                tool.execute(ctx, pattern, path, max).await
            }
            "shell_exec" => {
                let command = args["command"].as_str().ok_or_else(|| {
                    ToolError::InvalidArguments("command required".into())
                })?;
                let timeout = args["timeout_secs"].as_u64().map(std::time::Duration::from_secs);
                let tool = ShellExec;
                tool.execute(ctx, command, timeout).await
            }
            "test_run" => {
                let filter = args["filter"].as_str();
                let tool = TestRunner::detect(&ctx.cwd)?;
                tool.execute(ctx, filter).await
            }
            "git_diff" => {
                let staged = args["staged"].as_bool().unwrap_or(false);
                let tool = GitDiff;
                tool.execute(ctx, staged).await
            }
            "git_status" => {
                let tool = GitStatus;
                tool.execute(ctx).await
            }
            "git_commit" => {
                let message = args["message"].as_str().ok_or_else(|| {
                    ToolError::InvalidArguments("message required".into())
                })?;
                let files: Vec<String> = args["files"]
                    .as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                let tool = GitCommit;
                tool.execute(ctx, message, &files).await
            }
            _ => Err(ToolError::NotFound(name.to_string())),
        }
    }
}

/// Keys models commonly use for a filesystem path.
const PATH_KEYS: &[&str] = &[
    "path",
    "file",
    "file_path",
    "filepath",
    "filename",
    "dir",
    "directory",
];

/// Actionable message telling the model exactly what shape is expected.
fn path_error(args: &Value) -> String {
    let keys: Vec<&str> = args
        .as_object()
        .map(|o| o.keys().map(String::as_str).collect())
        .unwrap_or_default();
    format!(
        "missing `path` argument (got keys: [{}]). \
         Pass a project-relative path, e.g. {{\"path\": \"src/lib.rs\"}}.",
        keys.join(", ")
    )
}

impl Default for ToolExecutor {
    fn default() -> Self {
        Self::new()
    }
}
