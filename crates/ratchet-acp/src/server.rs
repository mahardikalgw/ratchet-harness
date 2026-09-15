use crate::{
    error::AcpResult,
    types::*,
};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

/// ACP server that accepts editor connections.
pub struct AcpServer {
    port: u16,
    handler: Arc<dyn AcpHandler>,
}

/// Trait for handling ACP requests. Implemented by ratchet-core.
#[async_trait::async_trait]
pub trait AcpHandler: Send + Sync {
    async fn initialize(&self, params: InitializeParams) -> AcpResult<InitializeResult>;
    async fn agent_run(&self, params: AgentRunParams) -> AcpResult<AgentRunResult>;
    async fn agent_plan(&self, params: AgentPlanParams) -> AcpResult<AgentPlanResult>;
    async fn approve(&self, params: ApprovalRequest) -> AcpResult<ApprovalResponse>;
    async fn status(&self, run_id: &str) -> AcpResult<AgentRunResult>;
}

impl AcpServer {
    pub fn new(port: u16, handler: Arc<dyn AcpHandler>) -> Self {
        Self { port, handler }
    }

    pub async fn run(&self) -> AcpResult<()> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", self.port)).await?;
        println!("ACP server listening on port {}", self.port);

        loop {
            let (stream, addr) = listener.accept().await?;
            println!("ACP client connected from {}", addr);
            let handler = Arc::clone(&self.handler);
            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, handler).await {
                    eprintln!("ACP connection error: {}", e);
                }
            });
        }
    }
}

async fn handle_connection(
    stream: TcpStream,
    handler: Arc<dyn AcpHandler>,
) -> AcpResult<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(req) => req,
            Err(e) => {
                let error_response = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: None,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32700,
                        message: format!("parse error: {}", e),
                        data: None,
                    }),
                };
                let response_line = serde_json::to_string(&error_response)?;
                writer.write_all(response_line.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                writer.flush().await?;
                line.clear();
                continue;
            }
        };

        let response = dispatch_request(&handler, request).await;
        let response_line = serde_json::to_string(&response)?;
        writer.write_all(response_line.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
        line.clear();
    }

    Ok(())
}

async fn dispatch_request(
    handler: &Arc<dyn AcpHandler>,
    request: JsonRpcRequest,
) -> JsonRpcResponse {
    let id = request.id.clone();

    let result = match request.method.as_str() {
        "initialize" => {
            let params: InitializeParams = match parse_params(request.params) {
                Ok(p) => p,
                Err(e) => return error_response(id, -32602, e),
            };
            match handler.initialize(params).await {
                Ok(result) => Ok(serde_json::to_value(result).unwrap()),
                Err(e) => Err((-32603, e.to_string())),
            }
        }
        "agent/run" => {
            let params: AgentRunParams = match parse_params(request.params) {
                Ok(p) => p,
                Err(e) => return error_response(id, -32602, e),
            };
            match handler.agent_run(params).await {
                Ok(result) => Ok(serde_json::to_value(result).unwrap()),
                Err(e) => Err((-32603, e.to_string())),
            }
        }
        "agent/plan" => {
            let params: AgentPlanParams = match parse_params(request.params) {
                Ok(p) => p,
                Err(e) => return error_response(id, -32602, e),
            };
            match handler.agent_plan(params).await {
                Ok(result) => Ok(serde_json::to_value(result).unwrap()),
                Err(e) => Err((-32603, e.to_string())),
            }
        }
        "agent/approve" => {
            let params: ApprovalRequest = match parse_params(request.params) {
                Ok(p) => p,
                Err(e) => return error_response(id, -32602, e),
            };
            match handler.approve(params).await {
                Ok(result) => Ok(serde_json::to_value(result).unwrap()),
                Err(e) => Err((-32603, e.to_string())),
            }
        }
        "agent/status" => {
            let run_id = match request.params.and_then(|p| p.get("run_id").cloned()) {
                Some(val) => match serde_json::from_value::<String>(val) {
                    Ok(s) => s,
                    Err(e) => return error_response(id, -32602, e.to_string()),
                },
                None => return error_response(id, -32602, "missing run_id".to_string()),
            };
            match handler.status(&run_id).await {
                Ok(result) => Ok(serde_json::to_value(result).unwrap()),
                Err(e) => Err((-32603, e.to_string())),
            }
        }
        _ => Err((-32601, format!("method not found: {}", request.method))),
    };

    match result {
        Ok(value) => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(value),
            error: None,
        },
        Err((code, message)) => error_response(id, code, message),
    }
}

fn parse_params<T: serde::de::DeserializeOwned>(
    params: Option<serde_json::Value>,
) -> Result<T, String> {
    match params {
        Some(p) => serde_json::from_value(p).map_err(|e| e.to_string()),
        None => Err("missing params".to_string()),
    }
}

fn error_response(
    id: Option<serde_json::Value>,
    code: i32,
    message: String,
) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message,
            data: None,
        }),
    }
}
