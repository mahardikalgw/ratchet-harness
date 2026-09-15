use async_trait::async_trait;
use ratchet_a2a::{A2aResult, AgentCard, Task, TaskSendParams, TaskState, TaskStatus};
use ratchet_server::{
    a2a::{A2aDispatcher, A2aHandler},
    dashboard::{DashboardSource, render_dashboard},
    http::HttpResponse,
    server::RatchetServer,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// ----- Fakes -----

/// Accepts tasks and reports them completed immediately.
struct EchoAgent {
    tasks: Mutex<HashMap<String, Task>>,
    counter: Mutex<u64>,
}

impl EchoAgent {
    fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
            counter: Mutex::new(0),
        }
    }

    fn next_id(&self) -> String {
        let mut c = self.counter.lock().unwrap();
        *c += 1;
        format!("task-{}", c)
    }
}

#[async_trait]
impl A2aHandler for EchoAgent {
    fn agent_card(&self) -> AgentCard {
        AgentCard::ratchet("http://127.0.0.1:0/a2a", "test")
    }

    async fn send(&self, params: TaskSendParams) -> A2aResult<Task> {
        let id = params.id.clone().unwrap_or_else(|| self.next_id());
        let mut task = Task::new(id.clone());
        task.transition(TaskStatus::with_message(TaskState::Working, "working"));
        task.transition(TaskStatus::with_message(TaskState::Completed, "done"));
        task.add_text_artifact("summary", "completed immediately");
        self.tasks.lock().unwrap().insert(id, task.clone());
        Ok(task)
    }

    async fn get(&self, id: &str) -> A2aResult<Task> {
        self.tasks
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| ratchet_a2a::A2aError::TaskNotFound(id.to_string()))
    }

    async fn cancel(&self, _id: &str) -> A2aResult<Task> {
        Err(ratchet_a2a::A2aError::NotCancelable(
            "completed".to_string(),
        ))
    }
}

struct FakeDashboard;

#[async_trait]
impl DashboardSource for FakeDashboard {
    async fn summary(&self) -> Value {
        json!({
            "total_tasks": 3,
            "tasks_passed": 2,
            "tasks_failed": 1,
            "total_input_tokens": 100,
            "total_output_tokens": 50,
            "total_estimated_cost_usd": 0.25,
            "by_provider": {"deepseek": {"tasks": 3, "estimated_cost_usd": 0.25}}
        })
    }
    async fn tasks(&self) -> Value {
        json!([{"task_id":"T-1","provider":"deepseek","model":"chat"}])
    }
    async fn specs(&self) -> Value {
        json!([{"id":"slugify","title":"Slugify","status":"draft","priority":"normal"}])
    }
}

fn dispatcher() -> Arc<A2aDispatcher> {
    Arc::new(A2aDispatcher::new(Arc::new(EchoAgent::new())))
}

// ----- A2A dispatcher -----

#[tokio::test]
async fn send_returns_a_task_and_get_finds_it() {
    let d = dispatcher();

    let result = d
        .handle(
            "tasks/send",
            serde_json::to_value(TaskSendParams {
                id: None,
                session_id: None,
                message: ratchet_a2a::Message::user_text("run spec:slugify"),
                metadata: json!({"spec_id": "slugify"}),
            })
            .unwrap(),
        )
        .await
        .unwrap();

    let task: Task = serde_json::from_value(result).unwrap();
    assert_eq!(task.status.state, TaskState::Completed);
    assert_eq!(task.artifacts.len(), 1);

    let fetched = d.handle("tasks/get", json!({"id": task.id})).await.unwrap();
    let fetched: Task = serde_json::from_value(fetched).unwrap();
    assert_eq!(fetched.id, task.id);
}

#[tokio::test]
async fn get_unknown_task_is_an_error() {
    let d = dispatcher();
    let err = d
        .handle("tasks/get", json!({"id": "nope"}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), -32001);
}

#[tokio::test]
async fn get_without_id_is_invalid_params() {
    let d = dispatcher();
    let err = d.handle("tasks/get", json!({})).await.unwrap_err();
    assert_eq!(err.code(), -32602);
}

#[tokio::test]
async fn cancelling_a_finished_task_is_rejected() {
    let d = dispatcher();
    let sent = d
        .handle(
            "tasks/send",
            serde_json::to_value(TaskSendParams {
                id: Some("fixed".to_string()),
                session_id: None,
                message: ratchet_a2a::Message::user_text("x"),
                metadata: json!({"spec_id": "s"}),
            })
            .unwrap(),
        )
        .await
        .unwrap();
    let task: Task = serde_json::from_value(sent).unwrap();

    let err = d
        .handle("tasks/cancel", json!({"id": task.id}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), -32002);
}

#[tokio::test]
async fn unknown_method_is_unsupported() {
    let d = dispatcher();
    let err = d.handle("tasks/explode", json!({})).await.unwrap_err();
    assert_eq!(err.code(), -32004);
}

// ----- Dashboard rendering -----

#[test]
fn dashboard_escapes_html_from_spec_titles() {
    // A spec title containing markup must not be injected into the page.
    let specs = json!([{
        "id": "x",
        "title": "<script>alert(1)</script>",
        "status": "draft",
        "priority": "normal"
    }]);

    let html = render_dashboard(&json!({}), &json!([]), &specs);
    assert!(!html.contains("<script>alert(1)</script>"));
    assert!(html.contains("&lt;script&gt;"));
}

#[test]
fn dashboard_renders_summary_numbers() {
    let summary = json!({
        "total_tasks": 7,
        "tasks_passed": 5,
        "tasks_failed": 2,
        "total_input_tokens": 1000,
        "total_output_tokens": 500,
        "total_estimated_cost_usd": 1.2345,
        "by_provider": {"deepseek": {"tasks": 7, "estimated_cost_usd": 1.2345}}
    });

    let html = render_dashboard(&summary, &json!([]), &json!([]));
    assert!(html.contains(">7<"));
    assert!(html.contains("$1.2345"));
    assert!(html.contains("deepseek"));
}

#[test]
fn dashboard_handles_empty_state() {
    let html = render_dashboard(&json!({}), &json!([]), &json!([]));
    assert!(html.contains("No specs found."));
    assert!(html.contains("No task records yet."));
}

// ----- HTTP surface (in-process routing) -----

#[test]
fn responses_carry_correct_reasons() {
    assert_eq!(HttpResponse::not_found().reason(), "Not Found");
    assert_eq!(
        HttpResponse::method_not_allowed().reason(),
        "Method Not Allowed"
    );
    assert_eq!(HttpResponse::json(&json!({})).reason(), "OK");
}

/// End-to-end over a real socket: start the server, hit it, assert.
#[tokio::test]
async fn serves_card_dashboard_and_a2a_over_http() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Reserve an ephemeral port, release it, then hand the number to the server.
    let port = {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap().port()
    };

    let handle = tokio::spawn(async move {
        let server = RatchetServer::new(port, Arc::new(FakeDashboard), dispatcher());
        let _ = server.run().await;
    });

    // Give the listener a moment to bind.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Agent card
    let body = http_get(port, "/.well-known/agent.json").await;
    let card: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(card["name"], "Ratchet");

    // Dashboard page
    let html = http_get(port, "/").await;
    assert!(html.contains("Ratchet"));
    assert!(html.contains("deepseek"));

    // Summary API
    let summary = http_get(port, "/api/summary").await;
    let summary: Value = serde_json::from_str(&summary).unwrap();
    assert_eq!(summary["total_tasks"], 3);

    // A2A JSON-RPC
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tasks/send",
        "params": {
            "message": {"role": "user", "parts": [{"type": "text", "text": "run spec:slugify"}]},
            "metadata": {"spec_id": "slugify"}
        }
    });
    let response = http_post(port, "/a2a", &payload.to_string()).await;
    let response: Value = serde_json::from_str(&response).unwrap();
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["result"]["status"]["state"], "completed");

    // JSON-RPC error shape
    let bad = json!({"jsonrpc":"2.0","id":2,"method":"tasks/get","params":{"id":"nope"}});
    let response = http_post(port, "/a2a", &bad.to_string()).await;
    let response: Value = serde_json::from_str(&response).unwrap();
    assert_eq!(response["error"]["code"], -32001);

    // Unknown path
    let not_found = http_get(port, "/nope").await;
    assert_eq!(not_found, "not found");

    handle.abort();

    async fn http_get(port: u16, path: &str) -> String {
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        read_body(&mut stream).await
    }

    async fn http_post(port: u16, path: &str, body: &str) -> String {
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        read_body(&mut stream).await
    }

    async fn read_body(stream: &mut tokio::net::TcpStream) -> String {
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.unwrap();
        let text = String::from_utf8_lossy(&raw);
        match text.split_once("\r\n\r\n") {
            Some((_, body)) => body.to_string(),
            None => text.to_string(),
        }
    }
}
