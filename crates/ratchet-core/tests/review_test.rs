use ratchet_core::review::ReviewDelta;
use ratchet_core::verification::{CriterionResult, CriterionStatus, VerificationReport};
use ratchet_core::ExecutionResult;
use ratchet_providers::traits::TokenUsage;
use ratchet_spec::{
    schema::{Plan, TaskGraph, TaskNode},
    TaskId, TaskStatus,
};

fn plan(modules: &[&str], task_ids: &[&str]) -> Plan {
    Plan {
        spec_id: "demo".to_string(),
        title: "Demo".to_string(),
        summary: "s".to_string(),
        affected_modules: modules.iter().map(|m| m.to_string()).collect(),
        data_model_changes: vec![],
        risk_notes: vec![],
        task_graph: TaskGraph {
            nodes: task_ids
                .iter()
                .map(|id| TaskNode {
                    id: TaskId(id.to_string()),
                    title: format!("task {id}"),
                    description: String::new(),
                    assigned_model: None,
                    verification: None,
                    estimated_tokens: None,
                })
                .collect(),
            edges: vec![],
        },
    }
}

fn result(id: &str, status: TaskStatus, changed: &[&str]) -> ExecutionResult {
    ExecutionResult {
        task_id: TaskId(id.to_string()),
        status,
        output: String::new(),
        tool_calls: vec![],
        usage: TokenUsage::default(),
        cost_usd: 0.01,
        turns: 2,
        changed_files: changed.iter().map(|s| s.to_string()).collect(),
        provider: "mock".to_string(),
        model: "mock".to_string(),
        roles: vec!["implementer".to_string()],
        review: None,
    }
}

fn verification(passed: bool) -> VerificationReport {
    VerificationReport {
        spec_id: "demo".to_string(),
        overall_passed: passed,
        criterion_results: vec![CriterionResult {
            criterion_id: "AC-1".to_string(),
            description: "works".to_string(),
            status: if passed {
                CriterionStatus::Passed
            } else {
                CriterionStatus::Failed
            },
            note: String::new(),
        }],
        changed_files: vec![],
        auto_passed: if passed { 1 } else { 0 },
        auto_failed: if passed { 0 } else { 1 },
        manual: 0,
        generated_at: chrono::Utc::now(),
        summary: "summary".to_string(),
    }
}

#[test]
fn clean_run_has_no_anomalies() {
    let plan = plan(&["src/billing"], &["T-1", "T-2"]);
    let results = vec![
        result("T-1", TaskStatus::Done, &["src/billing/a.rs"]),
        result("T-2", TaskStatus::Done, &["src/billing/b.rs"]),
    ];

    let delta = ReviewDelta::compute(&plan, &results, Some(&verification(true)));
    assert!(delta.is_clean());
    assert_eq!(delta.executed_tasks, 2);
    assert!(delta.unplanned_changes.is_empty());
    assert_eq!(delta.total_turns, 4);
    assert!((delta.total_cost_usd - 0.02).abs() < 1e-9);
}

#[test]
fn detects_planned_but_unexecuted_tasks() {
    let plan = plan(&["src/billing"], &["T-1", "T-2"]);
    let results = vec![result("T-1", TaskStatus::Done, &["src/billing/a.rs"])];

    let delta = ReviewDelta::compute(&plan, &results, Some(&verification(true)));
    assert!(!delta.is_clean());
    assert_eq!(delta.unexecuted_tasks, vec!["T-2"]);
}

#[test]
fn detects_changes_outside_planned_modules() {
    let plan = plan(&["src/billing"], &["T-1"]);
    let results = vec![result(
        "T-1",
        TaskStatus::Done,
        &["src/billing/a.rs", "src/unrelated/c.rs"],
    )];

    let delta = ReviewDelta::compute(&plan, &results, Some(&verification(true)));
    assert!(!delta.is_clean());
    assert_eq!(delta.unplanned_changes, vec!["src/unrelated/c.rs"]);
}

#[test]
fn detects_failed_tasks() {
    let plan = plan(&["src"], &["T-1"]);
    let results = vec![result("T-1", TaskStatus::Failed, &[])];

    let delta = ReviewDelta::compute(&plan, &results, Some(&verification(false)));
    assert!(!delta.is_clean());
    assert_eq!(delta.failed_tasks, vec!["T-1"]);
}

#[test]
fn failed_verification_makes_the_run_dirty() {
    let plan = plan(&["src"], &["T-1"]);
    let results = vec![result("T-1", TaskStatus::Done, &["src/a.rs"])];

    let delta = ReviewDelta::compute(&plan, &results, Some(&verification(false)));
    assert!(!delta.is_clean());
    assert_eq!(delta.verification_passed, Some(false));
}

#[test]
fn no_planned_modules_means_no_unplanned_changes() {
    let plan = plan(&[], &["T-1"]);
    let results = vec![result("T-1", TaskStatus::Done, &["anything/at/all.rs"])];

    let delta = ReviewDelta::compute(&plan, &results, Some(&verification(true)));
    assert!(delta.unplanned_changes.is_empty());
    assert!(delta.is_clean());
}

#[test]
fn detects_a_run_that_changed_nothing() {
    let plan = plan(&["src"], &["T-1"]);
    let results = vec![result("T-1", TaskStatus::Done, &[])];

    let delta = ReviewDelta::compute(&plan, &results, Some(&verification(true)));
    assert!(delta.no_changes);
    assert!(!delta.is_clean());
}

#[test]
fn render_mentions_the_verdict() {
    let plan = plan(&["src"], &["T-1", "T-2"]);
    let results = vec![result("T-1", TaskStatus::Done, &["src/a.rs"])];

    let text = ReviewDelta::compute(&plan, &results, Some(&verification(true))).render();
    assert!(text.contains("Review: demo"));
    assert!(text.contains("Planned but not executed"));
    assert!(text.contains("Verdict:"));
}
