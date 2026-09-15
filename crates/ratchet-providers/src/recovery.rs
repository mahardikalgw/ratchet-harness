//! Recovery of tool calls that a model emitted as *text* rather than through
//! the native tool-calling API.
//!
//! Small and local models (and some regional APIs) frequently describe a tool
//! call in prose, often inside a fenced code block:
//!
//! ```text
//! [{"name": "file_read", "arguments": {"path": "src/lib.rs"}}]
//! ```
//!
//! Rather than treating that as a final answer, the harness structurally
//! converts it back into a real tool call. Without this, model-agnosticism
//! would only hold for frontier models.

use serde_json::Value;

/// A tool call recovered from free text.
#[derive(Debug, Clone, PartialEq)]
pub struct RecoveredToolCall {
    pub name: String,
    pub arguments: Value,
}

/// Scan `text` for tool-call-shaped JSON referencing one of `known_tools`.
pub fn recover_tool_calls(text: &str, known_tools: &[String]) -> Vec<RecoveredToolCall> {
    let mut out = Vec::new();

    for value in json_candidates(text) {
        collect_from_value(&value, known_tools, &mut out);
    }

    out
}

fn collect_from_value(value: &Value, known: &[String], out: &mut Vec<RecoveredToolCall>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_from_value(item, known, out);
            }
        }
        Value::Object(map) => {
            // A wrapper like {"tool_calls": [...]} or {"tool_call": {...}}.
            for key in ["tool_calls", "tool_call", "calls", "actions"] {
                if let Some(inner) = map.get(key) {
                    collect_from_value(inner, known, out);
                }
            }

            if let Some(call) = parse_tool_call(map, known) {
                out.push(call);
            }
        }
        _ => {}
    }
}

fn parse_tool_call(
    map: &serde_json::Map<String, Value>,
    known: &[String],
) -> Option<RecoveredToolCall> {
    // `function` may be a nested object (OpenAI style) or absent.
    let (name, args_source) = if let Some(func) = map.get("function").and_then(|f| f.as_object()) {
        let name = func.get("name").and_then(|v| v.as_str())?;
        let args = func
            .get("arguments")
            .or_else(|| func.get("parameters"))
            .or_else(|| func.get("input"));
        (name, args)
    } else {
        let name = map
            .get("name")
            .or_else(|| map.get("tool"))
            .or_else(|| map.get("tool_name"))
            .and_then(|v| v.as_str())?;
        let args = map
            .get("arguments")
            .or_else(|| map.get("args"))
            .or_else(|| map.get("parameters"))
            .or_else(|| map.get("input"));
        (name, args)
    };

    if !known.iter().any(|k| k == name) {
        return None;
    }

    let arguments = match args_source {
        // Arguments are sometimes a JSON-encoded string rather than an object.
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
        Some(other) => other.clone(),
        None => Value::Object(serde_json::Map::new()),
    };

    // A null/absent argument set becomes an empty object so tool dispatch has
    // something to index into.
    let arguments = if arguments.is_null() {
        Value::Object(serde_json::Map::new())
    } else {
        arguments
    };

    Some(RecoveredToolCall {
        name: name.to_string(),
        arguments,
    })
}

/// Extract every balanced JSON value from arbitrary text.
///
/// Handles fenced code blocks, bare values, and braces inside strings.
fn json_candidates(text: &str) -> Vec<Value> {
    let stripped = strip_fences(text);
    let bytes = stripped.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        if b != b'{' && b != b'[' {
            i += 1;
            continue;
        }

        let open = b;
        let close = if open == b'{' { b'}' } else { b']' };
        let mut depth = 0i32;
        let mut in_string = false;
        let mut escaped = false;
        let mut end = None;

        for (offset, &c) in bytes[i..].iter().enumerate() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if c == b'\\' {
                    escaped = true;
                } else if c == b'"' {
                    in_string = false;
                }
                continue;
            }
            match c {
                b'"' => in_string = true,
                _ if c == open => depth += 1,
                _ if c == close => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i + offset);
                        break;
                    }
                }
                _ => {}
            }
        }

        match end {
            Some(end) => {
                if let Ok(value) = serde_json::from_str::<Value>(&stripped[i..=end]) {
                    out.push(value);
                }
                i = end + 1;
            }
            None => break,
        }
    }

    out
}

/// Remove ``` fences so fenced JSON is still discoverable.
fn strip_fences(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            out.push('\n');
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}
