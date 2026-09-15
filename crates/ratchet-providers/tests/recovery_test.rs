use ratchet_providers::recovery::recover_tool_calls;
use serde_json::json;

fn known() -> Vec<String> {
    vec![
        "file_read".to_string(),
        "file_write".to_string(),
        "list_dir".to_string(),
        "test_run".to_string(),
    ]
}

#[test]
fn recovers_fenced_array_of_tool_calls() {
    let text = "Sure, let me look.\n\n```json\n[{\"name\":\"list_dir\",\"arguments\":{\"path\":\".\"}}]\n```\n";
    let calls = recover_tool_calls(text, &known());
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "list_dir");
    assert_eq!(calls[0].arguments, json!({"path": "."}));
}

#[test]
fn recovers_bare_array_without_fence() {
    let text =
        "I will read the file [{\"name\":\"file_read\",\"arguments\":{\"path\":\"src/lib.rs\"}}]";
    let calls = recover_tool_calls(text, &known());
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "file_read");
}

#[test]
fn recovers_openai_style_function_wrapper() {
    let text = r#"{"function":{"name":"file_write","arguments":{"path":"a.txt","content":"hi"}}}"#;
    let calls = recover_tool_calls(text, &known());
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "file_write");
    assert_eq!(calls[0].arguments["content"], "hi");
}

#[test]
fn recovers_string_encoded_arguments() {
    let text = r#"{"name":"file_write","arguments":"{\"path\":\"a.txt\",\"content\":\"hi\"}"}"#;
    let calls = recover_tool_calls(text, &known());
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].arguments,
        json!({"path": "a.txt", "content": "hi"})
    );
}

#[test]
fn recovers_multiple_calls_in_one_array() {
    let text = r#"[
        {"name":"list_dir","arguments":{"path":"."}},
        {"name":"test_run","arguments":{}}
    ]"#;
    let calls = recover_tool_calls(text, &known());
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].name, "list_dir");
    assert_eq!(calls[1].name, "test_run");
}

#[test]
fn recovers_from_a_tool_calls_wrapper_object() {
    let text = r#"{"tool_calls":[{"name":"list_dir","arguments":{"path":"src"}}]}"#;
    let calls = recover_tool_calls(text, &known());
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments["path"], "src");
}

#[test]
fn ignores_unknown_tool_names() {
    // Hallucinated / not-registered tools must not become real calls.
    let text = r#"[{"name":"delete_everything","arguments":{}}]"#;
    assert!(recover_tool_calls(text, &known()).is_empty());
}

#[test]
fn ignores_plain_prose() {
    let text = "I have created the slugify function and added two tests. cargo test passes.";
    assert!(recover_tool_calls(text, &known()).is_empty());
}

#[test]
fn ignores_unrelated_json() {
    let text = r#"{"summary":"did the thing","tasks":[{"id":"T-1","title":"x"}]}"#;
    assert!(recover_tool_calls(text, &known()).is_empty());
}

#[test]
fn empty_arguments_become_an_object() {
    let text = r#"{"name":"test_run","arguments":null}"#;
    let calls = recover_tool_calls(text, &known());
    assert_eq!(calls.len(), 1);
    assert!(calls[0].arguments.is_object());
}

#[test]
fn recovers_when_braces_appear_inside_strings() {
    let text = r#"[{"name":"file_write","arguments":{"path":"a.txt","content":"fn main() { println!(\"hi\"); }"}}]"#;
    let calls = recover_tool_calls(text, &known());
    assert_eq!(calls.len(), 1);
    assert!(
        calls[0].arguments["content"]
            .as_str()
            .unwrap()
            .contains("println!")
    );
}
