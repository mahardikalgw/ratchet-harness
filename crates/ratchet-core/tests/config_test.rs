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

    assert!(
        !text.contains("[delegation]"),
        "unexpected empty delegation table:\n{text}"
    );
    assert!(
        !text.contains("[mcp.servers]"),
        "unexpected empty mcp table:\n{text}"
    );
    assert!(
        !text.contains("plugins = []"),
        "unexpected empty plugins key:\n{text}"
    );
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
    let with_delegation = format!("{base}\n[delegation]\nreview = true\nmax_review_rounds = 3\n");

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

#[test]
fn scaffold_invents_no_providers() {
    // Regression: `init` used to seed claude/deepseek entries. Routing then
    // pointed at providers with no credentials, so the first `run` failed for
    // a reason the user could not see.
    let config = ProjectConfig::scaffold("demo");
    assert!(
        config.providers.is_empty(),
        "scaffold must not invent providers: {:?}",
        config.providers.keys().collect::<Vec<_>>()
    );
    assert!(config.routing.default.is_none());
    assert!(config.routing.planning_tasks.is_none());
}

#[test]
fn scaffolded_file_is_minimal() {
    let text = toml::to_string_pretty(&ProjectConfig::scaffold("demo")).unwrap();
    assert!(
        !text.contains("[providers"),
        "unexpected providers table:\n{text}"
    );
    assert!(
        !text.contains("[routing]"),
        "unexpected routing table:\n{text}"
    );
    assert!(text.contains("[project]"));
}

#[test]
fn routing_is_set_once_a_provider_is_added() {
    // Mirrors what `ratchet provider add` does, minus the credential check.
    let mut config = ProjectConfig::scaffold("demo");
    config.providers.insert(
        "mock".to_string(),
        ratchet_core::config::ProviderSettings {
            kind: "mimo".to_string(),
            api_key_env: Some("MOCK_KEY".to_string()),
            base_url: None,
            model: Some("m".to_string()),
            extra_headers: Vec::new(),
        },
    );
    config.routing.default = Some("mock".to_string());

    let text = toml::to_string_pretty(&config).unwrap();
    let back: ProjectConfig = toml::from_str(&text).unwrap();
    assert_eq!(back.routing.default.as_deref(), Some("mock"));
}
