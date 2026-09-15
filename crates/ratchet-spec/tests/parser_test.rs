use ratchet_spec::{SpecParser, SpecValidator};

const SPEC: &str = r#"---
id: billing-reminders
title: "Billing Reminders"
status: draft
priority: high
tags: [billing, notifications]
dependencies: []
---

# Goals

- Send payment reminders before due date
- Support email and WhatsApp channels

# Non-Goals

- Handling actual payments

# Acceptance Criteria

- [ ] AC-1: Reminder sent 3 days before due date
- [ ] AC-2: Delivery failure is logged

# Constraints

- Must respect user timezone
"#;

#[test]
fn parses_frontmatter_fields() {
    let parser = SpecParser::new();
    let spec = parser.parse(SPEC).unwrap();

    assert_eq!(spec.frontmatter.id, "billing-reminders");
    assert_eq!(spec.frontmatter.title, "Billing Reminders");
    assert_eq!(spec.frontmatter.tags, vec!["billing", "notifications"]);
    assert_eq!(spec.frontmatter.priority, ratchet_spec::format::Priority::High);
}

#[test]
fn parses_markdown_sections() {
    let parser = SpecParser::new();
    let spec = parser.parse(SPEC).unwrap();

    let goals = spec.goals_section().expect("goals");
    assert!(goals.body.contains("Send payment reminders"));

    let ac = spec.acceptance_criteria_section().expect("acceptance");
    assert!(ac.body.contains("AC-1"));
    assert!(ac.body.contains("AC-2"));

    let ng = spec.non_goals_section().expect("non-goals");
    assert!(ng.body.contains("Handling actual payments"));
}

#[test]
fn validates_clean_spec() {
    let parser = SpecParser::new();
    let spec = parser.parse(SPEC).unwrap();
    let validator = SpecValidator::new();

    let issues = validator.validate(&spec).unwrap();
    let errors: Vec<_> = issues.iter().filter(|i| i.is_error()).collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    assert!(validator.is_valid(&spec).unwrap());
}

#[test]
fn flags_missing_frontmatter_as_error() {
    let parser = SpecParser::new();
    let spec = parser.parse("# Just a heading\n\nSome text.").unwrap();
    let validator = SpecValidator::new();

    let issues = validator.validate(&spec).unwrap();
    assert!(issues.iter().any(|i| i.is_error()));
    assert!(!validator.is_valid(&spec).unwrap());
}

#[test]
fn handles_body_without_frontmatter() {
    let parser = SpecParser::new();
    let spec = parser.parse("# Heading\n\nBody line one.\nBody line two.").unwrap();

    assert!(spec.frontmatter.id.is_empty());
    assert!(!spec.sections.is_empty());
}
