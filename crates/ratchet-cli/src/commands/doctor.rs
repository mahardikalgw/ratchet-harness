use anyhow::Result;
use ratchet_core::ProjectConfig;
use std::path::Path;

use crate::secrets::describe_source;

/// One command that answers "why isn't this working?".
///
/// Every check prints either a concrete next action or nothing at all, so the
/// output is a to-do list rather than a wall of status lines.
pub async fn run(project_dir: &Path, online: bool) -> Result<()> {
    let mut problems = 0usize;
    let mut hints: Vec<String> = Vec::new();

    println!("Ratchet doctor\n");

    // --- config ---------------------------------------------------------
    let config_path = project_dir.join("ratchet.toml");
    if !config_path.exists() {
        println!("✗ no ratchet.toml here — run `ratchet init <name>` first");
        std::process::exit(1);
    }
    println!("✓ ratchet.toml found");

    let config = ProjectConfig::load(&config_path)?;

    // --- project layout -------------------------------------------------
    let spec_dir = project_dir.join(".ratchet").join("spec");
    if spec_dir.exists() {
        let count = std::fs::read_dir(&spec_dir)
            .map(|d| d.filter_map(|e| e.ok()).count())
            .unwrap_or(0);
        if count == 0 {
            println!("• no specs yet — create one with `ratchet spec new <id>`");
        } else {
            println!("✓ {count} spec(s) in .ratchet/spec");
        }
    } else {
        println!("• .ratchet/spec is missing — `ratchet init` recreates it");
    }

    if project_dir.join(".git").exists() {
        println!("✓ git repository detected (needed for diff/review)");
    } else {
        println!("✗ not a git repository — `ratchet review` and verify-diff need git");
        hints.push("git init".to_string());
        problems += 1;
    }

    // --- providers ------------------------------------------------------
    if config.providers.is_empty() {
        println!("✗ no providers configured");
        hints.push("ratchet provider add <name> --kind <kind> --key-env <ENV_VAR>".to_string());
        problems += 1;
    } else {
        println!("\nProviders:");
        for (name, settings) in &config.providers {
            let source = describe_source(name, settings.api_key_env.as_deref());
            let usable = source != "missing" || settings.api_key_env.is_none();

            if usable {
                println!(
                    "  ✓ {name} ({}, model={}, credential={source})",
                    settings.kind,
                    settings.model.as_deref().unwrap_or("default")
                );
            } else {
                println!(
                    "  ✗ {name} ({}) — no credential in ${} or the keychain",
                    settings.kind,
                    settings.api_key_env.as_deref().unwrap_or("-")
                );
                hints.push(format!("ratchet provider login {name}"));
                problems += 1;
            }
        }
    }

    // --- routing --------------------------------------------------------
    match &config.routing.default {
        Some(default) if config.providers.contains_key(default) => {
            println!("\n✓ routing.default = {default}");
        }
        Some(default) => {
            println!("\n✗ routing.default points at '{default}', which is not configured");
            hints.push(
                "ratchet provider add ... (re-add it, or remove and re-add a provider)".to_string(),
            );
            problems += 1;
        }
        None => {
            println!(
                "\n✗ routing.default is unset — `ratchet run` will not know which model to use"
            );
            hints.push("add a provider to set it automatically".to_string());
            problems += 1;
        }
    }

    // --- live connectivity (optional) -----------------------------------
    if online {
        println!("\nConnectivity:");
        let (_, providers) = super::helpers::load_project(project_dir)?;
        if providers.is_empty() {
            println!("  (skipped — no usable providers)");
        }
        for (name, provider) in &providers {
            let request = ratchet_providers::ChatRequest {
                messages: vec![ratchet_providers::types::Message {
                    role: ratchet_providers::types::MessageRole::User,
                    content: "ping".to_string(),
                    tool_calls: None,
                    tool_results: None,
                }],
                tools: vec![],
                temperature: Some(0.0),
                max_tokens: Some(1),
                model: None,
            };
            match provider.complete(request).await {
                Ok(_) => println!("  ✓ {name} reachable"),
                Err(e) => {
                    println!("  ✗ {name}: {e}");
                    hints.push(format!("ratchet provider test {name}"));
                    problems += 1;
                }
            }
        }
    }

    // --- verdict --------------------------------------------------------
    println!();
    if problems == 0 {
        println!("No problems found. Try: ratchet spec new demo && ratchet plan demo");
    } else {
        println!("{problems} problem(s) found. Fix these:");
        for hint in hints {
            println!("  → {hint}");
        }
        std::process::exit(1);
    }

    Ok(())
}
