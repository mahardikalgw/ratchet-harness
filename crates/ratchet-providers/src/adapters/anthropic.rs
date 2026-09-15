use crate::{
    error::{ProviderError, ProviderResult},
    models::CostModel,
    traits::*,
    types::*,
};
use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl AnthropicProvider {
    pub fn new(config: super::ProviderConfig) -> ProviderResult<Self> {
        let api_key = config
            .api_key
            .ok_or_else(|| ProviderError::Config("Anthropic API key required".into()))?;
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(
                    config.timeout_secs.unwrap_or(120),
                ))
                .build()?,
            api_key,
            base_url: config
                .base_url
                .unwrap_or_else(|| "https://api.anthropic.com/v1".to_string()),
            model: config
                .model
                .unwrap_or_else(|| "claude-sonnet-4-20250514".to_string()),
        })
    }
}

#[async_trait]
impl ModelProvider for AnthropicProvider {
    async fn complete(&self, req: ChatRequest) -> ProviderResult<ChatResponse> {
        let model = req.model.clone().unwrap_or_else(|| self.model.clone());
        let body = AnthropicRequest::from_chat_request(req, &model);
        let response = self
            .client
            .post(format!("{}/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if response.status().as_u16() == 429 {
            return Err(ProviderError::RateLimited {
                provider: "anthropic".to_string(),
            });
        }
        if response.status().as_u16() == 401 {
            return Err(ProviderError::Auth {
                provider: "anthropic".to_string(),
            });
        }

        let anthropic_resp: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::api("anthropic", 200, e.to_string()))?;

        let content = anthropic_resp
            .content
            .iter()
            .filter_map(|c| match c {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");

        let tool_calls = anthropic_resp
            .content
            .iter()
            .filter_map(|c| match c {
                ContentBlock::ToolUse { id, name, input } => Some(ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: input.clone(),
                }),
                _ => None,
            })
            .collect();

        Ok(ChatResponse {
            content,
            tool_calls,
            usage: TokenUsage {
                input_tokens: anthropic_resp.usage.input_tokens,
                output_tokens: anthropic_resp.usage.output_tokens,
                cached_tokens: anthropic_resp.usage.cache_read_input_tokens.unwrap_or(0),
            },
            model,
            provider: "anthropic".to_string(),
            finish_reason: anthropic_resp.stop_reason,
        })
    }

    async fn stream(&self, req: ChatRequest) -> ProviderResult<ChatStream> {
        let model = req.model.clone().unwrap_or_else(|| self.model.clone());
        let body = AnthropicRequest::streaming(req, &model);
        let response = self
            .client
            .post(format!("{}/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if status.as_u16() == 429 {
            return Err(ProviderError::RateLimited {
                provider: "anthropic".to_string(),
            });
        }
        if status.as_u16() == 401 {
            return Err(ProviderError::Auth {
                provider: "anthropic".to_string(),
            });
        }
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(ProviderError::api("anthropic", status.as_u16(), text));
        }

        let stream = response
            .bytes_stream()
            .eventsource()
            .filter_map(|event| async move {
                match event {
                    Ok(ev) => parse_anthropic_sse(&ev.data),
                    Err(e) => Some(Err(ProviderError::Stream(e.to_string()))),
                }
            });

        Ok(Box::pin(stream))
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            supports_extended_thinking: true,
            max_context_tokens: 200_000,
            max_output_tokens: 8192,
        }
    }

    fn cost_model(&self) -> CostModel {
        CostModel {
            usd_per_million_input_tokens: 3.0,
            usd_per_million_output_tokens: 15.0,
            usd_per_million_cached_tokens: Some(0.30),
        }
    }

    fn name(&self) -> &str {
        "anthropic"
    }
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContentBlock>,
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

#[derive(Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<ContentBlock>,
    usage: AnthropicUsage,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    #[allow(dead_code)]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: u64,
    output_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
}

impl AnthropicRequest {
    fn from_chat_request(req: ChatRequest, model: &str) -> Self {
        let mut system_parts: Vec<String> = Vec::new();
        let mut messages: Vec<AnthropicMessage> = Vec::new();

        for m in req.messages {
            match m.role {
                MessageRole::System => {
                    if !m.content.is_empty() {
                        system_parts.push(m.content);
                    }
                }
                MessageRole::Assistant => {
                    let mut blocks = Vec::new();
                    if !m.content.is_empty() {
                        blocks.push(AnthropicContentBlock::Text { text: m.content });
                    }
                    if let Some(tcs) = m.tool_calls {
                        for tc in tcs {
                            blocks.push(AnthropicContentBlock::ToolUse {
                                id: tc.id,
                                name: tc.name,
                                input: tc.arguments,
                            });
                        }
                    }
                    if !blocks.is_empty() {
                        messages.push(AnthropicMessage {
                            role: "assistant".to_string(),
                            content: blocks,
                        });
                    }
                }
                MessageRole::Tool => {
                    let mut blocks = Vec::new();
                    if let Some(results) = m.tool_results {
                        for r in results {
                            blocks.push(AnthropicContentBlock::ToolResult {
                                tool_use_id: r.tool_call_id,
                                content: r.content,
                                is_error: if r.is_error { Some(true) } else { None },
                            });
                        }
                    } else if !m.content.is_empty() {
                        blocks.push(AnthropicContentBlock::Text { text: m.content });
                    }
                    if !blocks.is_empty() {
                        messages.push(AnthropicMessage {
                            role: "user".to_string(),
                            content: blocks,
                        });
                    }
                }
                MessageRole::User => {
                    messages.push(AnthropicMessage {
                        role: "user".to_string(),
                        content: vec![AnthropicContentBlock::Text { text: m.content }],
                    });
                }
            }
        }

        Self {
            model: model.to_string(),
            max_tokens: req.max_tokens.unwrap_or(4096),
            system: if system_parts.is_empty() {
                None
            } else {
                Some(system_parts.join("\n\n"))
            },
            messages,
            tools: if req.tools.is_empty() {
                None
            } else {
                Some(
                    req.tools
                        .into_iter()
                        .map(|t| AnthropicTool {
                            name: t.name,
                            description: t.description,
                            input_schema: t.parameters,
                        })
                        .collect(),
                )
            },
            temperature: req.temperature,
            stream: None,
        }
    }

    fn streaming(req: ChatRequest, model: &str) -> Self {
        let mut body = Self::from_chat_request(req, model);
        body.stream = Some(true);
        body
    }
}

// ----- Streaming SSE -----

/// Parse one Anthropic SSE event payload into a stream chunk.
///
/// Text deltas become `content_delta`; tool-use blocks become `tool_call_deltas`
/// keyed by content-block index (the consumer concatenates `arguments_delta`).
pub fn parse_anthropic_sse(data: &str) -> Option<ProviderResult<ChatStreamChunk>> {
    let data = data.trim();
    if data.is_empty() {
        return None;
    }

    let value: serde_json::Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(e) => {
            return Some(Err(ProviderError::Stream(format!(
                "malformed stream event: {e}"
            ))));
        }
    };

    let event_type = value.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let mut chunk = ChatStreamChunk {
        content_delta: String::new(),
        tool_call_deltas: Vec::new(),
        usage: None,
        finish_reason: None,
    };

    match event_type {
        "content_block_start" => {
            let index = value.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
            if let Some(block) = value.get("content_block") {
                if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    chunk.tool_call_deltas.push(ToolCallDelta {
                        index,
                        id: block.get("id").and_then(|v| v.as_str()).map(String::from),
                        name: block.get("name").and_then(|v| v.as_str()).map(String::from),
                        arguments_delta: String::new(),
                    });
                }
            }
        }
        "content_block_delta" => {
            let index = value.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
            if let Some(delta) = value.get("delta") {
                match delta.get("type").and_then(|t| t.as_str()) {
                    Some("text_delta") => {
                        chunk.content_delta = delta
                            .get("text")
                            .and_then(|t| t.as_str())
                            .unwrap_or_default()
                            .to_string();
                    }
                    Some("input_json_delta") => {
                        chunk.tool_call_deltas.push(ToolCallDelta {
                            index,
                            id: None,
                            name: None,
                            arguments_delta: delta
                                .get("partial_json")
                                .and_then(|t| t.as_str())
                                .unwrap_or_default()
                                .to_string(),
                        });
                    }
                    _ => {}
                }
            }
        }
        "message_start" => {
            if let Some(usage) = value.get("message").and_then(|m| m.get("usage")) {
                chunk.usage = Some(TokenUsage {
                    input_tokens: usage
                        .get("input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0),
                    output_tokens: usage
                        .get("output_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0),
                    cached_tokens: usage
                        .get("cache_read_input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0),
                });
            }
        }
        "message_delta" => {
            if let Some(reason) = value
                .get("delta")
                .and_then(|d| d.get("stop_reason"))
                .and_then(|r| r.as_str())
            {
                chunk.finish_reason = Some(reason.to_string());
            }
            if let Some(usage) = value.get("usage") {
                let output = usage.get("output_tokens").and_then(|v| v.as_u64());
                if let Some(output) = output {
                    chunk.usage = Some(TokenUsage {
                        input_tokens: 0,
                        output_tokens: output,
                        cached_tokens: 0,
                    });
                }
            }
        }
        _ => return None,
    }

    Some(Ok(chunk))
}
