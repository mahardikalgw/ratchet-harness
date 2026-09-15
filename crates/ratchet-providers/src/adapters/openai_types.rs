use crate::{error::ProviderError, traits::ChatRequest, traits::ChatStream, types::MessageRole};
use futures::StreamExt;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct OpenAiCompatibleRequest {
    pub model: String,
    pub messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
}

#[derive(Serialize)]
pub struct StreamOptions {
    pub include_usage: bool,
}

#[derive(Serialize, Deserialize)]
pub struct OpenAiMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct OpenAiTool {
    #[serde(rename = "type")]
    pub _type: String,
    pub function: OpenAiFunction,
}

#[derive(Serialize, Deserialize)]
pub struct OpenAiFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct OpenAiToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub _type: String,
    pub function: OpenAiFunctionCall,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct OpenAiFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Deserialize)]
pub struct OpenAiCompatibleResponse {
    pub choices: Vec<OpenAiChoice>,
    pub usage: OpenAiUsage,
}

#[derive(Deserialize)]
pub struct OpenAiChoice {
    pub message: OpenAiResponseMessage,
    pub finish_reason: Option<String>,
}

#[derive(Deserialize)]
pub struct OpenAiResponseMessage {
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Deserialize)]
pub struct OpenAiUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

impl OpenAiCompatibleRequest {
    pub fn from_chat_request(req: ChatRequest, model: &str) -> Self {
        let mut messages = Vec::new();

        for m in req.messages {
            match m.role {
                MessageRole::Assistant => {
                    let tool_calls = m.tool_calls.map(|tcs| {
                        tcs.into_iter()
                            .map(|tc| OpenAiToolCall {
                                id: tc.id,
                                _type: "function".to_string(),
                                function: OpenAiFunctionCall {
                                    name: tc.name,
                                    arguments: serde_json::to_string(&tc.arguments)
                                        .unwrap_or_else(|_| "{}".to_string()),
                                },
                            })
                            .collect::<Vec<_>>()
                    });

                    messages.push(OpenAiMessage {
                        role: "assistant".to_string(),
                        content: if m.content.is_empty() {
                            None
                        } else {
                            Some(m.content)
                        },
                        tool_calls,
                        tool_call_id: None,
                    });
                }
                MessageRole::Tool => {
                    // OpenAI expects one `tool` message per tool result.
                    if let Some(results) = m.tool_results {
                        for r in results {
                            messages.push(OpenAiMessage {
                                role: "tool".to_string(),
                                content: Some(r.content),
                                tool_calls: None,
                                tool_call_id: Some(r.tool_call_id),
                            });
                        }
                    } else {
                        messages.push(OpenAiMessage {
                            role: "tool".to_string(),
                            content: Some(m.content),
                            tool_calls: None,
                            tool_call_id: None,
                        });
                    }
                }
                role => {
                    let role = match role {
                        MessageRole::System => "system",
                        MessageRole::User => "user",
                        _ => unreachable!(),
                    };
                    messages.push(OpenAiMessage {
                        role: role.to_string(),
                        content: Some(m.content),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
            }
        }

        Self {
            model: model.to_string(),
            messages,
            tools: if req.tools.is_empty() {
                None
            } else {
                Some(
                    req.tools
                        .into_iter()
                        .map(|t| OpenAiTool {
                            _type: "function".to_string(),
                            function: OpenAiFunction {
                                name: t.name,
                                description: t.description,
                                parameters: t.parameters,
                            },
                        })
                        .collect(),
                )
            },
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            stream: None,
            stream_options: None,
        }
    }

    /// Build a request body for streaming, including usage in the final chunk.
    pub fn streaming(req: ChatRequest, model: &str) -> Self {
        let mut body = Self::from_chat_request(req, model);
        body.stream = Some(true);
        body.stream_options = Some(StreamOptions {
            include_usage: true,
        });
        body
    }
}

// ----- Streaming SSE -----

#[derive(Deserialize)]
struct OpenAiStreamChunk {
    #[serde(default)]
    choices: Vec<OpenAiStreamChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct OpenAiStreamChoice {
    #[serde(default)]
    delta: OpenAiDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct OpenAiDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiStreamToolCall>>,
}

#[derive(Deserialize)]
struct OpenAiStreamToolCall {
    #[serde(default)]
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<OpenAiStreamFunction>,
}

#[derive(Deserialize)]
struct OpenAiStreamFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

/// Parse one `data:` payload from an OpenAI-compatible SSE stream.
/// Returns `None` for the `[DONE]` sentinel.
pub fn parse_openai_sse(
    data: &str,
) -> Option<crate::ProviderResult<crate::traits::ChatStreamChunk>> {
    use crate::traits::{ChatStreamChunk, TokenUsage, ToolCallDelta};

    let data = data.trim();
    if data.is_empty() || data == "[DONE]" {
        return None;
    }

    let chunk: OpenAiStreamChunk = match serde_json::from_str(data) {
        Ok(c) => c,
        Err(e) => {
            return Some(Err(ProviderError::Stream(format!(
                "malformed stream chunk: {e}"
            ))));
        }
    };

    let choice = chunk.choices.into_iter().next();
    let (content_delta, tool_call_deltas, finish_reason) = match choice {
        Some(c) => {
            let deltas = c
                .delta
                .tool_calls
                .unwrap_or_default()
                .into_iter()
                .map(|tc| ToolCallDelta {
                    index: tc.index,
                    id: tc.id,
                    name: tc.function.as_ref().and_then(|f| f.name.clone()),
                    arguments_delta: tc.function.and_then(|f| f.arguments).unwrap_or_default(),
                })
                .collect();
            (c.delta.content.unwrap_or_default(), deltas, c.finish_reason)
        }
        None => (String::new(), Vec::new(), None),
    };

    let usage = chunk.usage.map(|u| TokenUsage {
        input_tokens: u.prompt_tokens,
        output_tokens: u.completion_tokens,
        cached_tokens: 0,
    });

    Some(Ok(ChatStreamChunk {
        content_delta,
        tool_call_deltas,
        usage,
        finish_reason,
    }))
}

/// Turn an HTTP byte stream into a `ChatStream` of parsed chunks.
pub fn stream_from_response(response: reqwest::Response) -> ChatStream {
    use eventsource_stream::Eventsource;

    let stream = response
        .bytes_stream()
        .eventsource()
        .filter_map(|event| async move {
            match event {
                Ok(ev) => parse_openai_sse(&ev.data),
                Err(e) => Some(Err(ProviderError::Stream(e.to_string()))),
            }
        });

    Box::pin(stream)
}

/// Validate the HTTP status, then deserialize an OpenAI-compatible body.
///
/// Without this, a 401/403 arrives as "error decoding response body" — the
/// server's JSON error object simply does not match the success shape — which
/// hides the real cause from the user.
pub async fn parse_completion_response(
    provider: &str,
    response: reqwest::Response,
) -> crate::ProviderResult<OpenAiCompatibleResponse> {
    let status = response.status();

    if status.as_u16() == 401 || status.as_u16() == 403 {
        let _ = response.text().await;
        return Err(ProviderError::Auth {
            provider: provider.to_string(),
        });
    }
    if status.as_u16() == 429 {
        return Err(ProviderError::RateLimited {
            provider: provider.to_string(),
        });
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(ProviderError::api(
            provider,
            status.as_u16(),
            summarize_error_body(&body),
        ));
    }

    response
        .json::<OpenAiCompatibleResponse>()
        .await
        .map_err(|e| {
            ProviderError::api(
                provider,
                200,
                format!("could not parse response body as an OpenAI-compatible payload: {e}"),
            )
        })
}

/// Pull the human-readable part out of a provider error body, whatever shape
/// it uses (`{"error":{"message":...}}`, `{"message":...}`, or raw text).
fn summarize_error_body(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        for path in [
            &["error", "message"][..],
            &["error", "param"][..],
            &["message"][..],
            &["error"][..],
            &["msg"][..],
        ] {
            let mut cursor = &value;
            let mut found = true;
            for key in path {
                match cursor.get(*key) {
                    Some(next) => cursor = next,
                    None => {
                        found = false;
                        break;
                    }
                }
            }
            if found {
                if let Some(text) = cursor.as_str() {
                    return text.to_string();
                }
            }
        }
    }
    body.chars().take(300).collect()
}

/// Send a streaming chat request through a prepared builder.
pub async fn stream_chat(
    builder: reqwest::RequestBuilder,
    req: ChatRequest,
    model: &str,
    provider: &str,
) -> crate::ProviderResult<ChatStream> {
    let body = OpenAiCompatibleRequest::streaming(req, model);
    let response = builder.json(&body).send().await?;
    let status = response.status();

    if status.as_u16() == 429 {
        return Err(ProviderError::RateLimited {
            provider: provider.to_string(),
        });
    }
    if status.as_u16() == 401 {
        return Err(ProviderError::Auth {
            provider: provider.to_string(),
        });
    }
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(ProviderError::api(provider, status.as_u16(), text));
    }

    Ok(stream_from_response(response))
}
