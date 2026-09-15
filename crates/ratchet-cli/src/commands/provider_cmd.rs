use anyhow::Result;
use dialoguer::{Password, theme::ColorfulTheme};
use ratchet_core::ProjectConfig;
use std::io::IsTerminal;
use std::path::Path;

use crate::secrets::{delete_secret, describe_source, store_secret};

pub async fn add(
    project_dir: &Path,
    name: &str,
    kind: &str,
    key_env: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    via: Option<String>,
) -> Result<()> {
    let config_path = project_dir.join("ratchet.toml");
    let mut config = if config_path.exists() {
        ProjectConfig::load(&config_path)?
    } else {
        ProjectConfig::scaffold("unnamed")
    };

    let (final_kind, final_base_url, final_model) = match via.as_deref() {
        Some("openrouter") => (
            "openai_compatible".to_string(),
            Some("https://openrouter.ai/api/v1".to_string()),
            model.or_else(|| Some(format!("{}/{}", kind, name))),
        ),
        _ => (kind.to_string(), base_url, model),
    };

    config.providers.insert(
        name.to_string(),
        ratchet_core::config::ProviderSettings {
            kind: final_kind,
            api_key_env: key_env,
            base_url: final_base_url,
            model: final_model,
            extra_headers: Vec::new(),
        },
    );

    config.save(&config_path)?;
    println!("✅ Added provider '{name}' to {:?}", config_path);
    println!("   Store its credential with: ratchet provider login {name}");

    Ok(())
}

/// Prompt for and store a provider credential in the OS keychain.
pub async fn login(project_dir: &Path, name: &str) -> Result<()> {
    let config = ProjectConfig::load(&project_dir.join("ratchet.toml"))?;
    if !config.providers.contains_key(name) {
        anyhow::bail!(
            "provider '{name}' is not configured — add it first with \
             `ratchet provider add {name} --kind <kind>`"
        );
    }

    if !std::io::stdin().is_terminal() {
        anyhow::bail!("`provider login` needs an interactive terminal");
    }

    let secret = Password::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("API key for '{name}'"))
        .interact()?;

    if secret.trim().is_empty() {
        anyhow::bail!("empty credential, nothing stored");
    }

    store_secret(name, secret.trim())?;
    println!("✅ Stored credential for '{name}' in the OS keychain");
    Ok(())
}

pub async fn logout(project_dir: &Path, name: &str) -> Result<()> {
    let _ = ProjectConfig::load(&project_dir.join("ratchet.toml"))?;
    delete_secret(name)?;
    println!("✅ Removed stored credential for '{name}'");
    Ok(())
}

pub async fn list(project_dir: &Path) -> Result<()> {
    let config = ProjectConfig::load(&project_dir.join("ratchet.toml"))?;

    println!(
        "{:<20} {:<20} {:<30} {:<12} SOURCE",
        "NAME", "KIND", "MODEL", "CREDENTIAL"
    );
    println!("{}", "-".repeat(100));

    for (name, settings) in &config.providers {
        let source = describe_source(name, settings.api_key_env.as_deref());
        let icon = match source {
            "environment" => "env",
            "keychain" => "keychain",
            _ => "✗ missing",
        };
        println!(
            "{:<20} {:<20} {:<30} {:<12} {}",
            name,
            settings.kind,
            settings.model.as_deref().unwrap_or("default"),
            icon,
            settings.api_key_env.as_deref().unwrap_or("-")
        );
    }

    println!("\nRouting:");
    println!("  Default:  {:?}", config.routing.default);
    println!("  Planning: {:?}", config.routing.planning_tasks);
    println!("  Policy:   {:?}", config.routing.policy);

    if !config.mcp.servers.is_empty() {
        println!("\nMCP servers:");
        for (name, server) in &config.mcp.servers {
            println!("  {name}: {} {}", server.command, server.args.join(" "));
        }
    }

    Ok(())
}

pub async fn remove(project_dir: &Path, name: &str) -> Result<()> {
    let config_path = project_dir.join("ratchet.toml");
    let mut config = ProjectConfig::load(&config_path)?;

    if config.providers.remove(name).is_some() {
        // Drop routing references to the provider we just removed, otherwise
        // the config silently points at something that no longer exists.
        let mut cleared = Vec::new();
        if config.routing.default.as_deref() == Some(name) {
            config.routing.default = None;
            cleared.push("default");
        }
        if config.routing.planning_tasks.as_deref() == Some(name) {
            config.routing.planning_tasks = None;
            cleared.push("planning_tasks");
        }
        config
            .delegation
            .roles
            .retain(|_, provider| provider != name);

        if !cleared.is_empty() {
            println!("   cleared stale routing: {}", cleared.join(", "));
        }

        config.save(&config_path)?;
        // Best-effort: also drop any stored credential.
        let _ = delete_secret(name);
        println!("✅ Removed provider '{name}'");
    } else {
        anyhow::bail!("provider '{name}' not found");
    }

    Ok(())
}
