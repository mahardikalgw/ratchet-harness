use anyhow::Result;
use dialoguer::{Password, theme::ColorfulTheme};
use ratchet_core::ProjectConfig;
use std::io::IsTerminal;
use std::path::Path;

use crate::secrets::{delete_secret, describe_source, store_secret};

#[allow(clippy::too_many_arguments)]
pub async fn add(
    project_dir: &Path,
    name: &str,
    kind: &str,
    key_env: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    via: Option<String>,
    default: bool,
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

    // Make the project immediately runnable: routing must point somewhere
    // valid, otherwise the first `ratchet run` fails for a reason the user
    // cannot see. Selecting the first provider as the default removes the
    // need to hand-edit ratchet.toml.
    // A provider is a useful default only if it exists *and* has a credential.
    let current_default_usable = match &config.routing.default {
        Some(current) => match config.providers.get(current) {
            Some(settings) => {
                settings.api_key_env.is_none()
                    || crate::secrets::resolve_secret(current, settings.api_key_env.as_deref())
                        .is_some()
            }
            None => false,
        },
        None => false,
    };
    let this_provider_usable = config
        .providers
        .get(name)
        .map(|s| {
            s.api_key_env.is_none()
                || crate::secrets::resolve_secret(name, s.api_key_env.as_deref()).is_some()
        })
        .unwrap_or(false);

    let selected_default = default || !current_default_usable;
    let stale_default = !current_default_usable;

    if !stale_default && !default && !this_provider_usable {
        println!(
            "   note: '{name}' has no credential, so routing still points at \
             '{}'",
            config.routing.default.as_deref().unwrap_or("-")
        );
    }

    if selected_default {
        config.routing.default = Some(name.to_string());
    }
    if config.routing.planning_tasks.is_none() || stale_default {
        config.routing.planning_tasks = Some(name.to_string());
    }

    config.save(&config_path)?;

    println!("✅ Added provider '{name}' ({kind})");
    if selected_default {
        println!("   routing.default = {name}");
    }
    println!();
    println!("Next:");
    // `key_env` was moved into the settings, so re-read it from there.
    let has_env = config
        .providers
        .get(name)
        .and_then(|p| p.api_key_env.as_deref())
        .is_some();
    if has_env {
        println!("  ratchet provider test {name}     # verify the credential works");
    } else {
        println!("  ratchet provider login {name}    # store the credential");
        println!("  ratchet provider test {name}     # verify it works");
    }

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
        // Promote a surviving provider instead of leaving a hole.
        let fallback = config.providers.keys().next().cloned();
        let mut cleared = Vec::new();
        if config.routing.default.as_deref() == Some(name) {
            config.routing.default = fallback.clone();
            cleared.push("default");
        }
        if config.routing.planning_tasks.as_deref() == Some(name) {
            config.routing.planning_tasks = fallback.clone();
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
