use crate::CoreResult;
use ratchet_spec::schema::{SpecSchema, VerificationStep};
use ratchet_spec::AcceptanceCriterion;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Generates and evaluates verification reports.
///
/// Evaluation is honest about what it can and cannot check automatically:
/// criteria with a machine-checkable step (test/lint/diff) are evaluated
/// against *observed* evidence — the declared command is actually executed —
/// and everything else is reported as `Manual`, never silently passed.
pub struct VerificationEngine;

/// Outcome of running a single shell command.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandOutcome {
    pub passed: bool,
    pub output: String,
}

/// A verdict contributed by an external gate plugin.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginVerdict {
    pub plugin: String,
    pub status: CriterionStatus,
    pub note: String,
}

/// Context gathered from the working tree that criteria are checked against.
#[derive(Debug, Clone, Default)]
pub struct VerificationEvidence {
    /// Combined stdout/stderr of the project's test suite (fallback evidence).
    pub test_output: String,
    /// Whether the project test suite exited successfully.
    pub test_passed: bool,
    /// Files changed in the working tree.
    pub changed_files: Vec<String>,
    /// Results of commands explicitly declared in acceptance criteria,
    /// keyed by the command string.
    pub command_results: HashMap<String, CommandOutcome>,
    /// Verdicts from external gate plugins, keyed by criterion id.
    ///
    /// A plugin verdict takes precedence over the built-in check: a project
    /// that installs a domain-specific gate is stating that the gate knows
    /// better than the generic heuristic.
    pub plugin_results: HashMap<String, PluginVerdict>,
}

impl VerificationEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn verify_spec_conformance(
        &self,
        schema: &SpecSchema,
        evidence: &VerificationEvidence,
    ) -> CoreResult<VerificationReport> {
        let results: Vec<CriterionResult> = schema
            .acceptance_criteria
            .iter()
            .map(|c| self.check_criterion(c, evidence))
            .collect();

        let auto_passed = results
            .iter()
            .filter(|r| r.status == CriterionStatus::Passed)
            .count();
        let auto_failed = results
            .iter()
            .filter(|r| r.status == CriterionStatus::Failed)
            .count();
        let manual = results
            .iter()
            .filter(|r| r.status == CriterionStatus::Manual)
            .count();

        // A spec passes when nothing failed automatically and, if the project
        // test suite ran, it succeeded. Manual items never fail the run, but
        // they are never counted as verified either.
        let overall_passed =
            auto_failed == 0 && (evidence.test_output.is_empty() || evidence.test_passed);

        let summary = format!(
            "{auto_passed} auto-verified, {auto_failed} failed, {manual} need manual review \
             (test suite: {})",
            if evidence.test_output.is_empty() {
                "not run"
            } else if evidence.test_passed {
                "passed"
            } else {
                "failed"
            }
        );

        Ok(VerificationReport {
            spec_id: schema.id.clone(),
            overall_passed,
            criterion_results: results,
            changed_files: evidence.changed_files.clone(),
            auto_passed,
            auto_failed,
            manual,
            generated_at: chrono::Utc::now(),
            summary,
        })
    }

    fn check_criterion(
        &self,
        criterion: &AcceptanceCriterion,
        evidence: &VerificationEvidence,
    ) -> CriterionResult {
        // An external gate wins over the built-in heuristic.
        if let Some(verdict) = evidence.plugin_results.get(&criterion.id) {
            return CriterionResult {
                criterion_id: criterion.id.clone(),
                description: criterion.description.clone(),
                status: verdict.status,
                note: format!("[{}] {}", verdict.plugin, verdict.note),
            };
        }

        let (status, note) = match &criterion.verification {
            Some(VerificationStep::Test { command, expected: _ }) => {
                match evidence.command_results.get(command) {
                    // The command's exit status is authoritative. `expected` is
                    // advisory only: models routinely emit prose there, and a
                    // missing marker is not evidence the tests failed.
                    Some(outcome) if outcome.passed => {
                        // A command that succeeds while running no tests is not
                        // evidence of anything. Reporting it as "verified" would
                        // be the most misleading outcome the tool could produce.
                        if ran_no_tests(&outcome.output) {
                            (
                                CriterionStatus::Manual,
                                format!("`{command}` succeeded but ran no tests"),
                            )
                        } else {
                            (CriterionStatus::Passed, format!("`{command}` exited 0"))
                        }
                    }
                    Some(outcome) => (
                        CriterionStatus::Failed,
                        format!("`{command}` exited non-zero: {}", first_line(&outcome.output)),
                    ),
                    // Fall back to the project-wide test run if the specific
                    // command was not executed.
                    None if !evidence.test_output.is_empty() => {
                        if evidence.test_passed {
                            (CriterionStatus::Passed, "project test suite passed".to_string())
                        } else {
                            (CriterionStatus::Failed, "project test suite failed".to_string())
                        }
                    }
                    None => (
                        CriterionStatus::Manual,
                        format!("`{command}` was not run"),
                    ),
                }
            }
            Some(VerificationStep::Lint { tool, must_pass }) => {
                match evidence.command_results.get(tool) {
                    Some(outcome) if outcome.passed => {
                        (CriterionStatus::Passed, format!("`{tool}` passed"))
                    }
                    Some(_) if *must_pass => {
                        (CriterionStatus::Failed, format!("`{tool}` failed"))
                    }
                    Some(_) => (
                        CriterionStatus::Manual,
                        format!("`{tool}` failed but is not required"),
                    ),
                    None => (
                        CriterionStatus::Manual,
                        format!("lint `{tool}` was not run"),
                    ),
                }
            }
            Some(VerificationStep::Diff { pattern }) => {
                if evidence.changed_files.iter().any(|f| f.contains(pattern.as_str())) {
                    (CriterionStatus::Passed, format!("changed file matches `{pattern}`"))
                } else {
                    (
                        CriterionStatus::Failed,
                        format!("no changed file matches `{pattern}`"),
                    )
                }
            }
            Some(VerificationStep::Manual { instructions }) => (
                CriterionStatus::Manual,
                if instructions.is_empty() {
                    "manual verification required".to_string()
                } else {
                    instructions.clone()
                },
            ),
            None => (
                CriterionStatus::Manual,
                "no verification step defined".to_string(),
            ),
        };

        CriterionResult {
            criterion_id: criterion.id.clone(),
            description: criterion.description.clone(),
            status,
            note,
        }
    }
}

impl Default for VerificationEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Heuristic: did a test command succeed without actually running any tests?
///
/// Positive evidence wins. Cargo prints `running 0 tests` for the *doc-test*
/// section even when unit tests ran, so an empty-marker check alone would
/// wrongly downgrade a passing suite. We therefore look for any runner
/// reporting a non-zero pass count first.
fn ran_no_tests(output: &str) -> bool {
    if count_passed(output) > 0 {
        return false;
    }

    let lower = output.to_lowercase();
    const EMPTY_MARKERS: &[&str] = &[
        "running 0 tests",     // cargo
        "no tests ran",        // pytest
        "0 passing",           // mocha
        "no test files found", // vitest / jest
        "0 tests found",
    ];
    EMPTY_MARKERS.iter().any(|m| lower.contains(m))
}

/// Largest `N` found immediately before a pass marker (`passed`, `passing`).
///
/// Covers cargo/pytest (`12 passed`) and mocha/jest (`12 passing`).
fn count_passed(output: &str) -> u64 {
    let lower = output.to_lowercase();
    let bytes = lower.as_bytes();
    let mut max = 0u64;

    for marker in ["passed", "passing"] {
        let mut from = 0usize;
        while let Some(rel) = lower[from..].find(marker) {
            let at = from + rel;

            // Walk backwards over spaces, then over the digits.
            let mut end = at;
            while end > 0 && bytes[end - 1] == b' ' {
                end -= 1;
            }
            let mut start = end;
            while start > 0 && bytes[start - 1].is_ascii_digit() {
                start -= 1;
            }

            if start < end {
                if let Ok(n) = lower[start..end].parse::<u64>() {
                    max = max.max(n);
                }
            }
            from = at + marker.len();
        }
    }

    max
}

/// First non-empty line, trimmed and length-capped for error notes.
fn first_line(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    line.chars().take(120).collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerificationReport {
    pub spec_id: String,
    pub overall_passed: bool,
    pub criterion_results: Vec<CriterionResult>,
    pub changed_files: Vec<String>,
    pub auto_passed: usize,
    pub auto_failed: usize,
    pub manual: usize,
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CriterionResult {
    pub criterion_id: String,
    pub description: String,
    pub status: CriterionStatus,
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CriterionStatus {
    Passed,
    Failed,
    Manual,
}

impl CriterionStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            CriterionStatus::Passed => "✅",
            CriterionStatus::Failed => "❌",
            CriterionStatus::Manual => "🟡",
        }
    }
}
