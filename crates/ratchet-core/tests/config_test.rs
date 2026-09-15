use ratchet_core::ProjectConfig;

#[test]
fn scaffolded_config_round_trips() {
    let config = ProjectConfig::scaffold("demo");
    let text = toml::to_string_pretty(&config).unwrap();
    let back: ProjectConfig = toml::from_str(&text).unwrap();
    assert_eq!(config, back);
}

#[test]
fn empty_sections_are_not_serialised() {
    // Writing `[delegation.roles]` and friends when they are empty is noise,
    // and it makes it easy to accidentally append a duplicate table header.
    let config = ProjectConfig::scaffold("demo");
    let text = toml::to_string_pretty(&config).unwrap();

    assert!(!text.contains("[delegation]"), "unexpected empty delegation table:\n{text}");
    assert!(!text.contains("[mcp.servers]"), "unexpected empty mcp table:\n{text}");
    assert!(!text.contains("plugins = []"), "unexpected empty plugins key:\n{text}");
}

#[test]
fn configured_sections_are_serialised() {
    let mut config = ProjectConfig::scaffold("demo");
    config.delegation.review = true;
    config
        .delegation
        .roles
        .insert("reviewer".to_string(), "claude".to_string());

    let text = toml::to_string_pretty(&config).unwrap();
    assert!(text.contains("[delegation]"));
    assert!(text.contains("review = true"));
    assert!(text.contains("[delegation.roles]"));

    let back: ProjectConfig = toml::from_str(&text).unwrap();
    assert_eq!(back.delegation, config.delegation);
}

#[test]
fn appended_delegation_table_parses() {
    // Regression: the generated file must not already contain `[delegation]`,
    // so appending one is valid TOML.
    let base = toml::to_string_pretty(&ProjectConfig::scaffold("demo")).unwrap();
    let with_delegation = format!(
        "{base}\n[delegation]\nreview = true\nmax_review_rounds = 3\n"
    );

    let parsed: ProjectConfig = toml::from_str(&with_delegation).unwrap();
    assert!(parsed.delegation.review);
    assert_eq!(parsed.delegation.max_review_rounds, 3);
}

#[test]
fn disabled_review_is_treated_as_default() {
    let config = ProjectConfig::default();
    assert!(config.delegation.is_default());

    let mut enabled = ProjectConfig::default();
    enabled.delegation.review = true;
    assert!(!enabled.delegation.is_default());
}
