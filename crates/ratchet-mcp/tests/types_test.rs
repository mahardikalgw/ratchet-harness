use ratchet_mcp::types::*;

#[test]
fn initialize_request_round_trips() {
    let req = InitializeRequest {
        protocol_version: MCP_PROTOCOL_VERSION.to_string(),
        capabilities: ClientCapabilities {
            tools: Some(ToolsCapability {
                list_changed: false,
            }),
            resources: None,
        },
        client_info: Implementation {
            name: "ratchet".to_string(),
            version: "0.1.0".to_string(),
        },
    };

    let json = serde_json::to_string(&req).unwrap();
    let back: InitializeRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(back.protocol_version, MCP_PROTOCOL_VERSION);
    assert_eq!(back.client_info.name, "ratchet");
}

#[test]
fn parses_tools_list_response() {
    let raw = r#"{
        "tools": [
            {
                "name": "get_weather",
                "description": "Get weather",
                "input_schema": {"type": "object"}
            }
        ]
    }"#;

    let resp: ListToolsResponse = serde_json::from_str(raw).unwrap();
    assert_eq!(resp.tools.len(), 1);
    assert_eq!(resp.tools[0].name, "get_weather");
    assert_eq!(resp.tools[0].description.as_deref(), Some("Get weather"));
}

#[test]
fn parses_tool_call_response_with_text_content() {
    let raw = r#"{
        "content": [{"type": "text", "text": "sunny"}],
        "is_error": false
    }"#;

    let resp: CallToolResponse = serde_json::from_str(raw).unwrap();
    assert!(!resp.is_error);
    assert_eq!(resp.content.len(), 1);
    match &resp.content[0] {
        ToolContent::Text { text } => assert_eq!(text, "sunny"),
        _ => panic!("expected text content"),
    }
}

#[test]
fn serializes_jsonrpc_request() {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: RequestId::Number(1),
        method: "tools/list".to_string(),
        params: None,
    };
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("\"method\":\"tools/list\""));
    assert!(json.contains("\"id\":1"));
}
