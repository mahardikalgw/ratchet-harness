use crate::{task_executor::ExecutionResult, verification::VerificationReport};
use ratchet_spec::schema::Plan;
use serde::{Deserialize, Serialize};

/// Plan-vs-actual delta surfaced at the review gate.
///
/// The point is to send reviewer attention to *intent conformance* rather than
/// making them re-read every line of the diff.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewDelta {
    pub spec_id: String,
    pub planned_tasks: usize,
    pub executed_tasks: usize,
    /// Planned tasks that produced no execution result.
    pub unexecuted_tasks: Vec<String>,
    /// Tasks that ran but did not finish cleanly.
    pub failed_tasks: Vec<String>,
    /// Modules the plan said would be touched.
    pub planned_modules: Vec<String>,
    /// Files actually changed in the working tree.
    pub changed_files: Vec<String>,
    /// Changed files that fall outside every planned module.
    pub unplanned_changes: Vec<String>,
    /// True when the run produced no file changes at all — usually a sign the
    /// model narrated the work instead of doing it.
    pub no_changes: bool,
    /// Total model turns spent across all tasks.
    pub total_turns: usize,
    pub total_cost_usd: f64,
    pub verification_summary: Option<String>,
    pub verification_passed: Option<bool>,
}

impl ReviewDelta {
    pub fn compute(
        plan: &Plan,
        results: &[ExecutionResult],
        verification: Option<&VerificationReport>,
    ) -> Self {
        let planned_ids: Vec<String> = plan
            .task_graph
            .nodes
            .iter()
            .map(|n| n.id.0.clone())
            .collect();

        let executed_ids: Vec<String> = results.iter().map(|r| r.task_id.0.clone()).collect();

        let unexecuted_tasks = planned_ids
            .iter()
            .filter(|id| !executed_ids.contains(id))
            .cloned()
            .collect();

        let failed_tasks: Vec<String> = results
            .iter()
            .filter(|r| {
                !matches!(
                    r.status,
                    ratchet_spec::TaskStatus::Done | ratchet_spec::TaskStatus::Skipped
                )
            })
            .map(|r| r.task_id.0.clone())
            .collect();

        // Union of every changed file across task results.
        let mut changed_files: Vec<String> = results
            .iter()
            .flat_map(|r| r.changed_files.iter().cloned())
            .collect();
        changed_files.sort();
        changed_files.dedup();

        let modules = &plan.affected_modules;
        let unplanned_changes = if modules.is_empty() {
            Vec::new()
        } else {
            changed_files
                .iter()
                .filter(|f| {
                    !modules
                        .iter()
                        .any(|m| f.starts_with(m.trim_end_matches('/')))
                })
                .cloned()
                .collect()
        };

        Self {
            spec_id: plan.spec_id.clone(),
            planned_tasks: planned_ids.len(),
            executed_tasks: executed_ids.len(),
            unexecuted_tasks,
            failed_tasks,
            no_changes: changed_files.is_empty(),
            planned_modules: modules.clone(),
            changed_files,
            unplanned_changes,
            total_turns: results.iter().map(|r| r.turns).sum(),
            total_cost_usd: results.iter().map(|r| r.cost_usd).sum(),
            verification_summary: verification.map(|v| v.summary.clone()),
            verification_passed: verification.map(|v| v.overall_passed),
        }
    }

    /// True when nothing about the run needs a human's attention.
    pub fn is_clean(&self) -> bool {
        self.unexecuted_tasks.is_empty()
            && self.failed_tasks.is_empty()
            && self.unplanned_changes.is_empty()
            && !self.no_changes
            && self.verification_passed.unwrap_or(true)
    }

    pub fn render(&self) -> String {
        let mut out = format!(
            "# Review: {}\n\n**Tasks:** {}/{} executed\n\n**Cost:** ${:.4} across {} turn(s)\n\n",
            self.spec_id,
            self.executed_tasks,
            self.planned_tasks,
            self.total_cost_usd,
            self.total_turns
        );

        out.push_str("## Plan vs actual\n\n");
        out.push_str(&format!(
            "- Planned modules: {}\n",
            list_or_none(&self.planned_modules)
        ));
        out.push_str(&format!(
            "- Changed files: {}\n",
            list_or_none(&self.changed_files)
        ));

        if !self.unexecuted_tasks.is_empty() {
            out.push_str(&format!(
                "- ⚠️ Planned but not executed: {}\n",
                self.unexecuted_tasks.join(", ")
            ));
        }
        if !self.failed_tasks.is_empty() {
            out.push_str(&format!(
                "- ❌ Failed tasks: {}\n",
                self.failed_tasks.join(", ")
            ));
        }
        if !self.unplanned_changes.is_empty() {
            out.push_str(&format!(
                "- ⚠️ Changed outside planned modules: {}\n",
                self.unplanned_changes.join(", ")
            ));
        }
        if self.no_changes {
            out.push_str(
                "- ⚠️ No files changed at all — the model may have described the \
                 work without performing it.\n",
            );
        }

        out.push_str("\n## Verification\n\n");
        match (&self.verification_passed, &self.verification_summary) {
            (Some(true), Some(s)) => out.push_str(&format!("✅ PASSED — {s}\n")),
            (Some(false), Some(s)) => out.push_str(&format!("❌ FAILED — {s}\n")),
            _ => out.push_str("_not run_\n"),
        }

        out.push_str(&format!(
            "\n**Verdict:** {}\n",
            if self.is_clean() {
                "no anomalies — review the diff for intent conformance"
            } else {
                "anomalies above need human attention"
            }
        ));

        out
    }
}

fn list_or_none(items: &[String]) -> String {
    if items.is_empty() {
        "_none_".to_string()
    } else {
        items.join(", ")
    }
}
