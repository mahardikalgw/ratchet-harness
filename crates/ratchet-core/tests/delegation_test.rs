use ratchet_core::delegation::{parse_review_verdict, AgentRole};
use std::collections::HashMap;

#[test]
fn parses_role_names_with_aliases() {
    assert_eq!(AgentRole::parse("planner"), Some(AgentRole::Planner));
    assert_eq!(AgentRole::parse("PLAN"), Some(AgentRole::Planner));
    assert_eq!(AgentRole::parse("implementer"), Some(AgentRole::Implementer));
    assert_eq!(AgentRole::parse("coder"), Some(AgentRole::Implementer));
    assert_eq!(AgentRole::parse("review"), Some(AgentRole::Reviewer));
    assert_eq!(AgentRole::parse("tester"), Some(AgentRole::Tester));
    assert_eq!(AgentRole::parse("nonsense"), None);
}

#[test]
fn roles_declare_distinct_capabilities() {
    // The planner needs reasoning; the implementer needs tools. Routing relies
    // on this to send each role to an appropriate model.
    assert!(AgentRole::Planner.required_capabilities().needs_extended_thinking);
    assert!(AgentRole::Implementer.required_capabilities().needs_tools);
    assert!(!AgentRole::Reviewer.required_capabilities().needs_tools);
    assert!(AgentRole::Implementer.uses_tools());
    assert!(!AgentRole::Reviewer.uses_tools());
}

#[test]
fn parses_a_clean_approval_verdict() {
    let verdict = parse_review_verdict(
        r#"{"approved": true, "issues": [], "summary": "looks correct"}"#,
    );
    assert!(verdict.parsed);
    assert!(verdict.approved);
    assert!(!verdict.should_retry());
    assert_eq!(verdict.summary, "looks correct");
}

#[test]
fn parses_a_rejection_with_issues() {
    let verdict = parse_review_verdict(
        r#"{"approved": false, "issues": ["missing test", "unused import"], "summary": "incomplete"}"#,
    );
    assert!(verdict.parsed);
    assert!(!verdict.approved);
    assert_eq!(verdict.issues.len(), 2);
    assert!(verdict.should_retry());
}

#[test]
fn parses_a_fenced_verdict() {
    let verdict = parse_review_verdict(
        "Here is my review:\n\n```json\n{\"approved\": true, \"summary\": \"ok\"}\n```\n",
    );
    assert!(verdict.parsed);
    assert!(verdict.approved);
}

#[test]
fn accepts_common_field_aliases() {
    let verdict = parse_review_verdict(r#"{"pass": true, "problems": [], "reason": "fine"}"#);
    assert!(verdict.parsed);
    assert!(verdict.approved);
    assert_eq!(verdict.summary, "fine");
}

#[test]
fn defaults_missing_fields_safely() {
    // A bare object parses, and `approved` defaults to false.
    let verdict = parse_review_verdict(r#"{"summary": "hmm"}"#);
    assert!(verdict.parsed);
    assert!(!verdict.approved);
    assert!(verdict.should_retry());
}

#[test]
fn unparseable_output_never_triggers_a_retry_loop() {
    let verdict = parse_review_verdict("I think it is probably fine, hard to say.");
    assert!(!verdict.parsed);
    assert!(!verdict.approved);
    // Critically: no retry, so garbage output cannot loop forever.
    assert!(!verdict.should_retry());
    assert!(verdict.summary.contains("probably fine"));
}

#[test]
fn ignores_braces_inside_strings() {
    let verdict = parse_review_verdict(
        r#"{"approved": true, "summary": "handles {braces} and \"quotes\" fine"}"#,
    );
    assert!(verdict.parsed);
    assert!(verdict.approved);
    assert!(verdict.summary.contains("{braces}"));
}

#[test]
fn resolves_role_provider_from_settings() {
    let settings = ratchet_core::DelegationSettings {
        review: true,
        max_review_rounds: 2,
        roles: HashMap::from([
            ("reviewer".to_string(), "claude".to_string()),
            ("implementer".to_string(), "  ".to_string()), // blank is ignored
        ]),
    };

    assert_eq!(
        ratchet_core::provider_for_role(&settings, AgentRole::Reviewer),
        Some("claude".to_string())
    );
    assert_eq!(
        ratchet_core::provider_for_role(&settings, AgentRole::Implementer),
        None
    );
    assert_eq!(
        ratchet_core::provider_for_role(&settings, AgentRole::Tester),
        None
    );
}
