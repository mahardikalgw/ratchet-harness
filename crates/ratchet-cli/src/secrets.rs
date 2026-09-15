use keyring::Entry;

/// Service name under which Ratchet stores provider credentials.
const SERVICE: &str = "ratchet";

/// Resolve a provider credential, preferring the environment then falling back
/// to the OS keychain.
///
/// Ordering matters: env vars make CI and ephemeral shells easy, while the
/// keychain lets a developer keep a key on disk-bound machines without
/// exporting it in every shell.
pub fn resolve_secret(provider: &str, env_var: Option<&str>) -> Option<String> {
    if let Some(var) = env_var {
        if let Ok(value) = std::env::var(var) {
            let value = value.trim().to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }

    match Entry::new(SERVICE, provider) {
        Ok(entry) => match entry.get_password() {
            Ok(secret) if !secret.trim().is_empty() => Some(secret),
            _ => None,
        },
        Err(_) => None,
    }
}

/// Store a credential in the OS keychain.
pub fn store_secret(provider: &str, secret: &str) -> anyhow::Result<()> {
    let entry = Entry::new(SERVICE, provider)
        .map_err(|e| anyhow::anyhow!("keychain unavailable: {e}"))?;
    entry
        .set_password(secret)
        .map_err(|e| anyhow::anyhow!("failed to store credential: {e}"))?;
    Ok(())
}

/// Remove a stored credential from the OS keychain.
pub fn delete_secret(provider: &str) -> anyhow::Result<()> {
    let entry = Entry::new(SERVICE, provider)
        .map_err(|e| anyhow::anyhow!("keychain unavailable: {e}"))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(anyhow::anyhow!("failed to delete credential: {e}")),
    }
}

/// Where a credential came from, for user-facing diagnostics.
pub fn describe_source(provider: &str, env_var: Option<&str>) -> &'static str {
    if let Some(var) = env_var {
        if std::env::var(var).map(|v| !v.trim().is_empty()).unwrap_or(false) {
            return "environment";
        }
    }
    match Entry::new(SERVICE, provider).and_then(|e| e.get_password()) {
        Ok(_) => "keychain",
        Err(_) => "missing",
    }
}
