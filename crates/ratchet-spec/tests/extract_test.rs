use ratchet_spec::SpecExtractor;
use ratchet_spec::{schema::VerificationStep, SpecParser};

const SPEC: &str = r#"---
id: auth
title: "Auth"
status: draft
---

# Goals

- Support email/password login
- Support OAuth

# Non-Goals

- Password reset

# Acceptance Criteria

- [ ] AC-1: Login returns 200 [verify: cargo test auth::login]
- [x] AC-2: Sessions expire after 24h
- AC-3: Failed logins are rate limited
- [ ] AC-4: Billing code changed [verify-diff: src/billing/]
- [ ] AC-5: Clippy is clean [verify-lint: cargo clippy]
"#;

fn parse() -> ratchet_spec::SpecFile {
    SpecParser::new().parse(SPEC).unwrap()
}

#[test]
fn extracts_all_acceptance_criteria() {
    let criteria = SpecExtractor::acceptance_criteria(&parse());
    assert_eq!(criteria.len(), 5);
    assert_eq!(criteria[0].id, "AC-1");
    assert_eq!(criteria[1].id, "AC-2");
    assert_eq!(criteria[2].id, "AC-3");
}

#[test]
fn extracts_diff_annotation() {
    let criteria = SpecExtractor::acceptance_criteria(&parse());
    match &criteria[3].verification {
        Some(VerificationStep::Diff { pattern }) => assert_eq!(pattern, "src/billing/"),
        other => panic!("expected diff step, got {other:?}"),
    }
    assert_eq!(criteria[3].description, "Billing code changed");
}

#[test]
fn extracts_lint_annotation() {
    let criteria = SpecExtractor::acceptance_criteria(&parse());
    match &criteria[4].verification {
        Some(VerificationStep::Lint { tool, must_pass }) => {
            assert_eq!(tool, "cargo clippy");
            assert!(*must_pass);
        }
        other => panic!("expected lint step, got {other:?}"),
    }
}

#[test]
fn extracts_verify_annotation_as_test_step() {
    let criteria = SpecExtractor::acceptance_criteria(&parse());
    match &criteria[0].verification {
        Some(VerificationStep::Test { command, .. }) => {
            assert_eq!(command, "cargo test auth::login");
        }
        other => panic!("expected test step, got {other:?}"),
    }
}

#[test]
fn strips_verify_annotation_from_description() {
    let criteria = SpecExtractor::acceptance_criteria(&parse());
    assert_eq!(criteria[0].description, "Login returns 200");
    assert!(!criteria[0].description.contains("[verify:"));
}

#[test]
fn criteria_without_annotation_have_no_verification() {
    let criteria = SpecExtractor::acceptance_criteria(&parse());
    assert!(criteria[1].verification.is_none());
    assert!(criteria[2].verification.is_none());
}

#[test]
fn extracts_goals_and_non_goals() {
    let spec = parse();
    let goals = SpecExtractor::goals(&spec);
    assert_eq!(goals.len(), 2);
    assert_eq!(goals[0].description, "Support email/password login");

    let non_goals = SpecExtractor::non_goals(&spec);
    assert_eq!(non_goals, vec!["Password reset"]);
}

#[test]
fn handles_spec_without_criteria_section() {
    let spec = SpecParser::new().parse("# Title\n\nSome text.").unwrap();
    assert!(SpecExtractor::acceptance_criteria(&spec).is_empty());
}
