use crate::{error::ProviderResult, models::CostModel, types::*};
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Send a chat completion request and return the full response.
    async fn complete(&self, req: ChatRequest) -> ProviderResult<ChatResponse>;

    /// Send a chat completion request and return a streaming response.
    async fn stream(&self, req: ChatRequest) -> ProviderResult<ChatStream>;

    /// Return the capabilities of this provider.
    fn capabilities(&self) -> ProviderCapabilities;

    /// Return the cost model for this provider.
    fn cost_model(&self) -> CostModel;

    /// Human-readable provider name.
    fn name(&self) -> &str;
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProviderCapabilities {
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_streaming: bool,
    pub supports_extended_thinking: bool,
    pub max_context_tokens: u64,
    pub max_output_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChatRequest {
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u64>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: TokenUsage,
    pub model: String,
    /// Name of the provider that actually served this response. With failover
    /// this is not necessarily the provider that was preferred.
    pub provider: String,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
}

impl TokenUsage {
    /// Accumulate another usage record into this one.
    pub fn add(&mut self, other: &TokenUsage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cached_tokens += other.cached_tokens;
    }

    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

pub type ChatStream = Pin<Box<dyn Stream<Item = ProviderResult<ChatStreamChunk>> + Send>>;

#[derive(Debug, Clone, PartialEq)]
pub struct ChatStreamChunk {
    pub content_delta: String,
    pub tool_call_deltas: Vec<ToolCallDelta>,
    pub usage: Option<TokenUsage>,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    pub name: Option<String>,
    pub arguments_delta: String,
}
