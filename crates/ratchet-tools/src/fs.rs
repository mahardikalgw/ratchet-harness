use crate::{ToolContext, error::ToolResult, path::resolve_path};
use serde_json::Value;

pub struct FileRead;

impl FileRead {
    pub async fn execute(
        &self,
        ctx: &ToolContext,
        path: &str,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> ToolResult<Value> {
        let full_path = resolve_path(&ctx.cwd, path);
        ctx.sandbox
            .check_read(&full_path)
            .map_err(|e| crate::error::ToolError::SandboxViolation(e.to_string()))?;

        let content = tokio::fs::read_to_string(&full_path).await?;
        let lines: Vec<&str> = content.lines().collect();
        let start = offset.unwrap_or(0).saturating_sub(1);
        let end = limit
            .map(|l| (start + l).min(lines.len()))
            .unwrap_or(lines.len());
        let selected: Vec<String> = lines[start..end].iter().map(|s| s.to_string()).collect();

        Ok(serde_json::json!({
            "path": path,
            "content": selected.join("\n"),
            "total_lines": lines.len(),
            "shown_lines": selected.len(),
        }))
    }
}

pub struct FileWrite;

impl FileWrite {
    pub async fn execute(&self, ctx: &ToolContext, path: &str, content: &str) -> ToolResult<Value> {
        let full_path = resolve_path(&ctx.cwd, path);
        ctx.sandbox
            .check_write(&full_path)
            .map_err(|e| crate::error::ToolError::SandboxViolation(e.to_string()))?;

        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&full_path, content).await?;

        Ok(serde_json::json!({
            "path": path,
            "bytes_written": content.len(),
        }))
    }
}

pub struct FilePatch;

impl FilePatch {
    pub async fn execute(
        &self,
        ctx: &ToolContext,
        path: &str,
        old_text: &str,
        new_text: &str,
    ) -> ToolResult<Value> {
        let full_path = resolve_path(&ctx.cwd, path);
        ctx.sandbox
            .check_write(&full_path)
            .map_err(|e| crate::error::ToolError::SandboxViolation(e.to_string()))?;

        let content = tokio::fs::read_to_string(&full_path).await?;
        if !content.contains(old_text) {
            return Err(crate::error::ToolError::Execution(
                "old_text not found in file".into(),
            ));
        }
        let new_content = content.replace(old_text, new_text);
        tokio::fs::write(&full_path, new_content).await?;

        Ok(serde_json::json!({
            "path": path,
            "patched": true,
        }))
    }
}
