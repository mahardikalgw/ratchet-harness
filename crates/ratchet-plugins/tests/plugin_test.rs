use ratchet_plugins::{
    GateCriterion, GateRequest, GateStatus, PluginHost, PluginKind, PluginManifest,
};
use std::path::Path;

/// Locate a Python interpreter.
///
/// Plugins are language-agnostic, but these tests need *some* interpreter, and
/// the executable name differs per platform (`python3` is not guaranteed on
/// Windows). Tests skip rather than fail when none is installed.
fn python() -> Option<&'static str> {
    ["python3", "python", "py"].into_iter().find(|candidate| {
        std::process::Command::new(candidate)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

/// Write a plugin script into `dir` and return a manifest for it.
/// Returns `None` when no interpreter is available to run it.
fn install(dir: &Path, name: &str, kind: PluginKind, body: &str) -> Option<PluginManifest> {
    let interpreter = python()?;
    let path = dir.join(format!("{name}.py"));
    std::fs::write(&path, body).unwrap();
    Some(PluginManifest {
        name: name.to_string(),
        kind,
        command: interpreter.to_string(),
        args: vec![path.display().to_string()],
        criteria: Vec::new(),
        timeout_secs: 20,
    })
}

/// Skip a test cleanly when the interpreter is missing.
macro_rules! require_python {
    () => {
        if python().is_none() {
            eprintln!("skipping: no python interpreter available");
            return;
        }
    };
}

const GATE_OK: &str = r#"
import json, sys
req = json.load(sys.stdin)
results = []
for c in req["criteria"]:
    # Pass AC-1, fail AC-2
    status = "passed" if c["id"] == "AC-1" else "failed"
    results.append({"criterion_id": c["id"], "status": status, "note": "checked by test gate"})
print(json.dumps({"results": results}))
"#;

const GATE_CRASH: &str = r#"
import sys
sys.exit(3)
"#;

const GATE_BAD_JSON: &str = r#"
print("not json at all")
"#;

const TOOL_PLUGIN: &str = r#"
import json, sys
req = json.load(sys.stdin)
if req["type"] == "describe":
    print(json.dumps({"tools": [{
        "name": "word_count",
        "description": "Count words in text",
        "parameters": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}
    }]}))
elif req["type"] == "tool_call":
    text = req["arguments"].get("text", "")
    print(json.dumps({"content": str(len(text.split())), "is_error": False}))
else:
    print(json.dumps({"content": "unknown request", "is_error": True}))
"#;

fn request() -> GateRequest {
    GateRequest {
        spec_id: "demo".to_string(),
        criteria: vec![
            GateCriterion {
                id: "AC-1".to_string(),
                description: "first".to_string(),
            },
            GateCriterion {
                id: "AC-2".to_string(),
                description: "second".to_string(),
            },
        ],
        changed_files: vec!["src/lib.rs".to_string()],
        test_passed: Some(true),
        test_output: "test result: ok".to_string(),
        working_dir: ".".to_string(),
    }
}

#[tokio::test]
async fn gate_plugin_returns_per_criterion_verdicts() {
    let dir = tempfile::tempdir().unwrap();
    require_python!();
    let manifest = install(dir.path(), "mygate", PluginKind::Gate, GATE_OK).unwrap();
    let host = PluginHost::from_manifests(vec![manifest], dir.path());

    let results = host.run_gates(&request()).await;
    assert_eq!(results.len(), 2);

    let by_id: std::collections::HashMap<_, _> = results
        .into_iter()
        .map(|(plugin, r)| (r.criterion_id.clone(), (plugin, r)))
        .collect();

    assert_eq!(by_id["AC-1"].1.status, GateStatus::Passed);
    assert_eq!(by_id["AC-2"].1.status, GateStatus::Failed);
    assert_eq!(by_id["AC-1"].0, "mygate");
}

#[tokio::test]
async fn gate_plugin_only_sees_its_configured_criteria() {
    let dir = tempfile::tempdir().unwrap();
    require_python!();
    let mut manifest = install(dir.path(), "scoped", PluginKind::Gate, GATE_OK).unwrap();
    manifest.criteria = vec!["AC-1".to_string()];
    let host = PluginHost::from_manifests(vec![manifest], dir.path());

    let results = host.run_gates(&request()).await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].1.criterion_id, "AC-1");
}

#[tokio::test]
async fn crashing_gate_degrades_to_manual_not_silent_pass() {
    let dir = tempfile::tempdir().unwrap();
    require_python!();
    let manifest = install(dir.path(), "broken", PluginKind::Gate, GATE_CRASH).unwrap();
    let host = PluginHost::from_manifests(vec![manifest], dir.path());

    let results = host.run_gates(&request()).await;
    assert_eq!(results.len(), 2);
    for (_, r) in &results {
        // A broken gate must never masquerade as verified.
        assert_eq!(r.status, GateStatus::Manual);
        assert!(r.note.contains("error"));
    }
}

#[tokio::test]
async fn malformed_gate_output_degrades_to_manual() {
    let dir = tempfile::tempdir().unwrap();
    require_python!();
    let manifest = install(dir.path(), "garbage", PluginKind::Gate, GATE_BAD_JSON).unwrap();
    let host = PluginHost::from_manifests(vec![manifest], dir.path());

    let results = host.run_gates(&request()).await;
    assert!(!results.is_empty());
    for (_, r) in &results {
        assert_eq!(r.status, GateStatus::Manual);
    }
}

#[tokio::test]
async fn tool_plugin_describes_and_executes() {
    let dir = tempfile::tempdir().unwrap();
    require_python!();
    let manifest = install(dir.path(), "tools", PluginKind::Tool, TOOL_PLUGIN).unwrap();
    let host = PluginHost::from_manifests(vec![manifest], dir.path());

    let descriptors = host.describe("tools").await.unwrap();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].name, "word_count");

    let response = host
        .call_tool(
            "tools",
            "word_count",
            serde_json::json!({"text": "one two three"}),
        )
        .await
        .unwrap();
    assert!(!response.is_error);
    assert_eq!(response.content, "3");
}

#[tokio::test]
async fn unknown_plugin_is_reported() {
    let host = PluginHost::default();
    assert!(host.describe("nope").await.is_err());
}

#[tokio::test]
async fn timeout_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    require_python!();
    let mut manifest = install(
        dir.path(),
        "slow",
        PluginKind::Gate,
        "import time\ntime.sleep(30)\n",
    )
    .unwrap();
    manifest.timeout_secs = 1;
    let host = PluginHost::from_manifests(vec![manifest], dir.path());

    let started = std::time::Instant::now();
    let results = host.run_gates(&request()).await;
    assert!(
        started.elapsed().as_secs() < 10,
        "timeout should be enforced"
    );
    for (_, r) in &results {
        assert_eq!(r.status, GateStatus::Manual);
    }
}
