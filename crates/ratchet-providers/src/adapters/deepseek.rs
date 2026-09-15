use crate::{
    adapters::openai_types::*,
    error::{ProviderError, ProviderResult},
    models::CostModel,
    traits::*,
    types::*,
};
use async_trait::async_trait;
use reqwest::Client;

pub struct DeepSeekProvider {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl DeepSeekProvider {
    pub fn new(config: super::ProviderConfig) -> ProviderResult<Self> {
        let api_key = config
            .api_key
            .ok_or_else(|| ProviderError::Config("DeepSeek API key required".into()))?;
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(
                    config.timeout_secs.unwrap_or(120),
                ))
                .build()?,
            api_key,
            base_url: config
                .base_url
                .unwrap_or_else(|| "https://api.deepseek.com".to_string()),
            model: config.model.unwrap_or_else(|| "deepseek-chat".to_string()),
        })
    }
}

#[async_trait]
impl ModelProvider for DeepSeekProvider {
    async fn complete(&self, req: ChatRequest) -> ProviderResult<ChatResponse> {
        let model = req.model.clone().unwrap_or_else(|| self.model.clone());
        let body = OpenAiCompatibleRequest::from_chat_request(req, &model);
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;

        let resp: OpenAiCompatibleResponse =
            crate::adapters::openai_types::parse_completion_response("deepseek", response).await?;
        let choice = resp
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| ProviderError::api("deepseek", 200, "no choices returned"))?;

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
            provider: "deepseek".to_string(),
            finish_reason: choice.finish_reason,
        })
    }

    async fn stream(&self, req: ChatRequest) -> ProviderResult<ChatStream> {
        let builder = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key);
        crate::adapters::openai_types::stream_chat(builder, req, &self.model, "deepseek").await
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            supports_extended_thinking: false,
            max_context_tokens: 64_000,
            max_output_tokens: 8192,
        }
    }

    fn cost_model(&self) -> CostModel {
        CostModel {
            usd_per_million_input_tokens: 0.14,
            usd_per_million_output_tokens: 0.28,
            usd_per_million_cached_tokens: Some(0.014),
        }
    }

    fn name(&self) -> &str {
        "deepseek"
    }
}
