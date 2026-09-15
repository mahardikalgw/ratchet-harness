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
    assert_eq!(
        spec.frontmatter.priority,
        ratchet_spec::format::Priority::High
    );
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
    let spec = parser
        .parse("# Heading\n\nBody line one.\nBody line two.")
        .unwrap();

    assert!(spec.frontmatter.id.is_empty());
    assert!(!spec.sections.is_empty());
}

// ---------------------------------------------------------------------------
// The body must be preserved verbatim: markdown syntax characters appear in
// real file paths, and rewriting the body from parsed events destroys them.
// ---------------------------------------------------------------------------

#[test]
fn preserves_dunder_names_in_paths() {
    // `__init__` parses as markdown emphasis; rebuilding the body turned
    // `app/__init__.py` into `app/init.py`.
    let spec = SpecParser::new()
        .parse(
            "---\nid: x\n---\n\n# Acceptance Criteria\n\n\
             - [ ] AC-1: app/__init__.py berubah [verify-diff: app/__init__.py]\n",
        )
        .unwrap();

    let body = &spec.acceptance_criteria_section().unwrap().body;
    assert!(
        body.contains("app/__init__.py"),
        "dunder mangled, got: {body:?}"
    );
    assert!(!body.contains("app/init.py"));
}

#[test]
fn preserves_underscores_and_asterisks_in_text() {
    let spec = SpecParser::new()
        .parse(
            "---\nid: x\n---\n\n# Goals\n\n\
             - handle my_var and *args and **kwargs\n\
             - keep snake_case_names intact\n",
        )
        .unwrap();

    let body = &spec.goals_section().unwrap().body;
    assert!(body.contains("my_var"), "{body:?}");
    assert!(body.contains("*args"), "{body:?}");
    assert!(body.contains("**kwargs"), "{body:?}");
    assert!(body.contains("snake_case_names"), "{body:?}");
}

#[test]
fn preserves_paths_with_underscores() {
    let spec = SpecParser::new()
        .parse(
            "---\nid: x\n---\n\n# Acceptance Criteria\n\n\
             - [ ] AC-1: src/my_module/sub_dir.py berubah [verify-diff: src/my_module/]\n",
        )
        .unwrap();

    let body = &spec.acceptance_criteria_section().unwrap().body;
    assert!(body.contains("src/my_module/sub_dir.py"), "{body:?}");
}

#[test]
fn preserves_inline_code_and_bullets_verbatim() {
    let spec = SpecParser::new()
        .parse(
            "---\nid: x\n---\n\n# Goals\n\n\
             - run `cargo test --all`\n\
             - *emphasis* stays\n",
        )
        .unwrap();

    let body = &spec.goals_section().unwrap().body;
    assert!(body.contains("`cargo test --all`"), "{body:?}");
    assert!(body.contains("*emphasis*"), "{body:?}");
}

#[test]
fn preserves_fenced_code_blocks() {
    let source = "---\nid: x\n---\n\n# Notes\n\n```python\nx = __init__\n```\n";
    let spec = SpecParser::new().parse(source).unwrap();
    let body = &spec.section("Notes").unwrap().body;
    assert!(body.contains("```python"), "{body:?}");
    assert!(body.contains("__init__"), "{body:?}");
}

#[test]
fn preamble_before_the_first_heading_is_kept() {
    let spec = SpecParser::new()
        .parse("---\nid: x\n---\n\nLoose preamble text.\n\n# Goals\n\n- one\n")
        .unwrap();

    let untitled = spec
        .sections
        .iter()
        .find(|s| s.heading.is_none())
        .expect("preamble section");
    assert!(untitled.body.contains("Loose preamble text."));
}
