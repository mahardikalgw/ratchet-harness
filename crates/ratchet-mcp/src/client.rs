use crate::{
    error::{McpError, McpResult},
    types::*,
};
use serde::de::DeserializeOwned;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};

/// An MCP client that communicates with a server over stdio.
pub struct McpClient {
    stdin: ChildStdin,
    reader: BufReader<ChildStdout>,
    next_id: u64,
    server_capabilities: Option<ServerCapabilities>,
}

impl McpClient {
    /// Connect to an MCP server via stdio.
    pub async fn connect_stdio(command: &str, args: &[String]) -> McpResult<Self> {
        let mut child = tokio::process::Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdin = child.stdin.take().ok_or_else(|| {
            McpError::Transport("failed to capture stdin".into())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            McpError::Transport("failed to capture stdout".into())
        })?;

        let mut client = Self {
            stdin,
            reader: BufReader::new(stdout),
            next_id: 1,
            server_capabilities: None,
        };

        client.initialize().await?;
        Ok(client)
    }

    async fn initialize(&mut self) -> McpResult<()> {
        let request = InitializeRequest {
            protocol_version: MCP_PROTOCOL_VERSION.to_string(),
            capabilities: ClientCapabilities {
                tools: Some(ToolsCapability { list_changed: false }),
                resources: Some(ResourcesCapability { subscribe: false, list_changed: false }),
            },
            client_info: Implementation {
                name: "ratchet".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        };

        let response: InitializeResponse = self
            .request("initialize", Some(serde_json::to_value(request)?))
            .await?;

        if response.protocol_version != MCP_PROTOCOL_VERSION {
            return Err(McpError::Protocol(format!(
                "version mismatch: expected {}, got {}",
                MCP_PROTOCOL_VERSION, response.protocol_version
            )));
        }

        self.server_capabilities = Some(response.capabilities);

        // Send initialized notification
        self.notify("notifications/initialized", None).await?;

        Ok(())
    }

    pub fn supports_tools(&self) -> bool {
        self.server_capabilities
            .as_ref()
            .and_then(|c| c.tools.as_ref())
            .is_some()
    }

    pub fn supports_resources(&self) -> bool {
        self.server_capabilities
            .as_ref()
            .and_then(|c| c.resources.as_ref())
            .is_some()
    }

    /// List available tools from the server.
    pub async fn list_tools(&mut self) -> McpResult<Vec<McpTool>> {
        let response: ListToolsResponse = self
            .request("tools/list", Some(serde_json::to_value(ListToolsRequest { cursor: None })?))
            .await?;
        Ok(response.tools)
    }

    /// Call a tool on the server.
    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: Option<serde_json::Value>,
    ) -> McpResult<CallToolResponse> {
        let request = CallToolRequest {
            name: name.to_string(),
            arguments,
        };
        self.request("tools/call", Some(serde_json::to_value(request)?))
            .await
    }

    /// List available resources.
    pub async fn list_resources(&mut self) -> McpResult<Vec<Resource>> {
        let response: ListResourcesResponse = self
            .request("resources/list", Some(serde_json::to_value(ListResourcesRequest { cursor: None })?))
            .await?;
        Ok(response.resources)
    }

    /// Read a resource by URI.
    pub async fn read_resource(&mut self, uri: &str) -> McpResult<ReadResourceResponse> {
        let request = ReadResourceRequest { uri: uri.to_string() };
        self.request("resources/read", Some(serde_json::to_value(request)?))
            .await
    }

    async fn request<T: DeserializeOwned>(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> McpResult<T> {
        let id = self.next_id;
        self.next_id += 1;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Number(id as i64),
            method: method.to_string(),
            params,
        };

        let request_line = serde_json::to_string(&request)?;
        let stdin = &mut self.stdin;
        stdin.write_all(request_line.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;

        // Read response line
        let mut response_line = String::new();
        let bytes_read = self.reader.read_line(&mut response_line).await?;
        if bytes_read == 0 {
            return Err(McpError::Transport("server closed connection".into()));
        }

        let response: JsonRpcResponse = serde_json::from_str(&response_line)?;

        if let Some(error) = response.error {
            return Err(McpError::JsonRpc {
                code: error.code,
                message: error.message,
            });
        }

        let result = response.result.ok_or_else(|| {
            McpError::JsonRpc {
                code: -32603,
                message: "empty result".into(),
            }
        })?;

        Ok(serde_json::from_value(result)?)
    }

    async fn notify(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> McpResult<()> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Null,
            method: method.to_string(),
            params,
        };

        let request_line = serde_json::to_string(&request)?;
        let stdin = &mut self.stdin;
        stdin.write_all(request_line.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;

        Ok(())
    }
}

/// Registry of connected MCP clients.
pub struct McpClientRegistry {
    clients: Vec<(String, McpClient)>,
}

impl McpClientRegistry {
    pub fn new() -> Self {
        Self { clients: Vec::new() }
    }

    pub fn add(&mut self, name: String, client: McpClient) {
        self.clients.push((name, client));
    }

    pub fn get(&mut self, name: &str) -> Option<&mut McpClient> {
        self.clients.iter_mut().find(|(n, _)| n == name).map(|(_, c)| c)
    }

    pub fn list(&self) -> Vec<&str> {
        self.clients.iter().map(|(n, _)| n.as_str()).collect()
    }

    pub async fn list_all_tools(&mut self) -> McpResult<Vec<(String, McpTool)>> {
        let mut all_tools = Vec::new();
        for (server_name, client) in &mut self.clients {
            if client.supports_tools() {
                let tools = client.list_tools().await?;
                for tool in tools {
                    all_tools.push((format!("{}.{}", server_name, tool.name), tool));
                }
            }
        }
        Ok(all_tools)
    }
}

impl Default for McpClientRegistry {
    fn default() -> Self {
        Self::new()
    }
}
