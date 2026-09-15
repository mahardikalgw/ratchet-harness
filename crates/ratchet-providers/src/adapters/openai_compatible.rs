use crate::{
    adapters::openai_types::*,
    error::{ProviderError, ProviderResult},
    models::CostModel,
    traits::*,
    types::*,
};
use async_trait::async_trait;
use reqwest::Client;

/// Generic OpenAI-compatible provider for third-party resellers (OpenRouter, etc.)
pub struct OpenAiCompatibleProvider {
    client: Client,
    api_key: Option<String>,
    base_url: String,
    model: String,
}

impl OpenAiCompatibleProvider {
    pub fn new(config: super::ProviderConfig) -> ProviderResult<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(
                    config.timeout_secs.unwrap_or(120),
                ))
                .build()?,
            api_key: config.api_key,
            base_url: config.base_url.ok_or_else(|| {
                ProviderError::Config("base_url required for openai_compatible provider".into())
            })?,
            model: config.model.unwrap_or_else(|| "gpt-4o".to_string()),
        })
    }
}

#[async_trait]
impl ModelProvider for OpenAiCompatibleProvider {
    async fn complete(&self, req: ChatRequest) -> ProviderResult<ChatResponse> {
        let model = req.model.clone().unwrap_or_else(|| self.model.clone());
        let body = OpenAiCompatibleRequest::from_chat_request(req, &model);
        let mut request = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .json(&body);

        if let Some(ref key) = self.api_key {
            request = request.bearer_auth(key);
        }

        let response = request.send().await?;

        let resp: OpenAiCompatibleResponse =
            crate::adapters::openai_types::parse_completion_response("openai_compatible", response)
                .await?;
        let choice =
            resp.choices.into_iter().next().ok_or_else(|| {
                ProviderError::api("openai_compatible", 200, "no choices returned")
            })?;

        Ok(ChatResponse {
            content: choice.message.content.unwrap_or_default(),
            tool_calls: choice
                .message
                .tool_calls
                .unwrap_or_default()
                .into_iter()
                .map(|tc| ToolCall {
                    id: tc.id,
                    name: tc.function.name,
                    arguments: serde_json::from_str(&tc.function.arguments).unwrap_or_default(),
                })
                .collect(),
            usage: TokenUsage {
                input_tokens: resp.usage.prompt_tokens,
                output_tokens: resp.usage.completion_tokens,
                cached_tokens: 0,
            },
            model,
            provider: "openai_compatible".to_string(),
            finish_reason: choice.finish_reason,
        })
    }

    async fn stream(&self, req: ChatRequest) -> ProviderResult<ChatStream> {
        let mut builder = self
            .client
            .post(format!("{}/chat/completions", self.base_url));
        if let Some(ref key) = self.api_key {
            builder = builder.bearer_auth(key);
        }
        crate::adapters::openai_types::stream_chat(builder, req, &self.model, "openai_compatible")
            .await
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            supports_extended_thinking: false,
            max_context_tokens: 128_000,
            max_output_tokens: 4096,
        }
    }

    fn cost_model(&self) -> CostModel {
        // Unknown — user should override
        CostModel::default()
    }

    fn name(&self) -> &str {
        "openai_compatible"
    }
}
