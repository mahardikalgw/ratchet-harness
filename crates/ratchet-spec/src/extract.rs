use crate::format::SpecFile;
use crate::schema::{AcceptanceCriterion, Goal, VerificationStep};

/// Extracts structured requirements from a parsed spec file.
pub struct SpecExtractor;

impl SpecExtractor {
    /// Parse acceptance criteria from the `Acceptance Criteria` section.
    ///
    /// Supports checkbox lists (`- [ ] AC-1: ...`), plain bullets, and an
    /// optional machine-readable annotation naming the verification kind:
    ///
    /// ```text
    /// - [ ] AC-1: Login returns 200 [verify: cargo test auth::login]
    /// - [ ] AC-2: Billing code changed [verify-diff: src/billing/]
    /// - [ ] AC-3: Clippy is clean [verify-lint: cargo clippy]
    /// - [ ] AC-4: Copy reads well
    /// ```
    ///
    /// `[verify: ...]` is shorthand for a test step. Criteria with no
    /// annotation are reported as needing manual review.
    pub fn acceptance_criteria(spec: &SpecFile) -> Vec<AcceptanceCriterion> {
        let mut out = Vec::new();
        let Some(section) = spec.acceptance_criteria_section() else {
            return out;
        };

        let mut idx = 0usize;
        for line in section.body.lines() {
            let t = line.trim();
            if t.is_empty() {
                continue;
            }

            let body = strip_bullet(t);
            let Some(body) = body else { continue };
            if body.is_empty() {
                continue;
            }

            idx += 1;
            out.push(Self::parse_criterion(body, idx));
        }

        out
    }

    /// Parse goals from the `Goals` section.
    pub fn goals(spec: &SpecFile) -> Vec<Goal> {
        let mut out = Vec::new();
        let Some(section) = spec.goals_section() else {
            return out;
        };

        let mut idx = 0usize;
        for line in section.body.lines() {
            let Some(body) = strip_bullet(line.trim()) else {
                continue;
            };
            if body.is_empty() {
                continue;
            }
            idx += 1;
            out.push(Goal {
                id: format!("G-{idx}"),
                description: body.to_string(),
                priority: spec.frontmatter.priority,
            });
        }
        out
    }

    /// Parse non-goals into plain strings.
    pub fn non_goals(spec: &SpecFile) -> Vec<String> {
        let Some(section) = spec.non_goals_section() else {
            return Vec::new();
        };
        section
            .body
            .lines()
            .filter_map(|l| strip_bullet(l.trim()))
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    fn parse_criterion(body: &str, idx: usize) -> AcceptanceCriterion {
        // Split an explicit "AC-N:" prefix from the description text.
        let (id, text) = match body.find(':') {
            Some(colon) => {
                let maybe_id = body[..colon].trim();
                if maybe_id.to_ascii_uppercase().starts_with("AC") {
                    (maybe_id.to_string(), body[colon + 1..].trim().to_string())
                } else {
                    (format!("AC-{idx}"), body.to_string())
                }
            }
            None => (format!("AC-{idx}"), body.to_string()),
        };

        let verification = extract_verification_annotation(&text);

        AcceptanceCriterion {
            id,
            description: strip_verify_annotation(&text),
            verification,
            must: true,
        }
    }
}

/// If the line is a bullet (`- `, `* `, with optional `[ ]`/`[x]` checkbox),
/// return the remaining text.
fn strip_bullet(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("-"))
        .or_else(|| line.strip_prefix("*"))?;
    let rest = rest.trim_start();
    // Checkbox forms
    let rest = rest
        .strip_prefix("[ ]")
        .or_else(|| rest.strip_prefix("[x]"))
        .or_else(|| rest.strip_prefix("[X]"))
        .or_else(|| rest.strip_prefix("[]"))
        .unwrap_or(rest);
    Some(rest.trim())
}

/// Extract a `[verify: ...]` / `[verify-diff: ...]` / `[verify-lint: ...]`
/// annotation and map it to a verification step.
fn extract_verification_annotation(text: &str) -> Option<VerificationStep> {
    if let Some(pattern) = annotation_value(text, "verify-diff:") {
        return Some(VerificationStep::Diff { pattern });
    }
    if let Some(tool) = annotation_value(text, "verify-lint:") {
        return Some(VerificationStep::Lint {
            tool,
            must_pass: true,
        });
    }
    if let Some(command) = annotation_value(text, "verify:") {
        return Some(VerificationStep::Test {
            command,
            expected: "test result: ok".to_string(),
        });
    }
    None
}

/// Read the value of `[<key> ...]`, if present and non-empty.
///
/// Checks the longer keys first so `verify-diff:` is never parsed as `verify:`.
fn annotation_value(text: &str, key: &str) -> Option<String> {
    let start = text.find(&format!("[{key}"))? + key.len() + 1;
    let after = &text[start..];
    let end = after.find(']')?;
    let value = after[..end].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

/// Remove any `[verify...]` annotation from display text.
fn strip_verify_annotation(text: &str) -> String {
    let mut out = text.to_string();
    while let Some(start) = out.find("[verify") {
        if let Some(rel_end) = out[start..].find(']') {
            let end = start + rel_end + 1;
            out.replace_range(start..end, "");
        } else {
            break;
        }
    }
    out.trim().to_string()
}
