use crate::{
    error::{ProviderError, ProviderResult},
    models::CostModel,
    traits::*,
    types::*,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct OllamaProvider {
    client: Client,
    base_url: String,
    model: String,
}

impl OllamaProvider {
    pub fn new(config: super::ProviderConfig) -> ProviderResult<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(config.timeout_secs.unwrap_or(600)))
                .build()?,
            base_url: config.base_url.unwrap_or_else(|| {
                "http://localhost:11434".to_string()
            }),
            model: config.model.unwrap_or_else(|| "llama3.1".to_string()),
        })
    }
}

#[async_trait]
impl ModelProvider for OllamaProvider {
    async fn complete(&self, req: ChatRequest) -> ProviderResult<ChatResponse> {
        let model = req.model.clone().unwrap_or_else(|| self.model.clone());
        let body = OllamaRequest::from_chat_request(req, &model);

        tracing::debug!(
            tools = body.tools.as_ref().map(|t| t.len()).unwrap_or(0),
            messages = body.messages.len(),
            "ollama request"
        );
        if tracing::enabled!(tracing::Level::TRACE) {
            tracing::trace!(body = %serde_json::to_string(&body).unwrap_or_default(), "ollama request body");
        }

        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(ProviderError::api("ollama", status.as_u16(), text));
        }

        let resp: OllamaResponse = response.json().await.map_err(|e| {
            ProviderError::api("ollama", 200, format!("malformed response: {e}"))
        })?;

        let tool_calls = resp
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, tc)| ToolCall {
                // Ollama does not issue call ids; synthesise stable ones so the
                // tool-result round-trip has something to key on.
                id: format!("call-{i}"),
                name: tc.function.name,
                arguments: tc.function.arguments,
            })
            .collect();

        Ok(ChatResponse {
            content: resp.message.content,
            tool_calls,
            usage: TokenUsage {
                input_tokens: resp.prompt_eval_count.unwrap_or(0),
                output_tokens: resp.eval_count.unwrap_or(0),
                cached_tokens: 0,
            },
            model,
            provider: "ollama".to_string(),
            finish_reason: resp.done.then_some("stop".to_string()),
        })
    }

    async fn stream(&self, _req: ChatRequest) -> ProviderResult<ChatStream> {
        Err(ProviderError::UnsupportedCapability(
            "streaming not yet implemented for ollama".into(),
        ))
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_tools: true,
            supports_vision: false,
            supports_streaming: false,
            supports_extended_thinking: false,
            // Ollama's served context is decided at load time (often 4096),
            // which is far below what the model can theoretically accept.
            max_context_tokens: 8_192,
            max_output_tokens: 4096,
        }
    }

    fn cost_model(&self) -> CostModel {
        CostModel::default() // Local inference is free.
    }

    fn name(&self) -> &str {
        "ollama"
    }
}

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OllamaTool>>,
    stream: bool,
}

#[derive(Serialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct OllamaTool {
    #[serde(rename = "type")]
    _type: String,
    function: OllamaFunction,
}

#[derive(Serialize)]
struct OllamaFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Deserialize)]
struct OllamaResponse {
    message: OllamaResponseMessage,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    prompt_eval_count: Option<u64>,
    #[serde(default)]
    eval_count: Option<u64>,
}

#[derive(Deserialize)]
struct OllamaResponseMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Option<Vec<OllamaToolCall>>,
}

#[derive(Deserialize)]
struct OllamaToolCall {
    function: OllamaToolCallFunction,
}

#[derive(Deserialize)]
struct OllamaToolCallFunction {
    name: String,
    #[serde(default)]
    arguments: serde_json::Value,
}

impl OllamaRequest {
    fn from_chat_request(req: ChatRequest, model: &str) -> Self {
        Self {
            model: model.to_string(),
            messages: req
                .messages
                .into_iter()
                .map(|m| OllamaMessage {
                    role: match m.role {
                        MessageRole::System => "system".to_string(),
                        MessageRole::User => "user".to_string(),
                        MessageRole::Assistant => "assistant".to_string(),
                        MessageRole::Tool => "tool".to_string(),
                    },
                    content: if m.content.is_empty() {
                        // Surface tool results as text; Ollama's tool-result
                        // schema is not consistently supported across models.
                        m.tool_results
                            .map(|rs| {
                                rs.into_iter()
                                    .map(|r| r.content)
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            })
                            .unwrap_or_default()
                    } else {
                        m.content
                    },
                })
                .collect(),
            tools: if req.tools.is_empty() {
                None
            } else {
                Some(
                    req.tools
                        .into_iter()
                        .map(|t| OllamaTool {
                            _type: "function".to_string(),
                            function: OllamaFunction {
                                name: t.name,
                                description: t.description,
                                parameters: t.parameters,
                            },
                        })
                        .collect(),
                )
            },
            stream: false,
        }
    }
}
