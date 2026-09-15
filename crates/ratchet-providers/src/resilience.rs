use crate::{
    error::{ProviderError, ProviderResult},
    models::CostModel,
    traits::*,
};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

/// Exponential-backoff retry policy for transient provider failures.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Total attempts (including the first), so `1` disables retrying.
    pub max_attempts: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub multiplier: f64,
    /// Add pseudo-random jitter so concurrent callers don't retry in lockstep.
    pub jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(30),
            multiplier: 2.0,
            jitter: true,
        }
    }
}

impl RetryPolicy {
    /// No retrying — fail fast.
    pub fn none() -> Self {
        Self {
            max_attempts: 1,
            ..Self::default()
        }
    }

    /// Delay before the retry that follows `failures` consecutive failures.
    /// Returns `None` when the attempt budget is exhausted.
    pub fn delay_after_failure(&self, failures: u32) -> Option<Duration> {
        if failures >= self.max_attempts {
            return None;
        }
        let exp = self.initial_backoff.mul_f64(self.multiplier.powi(failures as i32 - 1));
        let capped = exp.min(self.max_backoff);
        Some(if self.jitter {
            capped + self.jitter_for(failures)
        } else {
            capped
        })
    }

    fn jitter_for(&self, failures: u32) -> Duration {
        // Cheap, dependency-free jitter derived from the clock and attempt.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        Duration::from_millis(u64::from(nanos % 250).saturating_add(u64::from(failures) * 17))
    }
}

/// Run `op`, retrying while the error is classified as transient.
pub async fn retry_async<F, Fut, T>(policy: &RetryPolicy, mut op: F) -> ProviderResult<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ProviderResult<T>>,
{
    let mut failures = 0u32;
    loop {
        match op().await {
            Ok(value) => return Ok(value),
            Err(e) => {
                failures += 1;
                if !e.is_retryable() {
                    return Err(e);
                }
                match policy.delay_after_failure(failures) {
                    Some(delay) => {
                        tracing::warn!(
                            failures,
                            delay_ms = delay.as_millis() as u64,
                            error = %e,
                            "retrying provider call"
                        );
                        tokio::time::sleep(delay).await;
                    }
                    None => return Err(e),
                }
            }
        }
    }
}

/// Wraps one or more providers, retrying each and failing over to the next.
///
/// Providers are tried in order. A provider that exhausts its retries hands
/// off to the next, so an outage on one backend does not block work that
/// another backend could serve.
#[derive(Clone)]
pub struct FailoverProvider {
    providers: Vec<Arc<dyn ModelProvider>>,
    retry: RetryPolicy,
}

impl FailoverProvider {
    pub fn new(providers: Vec<Arc<dyn ModelProvider>>, retry: RetryPolicy) -> Self {
        Self { providers, retry }
    }

    /// Wrap a single provider with retry (no failover).
    pub fn single(provider: Arc<dyn ModelProvider>, retry: RetryPolicy) -> Self {
        Self {
            providers: vec![provider],
            retry,
        }
    }

    pub fn len(&self) -> usize {
        self.providers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    fn primary(&self) -> Option<&Arc<dyn ModelProvider>> {
        self.providers.first()
    }
}

#[async_trait]
impl ModelProvider for FailoverProvider {
    async fn complete(&self, req: ChatRequest) -> ProviderResult<ChatResponse> {
        if self.providers.is_empty() {
            return Err(ProviderError::Config("no providers available".into()));
        }

        let mut last_error: Option<ProviderError> = None;

        for (idx, provider) in self.providers.iter().enumerate() {
            let name = provider.name().to_string();
            match retry_async(&self.retry, || provider.complete(req.clone())).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    if idx + 1 < self.providers.len() {
                        tracing::warn!(
                            provider = %name,
                            error = %e,
                            "provider failed, failing over"
                        );
                    }
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| ProviderError::Config("no providers available".into())))
    }

    async fn stream(&self, req: ChatRequest) -> ProviderResult<ChatStream> {
        if self.providers.is_empty() {
            return Err(ProviderError::Config("no providers available".into()));
        }

        let mut last_error: Option<ProviderError> = None;

        for provider in &self.providers {
            // Only the initial connection is retried; a stream that fails
            // mid-flight is handed to the caller to decide.
            match retry_async(&self.retry, || provider.stream(req.clone())).await {
                Ok(stream) => return Ok(stream),
                Err(e) => last_error = Some(e),
            }
        }

        Err(last_error.unwrap_or_else(|| ProviderError::Config("no providers available".into())))
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.primary()
            .map(|p| p.capabilities())
            .unwrap_or_default()
    }

    fn cost_model(&self) -> CostModel {
        self.primary()
            .map(|p| p.cost_model())
            .unwrap_or_default()
    }

    fn name(&self) -> &str {
        self.primary().map(|p| p.name()).unwrap_or("failover")
    }
}

impl std::fmt::Debug for FailoverProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FailoverProvider")
            .field(
                "providers",
                &self.providers.iter().map(|p| p.name()).collect::<Vec<_>>(),
            )
            .field("retry", &self.retry)
            .finish()
    }
}
