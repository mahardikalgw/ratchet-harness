use crate::{SpecError, SpecResult, TaskId};
use serde::{Deserialize, Serialize};

/// Structured representation of a spec's requirements.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SpecSchema {
    pub id: String,
    pub title: String,
    pub goals: Vec<Goal>,
    pub non_goals: Vec<String>,
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
    pub constraints: Vec<Constraint>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub description: String,
    #[serde(default)]
    pub priority: crate::format::Priority,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AcceptanceCriterion {
    pub id: String,
    pub description: String,
    #[serde(default)]
    pub verification: Option<VerificationStep>,
    #[serde(default)]
    pub must: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VerificationStep {
    #[serde(rename = "test")]
    Test { command: String, expected: String },
    #[serde(rename = "lint")]
    Lint { tool: String, must_pass: bool },
    #[serde(rename = "manual")]
    Manual { instructions: String },
    #[serde(rename = "diff")]
    Diff { pattern: String },
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Constraint {
    pub kind: ConstraintKind,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConstraintKind {
    #[default]
    Performance,
    Security,
    Compatibility,
    Regulatory,
    Budget,
    Time,
}

/// A technical plan generated from a spec.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Plan {
    pub spec_id: String,
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub affected_modules: Vec<String>,
    #[serde(default)]
    pub data_model_changes: Vec<String>,
    #[serde(default)]
    pub risk_notes: Vec<String>,
    pub task_graph: TaskGraph,
}

/// A directed acyclic graph of tasks.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TaskGraph {
    #[serde(default)]
    pub nodes: Vec<TaskNode>,
    #[serde(default)]
    pub edges: Vec<(TaskId, TaskId)>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TaskNode {
    pub id: TaskId,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub assigned_model: Option<String>,
    #[serde(default)]
    pub verification: Option<VerificationStep>,
    #[serde(default)]
    pub estimated_tokens: Option<u64>,
}

impl TaskGraph {
    pub fn root_tasks(&self) -> Vec<&TaskNode> {
        let has_incoming: std::collections::HashSet<_> =
            self.edges.iter().map(|(_, to)| to.clone()).collect();
        self.nodes
            .iter()
            .filter(|n| !has_incoming.contains(&n.id))
            .collect()
    }

    pub fn dependencies_of(&self, id: &TaskId) -> Vec<&TaskNode> {
        let deps: std::collections::HashSet<_> = self
            .edges
            .iter()
            .filter(|(_, to)| to == id)
            .map(|(from, _)| from.clone())
            .collect();
        self.nodes.iter().filter(|n| deps.contains(&n.id)).collect()
    }

    pub fn dependents_of(&self, id: &TaskId) -> Vec<&TaskNode> {
        let deps: std::collections::HashSet<_> = self
            .edges
            .iter()
            .filter(|(from, _)| from == id)
            .map(|(_, to)| to.clone())
            .collect();
        self.nodes.iter().filter(|n| deps.contains(&n.id)).collect()
    }

    pub fn validate(&self) -> SpecResult<()> {
        // Check for cycles using DFS
        let mut visited = std::collections::HashSet::new();
        let mut rec_stack = std::collections::HashSet::new();

        fn has_cycle(
            graph: &TaskGraph,
            node: &TaskId,
            visited: &mut std::collections::HashSet<TaskId>,
            rec_stack: &mut std::collections::HashSet<TaskId>,
        ) -> bool {
            visited.insert(node.clone());
            rec_stack.insert(node.clone());

            for (_, to) in graph.edges.iter().filter(|(from, _)| from == node) {
                if (!visited.contains(to) && has_cycle(graph, to, visited, rec_stack))
                    || rec_stack.contains(to)
                {
                    return true;
                }
            }

            rec_stack.remove(node);
            false
        }

        for node in &self.nodes {
            if !visited.contains(&node.id)
                && has_cycle(self, &node.id, &mut visited, &mut rec_stack)
            {
                return Err(SpecError::TaskGraph(format!(
                    "cycle detected in task graph around task {}",
                    node.id.0
                )));
            }
        }

        // Check all edges reference existing nodes
        let node_ids: std::collections::HashSet<_> =
            self.nodes.iter().map(|n| n.id.clone()).collect();
        for (from, to) in &self.edges {
            if !node_ids.contains(from) {
                return Err(SpecError::InvalidReference(format!(
                    "edge from non-existent task {}",
                    from.0
                )));
            }
            if !node_ids.contains(to) {
                return Err(SpecError::InvalidReference(format!(
                    "edge to non-existent task {}",
                    to.0
                )));
            }
        }

        Ok(())
    }
}
