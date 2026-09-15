use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Registry of available tools.
#[derive(Debug, Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, ToolDefinition>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
        };
        registry.register_builtins();
        registry
    }

    pub fn register(&mut self, tool: ToolDefinition) {
        self.tools.insert(tool.name.clone(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    pub fn list(&self) -> Vec<&ToolDefinition> {
        self.tools.values().collect()
    }

    fn register_builtins(&mut self) {
        self.register(ToolDefinition {
            name: "file_read".to_string(),
            description: "Read the contents of a file".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "limit": { "type": "integer", "description": "Max lines to read" },
                    "offset": { "type": "integer", "description": "Line to start from (1-indexed)" }
                },
                "required": ["path"]
            }),
        });

        self.register(ToolDefinition {
            name: "file_write".to_string(),
            description: "Write content to a file".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }),
        });

        self.register(ToolDefinition {
            name: "file_patch".to_string(),
            description: "Apply a text patch to a file".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "old_text": { "type": "string" },
                    "new_text": { "type": "string" }
                },
                "required": ["path", "old_text", "new_text"]
            }),
        });

        self.register(ToolDefinition {
            name: "list_dir".to_string(),
            description: "List files and directories at a path".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path, relative to project root" }
                },
                "required": ["path"]
            }),
        });

        self.register(ToolDefinition {
            name: "grep".to_string(),
            description: "Search file contents with a regular expression".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Rust-style regex" },
                    "path": { "type": "string", "description": "Root path to search (default: .)" },
                    "max_results": { "type": "integer", "description": "Max matches to return (default 100)" }
                },
                "required": ["pattern"]
            }),
        });

        self.register(ToolDefinition {
            name: "shell_exec".to_string(),
            description: "Execute a shell command".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "timeout_secs": { "type": "integer" }
                },
                "required": ["command"]
            }),
        });

        self.register(ToolDefinition {
            name: "test_run".to_string(),
            description: "Run the project's test suite".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "filter": { "type": "string", "description": "Optional test filter" }
                }
            }),
        });

        self.register(ToolDefinition {
            name: "git_diff".to_string(),
            description: "Show git diff".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "staged": { "type": "boolean" }
                }
            }),
        });

        self.register(ToolDefinition {
            name: "git_status".to_string(),
            description: "Show git status".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        });

        self.register(ToolDefinition {
            name: "git_commit".to_string(),
            description: "Create a git commit".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string" },
                    "files": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["message"]
            }),
        });
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
