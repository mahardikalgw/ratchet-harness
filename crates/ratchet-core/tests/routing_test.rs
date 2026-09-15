use async_trait::async_trait;
use ratchet_core::{
    config::RoutingPolicy,
    routing::{RequiredCapabilities, Router, RoutingRequest, TaskType},
};
use ratchet_providers::{
    ModelProvider,
    error::ProviderResult,
    models::CostModel,
    traits::{ChatRequest, ChatResponse, ChatStream, ProviderCapabilities},
};
use std::collections::HashMap;
use std::sync::Arc;

/// A configurable mock provider for routing tests.
struct MockProvider {
    name: String,
    caps: ProviderCapabilities,
    cost: CostModel,
}

#[async_trait]
impl ModelProvider for MockProvider {
    async fn complete(&self, _req: ChatRequest) -> ProviderResult<ChatResponse> {
        unimplemented!("not needed for routing tests")
    }

    async fn stream(&self, _req: ChatRequest) -> ProviderResult<ChatStream> {
        unimplemented!("not needed for routing tests")
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.caps.clone()
    }

    fn cost_model(&self) -> CostModel {
        self.cost.clone()
    }

    fn name(&self) -> &str {
        &self.name
    }
}

fn caps(tools: bool, vision: bool, thinking: bool, ctx: u64) -> ProviderCapabilities {
    ProviderCapabilities {
        supports_tools: tools,
        supports_vision: vision,
        supports_streaming: true,
        supports_extended_thinking: thinking,
        max_context_tokens: ctx,
        max_output_tokens: 4096,
    }
}

fn cost(input: f64, output: f64) -> CostModel {
    CostModel {
        usd_per_million_input_tokens: input,
        usd_per_million_output_tokens: output,
        usd_per_million_cached_tokens: None,
    }
}

fn registry() -> HashMap<String, Arc<dyn ModelProvider>> {
    let mut m: HashMap<String, Arc<dyn ModelProvider>> = HashMap::new();
    m.insert(
        "expensive".to_string(),
        Arc::new(MockProvider {
            name: "expensive".to_string(),
            caps: caps(true, true, true, 200_000),
            cost: cost(15.0, 75.0),
        }),
    );
    m.insert(
        "cheap".to_string(),
        Arc::new(MockProvider {
            name: "cheap".to_string(),
            caps: caps(true, false, false, 64_000),
            cost: cost(0.14, 0.28),
        }),
    );
    m.insert(
        "local".to_string(),
        Arc::new(MockProvider {
            name: "local".to_string(),
            caps: caps(false, false, false, 32_000),
            cost: cost(0.0, 0.0),
        }),
    );
    m
}

fn coding_request() -> RoutingRequest {
    RoutingRequest {
        task_type: TaskType::Coding,
        required_capabilities: RequiredCapabilities {
            needs_tools: true,
            min_context_tokens: 32_000,
            ..Default::default()
        },
        preferred_model: None,
    }
}

#[test]
fn cost_optimized_picks_cheapest_capable_provider() {
    let router = Router::new(RoutingPolicy::CostOptimized, registry());
    let result = router.route(&coding_request()).unwrap();
    // "local" is free but lacks tools, so "cheap" should win
    assert_eq!(result.provider_name, "cheap");
}

#[test]
fn capability_then_cost_prefers_capable_providers() {
    let router = Router::new(RoutingPolicy::CapabilityThenCost, registry());
    let result = router.route(&coding_request()).unwrap();
    // Both cheap and expensive support tools; scoring should favor one of them,
    // and local (no tools) should be avoided.
    assert_ne!(result.provider_name, "local");
}

#[test]
fn explicit_model_override_wins() {
    let router = Router::new(RoutingPolicy::CostOptimized, registry());
    let mut req = coding_request();
    req.preferred_model = Some("expensive".to_string());
    let result = router.route(&req).unwrap();
    assert_eq!(result.provider_name, "expensive");
}

#[test]
fn vision_requirement_filters_providers() {
    let router = Router::new(RoutingPolicy::CostOptimized, registry());
    let req = RoutingRequest {
        task_type: TaskType::Review,
        required_capabilities: RequiredCapabilities {
            needs_vision: true,
            min_context_tokens: 32_000,
            ..Default::default()
        },
        preferred_model: None,
    };
    let result = router.route(&req).unwrap();
    // Only "expensive" supports vision
    assert_eq!(result.provider_name, "expensive");
}

#[test]
fn extended_thinking_requirement_filters_providers() {
    let router = Router::new(RoutingPolicy::CapabilityThenCost, registry());
    let req = RoutingRequest {
        task_type: TaskType::Planning,
        required_capabilities: RequiredCapabilities {
            needs_extended_thinking: true,
            min_context_tokens: 100_000,
            ..Default::default()
        },
        preferred_model: None,
    };
    let result = router.route(&req).unwrap();
    assert_eq!(result.provider_name, "expensive");
}
