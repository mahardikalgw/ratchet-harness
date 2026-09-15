use crate::{ToolContext, error::ToolResult, path::resolve_path};
use serde_json::Value;

pub struct ListDir;

impl ListDir {
    pub async fn execute(&self, ctx: &ToolContext, path: &str) -> ToolResult<Value> {
        let full = resolve_path(&ctx.cwd, path);
        ctx.sandbox
            .check_read(&full)
            .map_err(|e| crate::error::ToolError::SandboxViolation(e.to_string()))?;

        let mut entries = tokio::fs::read_dir(&full).await?;
        let mut out = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let meta = entry.metadata().await.ok();
            let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            out.push(serde_json::json!({
                "name": entry.file_name().to_string_lossy(),
                "type": if is_dir { "dir" } else { "file" },
                "size": size,
            }));
        }

        // Directories first, then files, each alphabetical.
        out.sort_by_key(|e| {
            let is_file = e["type"] == "file";
            (is_file, e["name"].as_str().unwrap_or("").to_string())
        });

        Ok(serde_json::json!({
            "path": path,
            "entries": out,
            "count": out.len(),
        }))
    }
}

pub struct Grep;

impl Grep {
    pub async fn execute(
        &self,
        ctx: &ToolContext,
        pattern: &str,
        path: Option<&str>,
        max_results: Option<usize>,
    ) -> ToolResult<Value> {
        let root = resolve_path(&ctx.cwd, path.unwrap_or("."));
        ctx.sandbox
            .check_read(&root)
            .map_err(|e| crate::error::ToolError::SandboxViolation(e.to_string()))?;

        let regex = regex::Regex::new(pattern)
            .map_err(|e| crate::error::ToolError::InvalidArguments(format!("bad regex: {e}")))?;

        let limit = max_results.unwrap_or(100);
        let mut matches = Vec::new();
        let mut files_searched = 0usize;

        let walker = walk_files(&root);
        for file in walker {
            if matches.len() >= limit {
                break;
            }
            let Ok(content) = tokio::fs::read_to_string(&file).await else {
                continue;
            };
            files_searched += 1;
            let rel = file
                .strip_prefix(&ctx.cwd)
                .unwrap_or(&file)
                .to_string_lossy()
                .to_string();

            for (i, line) in content.lines().enumerate() {
                if regex.is_match(line) {
                    matches.push(serde_json::json!({
                        "file": rel,
                        "line": i + 1,
                        "text": line.trim(),
                    }));
                    if matches.len() >= limit {
                        break;
                    }
                }
            }
        }

        Ok(serde_json::json!({
            "pattern": pattern,
            "matches": matches,
            "match_count": matches.len(),
            "files_searched": files_searched,
            "truncated": matches.len() >= limit,
        }))
    }
}

/// Produce a compact listing of the project layout for prompt context.
///
/// Weaker models otherwise invent placeholder paths (`/path/to/file.txt`)
/// instead of exploring. Giving every model the same deterministic map of the
/// repository removes that failure mode.
pub fn repo_map(cwd: &std::path::Path, max_entries: usize) -> String {
    let mut files: Vec<String> = walk_files(cwd)
        .into_iter()
        .filter_map(|p| {
            p.strip_prefix(cwd)
                .ok()
                .map(|r| r.to_string_lossy().to_string())
        })
        .filter(|p| !p.starts_with(".ratchet"))
        .collect();

    files.sort();

    let truncated = files.len() > max_entries;
    files.truncate(max_entries);

    let mut out = files
        .iter()
        .map(|f| format!("- {f}"))
        .collect::<Vec<_>>()
        .join("\n");

    if truncated {
        out.push_str("\n- … (truncated)");
    }
    out
}

/// Breadth-first walk that skips VCS/build directories and oversized files.
fn walk_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    const SKIP: &[&str] = &[
        ".git",
        "target",
        "node_modules",
        ".ratchet",
        "dist",
        "build",
        ".venv",
        "__pycache__",
    ];
    const MAX_FILES: usize = 5000;
    const MAX_DEPTH: usize = 8;

    let mut out = Vec::new();
    let mut queue = vec![(root.to_path_buf(), 0usize)];

    while let Some((dir, depth)) = queue.pop() {
        if out.len() >= MAX_FILES || depth > MAX_DEPTH {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if SKIP.contains(&name.as_str()) {
                continue;
            }
            if path.is_dir() {
                queue.push((path, depth + 1));
            } else {
                out.push(path);
                if out.len() >= MAX_FILES {
                    break;
                }
            }
        }
    }

    out
}
