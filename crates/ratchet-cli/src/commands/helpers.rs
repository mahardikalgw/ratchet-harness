use anyhow::Result;
use ratchet_core::{AgentHarness, ProjectConfig};
use ratchet_providers::{
    adapters::{create_provider, ProviderConfig},
    ModelProvider,
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::approval::CliApprovalHandler;
use crate::secrets::resolve_secret;

/// A map of provider name to a constructed provider instance.
pub type ProviderMap = HashMap<String, Arc<dyn ModelProvider>>;

/// Load configured providers, skipping any without usable credentials.
///
/// Credentials are resolved from the environment first, then the OS keychain.
pub fn load_providers(config: &ProjectConfig) -> ProviderMap {
    let mut providers: ProviderMap = HashMap::new();

    for (name, settings) in &config.providers {
        let api_key = resolve_secret(name, settings.api_key_env.as_deref());

        // Warn only when the provider declares it needs a key but none was found.
        if api_key.is_none() && settings.api_key_env.is_some() {
            tracing::warn!(provider = %name, "provider skipped: no credential found");
            eprintln!(
                "⚠️  Skipping provider '{name}': no credential in ${} or keychain \
                 (run `ratchet provider login {name}`)",
                settings.api_key_env.as_deref().unwrap_or("-")
            );
            continue;
        }

        match create_provider(
            &settings.kind,
            ProviderConfig {
                api_key,
                base_url: settings.base_url.clone(),
                model: settings.model.clone(),
                timeout_secs: Some(120),
                extra_headers: settings.extra_headers.clone(),
            },
        ) {
            Ok(provider) => {
                providers.insert(name.clone(), provider);
            }
            Err(e) => {
                tracing::warn!(provider = %name, error = %e, "provider skipped");
                eprintln!("⚠️  Skipping provider '{name}': {e}");
            }
        }
    }

    providers
}

/// Load the project config and providers together.
pub fn load_project(project_dir: &Path) -> Result<(ProjectConfig, ProviderMap)> {
    let config = ProjectConfig::load(&project_dir.join("ratchet.toml"))?;
    let providers = load_providers(&config);
    Ok((config, providers))
}

/// Build a ready-to-run harness wired with interactive approvals.
pub async fn build_harness(project_dir: &Path) -> Result<AgentHarness> {
    let (config, providers) = load_project(project_dir)?;
    let harness = AgentHarness::new(config, providers)
        .await?
        .with_approval(Arc::new(CliApprovalHandler));
    Ok(harness)
}
