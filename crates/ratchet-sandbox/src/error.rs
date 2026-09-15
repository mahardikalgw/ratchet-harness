use thiserror::Error;

pub type SandboxResult<T> = Result<T, SandboxError>;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum SandboxError {
    #[error(
        "path '{path}' is outside the allowed scope ({allowed}). \
         Use a project-relative path such as 'src/lib.rs'."
    )]
    PathDenied { path: String, allowed: String },

    #[error(
        "path '{path}' is readable but not writable (it holds agent instructions, \
         so Ratchet will not modify it)"
    )]
    PathReadOnly { path: String },

    #[error("shell command denied: {command}")]
    ShellDenied { command: String },

    #[error("network access denied")]
    NetworkDenied,

    #[error("approval required: {action}")]
    ApprovalRequired { action: String },

    #[error("approval rejected: {action}")]
    ApprovalRejected { action: String },

    #[error("sandbox configuration error: {0}")]
    Config(String),

    #[error("execution error: {0}")]
    Execution(String),
}
