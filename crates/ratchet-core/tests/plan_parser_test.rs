use ratchet_core::PlanParser;
use ratchet_spec::SpecParser;

const SPEC: &str = r#"---
id: billing
title: "Billing"
status: draft
---

# Acceptance Criteria

- [ ] AC-1: Invoice totals are correct
- [ ] AC-2: Refunds are idempotent
"#;

fn spec() -> ratchet_spec::SpecFile {
    SpecParser::new().parse(SPEC).unwrap()
}

#[test]
fn parses_clean_json_plan() {
    let response = r#"{
        "summary": "Add a billing module.",
        "affected_modules": ["src/billing"],
        "data_model_changes": ["invoices table"],
        "risk_notes": ["money math"],
        "tasks": [
            {"id": "T-1", "title": "Add invoice model", "description": "create struct"},
            {"id": "T-2", "title": "Add totals", "description": "sum line items", "depends_on": ["T-1"]}
        ]
    }"#;

    let plan = PlanParser::parse(response, &spec()).unwrap();
    assert_eq!(plan.task_graph.nodes.len(), 2);
    assert_eq!(plan.task_graph.edges.len(), 1);
    assert_eq!(plan.affected_modules, vec!["src/billing"]);
    assert!(plan.task_graph.validate().is_ok());
}

#[test]
fn parses_json_inside_markdown_fence() {
    let response = "Here is the plan:\n\n```json\n{\"tasks\":[{\"id\":\"T-1\",\"title\":\"x\",\"description\":\"y\"}]}\n```\n";

    let plan = PlanParser::parse(response, &spec()).unwrap();
    assert_eq!(plan.task_graph.nodes.len(), 1);
}

#[test]
fn parses_bare_task_array() {
    let response = r#"[{"id":"T-1","title":"a","description":"b"}]"#;
    let plan = PlanParser::parse(response, &spec()).unwrap();
    assert_eq!(plan.task_graph.nodes.len(), 1);
}

#[test]
fn parses_verification_steps() {
    let response = r#"{
        "tasks": [{
            "id": "T-1",
            "title": "x",
            "description": "y",
            "verification": {"kind": "test", "command": "cargo test", "expected": "ok"}
        }]
    }"#;

    let plan = PlanParser::parse(response, &spec()).unwrap();
    assert!(plan.task_graph.nodes[0].verification.is_some());
}

#[test]
fn drops_edges_referencing_missing_tasks() {
    let response = r#"{
        "tasks": [{"id": "T-1", "title": "x", "description": "y", "depends_on": ["T-999"]}]
    }"#;

    let plan = PlanParser::parse(response, &spec()).unwrap();
    assert!(plan.task_graph.edges.is_empty());
    assert!(plan.task_graph.validate().is_ok());
}

#[test]
fn falls_back_to_one_task_per_criterion() {
    // No JSON at all → deterministic fallback from acceptance criteria.
    let response = "I could not produce JSON, sorry.";
    let plan = PlanParser::parse(response, &spec()).unwrap();

    assert_eq!(plan.task_graph.nodes.len(), 2);
    assert_eq!(plan.task_graph.nodes[0].title, "AC-1");
    assert_eq!(plan.task_graph.nodes[1].title, "AC-2");
    // Fallback chains tasks sequentially so dependencies are explicit.
    assert_eq!(plan.task_graph.edges.len(), 1);
    assert!(plan.task_graph.validate().is_ok());
}

#[test]
fn falls_back_to_single_task_without_criteria() {
    let bare = SpecParser::new().parse("# Bare\n\nNo criteria.").unwrap();
    let plan = PlanParser::parse("nope", &bare).unwrap();
    assert_eq!(plan.task_graph.nodes.len(), 1);
}

#[test]
fn ignores_braces_inside_strings() {
    let response = r#"{"summary": "use {braces} carefully", "tasks": [{"id":"T-1","title":"x","description":"y"}]}"#;
    let plan = PlanParser::parse(response, &spec()).unwrap();
    assert_eq!(plan.task_graph.nodes.len(), 1);
}
