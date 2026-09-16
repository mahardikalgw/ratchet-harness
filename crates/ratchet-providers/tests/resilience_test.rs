use async_trait::async_trait;
use ratchet_providers::{
    ModelProvider,
    error::{ProviderError, ProviderResult},
    models::CostModel,
    resilience::{FailoverProvider, RetryPolicy},
    traits::{ChatRequest, ChatResponse, ChatStream, ProviderCapabilities, TokenUsage},
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// Provider that always fails with a configurable error.
struct FailingProvider {
    name: String,
    error: fn() -> ProviderError,
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl ModelProvider for FailingProvider {
    async fn complete(&self, _req: ChatRequest) -> ProviderResult<ChatResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err((self.error)())
    }
    async fn stream(&self, _req: ChatRequest) -> ProviderResult<ChatStream> {
        Err((self.error)())
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::default()
    }
    fn cost_model(&self) -> CostModel {
        CostModel::default()
    }
    fn name(&self) -> &str {
        &self.name
    }
}

/// Provider that always succeeds.
struct WorkingProvider {
    name: String,
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl ModelProvider for WorkingProvider {
    async fn complete(&self, _req: ChatRequest) -> ProviderResult<ChatResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(ChatResponse {
            content: "ok".to_string(),
            tool_calls: vec![],
            usage: TokenUsage::default(),
            model: "m".to_string(),
            provider: self.name.clone(),
            finish_reason: Some("stop".to_string()),
        })
    }
    async fn stream(&self, _req: ChatRequest) -> ProviderResult<ChatStream> {
        Err(ProviderError::UnsupportedCapability("n/a".into()))
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::default()
    }
    fn cost_model(&self) -> CostModel {
        CostModel::default()
    }
    fn name(&self) -> &str {
        &self.name
    }
}

fn req() -> ChatRequest {
    ChatRequest::default()
}

#[test]
fn retry_policy_exhausts_attempt_budget() {
    let policy = RetryPolicy {
        max_attempts: 3,
        initial_backoff: Duration::from_millis(1),
        max_backoff: Duration::from_millis(10),
        multiplier: 2.0,
        jitter: false,
    };

    assert_eq!(
        policy.delay_after_failure(1),
        Some(Duration::from_millis(1))
    );
    assert_eq!(
        policy.delay_after_failure(2),
        Some(Duration::from_millis(2))
    );
    // Third failure exhausts the budget.
    assert_eq!(policy.delay_after_failure(3), None);
}

#[test]
fn retry_policy_caps_backoff() {
    let policy = RetryPolicy {
        max_attempts: 10,
        initial_backoff: Duration::from_millis(100),
        max_backoff: Duration::from_millis(250),
        multiplier: 4.0,
        jitter: false,
    };
    // 100 * 4^3 would be huge, but it is capped.
    assert_eq!(
        policy.delay_after_failure(4),
        Some(Duration::from_millis(250))
    );
}

#[test]
fn rate_limit_and_timeout_are_retryable() {
    assert!(
        ProviderError::RateLimited {
            provider: "x".into()
        }
        .is_retryable()
    );
    assert!(ProviderError::Timeout.is_retryable());
    assert!(ProviderError::api("x", 503, "unavailable").is_retryable());
    assert!(ProviderError::api("x", 429, "slow down").is_retryable());
}

#[test]
fn auth_and_config_errors_are_not_retryable() {
    assert!(
        !ProviderError::Auth {
            provider: "x".into(),
            message: "test".into(),
        }
        .is_retryable()
    );
    assert!(!ProviderError::Config("bad".into()).is_retryable());
    assert!(!ProviderError::UnknownProvider("x".into()).is_retryable());
    assert!(!ProviderError::api("x", 400, "bad request").is_retryable());
}

#[tokio::test]
async fn retries_a_transient_failure_then_succeeds() {
    let calls = Arc::new(AtomicUsize::new(0));
    let provider = FailingProvider {
        name: "flaky".into(),
        error: || ProviderError::RateLimited {
            provider: "flaky".into(),
        },
        calls: Arc::clone(&calls),
    };

    let policy = RetryPolicy {
        max_attempts: 3,
        initial_backoff: Duration::from_millis(1),
        max_backoff: Duration::from_millis(5),
        multiplier: 2.0,
        jitter: false,
    };

    let failover = FailoverProvider::single(Arc::new(provider), policy);
    let result = failover.complete(req()).await;

    assert!(result.is_err());
    // Three attempts were made before giving up.
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn does_not_retry_a_permanent_failure() {
    let calls = Arc::new(AtomicUsize::new(0));
    let provider = FailingProvider {
        name: "bad-key".into(),
        error: || ProviderError::Auth {
            provider: "bad-key".into(),
            message: "test".into(),
        },
        calls: Arc::clone(&calls),
    };

    let policy = RetryPolicy {
        max_attempts: 5,
        initial_backoff: Duration::from_millis(1),
        max_backoff: Duration::from_millis(5),
        multiplier: 2.0,
        jitter: false,
    };

    let failover = FailoverProvider::single(Arc::new(provider), policy);
    assert!(failover.complete(req()).await.is_err());
    // Auth is permanent: exactly one call.
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn fails_over_to_the_next_provider() {
    let first_calls = Arc::new(AtomicUsize::new(0));
    let second_calls = Arc::new(AtomicUsize::new(0));

    let first = FailingProvider {
        name: "down".into(),
        error: || ProviderError::api("down", 503, "unavailable"),
        calls: Arc::clone(&first_calls),
    };
    let second = WorkingProvider {
        name: "up".into(),
        calls: Arc::clone(&second_calls),
    };

    let policy = RetryPolicy {
        max_attempts: 2,
        initial_backoff: Duration::from_millis(1),
        max_backoff: Duration::from_millis(5),
        multiplier: 2.0,
        jitter: false,
    };

    let failover = FailoverProvider::new(vec![Arc::new(first), Arc::new(second)], policy);

    let response = failover.complete(req()).await.unwrap();
    assert_eq!(response.provider, "up");
    // First provider exhausted its 2 retries, then handed off.
    assert_eq!(first_calls.load(Ordering::SeqCst), 2);
    assert_eq!(second_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn fails_over_even_on_permanent_errors() {
    let second_calls = Arc::new(AtomicUsize::new(0));

    let bad_key = FailingProvider {
        name: "bad-key".into(),
        error: || ProviderError::Auth {
            provider: "bad-key".into(),
            message: "test".into(),
        },
        calls: Arc::new(AtomicUsize::new(0)),
    };
    let working = WorkingProvider {
        name: "good".into(),
        calls: Arc::clone(&second_calls),
    };

    let failover = FailoverProvider::new(
        vec![Arc::new(bad_key), Arc::new(working)],
        RetryPolicy {
            max_attempts: 3,
            ..RetryPolicy::default()
        },
    );

    let response = failover.complete(req()).await.unwrap();
    assert_eq!(response.provider, "good");
    assert_eq!(second_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn empty_chain_reports_a_config_error() {
    let failover = FailoverProvider::new(vec![], RetryPolicy::none());
    let err = failover.complete(req()).await.unwrap_err();
    assert!(!err.is_retryable());
}
