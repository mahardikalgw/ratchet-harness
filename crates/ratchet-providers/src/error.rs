use thiserror::Error;

pub type ProviderResult<T> = Result<T, ProviderError>;

#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error ({provider}, status={status:?}): {message}")]
    Api {
        provider: String,
        message: String,
        status: Option<u16>,
    },

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("invalid configuration: {0}")]
    Config(String),

    #[error("rate limited: {provider}")]
    RateLimited { provider: String },

    #[error("unsupported capability: {0}")]
    UnsupportedCapability(String),

    #[error("stream error: {0}")]
    Stream(String),

    #[error("authentication error: {provider} — {message}")]
    Auth { provider: String, message: String },

    #[error("timeout")]
    Timeout,

    #[error("unknown provider: {0}")]
    UnknownProvider(String),
}

impl ProviderError {
    /// Whether retrying the same request could plausibly succeed.
    ///
    /// Auth, configuration, and capability errors are permanent — retrying
    /// them just burns time and tokens.
    pub fn is_retryable(&self) -> bool {
        match self {
            ProviderError::RateLimited { .. } | ProviderError::Timeout => true,
            ProviderError::Http(e) => {
                e.is_timeout() || e.is_connect() || e.is_request() || e.status().is_none()
            }
            ProviderError::Api { status, .. } => match status {
                Some(code) => *code >= 500 || *code == 408 || *code == 429,
                // Unknown status: assume transient rather than give up.
                None => true,
            },
            ProviderError::Stream(_) => true,
            ProviderError::Auth { .. }
            | ProviderError::Config(_)
            | ProviderError::UnsupportedCapability(_)
            | ProviderError::UnknownProvider(_)
            | ProviderError::Serialization(_) => false,
        }
    }

    pub fn provider_name(&self) -> Option<&str> {
        match self {
            ProviderError::Api { provider, .. }
            | ProviderError::RateLimited { provider }
            | ProviderError::Auth { provider, .. } => Some(provider),
            _ => None,
        }
    }

    /// Build an API error carrying an HTTP status code.
    pub fn api(provider: impl Into<String>, status: u16, message: impl Into<String>) -> Self {
        ProviderError::Api {
            provider: provider.into(),
            message: message.into(),
            status: Some(status),
        }
    }
}
