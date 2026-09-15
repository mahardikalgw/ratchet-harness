use crate::config::DelegationSettings;
use crate::routing::RequiredCapabilities;
use serde::{Deserialize, Serialize};

/// Which agent in the pipeline is acting.
///
/// PRD §8 P2: "planner agent + implementer agent + reviewer agent, each
/// possibly on a different model". Each role has its own prompt, its own
/// capability requirements, and can be routed to its own provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentRole {
    Planner,
    Implementer,
    Reviewer,
    Tester,
}

impl AgentRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentRole::Planner => "planner",
            AgentRole::Implementer => "implementer",
            AgentRole::Reviewer => "reviewer",
            AgentRole::Tester => "tester",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "planner" | "plan" => Some(AgentRole::Planner),
            "implementer" | "coder" | "implement" => Some(AgentRole::Implementer),
            "reviewer" | "review" => Some(AgentRole::Reviewer),
            "tester" | "test" => Some(AgentRole::Tester),
            _ => None,
        }
    }

    /// The system prompt for this role.
    pub fn system_prompt(&self) -> &'static str {
        match self {
            AgentRole::Planner => PLANNER_PROMPT,
            AgentRole::Implementer => IMPLEMENTER_PROMPT,
            AgentRole::Reviewer => REVIEWER_PROMPT,
            AgentRole::Tester => TESTER_PROMPT,
        }
    }

    /// Capabilities this role needs from its model. Lets routing send cheap
    /// boilerplate to a budget model and reasoning-heavy steps to a stronger one.
    pub fn required_capabilities(&self) -> RequiredCapabilities {
        match self {
            AgentRole::Planner => RequiredCapabilities {
                needs_extended_thinking: true,
                min_context_tokens: 100_000,
                ..Default::default()
            },
            AgentRole::Implementer => RequiredCapabilities {
                needs_tools: true,
                min_context_tokens: 32_000,
                ..Default::default()
            },
            AgentRole::Reviewer => RequiredCapabilities {
                min_context_tokens: 64_000,
                ..Default::default()
            },
            AgentRole::Tester => RequiredCapabilities {
                needs_tools: true,
                min_context_tokens: 32_000,
                ..Default::default()
            },
        }
    }

    pub fn uses_tools(&self) -> bool {
        matches!(self, AgentRole::Implementer | AgentRole::Tester)
    }
}

/// The reviewer's structured verdict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewVerdict {
    pub approved: bool,
    #[serde(default)]
    pub issues: Vec<String>,
    #[serde(default)]
    pub summary: String,
    /// False when the model did not return parseable JSON. An unparseable
    /// review never triggers a retry — it is surfaced instead, because
    /// looping on garbage output would burn tokens without converging.
    #[serde(default)]
    pub parsed: bool,
}

impl ReviewVerdict {
    pub fn unparseable(raw: &str) -> Self {
        Self {
            approved: false,
            issues: Vec::new(),
            summary: raw.trim().chars().take(500).collect(),
            parsed: false,
        }
    }

    /// Whether a rejected review should send the task back for another round.
    pub fn should_retry(&self) -> bool {
        self.parsed && !self.approved
    }
}

/// Parse the reviewer's response, accepting fenced JSON and light variation.
pub fn parse_review_verdict(response: &str) -> ReviewVerdict {
    let Some(json) = first_json_object(response) else {
        return ReviewVerdict::unparseable(response);
    };

    #[derive(Deserialize)]
    struct Raw {
        #[serde(default, alias = "pass", alias = "ok", alias = "lgtm")]
        approved: bool,
        #[serde(default, alias = "problems", alias = "concerns")]
        issues: Vec<String>,
        #[serde(default, alias = "reason", alias = "notes")]
        summary: String,
    }

    match serde_json::from_str::<Raw>(&json) {
        Ok(raw) => ReviewVerdict {
            approved: raw.approved,
            issues: raw.issues,
            summary: raw.summary,
            parsed: true,
        },
        Err(_) => ReviewVerdict::unparseable(response),
    }
}

/// Find the first balanced `{...}` in the text, ignoring braces in strings.
fn first_json_object(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let start = text.find('{')?;

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
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(text[start..=i].to_string());
                }
            }
            _ => {}
        }
    }

    None
}

/// Resolve which provider should serve a role, honouring config overrides.
pub fn provider_for_role(settings: &DelegationSettings, role: AgentRole) -> Option<String> {
    settings
        .roles
        .get(role.as_str())
        .cloned()
        .filter(|s| !s.trim().is_empty())
}

/// Build the prompt that asks the reviewer to assess a change.
pub fn review_request(task_title: &str, task_description: &str, diff: &str) -> String {
    format!(
        "Review the following completed task.\n\n\
         Task: {task_title}\n\
         Intent: {task_description}\n\n\
         Changes made:\n```diff\n{diff}\n```\n\n\
         Decide whether the change actually satisfies the intent.\n\
         Respond with a single JSON object and nothing else:\n\
         {{\"approved\": true|false, \"issues\": [\"...\"], \"summary\": \"...\"}}\n\n\
         Only approve if the change is correct, complete, and does not break \
         anything. If you cannot verify it from the diff, reject and say what \
         evidence is missing."
    )
}

/// Build the prompt that sends review feedback back to the implementer.
pub fn revision_request(issues: &[String], summary: &str) -> String {
    let mut out = String::from(
        "A reviewer rejected your previous attempt. Address every point below, \
         then stop calling tools and summarise.\n\n",
    );
    if !summary.trim().is_empty() {
        out.push_str(&format!("Reviewer summary: {}\n\n", summary.trim()));
    }
    if issues.is_empty() {
        out.push_str("No specific issues were listed; re-examine your change.\n");
    } else {
        out.push_str("Issues to fix:\n");
        for (i, issue) in issues.iter().enumerate() {
            out.push_str(&format!("{}. {}\n", i + 1, issue));
        }
    }
    out
}

const PLANNER_PROMPT: &str = r#"You are the planning agent in a spec-driven engineering harness.

You turn a specification into a technical plan and a task graph.
You do not write code. You produce a plan others will execute.

Be concrete: name the modules to change, the risks, and how each task will be
verified. Every task must be independently verifiable."#;

const IMPLEMENTER_PROMPT: &str = r#"You are the implementation agent in a spec-driven engineering harness.

You MUST perform the work with the provided tools. Describing a change is not
making it. Never claim something is done unless a tool call did it.

Workflow:
1. Call list_dir to see the project layout.
2. Call file_read on the files you intend to change.
3. Call file_write or file_patch to make the change.
4. Call test_run (or shell_exec with cargo test) to confirm tests pass.
5. Only then reply with a short plain-text summary and stop calling tools.

Rules:
- Use the tools; do not print tool calls as text or JSON in your reply.
- Make small, testable changes rather than large refactors.
- Prefer the dedicated file tools over shell commands for editing.
- If a tool returns an error, read it and adjust rather than repeating it.
- If you are genuinely blocked, say so explicitly rather than guessing."#;

const REVIEWER_PROMPT: &str = r#"You are the review agent in a spec-driven engineering harness.

You judge whether a completed change actually satisfies its intent. You do not
edit files and you do not run tools.

Be strict but fair. Approve only when the change is correct, complete, and
low-risk. When you reject, list concrete, actionable issues.

Always answer with a single JSON object:
{"approved": true|false, "issues": ["..."], "summary": "..."}"#;

const TESTER_PROMPT: &str = r#"You are the testing agent in a spec-driven engineering harness.

You add or strengthen tests using the provided tools. You do not change
production behaviour to make tests pass.

Workflow:
1. Call file_read to understand the code under test.
2. Call file_write or file_patch to add tests.
3. Call test_run to confirm they pass.
4. Reply with a short summary and stop calling tools."#;
