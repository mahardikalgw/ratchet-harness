use thiserror::Error;

pub type McpResult<T> = Result<T, McpError>;

#[derive(Error, Debug)]
pub enum McpError {
    #[error("transport error: {0}")]
    Transport(String),

    #[error("JSON-RPC error: code={code}, message={message}")]
    JsonRpc { code: i32, message: String },

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("tool not found: {0}")]
    ToolNotFound(String),

    #[error("resource not found: {0}")]
    ResourceNotFound(String),

    #[error("initialization failed: {0}")]
    InitFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("timeout")]
    Timeout,
}
