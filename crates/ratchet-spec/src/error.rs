use thiserror::Error;

pub type SpecResult<T> = Result<T, SpecError>;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum SpecError {
    #[error("parse error: {0}")]
    Parse(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("schema error: {0}")]
    Schema(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("task graph error: {0}")]
    TaskGraph(String),

    #[error("missing required field: {field} in {context}")]
    MissingField { field: String, context: String },

    #[error("invalid reference: {0}")]
    InvalidReference(String),
}
