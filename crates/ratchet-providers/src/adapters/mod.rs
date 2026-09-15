pub mod anthropic;
pub mod deepseek;
pub mod mimo;
pub mod ollama;
pub mod openai_compatible;
pub mod openai_types;

use crate::{error::ProviderError, traits::ModelProvider};
use std::sync::Arc;

/// Create a provider from configuration.
pub fn create_provider(
    kind: &str,
    config: ProviderConfig,
) -> Result<Arc<dyn ModelProvider>, ProviderError> {
    match kind {
        "anthropic" => Ok(Arc::new(anthropic::AnthropicProvider::new(config)?)),
        "deepseek" => Ok(Arc::new(deepseek::DeepSeekProvider::new(config)?)),
        "mimo" => Ok(Arc::new(mimo::MiMoProvider::new(config)?)),
        "ollama" => Ok(Arc::new(ollama::OllamaProvider::new(config)?)),
        "openai_compatible" => Ok(Arc::new(openai_compatible::OpenAiCompatibleProvider::new(
            config,
        )?)),
        _ => Err(ProviderError::UnknownProvider(kind.to_string())),
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProviderConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub timeout_secs: Option<u64>,
    pub extra_headers: Vec<(String, String)>,
}
