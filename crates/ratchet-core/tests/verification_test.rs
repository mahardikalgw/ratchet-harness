use ratchet_core::verification::{
    CommandOutcome, CriterionStatus, VerificationEngine, VerificationEvidence,
};
use ratchet_spec::schema::{AcceptanceCriterion, SpecSchema, VerificationStep};
use std::collections::HashMap;

fn schema(criteria: Vec<AcceptanceCriterion>) -> SpecSchema {
    SpecSchema {
        id: "test".to_string(),
        title: "Test".to_string(),
        acceptance_criteria: criteria,
        ..Default::default()
    }
}

fn test_criterion(id: &str, command: &str, expected: &str) -> AcceptanceCriterion {
    AcceptanceCriterion {
        id: id.to_string(),
        description: format!("criterion {id}"),
        verification: Some(VerificationStep::Test {
            command: command.to_string(),
            expected: expected.to_string(),
        }),
        must: true,
    }
}

fn outcome(passed: bool, output: &str) -> CommandOutcome {
    CommandOutcome {
        passed,
        output: output.to_string(),
    }
}

#[test]
fn declared_test_command_passing_marks_criterion_passed() {
    let engine = VerificationEngine::new();
    let mut command_results = HashMap::new();
    command_results.insert(
        "cargo test auth".to_string(),
        outcome(true, "test result: ok. 3 passed"),
    );

    let evidence = VerificationEvidence {
        command_results,
        ..Default::default()
    };

    let report = engine
        .verify_spec_conformance(
            &schema(vec![test_criterion("AC-1", "cargo test auth", "test result: ok")]),
            &evidence,
        )
        .unwrap();

    assert_eq!(report.criterion_results[0].status, CriterionStatus::Passed);
    assert!(report.overall_passed);
}

#[test]
fn declared_test_command_failing_marks_criterion_failed() {
    let engine = VerificationEngine::new();
    let mut command_results = HashMap::new();
    command_results.insert(
        "cargo test auth".to_string(),
        outcome(false, "test result: FAILED. 1 failed"),
    );

    let evidence = VerificationEvidence {
        command_results,
        ..Default::default()
    };

    let report = engine
        .verify_spec_conformance(
            &schema(vec![test_criterion("AC-1", "cargo test auth", "test result: ok")]),
            &evidence,
        )
        .unwrap();

    assert_eq!(report.criterion_results[0].status, CriterionStatus::Failed);
    assert_eq!(report.auto_failed, 1);
    assert!(!report.overall_passed);
}

#[test]
fn doctest_section_does_not_mask_real_tests() {
    let engine = VerificationEngine::new();
    let mut command_results = HashMap::new();
    // Real cargo output: unit tests ran, doctests did not.
    command_results.insert(
        "cargo test".to_string(),
        outcome(
            true,
            "running 1 test\ntest tests::it_works ... ok\n\n\
             test result: ok. 1 passed; 0 failed\n\n\
             Doc-tests mylib\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed",
        ),
    );

    let evidence = VerificationEvidence {
        command_results,
        ..Default::default()
    };

    let report = engine
        .verify_spec_conformance(
            &schema(vec![test_criterion("AC-1", "cargo test", "test result: ok")]),
            &evidence,
        )
        .unwrap();

    assert_eq!(report.criterion_results[0].status, CriterionStatus::Passed);
}

#[test]
fn a_command_that_ran_no_tests_is_manual_not_verified() {
    let engine = VerificationEngine::new();
    let mut command_results = HashMap::new();
    // `cargo test` on a crate with no tests exits 0 and prints this.
    command_results.insert(
        "cargo test".to_string(),
        outcome(true, "running 0 tests\n\ntest result: ok. 0 passed; 0 failed"),
    );

    let evidence = VerificationEvidence {
        command_results,
        ..Default::default()
    };

    let report = engine
        .verify_spec_conformance(
            &schema(vec![test_criterion("AC-1", "cargo test", "test result: ok")]),
            &evidence,
        )
        .unwrap();

    assert_eq!(report.criterion_results[0].status, CriterionStatus::Manual);
    assert!(report.criterion_results[0].note.contains("ran no tests"));
    // Nothing failed, but nothing was verified either.
    assert_eq!(report.auto_passed, 0);
    assert_eq!(report.manual, 1);
}

#[test]
fn lint_command_that_actually_ran_can_pass() {
    let engine = VerificationEngine::new();
    let mut command_results = HashMap::new();
    command_results.insert(
        "cargo clippy".to_string(),
        outcome(true, "Finished dev profile"),
    );

    let criterion = AcceptanceCriterion {
        id: "AC-1".to_string(),
        description: "clippy clean".to_string(),
        verification: Some(VerificationStep::Lint {
            tool: "cargo clippy".to_string(),
            must_pass: true,
        }),
        must: true,
    };

    let evidence = VerificationEvidence {
        command_results,
        ..Default::default()
    };

    let report = engine
        .verify_spec_conformance(&schema(vec![criterion]), &evidence)
        .unwrap();
    assert_eq!(report.criterion_results[0].status, CriterionStatus::Passed);
}

#[test]
fn lint_command_that_never_ran_is_manual_not_passed() {
    let engine = VerificationEngine::new();
    let criterion = AcceptanceCriterion {
        id: "AC-1".to_string(),
        description: "clippy clean".to_string(),
        verification: Some(VerificationStep::Lint {
            tool: "cargo clippy".to_string(),
            must_pass: true,
        }),
        must: true,
    };

    // Even though tests passed, an un-run lint must not be reported as passing.
    let evidence = VerificationEvidence {
        test_output: "test result: ok".to_string(),
        test_passed: true,
        ..Default::default()
    };

    let report = engine
        .verify_spec_conformance(&schema(vec![criterion]), &evidence)
        .unwrap();
    assert_eq!(report.criterion_results[0].status, CriterionStatus::Manual);
}

#[test]
fn lint_command_that_failed_marks_criterion_failed() {
    let engine = VerificationEngine::new();
    let mut command_results = HashMap::new();
    command_results.insert("cargo clippy".to_string(), outcome(false, "error: unused import"));

    let criterion = AcceptanceCriterion {
        id: "AC-1".to_string(),
        description: "clippy clean".to_string(),
        verification: Some(VerificationStep::Lint {
            tool: "cargo clippy".to_string(),
            must_pass: true,
        }),
        must: true,
    };

    let evidence = VerificationEvidence {
        command_results,
        ..Default::default()
    };

    let report = engine
        .verify_spec_conformance(&schema(vec![criterion]), &evidence)
        .unwrap();
    assert_eq!(report.criterion_results[0].status, CriterionStatus::Failed);
}

#[test]
fn criteria_without_verification_are_manual_not_passed() {
    let engine = VerificationEngine::new();
    let criterion = AcceptanceCriterion {
        id: "AC-1".to_string(),
        description: "looks right".to_string(),
        verification: None,
        must: true,
    };
    let evidence = VerificationEvidence {
        test_output: "test result: ok".to_string(),
        test_passed: true,
        ..Default::default()
    };

    let report = engine
        .verify_spec_conformance(&schema(vec![criterion]), &evidence)
        .unwrap();

    assert_eq!(report.criterion_results[0].status, CriterionStatus::Manual);
    assert_eq!(report.manual, 1);
    assert_eq!(report.auto_passed, 0);
    assert!(report.overall_passed);
}

#[test]
fn diff_criterion_checks_changed_files() {
    let engine = VerificationEngine::new();
    let criterion = AcceptanceCriterion {
        id: "AC-1".to_string(),
        description: "touches billing".to_string(),
        verification: Some(VerificationStep::Diff {
            pattern: "src/billing".to_string(),
        }),
        must: true,
    };
    let evidence = VerificationEvidence {
        changed_files: vec!["src/billing/invoice.rs".to_string()],
        ..Default::default()
    };

    let report = engine
        .verify_spec_conformance(&schema(vec![criterion]), &evidence)
        .unwrap();

    assert_eq!(report.criterion_results[0].status, CriterionStatus::Passed);
}

#[test]
fn diff_criterion_without_matching_file_fails() {
    let engine = VerificationEngine::new();
    let criterion = AcceptanceCriterion {
        id: "AC-1".to_string(),
        description: "touches billing".to_string(),
        verification: Some(VerificationStep::Diff {
            pattern: "src/billing".to_string(),
        }),
        must: true,
    };
    let evidence = VerificationEvidence {
        changed_files: vec!["docs/readme.md".to_string()],
        ..Default::default()
    };

    let report = engine
        .verify_spec_conformance(&schema(vec![criterion]), &evidence)
        .unwrap();

    assert_eq!(report.criterion_results[0].status, CriterionStatus::Failed);
}

#[test]
fn falls_back_to_project_test_output_when_command_not_run() {
    let engine = VerificationEngine::new();
    let evidence = VerificationEvidence {
        test_output: "running 3 tests\ntest result: ok. 3 passed".to_string(),
        test_passed: true,
        ..Default::default()
    };

    let report = engine
        .verify_spec_conformance(
            &schema(vec![test_criterion("AC-1", "cargo test", "test result: ok")]),
            &evidence,
        )
        .unwrap();

    assert_eq!(report.criterion_results[0].status, CriterionStatus::Passed);
    assert!(report.criterion_results[0].note.contains("project test suite"));
}

#[test]
fn failing_project_test_suite_fails_overall_even_without_criteria() {
    let engine = VerificationEngine::new();
    let evidence = VerificationEvidence {
        test_output: "test result: FAILED".to_string(),
        test_passed: false,
        ..Default::default()
    };

    let report = engine
        .verify_spec_conformance(&schema(vec![]), &evidence)
        .unwrap();

    assert!(!report.overall_passed);
}
