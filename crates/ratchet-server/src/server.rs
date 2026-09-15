use crate::{
    a2a::A2aDispatcher,
    dashboard::DashboardSource,
    http::{HttpRequest, HttpResponse},
};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

/// Maximum accepted request body. A local dashboard never needs more, and
/// bounding it prevents a malformed client from exhausting memory.
const MAX_BODY: usize = 1_000_000;

/// Serves the dashboard and the A2A endpoint on one port.
pub struct RatchetServer {
    port: u16,
    dashboard: Arc<dyn DashboardSource>,
    a2a: Arc<A2aDispatcher>,
}

impl RatchetServer {
    pub fn new(port: u16, dashboard: Arc<dyn DashboardSource>, a2a: Arc<A2aDispatcher>) -> Self {
        Self {
            port,
            dashboard,
            a2a,
        }
    }

    /// Bind the listening socket.
    ///
    /// Separated from [`Self::serve`] so a caller (or a test) can bind port 0
    /// and learn the assigned port without a bind/drop/re-bind race.
    pub async fn bind(port: u16) -> std::io::Result<TcpListener> {
        TcpListener::bind(("127.0.0.1", port)).await
    }

    pub async fn run(self) -> std::io::Result<()> {
        let listener = Self::bind(self.port).await?;
        self.serve(listener).await
    }

    /// Serve on an already-bound listener.
    pub async fn serve(self, listener: TcpListener) -> std::io::Result<()> {
        let port = listener.local_addr().map(|a| a.port()).unwrap_or(self.port);
        println!("📊 Dashboard: http://127.0.0.1:{port}/");
        println!("🤝 A2A agent card: http://127.0.0.1:{port}/.well-known/agent.json");

        let shared = Arc::new(self);

        loop {
            let (stream, _addr) = listener.accept().await?;
            let server = Arc::clone(&shared);
            tokio::spawn(async move {
                if let Err(e) = server.handle_connection(stream).await {
                    tracing::debug!(error = %e, "connection closed with error");
                }
            });
        }
    }

    async fn handle_connection(&self, stream: TcpStream) -> std::io::Result<()> {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        let Some(request) = read_request(&mut reader).await? else {
            return Ok(());
        };

        let response = self.route(&request).await;
        write_response(&mut writer, response).await
    }

    async fn route(&self, request: &HttpRequest) -> HttpResponse {
        let path = request.path.as_str();

        match (request.method.as_str(), path) {
            // ----- A2A -----
            ("GET", "/.well-known/agent.json") => {
                HttpResponse::json(&serde_json::to_value(self.a2a.card()).unwrap_or_default())
            }
            ("POST", "/a2a") => self.handle_a2a(request).await,

            // ----- Dashboard API -----
            ("GET", "/api/summary") => HttpResponse::json(&self.dashboard.summary().await),
            ("GET", "/api/tasks") => HttpResponse::json(&self.dashboard.tasks().await),
            ("GET", "/api/specs") => HttpResponse::json(&self.dashboard.specs().await),

            // ----- Dashboard page -----
            ("GET", "/") | ("GET", "/index.html") => {
                HttpResponse::html(self.dashboard.html().await)
            }

            ("GET", "/health") => HttpResponse::json(&serde_json::json!({"status": "ok"})),

            ("GET", _) => HttpResponse::not_found(),
            ("POST", _) => HttpResponse::not_found(),
            _ => HttpResponse::method_not_allowed(),
        }
    }

    async fn handle_a2a(&self, request: &HttpRequest) -> HttpResponse {
        let payload = match request.body_json() {
            Ok(value) => value,
            Err(e) => {
                return HttpResponse::bad_request(format!("invalid JSON body: {e}"));
            }
        };

        let id = payload
            .get("id")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let method = payload.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = payload
            .get("params")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        if method.is_empty() {
            return HttpResponse::json(&jsonrpc_error(id, -32600, "missing method"));
        }

        match self.a2a.handle(method, params).await {
            Ok(result) => HttpResponse::json(&jsonrpc_result(id, result)),
            Err(e) => HttpResponse::json(&jsonrpc_error(id, e.code(), &e.to_string())),
        }
    }
}

fn jsonrpc_result(id: serde_json::Value, result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn jsonrpc_error(id: serde_json::Value, code: i32, message: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message}
    })
}

/// Parse one HTTP/1.1 request. Returns `None` for an empty connection.
async fn read_request(
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
) -> std::io::Result<Option<HttpRequest>> {
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).await? == 0 {
        return Ok(None);
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    if method.is_empty() {
        return Ok(None);
    }

    let (path, query) = split_query(&target);

    // Headers.
    let mut headers = std::collections::HashMap::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            break;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    // Body, bounded by Content-Length.
    let content_length = headers
        .get("content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);

    let mut body = Vec::new();
    if content_length > 0 {
        if content_length > MAX_BODY {
            return Ok(Some(HttpRequest {
                method,
                path,
                query,
                headers,
                body: Vec::new(),
            }));
        }
        body = vec![0u8; content_length];
        reader.read_exact(&mut body).await?;
    }

    Ok(Some(HttpRequest {
        method,
        path,
        query,
        headers,
        body,
    }))
}

fn split_query(target: &str) -> (String, std::collections::HashMap<String, String>) {
    let mut query = std::collections::HashMap::new();
    match target.split_once('?') {
        Some((path, qs)) => {
            for pair in qs.split('&') {
                if let Some((k, v)) = pair.split_once('=') {
                    query.insert(k.to_string(), v.to_string());
                }
            }
            (path.to_string(), query)
        }
        None => (target.to_string(), query),
    }
}

async fn write_response(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    response: HttpResponse,
) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        response.reason(),
        response.content_type,
        response.body.len()
    );

    writer.write_all(head.as_bytes()).await?;
    writer.write_all(&response.body).await?;
    writer.flush().await
}
