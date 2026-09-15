//! Conversational spec elicitation.
//!
//! The rigid part of spec-driven development is *authoring* the spec. Editing a
//! markdown file by hand is a poor interface: it assumes the user already knows
//! what a good acceptance criterion looks like. This module turns that step into
//! a conversation — the model asks what it needs to know, then proposes a spec
//! for approval.
//!
//! The artifacts on disk are unchanged. Only the way they get written is.

use crate::CoreResult;
use ratchet_spec::{
    AcceptanceCriterion,
    format::{Priority, SpecFile, SpecFrontmatter, SpecSection, SpecStatus},
    schema::VerificationStep,
};
use serde::{Deserialize, Serialize};

/// One question the model wants answered before it can write a spec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Question {
    pub id: String,
    pub question: String,
    #[serde(default)]
    pub kind: QuestionKind,
    /// Suggested answers, shown as a menu.
    #[serde(default)]
    pub options: Vec<String>,
    /// Pre-selected answer when the user just presses enter.
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuestionKind {
    #[default]
    Text,
    Choice,
    Confirm,
}

/// A turn of the elicitation conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exchange {
    pub question: String,
    pub answer: String,
}

/// What the model produced this turn.
#[derive(Debug, Clone, PartialEq)]
pub enum DiscoveryOutcome {
    /// It needs more information.
    Questions {
        rationale: String,
        questions: Vec<Question>,
    },
    /// It has enough to propose a spec.
    Spec(SpecDraft),
    /// The response could not be understood; surfaced rather than guessed at.
    Unparseable(String),
}

/// A spec the model proposed, before it is persisted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpecDraft {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub goals: Vec<String>,
    #[serde(default)]
    pub non_goals: Vec<String>,
    #[serde(default)]
    pub acceptance_criteria: Vec<CriterionDraft>,
    #[serde(default)]
    pub constraints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CriterionDraft {
    #[serde(default)]
    pub id: String,
    pub description: String,
    /// A shell command that proves this criterion, e.g. `cargo test`.
    #[serde(default, alias = "verification")]
    pub verify: Option<String>,
    /// A path that must change, e.g. `src/lib.rs`.
    #[serde(default, alias = "diff")]
    pub verify_diff: Option<String>,
}

impl SpecDraft {
    /// Normalise identifiers and fill in missing criterion ids.
    pub fn normalize(mut self) -> Self {
        self.id = kebab_case(&self.id);
        if self.id.is_empty() {
            self.id = kebab_case(&self.title);
        }
        if self.id.is_empty() {
            self.id = "spec".to_string();
        }
        for (i, criterion) in self.acceptance_criteria.iter_mut().enumerate() {
            if criterion.id.trim().is_empty() {
                criterion.id = format!("AC-{}", i + 1);
            }
        }
        self
    }

    /// The verification step implied by this criterion, if any.
    pub fn verification_of(&self, criterion: &CriterionDraft) -> Option<VerificationStep> {
        if let Some(command) = criterion.verify.as_ref().filter(|c| !c.trim().is_empty()) {
            return Some(VerificationStep::Test {
                command: command.clone(),
                expected: "test result: ok".to_string(),
            });
        }
        if let Some(pattern) = criterion
            .verify_diff
            .as_ref()
            .filter(|p| !p.trim().is_empty())
        {
            return Some(VerificationStep::Diff {
                pattern: pattern.clone(),
            });
        }
        None
    }

    /// Criteria that are actually machine-checkable.
    pub fn auto_verifiable(&self) -> usize {
        self.acceptance_criteria
            .iter()
            .filter(|c| self.verification_of(c).is_some())
            .count()
    }
}

/// Render the draft as the spec file Ratchet persists.
pub fn render_spec(draft: &SpecDraft, intent: &str) -> String {
    let mut body = String::new();

    if !intent.trim().is_empty() {
        body.push_str("# Intent\n\n");
        body.push_str(intent.trim());
        body.push_str("\n\n");
    }

    body.push_str("# Goals\n\n");
    for goal in &draft.goals {
        body.push_str(&format!("- {goal}\n"));
    }

    if !draft.non_goals.is_empty() {
        body.push_str("\n# Non-Goals\n\n");
        for item in &draft.non_goals {
            body.push_str(&format!("- {item}\n"));
        }
    }

    body.push_str("\n# Acceptance Criteria\n\n");
    for criterion in &draft.acceptance_criteria {
        match draft.verification_of(criterion) {
            Some(VerificationStep::Test { command, .. }) => body.push_str(&format!(
                "- [ ] {}: {} [verify: {}]\n",
                criterion.id, criterion.description, command
            )),
            Some(VerificationStep::Diff { pattern }) => body.push_str(&format!(
                "- [ ] {}: {} [verify-diff: {}]\n",
                criterion.id, criterion.description, pattern
            )),
            Some(VerificationStep::Lint { tool, .. }) => body.push_str(&format!(
                "- [ ] {}: {} [verify-lint: {}]\n",
                criterion.id, criterion.description, tool
            )),
            _ => body.push_str(&format!(
                "- [ ] {}: {}\n",
                criterion.id, criterion.description
            )),
        }
    }

    if !draft.constraints.is_empty() {
        body.push_str("\n# Constraints\n\n");
        for constraint in &draft.constraints {
            body.push_str(&format!("- {constraint}\n"));
        }
    }

    format!(
        "---\nid: {}\ntitle: \"{}\"\nstatus: draft\npriority: normal\ntags: []\ndependencies: []\n---\n\n{}",
        draft.id,
        draft.title.replace('"', "'"),
        body
    )
}

/// Build a `SpecFile` so the rest of the pipeline can consume it directly.
pub fn to_spec_file(draft: &SpecDraft, intent: &str) -> CoreResult<SpecFile> {
    let raw = render_spec(draft, intent);
    ratchet_spec::SpecParser::new()
        .parse(&raw)
        .map_err(crate::CoreError::Spec)
}

/// Parse the model's discovery response.
pub fn parse_discovery(response: &str) -> DiscoveryOutcome {
    let Some(json) = first_json_object(response) else {
        return DiscoveryOutcome::Unparseable(response.trim().to_string());
    };

    #[derive(Deserialize)]
    struct Raw {
        #[serde(default)]
        done: bool,
        #[serde(default)]
        rationale: String,
        #[serde(default)]
        questions: Vec<Question>,
        #[serde(default)]
        spec: Option<SpecDraft>,
    }

    let Ok(raw) = serde_json::from_str::<Raw>(&json) else {
        return DiscoveryOutcome::Unparseable(response.trim().to_string());
    };

    if raw.done {
        match raw.spec {
            Some(spec) => {
                let mut spec = spec.normalize();
                // A spec with no criteria cannot be verified; fall back to a
                // single criterion derived from the title rather than shipping
                // something unusable.
                if spec.acceptance_criteria.is_empty() {
                    spec.acceptance_criteria.push(CriterionDraft {
                        id: "AC-1".to_string(),
                        description: format!("{} works as described", spec.title),
                        verify: None,
                        verify_diff: None,
                    });
                }
                DiscoveryOutcome::Spec(spec)
            }
            None => DiscoveryOutcome::Unparseable(
                "model said it was done but supplied no spec".to_string(),
            ),
        }
    } else if raw.questions.is_empty() {
        DiscoveryOutcome::Unparseable("model asked no questions and proposed no spec".to_string())
    } else {
        DiscoveryOutcome::Questions {
            rationale: raw.rationale,
            questions: raw.questions,
        }
    }
}

/// The prompt that drives elicitation.
pub fn discovery_prompt(intent: &str, transcript: &[Exchange], round: usize) -> String {
    let history = if transcript.is_empty() {
        "(nothing yet)".to_string()
    } else {
        transcript
            .iter()
            .map(|e| format!("Q: {}\nA: {}", e.question, e.answer))
            .collect::<Vec<_>>()
            .join("\n\n")
    };

    let nudge = if round >= 3 {
        "\n\nYou have asked several rounds of questions already. Unless a \
         missing answer would genuinely change the design, propose the spec now."
    } else {
        ""
    };

    format!(
        r#"You are turning a request into a precise, verifiable specification.

Ask before assuming. A question is worth asking only when the answer changes
what gets built. Otherwise decide, and state the assumption in the spec.

Rules:
- Ask at most 4 questions per turn, most important first.
- Offer concrete options when the choice is bounded (kind: "choice").
- Never ask about anything you can infer from the request.
- When you have enough for checkable acceptance criteria, propose the spec.
- Every criteria must be checkable. Prefer a command (`verify`) or a file that
  must change (`verify_diff`). Avoid vague criteria like "works well".
- A spec describes ONE feature or a small coherent slice, not a whole roadmap.
- Write in the user's language.

Reply with a single JSON object and nothing else.

To ask questions:
{{"done": false, "rationale": "why you need this", "questions": [
  {{"id": "q1", "question": "...", "kind": "choice",
    "options": ["a", "b"], "default": "a"}}
]}}

To propose the spec:
{{"done": true, "spec": {{
  "id": "kebab-case-id",
  "title": "Short title",
  "goals": ["..."],
  "non_goals": ["..."],
  "acceptance_criteria": [
    {{"id": "AC-1", "description": "...", "verify": "cargo test"}},
    {{"id": "AC-2", "description": "...", "verify_diff": "src/lib.rs"}}
  ],
  "constraints": ["..."]
}}}}

The user's request:
{intent}

Conversation so far:
{history}{nudge}"#
    )
}

/// Prompt used to narrate a completed run back to the user.
pub fn summary_prompt(intent: &str, facts: &str) -> String {
    format!(
        r#"You just finished working on this request:

{intent}

Here is what mechanically happened:

{facts}

Write a short, plain-language report to the user, in their language. Cover:
- what you changed, concretely
- which acceptance criteria are verified and which still need a human
- anything that did NOT go to plan, and what you would do next

Be direct. Do not claim success for anything not shown above. No markdown
headers, no JSON — just a few short paragraphs."#
    )
}

fn kebab_case(input: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true;
    for ch in input.chars() {
        if ch.is_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// Extract the first balanced `{...}`, ignoring braces inside strings.
fn first_json_object(text: &str) -> Option<String> {
    let trimmed = text.trim();

    // Prefer a fenced block if the model wrapped its answer.
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
    let start = candidate.find('{')?;

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
                    return Some(candidate[start..=i].to_string());
                }
            }
            _ => {}
        }
    }

    None
}

/// Convenience: build the criteria the pipeline will verify.
pub fn criteria_of(draft: &SpecDraft) -> Vec<AcceptanceCriterion> {
    draft
        .acceptance_criteria
        .iter()
        .map(|c| AcceptanceCriterion {
            id: c.id.clone(),
            description: c.description.clone(),
            verification: draft.verification_of(c),
            must: true,
        })
        .collect()
}

/// Default frontmatter for a drafted spec (used by tests and tooling).
pub fn frontmatter_of(draft: &SpecDraft) -> SpecFrontmatter {
    SpecFrontmatter {
        id: draft.id.clone(),
        title: draft.title.clone(),
        status: SpecStatus::Draft,
        tags: Vec::new(),
        assigned_model: None,
        priority: Priority::Normal,
        dependencies: Vec::new(),
        estimate: None,
    }
}

/// Section list for a drafted spec (used by tooling that inspects specs).
pub fn sections_of(draft: &SpecDraft, intent: &str) -> Vec<SpecSection> {
    let mut sections = Vec::new();
    if !intent.trim().is_empty() {
        sections.push(SpecSection {
            heading: Some("Intent".to_string()),
            level: 1,
            body: intent.trim().to_string(),
            metadata: Default::default(),
        });
    }
    sections.push(SpecSection {
        heading: Some("Goals".to_string()),
        level: 1,
        body: draft
            .goals
            .iter()
            .map(|g| format!("- {g}"))
            .collect::<Vec<_>>()
            .join("\n"),
        metadata: Default::default(),
    });
    sections
}
