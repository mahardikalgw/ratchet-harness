use crate::CoreResult;
use ratchet_spec::{
    schema::{Plan, TaskGraph, TaskNode, VerificationStep},
    SpecExtractor, SpecFile, TaskId,
};
use serde::Deserialize;

/// Parses a model's plan response into a structured `Plan` with a task graph.
pub struct PlanParser;

impl PlanParser {
    /// Parse the model response. Falls back to a criteria-derived task graph
    /// when the response contains no usable JSON.
    pub fn parse(response: &str, spec: &SpecFile) -> CoreResult<Plan> {
        match Self::parse_json(response) {
            Some(raw) => Ok(Self::into_plan(raw, spec)),
            None => Ok(Self::fallback(spec, response)),
        }
    }

    /// Extract and parse the JSON object embedded in the response.
    fn parse_json(response: &str) -> Option<RawPlan> {
        let json = extract_json_object(response)?;
        // Try the full shape first, then a bare task array.
        if let Ok(plan) = serde_json::from_str::<RawPlan>(&json) {
            return Some(plan);
        }
        if let Ok(tasks) = serde_json::from_str::<Vec<RawTask>>(&json) {
            return Some(RawPlan {
                summary: String::new(),
                tasks,
                affected_modules: vec![],
                data_model_changes: vec![],
                risk_notes: vec![],
            });
        }
        None
    }

    fn into_plan(raw: RawPlan, spec: &SpecFile) -> Plan {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        for (i, task) in raw.tasks.into_iter().enumerate() {
            let id = if task.id.trim().is_empty() {
                format!("T-{}", i + 1)
            } else {
                task.id
            };
            let node_id = TaskId(id.clone());

            for dep in &task.depends_on {
                edges.push((TaskId(dep.clone()), node_id.clone()));
            }

            nodes.push(TaskNode {
                id: node_id,
                title: task.title,
                description: task.description,
                assigned_model: task.assigned_model,
                verification: task.verification.map(RawVerification::into_step),
                estimated_tokens: task.estimated_tokens,
            });
        }

        let graph = TaskGraph { nodes, edges };
        // Drop edges referencing unknown tasks so validation passes cleanly.
        let graph = sanitize_graph(graph);

        Plan {
            spec_id: spec.frontmatter.id.clone(),
            title: spec.frontmatter.title.clone(),
            summary: raw.summary,
            affected_modules: raw.affected_modules,
            data_model_changes: raw.data_model_changes,
            risk_notes: raw.risk_notes,
            task_graph: graph,
        }
    }

    /// Deterministic fallback: one task per acceptance criterion.
    fn fallback(spec: &SpecFile, response: &str) -> Plan {
        let criteria = SpecExtractor::acceptance_criteria(spec);

        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut prev: Option<TaskId> = None;

        for (i, ac) in criteria.iter().enumerate() {
            let id = TaskId(format!("T-{}", i + 1));
            if let Some(p) = &prev {
                edges.push((p.clone(), id.clone()));
            }
            prev = Some(id.clone());

            nodes.push(TaskNode {
                id,
                title: ac.id.clone(),
                description: ac.description.clone(),
                assigned_model: None,
                verification: ac.verification.clone(),
                estimated_tokens: None,
            });
        }

        // If the spec had no criteria at all, emit a single task from the spec title.
        if nodes.is_empty() {
            nodes.push(TaskNode {
                id: TaskId("T-1".to_string()),
                title: spec.frontmatter.title.clone(),
                description: response.chars().take(2000).collect(),
                assigned_model: None,
                verification: None,
                estimated_tokens: None,
            });
        }

        Plan {
            spec_id: spec.frontmatter.id.clone(),
            title: spec.frontmatter.title.clone(),
            summary: response.to_string(),
            affected_modules: vec![],
            data_model_changes: vec![],
            risk_notes: vec![],
            task_graph: TaskGraph { nodes, edges },
        }
    }
}

/// Remove edges whose endpoints are not in the node set.
fn sanitize_graph(mut graph: TaskGraph) -> TaskGraph {
    let ids: std::collections::HashSet<_> = graph.nodes.iter().map(|n| n.id.clone()).collect();
    graph
        .edges
        .retain(|(from, to)| ids.contains(from) && ids.contains(to));
    graph
}

/// Find the first balanced top-level `{...}` or `[...]` in the text,
/// ignoring braces inside strings and stripping markdown code fences.
fn extract_json_object(text: &str) -> Option<String> {
    let trimmed = text.trim();

    // Prefer an explicit fenced block if present.
    let candidate = if let Some(start) = trimmed.find("```") {
        let after = &trimmed[start + 3..];
        let after = after
            .strip_prefix("json")
            .or_else(|| after.strip_prefix("JSON"))
            .unwrap_or(after);
        match after.find("```") {
            Some(end) => after[..end].trim().to_string(),
            None => after.trim().to_string(),
        }
    } else {
        trimmed.to_string()
    };

    let bytes = candidate.as_bytes();
    let start = candidate.find(['{', '['])?;
    let open = bytes[start];
    let close = if open == b'{' { b'}' } else { b']' };

    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;

    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            _ if b == open => depth += 1,
            _ if b == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(candidate[start..=i].to_string());
                }
            }
            _ => {}
        }
    }

    None
}

#[derive(Debug, Deserialize)]
struct RawPlan {
    #[serde(default)]
    summary: String,
    #[serde(default, alias = "task_graph", alias = "tasks")]
    tasks: Vec<RawTask>,
    #[serde(default)]
    affected_modules: Vec<String>,
    #[serde(default)]
    data_model_changes: Vec<String>,
    #[serde(default)]
    risk_notes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawTask {
    #[serde(default)]
    id: String,
    #[serde(default, alias = "name")]
    title: String,
    #[serde(default, alias = "desc")]
    description: String,
    #[serde(default, alias = "model")]
    assigned_model: Option<String>,
    #[serde(default, alias = "depends_on", alias = "deps", alias = "dependencies")]
    depends_on: Vec<String>,
    #[serde(default)]
    verification: Option<RawVerification>,
    #[serde(default)]
    estimated_tokens: Option<u64>,
}

/// Lenient representation of a model-supplied verification step.
///
/// Real models emit unforeseen `kind` values, prose `expected` strings, and
/// nulled fields. None of that should reject the whole plan — unknown shapes
/// degrade to a manual check instead.
#[derive(Debug, Deserialize)]
struct RawVerification {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    command: Option<String>,
    #[serde(default, alias = "tool")]
    lint_tool: Option<String>,
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    instructions: Option<String>,
    #[serde(default)]
    must_pass: Option<bool>,
    #[serde(default)]
    #[allow(dead_code)]
    expected: Option<String>,
}

impl RawVerification {
    fn into_step(self) -> VerificationStep {
        match self.kind.to_ascii_lowercase().as_str() {
            "test" => match self.command.filter(|c| !c.trim().is_empty()) {
                Some(command) => VerificationStep::Test {
                    command,
                    // Exit code decides pass/fail; `expected` is advisory only.
                    expected: "test result: ok".to_string(),
                },
                None => VerificationStep::Manual {
                    instructions: "model requested a test but named no command".to_string(),
                },
            },
            "lint" => match self.lint_tool.or(self.command) {
                Some(tool) if !tool.trim().is_empty() => VerificationStep::Lint {
                    tool,
                    must_pass: self.must_pass.unwrap_or(true),
                },
                _ => VerificationStep::Manual {
                    instructions: "model requested a lint but named no tool".to_string(),
                },
            },
            "diff" => match self.pattern {
                Some(pattern) if !pattern.trim().is_empty() => {
                    VerificationStep::Diff { pattern }
                }
                _ => VerificationStep::Manual {
                    instructions: "model requested a diff check but gave no pattern".to_string(),
                },
            },
            // Anything unrecognised (including "code-review", "manual", "")
            // becomes a manual check rather than poisoning the plan.
            other => VerificationStep::Manual {
                instructions: if other.is_empty() {
                    self.instructions
                        .unwrap_or_else(|| "manual verification required".to_string())
                } else {
                    format!("model suggested `{other}` verification")
                },
            },
        }
    }
}
