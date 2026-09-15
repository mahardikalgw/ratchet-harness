use async_trait::async_trait;
use ratchet_core::{
    config::{ProjectConfig, RoutingPolicy},
    routing::Router,
    task_executor::TaskExecutor,
};
use ratchet_memory::ProjectMemory;
use ratchet_providers::{
    error::ProviderResult,
    models::CostModel,
    traits::{
        ChatRequest, ChatResponse, ChatStream, ProviderCapabilities, TokenUsage,
    },
    types::{MessageRole, ToolCall},
    ModelProvider,
};
use ratchet_spec::{schema::TaskNode, SpecParser, TaskId};
use std::sync::{Arc, Mutex};

/// A provider that returns a scripted sequence of responses and records every
/// request it receives, so tests can assert on the conversation history.
struct ScriptedProvider {
    responses: Mutex<Vec<ChatResponse>>,
    requests: Mutex<Vec<ChatRequest>>,
}

impl ScriptedProvider {
    fn new(responses: Vec<ChatResponse>) -> Self {
        Self {
            responses: Mutex::new(responses),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }

    fn last_request(&self) -> ChatRequest {
        self.requests.lock().unwrap().last().cloned().unwrap()
    }
}

#[async_trait]
impl ModelProvider for ScriptedProvider {
    async fn complete(&self, req: ChatRequest) -> ProviderResult<ChatResponse> {
        self.requests.lock().unwrap().push(req);
        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            // Exhausted script: behave like a model that stops.
            return Ok(text_response("(script exhausted)"));
        }
        Ok(responses.remove(0))
    }

    async fn stream(&self, _req: ChatRequest) -> ProviderResult<ChatStream> {
        unimplemented!("not needed")
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_tools: true,
            supports_streaming: false,
            max_context_tokens: 128_000,
            max_output_tokens: 4096,
            ..Default::default()
        }
    }

    fn cost_model(&self) -> CostModel {
        CostModel {
            usd_per_million_input_tokens: 1.0,
            usd_per_million_output_tokens: 2.0,
            usd_per_million_cached_tokens: None,
        }
    }

    fn name(&self) -> &str {
        "mock"
    }
}

fn text_response(text: &str) -> ChatResponse {
    ChatResponse {
        content: text.to_string(),
        tool_calls: vec![],
        usage: TokenUsage::default(),
        model: "mock".to_string(),
        provider: "mock".to_string(),
        finish_reason: Some("stop".to_string()),
    }
}

fn tool_response(name: &str, args: serde_json::Value, tokens: (u64, u64)) -> ChatResponse {
    ChatResponse {
        content: String::new(),
        tool_calls: vec![ToolCall {
            id: "call-1".to_string(),
            name: name.to_string(),
            arguments: args,
        }],
        usage: TokenUsage {
            input_tokens: tokens.0,
            output_tokens: tokens.1,
            cached_tokens: 0,
        },
        model: "mock".to_string(),
        provider: "mock".to_string(),
        finish_reason: Some("tool_use".to_string()),
    }
}

fn spec() -> ratchet_spec::SpecFile {
    SpecParser::new()
        .parse(
            "---\nid: loop\nstatus: draft\ntitle: \"Loop\"\n---\n\n\
             # Acceptance Criteria\n\n- [ ] AC-1: it works\n",
        )
        .unwrap()
}

fn task() -> TaskNode {
    TaskNode {
        id: TaskId("T-1".to_string()),
        title: "Do the thing".to_string(),
        description: "Implement it".to_string(),
        assigned_model: None,
        verification: None,
        estimated_tokens: None,
    }
}

fn executor(provider: Arc<ScriptedProvider>, ratchet_dir: &std::path::Path) -> TaskExecutor {
    let config = ProjectConfig {
        ratchet_dir: ratchet_dir.to_path_buf(),
        ..Default::default()
    };

    let mut providers: std::collections::HashMap<String, Arc<dyn ModelProvider>> =
        std::collections::HashMap::new();
    providers.insert("mock".to_string(), provider);

    let router = Router::new(RoutingPolicy::Fixed, providers);
    TaskExecutor::new(config, router)
}

/// Build an executor with distinct implementer and reviewer providers, so the
/// multi-agent pipeline can be observed.
fn delegated_executor(
    implementer: Arc<ScriptedProvider>,
    reviewer: Arc<ScriptedProvider>,
    max_rounds: u32,
    ratchet_dir: &std::path::Path,
) -> TaskExecutor {
    let mut config = ProjectConfig {
        ratchet_dir: ratchet_dir.to_path_buf(),
        ..Default::default()
    };
    config.delegation.review = true;
    config.delegation.max_review_rounds = max_rounds;
    config
        .delegation
        .roles
        .insert("implementer".to_string(), "impl".to_string());
    config
        .delegation
        .roles
        .insert("reviewer".to_string(), "rev".to_string());

    let mut providers: std::collections::HashMap<String, Arc<dyn ModelProvider>> =
        std::collections::HashMap::new();
    providers.insert("impl".to_string(), implementer);
    providers.insert("rev".to_string(), reviewer);

    let router = Router::new(RoutingPolicy::Fixed, providers);
    TaskExecutor::new(config, router)
}

/// A reviewer response carrying a structured verdict.
fn review_response(approved: bool, issues: &[&str]) -> ChatResponse {
    ChatResponse {
        content: serde_json::json!({
            "approved": approved,
            "issues": issues,
            "summary": if approved { "looks good" } else { "needs work" }
        })
        .to_string(),
        tool_calls: vec![],
        usage: TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            cached_tokens: 0,
        },
        model: "mock".to_string(),
        provider: "mock".to_string(),
        finish_reason: Some("stop".to_string()),
    }
}

#[tokio::test]
async fn loop_executes_tool_then_feeds_result_back_and_finishes() {
    let dir = tempfile::tempdir().unwrap();
    let provider = Arc::new(ScriptedProvider::new(vec![
        tool_response(
            "file_read",
            serde_json::json!({"path": "does-not-exist.txt"}),
            (10, 5),
        ),
        ChatResponse {
            content: "Done.".to_string(),
            tool_calls: vec![],
            usage: TokenUsage {
                input_tokens: 20,
                output_tokens: 8,
                cached_tokens: 0,
            },
            model: "mock".to_string(),
            provider: "mock".to_string(),
            finish_reason: Some("stop".to_string()),
        },
    ]));

    let mut exec = executor(Arc::clone(&provider), dir.path());
    let memory = ProjectMemory::new(dir.path().join("memory.json"));

    let result = exec
        .execute_single_task(&task(), &spec(), &memory)
        .await
        .unwrap();

    // Two model turns: one tool call, one final answer.
    assert_eq!(result.turns, 2);
    assert_eq!(result.output, "Done.");
    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].name, "file_read");

    // Usage accumulates across turns.
    assert_eq!(result.usage.input_tokens, 30);
    assert_eq!(result.usage.output_tokens, 13);

    // The second request must contain the tool result fed back to the model.
    assert_eq!(provider.request_count(), 2);
    let second = provider.last_request();
    let tool_message = second
        .messages
        .iter()
        .find(|m| m.role == MessageRole::Tool)
        .expect("tool result message must be sent back to the model");
    let results = tool_message.tool_results.as_ref().expect("tool results");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].tool_call_id, "call-1");
    // The tool failed (path denied), and the error is reported to the model.
    assert!(results[0].is_error);
}

#[tokio::test]
async fn loop_stops_at_turn_cap_for_a_looping_model() {
    let dir = tempfile::tempdir().unwrap();
    // Always return a tool call — would loop forever without a cap.
    let responses: Vec<ChatResponse> = (0..50)
        .map(|_| {
            tool_response(
                "file_read",
                serde_json::json!({"path": "x.txt"}),
                (1, 1),
            )
        })
        .collect();

    let provider = Arc::new(ScriptedProvider::new(responses));
    let mut exec = executor(Arc::clone(&provider), dir.path());
    let memory = ProjectMemory::new(dir.path().join("memory.json"));

    let result = exec
        .execute_single_task(&task(), &spec(), &memory)
        .await
        .unwrap();

    assert_eq!(result.turns, 12, "must stop at MAX_TURNS");
    assert!(result.output.contains("stopped after 12 turns"));
    assert_eq!(provider.request_count(), 12);
}

#[tokio::test]
async fn immediate_final_answer_takes_one_turn() {
    let dir = tempfile::tempdir().unwrap();
    let provider = Arc::new(ScriptedProvider::new(vec![ChatResponse {
        content: "Nothing to do.".to_string(),
        tool_calls: vec![],
        usage: TokenUsage {
            input_tokens: 5,
            output_tokens: 3,
            cached_tokens: 0,
        },
        model: "mock".to_string(),
        provider: "mock".to_string(),
        finish_reason: Some("stop".to_string()),
    }]));

    let mut exec = executor(Arc::clone(&provider), dir.path());
    let memory = ProjectMemory::new(dir.path().join("memory.json"));

    let result = exec
        .execute_single_task(&task(), &spec(), &memory)
        .await
        .unwrap();

    assert_eq!(result.turns, 1);
    assert_eq!(result.output, "Nothing to do.");
    assert!(result.tool_calls.is_empty());
}

#[tokio::test]
async fn metrics_are_recorded_per_task() {
    let dir = tempfile::tempdir().unwrap();
    let provider = Arc::new(ScriptedProvider::new(vec![text_response("ok")]));

    let mut exec = executor(Arc::clone(&provider), dir.path());
    let memory = ProjectMemory::new(dir.path().join("memory.json"));

    exec.execute_single_task(&task(), &spec(), &memory)
        .await
        .unwrap();

    let metrics = exec.all_metrics();
    assert_eq!(metrics.len(), 1);
    assert_eq!(metrics[0].task_id, "T-1");
    assert_eq!(metrics[0].provider, "mock");
    assert_eq!(metrics[0].model, "mock");
}


// ----- Multi-agent delegation -----

#[tokio::test]
async fn review_pipeline_runs_reviewer_after_implementer() {
    let dir = tempfile::tempdir().unwrap();

    let implementer = Arc::new(ScriptedProvider::new(vec![text_response("implemented")]));
    let reviewer = Arc::new(ScriptedProvider::new(vec![review_response(true, &[])]));

    let mut exec = delegated_executor(
        Arc::clone(&implementer),
        Arc::clone(&reviewer),
        2,
        dir.path(),
    );
    let memory = ProjectMemory::new(dir.path().join("memory.json"));

    let result = exec
        .execute_single_task(&task(), &spec(), &memory)
        .await
        .unwrap();

    // The reviewer ran exactly once and approved.
    assert_eq!(implementer.request_count(), 1);
    assert_eq!(reviewer.request_count(), 1);

    let review = result.review.expect("review verdict");
    assert!(review.approved);
    assert_eq!(result.roles, vec!["implementer", "reviewer"]);
}

#[tokio::test]
async fn rejection_sends_the_task_back_for_revision() {
    let dir = tempfile::tempdir().unwrap();

    // Reject once, then approve.
    let reviewer = Arc::new(ScriptedProvider::new(vec![
        review_response(false, &["missing test", "unused import"]),
        review_response(true, &[]),
    ]));
    let implementer = Arc::new(ScriptedProvider::new(vec![
        text_response("first attempt"),
        text_response("revised"),
    ]));

    let mut exec = delegated_executor(
        Arc::clone(&implementer),
        Arc::clone(&reviewer),
        2,
        dir.path(),
    );
    let memory = ProjectMemory::new(dir.path().join("memory.json"));

    let result = exec
        .execute_single_task(&task(), &spec(), &memory)
        .await
        .unwrap();

    assert_eq!(implementer.request_count(), 2, "implementer should revise once");
    assert_eq!(reviewer.request_count(), 2);
    assert_eq!(
        result.roles,
        vec!["implementer", "reviewer", "implementer", "reviewer"]
    );

    let review = result.review.expect("review verdict");
    assert!(review.approved);
    assert_eq!(result.output, "revised");
}

#[tokio::test]
async fn revision_loop_is_bounded_by_max_rounds() {
    let dir = tempfile::tempdir().unwrap();

    // Reviewer never approves.
    let reviewer = Arc::new(ScriptedProvider::new(
        (0..10).map(|_| review_response(false, &["still broken"])).collect(),
    ));
    let implementer = Arc::new(ScriptedProvider::new(
        (0..10).map(|_| text_response("attempt")).collect(),
    ));

    let mut exec = delegated_executor(
        Arc::clone(&implementer),
        Arc::clone(&reviewer),
        1, // one revision allowed
        dir.path(),
    );
    let memory = ProjectMemory::new(dir.path().join("memory.json"));

    let result = exec
        .execute_single_task(&task(), &spec(), &memory)
        .await
        .unwrap();

    // 1 initial + 1 revision = 2 implementer passes, 2 reviews.
    assert_eq!(implementer.request_count(), 2);
    assert_eq!(reviewer.request_count(), 2);

    let review = result.review.expect("review verdict");
    assert!(!review.approved, "still rejected after exhausting rounds");
}

#[tokio::test]
async fn review_is_skipped_when_disabled() {
    let dir = tempfile::tempdir().unwrap();

    let provider = Arc::new(ScriptedProvider::new(vec![text_response("done")]));
    let mut exec = executor(Arc::clone(&provider), dir.path());
    let memory = ProjectMemory::new(dir.path().join("memory.json"));

    let result = exec
        .execute_single_task(&task(), &spec(), &memory)
        .await
        .unwrap();

    assert!(result.review.is_none());
    assert_eq!(result.roles, vec!["implementer"]);
}
