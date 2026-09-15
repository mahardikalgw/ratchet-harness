use thiserror::Error;

pub type MemoryResult<T> = Result<T, MemoryError>;

#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("storage error: {0}")]
    Storage(String),

    #[error("context overflow: {0}")]
    ContextOverflow(String),

    #[error("entry not found: {0}")]
    NotFound(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
