use thiserror::Error;

pub type ToolResult<T> = Result<T, ToolError>;

#[derive(Error, Debug)]
pub enum ToolError {
    #[error("tool execution failed: {0}")]
    Execution(String),

    #[error("tool not found: {0}")]
    NotFound(String),

    #[error("invalid arguments: {0}")]
    InvalidArguments(String),

    #[error("sandbox violation: {0}")]
    SandboxViolation(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
