use thiserror::Error;

pub type ObservabilityResult<T> = Result<T, ObservabilityError>;

#[derive(Error, Debug)]
pub enum ObservabilityError {
    #[error("storage error: {0}")]
    Storage(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
