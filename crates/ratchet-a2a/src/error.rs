use thiserror::Error;

pub type A2aResult<T> = Result<T, A2aError>;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum A2aError {
    #[error("task not found: {0}")]
    TaskNotFound(String),

    #[error("invalid params: {0}")]
    InvalidParams(String),

    #[error("task cannot be canceled in state {0}")]
    NotCancelable(String),

    #[error("unsupported operation: {0}")]
    UnsupportedOperation(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl A2aError {
    /// JSON-RPC error code for this error.
    pub fn code(&self) -> i32 {
        match self {
            A2aError::TaskNotFound(_) => -32001,
            A2aError::InvalidParams(_) => -32602,
            A2aError::NotCancelable(_) => -32002,
            A2aError::UnsupportedOperation(_) => -32004,
            A2aError::Internal(_) => -32603,
        }
    }
}
