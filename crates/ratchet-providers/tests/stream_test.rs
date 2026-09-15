use ratchet_providers::adapters::anthropic::parse_anthropic_sse;
use ratchet_providers::adapters::openai_types::parse_openai_sse;

// ----- OpenAI-compatible SSE -----

#[test]
fn parses_openai_content_delta() {
    let data = r#"{"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}"#;
    let chunk = parse_openai_sse(data).unwrap().unwrap();
    assert_eq!(chunk.content_delta, "Hello");
    assert!(chunk.tool_call_deltas.is_empty());
}

#[test]
fn ignores_done_sentinel_and_blank_lines() {
    assert!(parse_openai_sse("[DONE]").is_none());
    assert!(parse_openai_sse("   ").is_none());
    assert!(parse_openai_sse("").is_none());
}

#[test]
fn parses_openai_tool_call_delta() {
    let data = r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"get_weather","arguments":"{\"ci"}}]},"finish_reason":null}]}"#;
    let chunk = parse_openai_sse(data).unwrap().unwrap();
    assert_eq!(chunk.tool_call_deltas.len(), 1);
    assert_eq!(chunk.tool_call_deltas[0].index, 0);
    assert_eq!(chunk.tool_call_deltas[0].id.as_deref(), Some("call_1"));
    assert_eq!(
        chunk.tool_call_deltas[0].name.as_deref(),
        Some("get_weather")
    );
    assert_eq!(chunk.tool_call_deltas[0].arguments_delta, "{\"ci");
}

#[test]
fn parses_openai_usage_chunk() {
    let data = r#"{"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#;
    let chunk = parse_openai_sse(data).unwrap().unwrap();
    let usage = chunk.usage.expect("usage");
    assert_eq!(usage.input_tokens, 10);
    assert_eq!(usage.output_tokens, 5);
}

#[test]
fn parses_openai_finish_reason() {
    let data = r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#;
    let chunk = parse_openai_sse(data).unwrap().unwrap();
    assert_eq!(chunk.finish_reason.as_deref(), Some("stop"));
}

#[test]
fn malformed_openai_chunk_yields_error_not_panic() {
    let result = parse_openai_sse("{not json").unwrap();
    assert!(result.is_err());
}

// ----- Anthropic SSE -----

#[test]
fn parses_anthropic_text_delta() {
    let data =
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}"#;
    let chunk = parse_anthropic_sse(data).unwrap().unwrap();
    assert_eq!(chunk.content_delta, "Hi");
}

#[test]
fn parses_anthropic_tool_use_start() {
    let data = r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"search"}}"#;
    let chunk = parse_anthropic_sse(data).unwrap().unwrap();
    assert_eq!(chunk.tool_call_deltas.len(), 1);
    assert_eq!(chunk.tool_call_deltas[0].index, 1);
    assert_eq!(chunk.tool_call_deltas[0].id.as_deref(), Some("toolu_1"));
    assert_eq!(chunk.tool_call_deltas[0].name.as_deref(), Some("search"));
}

#[test]
fn parses_anthropic_partial_json_delta() {
    let data = r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"q\":"}}"#;
    let chunk = parse_anthropic_sse(data).unwrap().unwrap();
    assert_eq!(chunk.tool_call_deltas.len(), 1);
    assert_eq!(chunk.tool_call_deltas[0].index, 1);
    assert!(chunk.tool_call_deltas[0].id.is_none());
    assert_eq!(chunk.tool_call_deltas[0].arguments_delta, "{\"q\":");
}

#[test]
fn parses_anthropic_message_start_usage() {
    let data =
        r#"{"type":"message_start","message":{"usage":{"input_tokens":12,"output_tokens":1}}}"#;
    let chunk = parse_anthropic_sse(data).unwrap().unwrap();
    let usage = chunk.usage.expect("usage");
    assert_eq!(usage.input_tokens, 12);
    assert_eq!(usage.output_tokens, 1);
}

#[test]
fn parses_anthropic_stop_reason() {
    let data = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":15}}"#;
    let chunk = parse_anthropic_sse(data).unwrap().unwrap();
    assert_eq!(chunk.finish_reason.as_deref(), Some("end_turn"));
}

#[test]
fn ignores_unhandled_anthropic_event_types() {
    assert!(parse_anthropic_sse(r#"{"type":"message_stop"}"#).is_none());
    assert!(parse_anthropic_sse(r#"{"type":"ping"}"#).is_none());
    assert!(parse_anthropic_sse("").is_none());
}
