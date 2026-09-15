use crate::{CoreResult, config::RoutingPolicy};
use ratchet_providers::{CostModel, ModelProvider, ProviderCapabilities};
use std::collections::HashMap;
use std::sync::Arc;

/// Selects and ranks providers for a task based on the configured policy.
#[derive(Clone)]
pub struct Router {
    policy: RoutingPolicy,
    providers: HashMap<String, Arc<dyn ModelProvider>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoutingRequest {
    pub task_type: TaskType,
    pub required_capabilities: RequiredCapabilities,
    pub preferred_model: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskType {
    Planning,
    Coding,
    Review,
    Testing,
    Documentation,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RequiredCapabilities {
    pub needs_tools: bool,
    pub needs_vision: bool,
    pub needs_extended_thinking: bool,
    pub min_context_tokens: u64,
}

#[derive(Clone)]
pub struct RoutingResult {
    pub provider_name: String,
    pub provider: Arc<dyn ModelProvider>,
    pub reason: String,
}

impl Router {
    pub fn new(policy: RoutingPolicy, providers: HashMap<String, Arc<dyn ModelProvider>>) -> Self {
        Self { policy, providers }
    }

    /// The single best provider.
    pub fn route(&self, req: &RoutingRequest) -> CoreResult<RoutingResult> {
        self.route_all(req)?
            .into_iter()
            .next()
            .ok_or_else(|| crate::CoreError::NoProvider("no providers configured".into()))
    }

    /// All providers in preference order, so the caller can fail over.
    pub fn route_all(&self, req: &RoutingRequest) -> CoreResult<Vec<RoutingResult>> {
        if self.providers.is_empty() {
            return Err(crate::CoreError::NoProvider(
                "no providers configured".into(),
            ));
        }

        let mut ranked: Vec<RoutingResult> = Vec::new();
        let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();

        // An explicit model/provider override always wins.
        if let Some(preferred) = &req.preferred_model {
            if let Some(provider) = self.providers.get(preferred) {
                ranked.push(RoutingResult {
                    provider_name: preferred.clone(),
                    provider: Arc::clone(provider),
                    reason: format!("explicitly requested: {preferred}"),
                });
                used.insert(preferred.clone());
            }
        }

        for (name, reason) in self.rank(req) {
            if used.contains(&name) {
                continue;
            }
            if let Some(provider) = self.providers.get(&name) {
                ranked.push(RoutingResult {
                    provider_name: name.clone(),
                    provider: Arc::clone(provider),
                    reason,
                });
                used.insert(name);
            }
        }

        // Anything not ranked (e.g. missing a required capability) is still a
        // last-resort fallback rather than being dropped entirely.
        for (name, provider) in &self.providers {
            if used.contains(name) {
                continue;
            }
            ranked.push(RoutingResult {
                provider_name: name.clone(),
                provider: Arc::clone(provider),
                reason: "fallback (does not meet all requirements)".to_string(),
            });
        }

        Ok(ranked)
    }

    /// Produce `(provider_name, reason)` pairs in preference order.
    fn rank(&self, req: &RoutingRequest) -> Vec<(String, String)> {
        let capable: Vec<(&String, &Arc<dyn ModelProvider>)> = self
            .providers
            .iter()
            .filter(|(_, p)| self.meets_requirements(p.capabilities(), &req.required_capabilities))
            .collect();

        // Prefer providers that satisfy the requirements; fall back to all.
        let pool: Vec<(&String, &Arc<dyn ModelProvider>)> = if capable.is_empty() {
            self.providers.iter().collect()
        } else {
            capable
        };

        match self.policy {
            RoutingPolicy::Fixed => pool
                .iter()
                .map(|(n, p)| ((*n).clone(), format!("fixed default ({})", p.name())))
                .collect(),

            RoutingPolicy::CapabilityThenCost => {
                let mut scored: Vec<_> = pool
                    .iter()
                    .map(|(n, p)| {
                        let score = self.score_provider(p.capabilities(), &p.cost_model(), req);
                        ((*n).clone(), score)
                    })
                    .collect();
                scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                scored
                    .into_iter()
                    .map(|(n, _)| (n, "best capability match at reasonable cost".to_string()))
                    .collect()
            }

            RoutingPolicy::CostOptimized => {
                let mut scored: Vec<_> = pool
                    .iter()
                    .map(|(n, p)| {
                        let c = p.cost_model();
                        let avg = (c.usd_per_million_input_tokens
                            + c.usd_per_million_output_tokens)
                            / 2.0;
                        ((*n).clone(), avg)
                    })
                    .collect();
                scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                scored
                    .into_iter()
                    .map(|(n, _)| (n, "lowest cost per token".to_string()))
                    .collect()
            }

            RoutingPolicy::Fastest => {
                // Local models first (no network round-trip), then the rest.
                let mut ordered: Vec<(String, String)> = Vec::new();
                for (n, p) in &pool {
                    if p.name() == "ollama" {
                        ordered.push(((*n).clone(), "local model (fastest)".to_string()));
                    }
                }
                for (n, _) in &pool {
                    if !ordered.iter().any(|(name, _)| name == *n) {
                        ordered.push(((*n).clone(), "remote provider".to_string()));
                    }
                }
                ordered
            }
        }
    }

    fn meets_requirements(&self, caps: ProviderCapabilities, req: &RequiredCapabilities) -> bool {
        (!req.needs_tools || caps.supports_tools)
            && (!req.needs_vision || caps.supports_vision)
            && (!req.needs_extended_thinking || caps.supports_extended_thinking)
            && caps.max_context_tokens >= req.min_context_tokens
    }

    fn score_provider(
        &self,
        caps: ProviderCapabilities,
        cost: &CostModel,
        req: &RoutingRequest,
    ) -> f64 {
        let mut score = 0.0;

        if req.required_capabilities.needs_tools && caps.supports_tools {
            score += 10.0;
        }
        if req.required_capabilities.needs_vision && caps.supports_vision {
            score += 10.0;
        }
        if req.required_capabilities.needs_extended_thinking && caps.supports_extended_thinking {
            score += 10.0;
        }

        score += (caps.max_context_tokens as f64 / 10_000.0).min(5.0);

        let avg_cost =
            (cost.usd_per_million_input_tokens + cost.usd_per_million_output_tokens) / 2.0;
        score -= avg_cost * 2.0;

        score
    }
}
