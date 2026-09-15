use anyhow::Result;
use ratchet_core::ProjectConfig;
use ratchet_providers::{adapters::{create_provider, ProviderConfig}, traits::*, types::*};
use std::path::Path;

use crate::secrets::resolve_secret;

/// Validate a provider's credentials and model with a minimal live request.
///
/// This is the check that catches a wrong base URL, an expired key, or a
/// misspelled model name *before* a full agentic run fails halfway through.
pub async fn run(project_dir: &Path, name: Option<String>) -> Result<()> {
    let config = ProjectConfig::load(&project_dir.join("ratchet.toml"))?;

    let targets: Vec<(String, _)> = match name {
        Some(n) => {
            let settings = config
                .providers
                .get(&n)
                .ok_or_else(|| anyhow::anyhow!("provider '{n}' is not configured"))?;
            vec![(n, settings.clone())]
        }
        None => config
            .providers
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    };

    if targets.is_empty() {
        anyhow::bail!("no providers configured — see `ratchet provider add`");
    }

    let mut failures = 0usize;

    for (name, settings) in targets {
        let credential = resolve_secret(&name, settings.api_key_env.as_deref());

        if credential.is_none() && settings.api_key_env.is_some() {
            println!("✗ {name}: no credential found (env ${} or keychain)",
                settings.api_key_env.as_deref().unwrap_or("-"));
            failures += 1;
            continue;
        }

        let provider = match create_provider(
            &settings.kind,
            ProviderConfig {
                api_key: credential,
                base_url: settings.base_url.clone(),
                model: settings.model.clone(),
                timeout_secs: Some(30),
                extra_headers: settings.extra_headers.clone(),
            },
        ) {
            Ok(p) => p,
            Err(e) => {
                println!("✗ {name}: could not construct provider: {e}");
                failures += 1;
                continue;
            }
        };

        // Minimal round-trip: one token, no tools.
        let request = ChatRequest {
            messages: vec![Message {
                role: MessageRole::User,
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
            Ok(response) => {
                println!(
                    "✅ {name}: ok (kind={}, model={}, {} in / {} out)",
                    settings.kind,
                    response.model,
                    response.usage.input_tokens,
                    response.usage.output_tokens
                );
            }
            Err(e) => {
                println!("✗ {name}: {e}");
                if let Some(hint) = hint_for(&e) {
                    println!("   hint: {hint}");
                }
                failures += 1;
            }
        }
    }

    if failures > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn hint_for(error: &ratchet_providers::ProviderError) -> Option<&'static str> {
    use ratchet_providers::ProviderError as E;
    match error {
        E::Auth { .. } => Some(
            "the endpoint rejected the credential — check that the key is valid, \
             not expired, and belongs to this provider (and re-run `ratchet provider login`)",
        ),
        E::Http(e) if e.is_connect() => Some(
            "could not reach the host — check `base_url`; a guessed host will not resolve",
        ),
        E::Api { status: Some(404), .. } => Some(
            "404 usually means the base URL is wrong (it should include the `/v1` suffix \
             for OpenAI-compatible providers)",
        ),
        E::Api { status: Some(400), .. } => Some(
            "400 often means the model name is wrong — set `model` explicitly",
        ),
        _ => None,
    }
}
