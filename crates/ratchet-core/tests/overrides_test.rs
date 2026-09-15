use ratchet_core::RunOverrides;

fn overrides(provider: Option<&str>, model: Option<&str>) -> RunOverrides {
    RunOverrides {
        provider: provider.map(str::to_string),
        model: model.map(str::to_string),
    }
}

#[test]
fn a_run_override_beats_a_session_pin() {
    // `ratchet run --model x` inside a session pinned to something else.
    let run = overrides(Some("claude"), Some("opus"));
    let session = overrides(Some("deepseek"), Some("chat"));

    let merged = run.merged_over(&session);
    assert_eq!(merged.provider.as_deref(), Some("claude"));
    assert_eq!(merged.model.as_deref(), Some("opus"));
}

#[test]
fn a_session_pin_fills_the_gaps() {
    let run = overrides(None, Some("opus"));
    let session = overrides(Some("deepseek"), Some("chat"));

    let merged = run.merged_over(&session);
    // The run named a model; the provider comes from the session.
    assert_eq!(merged.provider.as_deref(), Some("deepseek"));
    assert_eq!(merged.model.as_deref(), Some("opus"));
}

#[test]
fn no_overrides_falls_through_to_config() {
    let merged = overrides(None, None).merged_over(&overrides(None, None));
    assert!(merged.is_empty());
    assert_eq!(merged.describe(), "from ratchet.toml");
}

#[test]
fn describe_reports_what_is_pinned() {
    assert_eq!(
        overrides(Some("claude"), None).describe(),
        "provider claude"
    );
    assert_eq!(overrides(None, Some("opus")).describe(), "model opus");
    assert_eq!(
        overrides(Some("claude"), Some("opus")).describe(),
        "provider claude, model opus"
    );
}

#[test]
fn merging_is_idempotent() {
    let session = overrides(Some("deepseek"), Some("chat"));
    let once = overrides(None, None).merged_over(&session);
    let twice = once.merged_over(&session);
    assert_eq!(once, twice);
}
