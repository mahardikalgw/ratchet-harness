use thiserror::Error;

pub type PluginResult<T> = Result<T, PluginError>;

#[derive(Error, Debug)]
pub enum PluginError {
    #[error("plugin '{plugin}' failed to start: {source}")]
    Spawn {
        plugin: String,
        #[source]
        source: std::io::Error,
    },

    #[error("plugin '{plugin}' timed out after {seconds}s")]
    Timeout { plugin: String, seconds: u64 },

    #[error("plugin '{plugin}' exited with status {code}: {stderr}")]
    Failed {
        plugin: String,
        code: String,
        stderr: String,
    },

    #[error("plugin '{plugin}' returned invalid JSON: {message}")]
    InvalidResponse { plugin: String, message: String },

    #[error("plugin '{plugin}' not found")]
    NotFound { plugin: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
