use ratchet_core::discovery::{
    DiscoveryOutcome, Exchange, QuestionKind, SpecDraft, parse_discovery, render_spec, to_spec_file,
};

fn questions_response() -> &'static str {
    r#"{
        "done": false,
        "rationale": "Dua hal yang mengubah desain:",
        "questions": [
            {"id": "q1", "question": "Fisik atau digital?",
             "kind": "choice", "options": ["fisik", "digital"], "default": "fisik"},
            {"id": "q2", "question": "Payment gateway?", "kind": "text"}
        ]
    }"#
}

fn spec_response() -> &'static str {
    r#"{
        "done": true,
        "spec": {
            "id": "Toko Online",
            "title": "Toko Online Sederhana",
            "goals": ["Menampilkan katalog"],
            "non_goals": ["Multi-vendor"],
            "acceptance_criteria": [
                {"id": "AC-1", "description": "Katalog ada", "verify_diff": "src/lib.rs"},
                {"description": "Test lulus", "verify": "cargo test"}
            ],
            "constraints": ["Bahasa Indonesia"]
        }
    }"#
}

#[test]
fn parses_a_question_turn() {
    match parse_discovery(questions_response()) {
        DiscoveryOutcome::Questions {
            rationale,
            questions,
        } => {
            assert!(rationale.contains("mengubah desain"));
            assert_eq!(questions.len(), 2);
            assert_eq!(questions[0].kind, QuestionKind::Choice);
            assert_eq!(questions[0].options, vec!["fisik", "digital"]);
            assert_eq!(questions[0].default.as_deref(), Some("fisik"));
            assert_eq!(questions[1].kind, QuestionKind::Text);
        }
        other => panic!("expected questions, got {other:?}"),
    }
}

#[test]
fn parses_a_spec_turn() {
    match parse_discovery(spec_response()) {
        DiscoveryOutcome::Spec(draft) => {
            assert_eq!(draft.title, "Toko Online Sederhana");
            assert_eq!(draft.goals, vec!["Menampilkan katalog"]);
            assert_eq!(draft.acceptance_criteria.len(), 2);
            // Both criteria are machine-checkable.
            assert_eq!(draft.auto_verifiable(), 2);
        }
        other => panic!("expected a spec, got {other:?}"),
    }
}

#[test]
fn normalises_a_spec_id_to_kebab_case() {
    // Models write ids as titles far too often to reject it.
    let DiscoveryOutcome::Spec(draft) = parse_discovery(spec_response()) else {
        panic!("expected spec");
    };
    assert_eq!(draft.id, "toko-online");
}

#[test]
fn fills_in_missing_criterion_ids() {
    let DiscoveryOutcome::Spec(draft) = parse_discovery(spec_response()) else {
        panic!("expected spec");
    };
    assert_eq!(draft.acceptance_criteria[0].id, "AC-1");
    assert_eq!(draft.acceptance_criteria[1].id, "AC-2");
}

#[test]
fn a_spec_without_criteria_still_gets_one() {
    // Otherwise the generated spec could never be verified.
    let raw = r#"{"done": true, "spec": {"id": "x", "title": "Thing", "goals": []}}"#;
    let DiscoveryOutcome::Spec(draft) = parse_discovery(raw) else {
        panic!("expected spec");
    };
    assert_eq!(draft.acceptance_criteria.len(), 1);
}

#[test]
fn parses_a_fenced_response() {
    let raw = format!("Here you go:\n\n```json\n{}\n```\n", spec_response());
    assert!(matches!(parse_discovery(&raw), DiscoveryOutcome::Spec(_)));
}

#[test]
fn unparseable_output_is_surfaced_not_guessed() {
    // A model that rambles must not be silently interpreted as approval.
    match parse_discovery("Sure, I can help with that!") {
        DiscoveryOutcome::Unparseable(raw) => assert!(raw.contains("Sure")),
        other => panic!("expected unparseable, got {other:?}"),
    }
}

#[test]
fn done_without_a_spec_is_unparseable() {
    let raw = r#"{"done": true}"#;
    assert!(matches!(
        parse_discovery(raw),
        DiscoveryOutcome::Unparseable(_)
    ));
}

#[test]
fn neither_questions_nor_spec_is_unparseable() {
    let raw = r#"{"done": false, "questions": []}"#;
    assert!(matches!(
        parse_discovery(raw),
        DiscoveryOutcome::Unparseable(_)
    ));
}

#[test]
fn braces_inside_strings_do_not_break_extraction() {
    let raw = r#"{"done": true, "spec": {"id": "x", "title": "Uses {braces}", "goals": ["a"]}}"#;
    let DiscoveryOutcome::Spec(draft) = parse_discovery(raw) else {
        panic!("expected spec");
    };
    assert_eq!(draft.title, "Uses {braces}");
}

#[test]
fn rendered_spec_carries_checkable_annotations() {
    let DiscoveryOutcome::Spec(draft) = parse_discovery(spec_response()) else {
        panic!("expected spec");
    };
    let text = render_spec(&draft, "buatkan toko online");

    assert!(text.contains("[verify-diff: src/lib.rs]"));
    assert!(text.contains("[verify: cargo test]"));
    assert!(text.contains("# Intent"));
    assert!(text.contains("buatkan toko online"));
}

#[test]
fn rendered_spec_reparses_into_the_pipeline() {
    // The generated file must be consumable by the existing verifier.
    let DiscoveryOutcome::Spec(draft) = parse_discovery(spec_response()) else {
        panic!("expected spec");
    };
    let file = to_spec_file(&draft, "intent").expect("parses");

    assert_eq!(file.frontmatter.id, "toko-online");
    assert_eq!(file.frontmatter.title, "Toko Online Sederhana");

    let criteria = ratchet_spec::SpecExtractor::acceptance_criteria(&file);
    assert_eq!(criteria.len(), 2);
    assert!(criteria[0].verification.is_some());
    assert!(criteria[1].verification.is_some());
}

#[test]
fn braces_in_json_do_not_confuse_the_renderer() {
    let draft = SpecDraft {
        id: "x".to_string(),
        title: "Quotes \"inside\"".to_string(),
        goals: vec![],
        non_goals: vec![],
        acceptance_criteria: vec![],
        constraints: vec![],
    };
    let text = render_spec(&draft, "");
    // The title is embedded in YAML frontmatter; embedded quotes would break it.
    assert!(text.contains("title: \"Quotes 'inside'\""));
}

#[test]
fn transcript_is_included_in_the_prompt() {
    let transcript = vec![Exchange {
        question: "Fisik atau digital?".to_string(),
        answer: "fisik".to_string(),
    }];
    let prompt = ratchet_core::discovery::discovery_prompt("toko online", &transcript, 2);

    assert!(prompt.contains("toko online"));
    assert!(prompt.contains("Fisik atau digital?"));
    assert!(prompt.contains("fisik"));
}

#[test]
fn later_rounds_nudge_the_model_to_stop_asking() {
    let early = ratchet_core::discovery::discovery_prompt("x", &[], 1);
    let late = ratchet_core::discovery::discovery_prompt("x", &[], 4);
    assert!(!early.contains("propose the spec now"));
    assert!(late.contains("propose the spec now"));
}
