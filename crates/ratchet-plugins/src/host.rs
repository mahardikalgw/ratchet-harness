use crate::{
    error::{PluginError, PluginResult},
    protocol::*,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A plugin bound to its working directory.
#[derive(Debug, Clone)]
pub struct PluginInvocation {
    pub manifest: PluginManifest,
    pub cwd: PathBuf,
}

/// Runs plugins declared in the project configuration.
#[derive(Debug, Clone, Default)]
pub struct PluginHost {
    plugins: Vec<PluginInvocation>,
}

impl PluginHost {
    pub fn new(plugins: Vec<PluginInvocation>) -> Self {
        Self { plugins }
    }

    pub fn from_manifests(manifests: Vec<PluginManifest>, cwd: impl Into<PathBuf>) -> Self {
        let cwd = cwd.into();
        Self {
            plugins: manifests
                .into_iter()
                .map(|manifest| PluginInvocation {
                    manifest,
                    cwd: cwd.clone(),
                })
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn gate_plugins(&self) -> impl Iterator<Item = &PluginInvocation> {
        self.plugins
            .iter()
            .filter(|p| p.manifest.kind == PluginKind::Gate)
    }

    pub fn tool_plugins(&self) -> impl Iterator<Item = &PluginInvocation> {
        self.plugins
            .iter()
            .filter(|p| p.manifest.kind == PluginKind::Tool)
    }

    /// Ask a tool plugin which tools it provides.
    pub async fn describe(&self, plugin_name: &str) -> PluginResult<Vec<ToolDescriptor>> {
        let invocation = self
            .plugin(plugin_name)
            .ok_or_else(|| PluginError::NotFound {
                plugin: plugin_name.to_string(),
            })?;

        let value = run_plugin(invocation, &PluginRequest::Describe).await?;
        let response: DescribeResponse =
            serde_json::from_value(value).map_err(|e| PluginError::InvalidResponse {
                plugin: invocation.manifest.name.clone(),
                message: e.to_string(),
            })?;
        Ok(response.tools)
    }

    /// Invoke a tool provided by a plugin.
    pub async fn call_tool(
        &self,
        plugin_name: &str,
        tool: &str,
        arguments: serde_json::Value,
    ) -> PluginResult<ToolCallResponse> {
        let invocation = self
            .plugin(plugin_name)
            .ok_or_else(|| PluginError::NotFound {
                plugin: plugin_name.to_string(),
            })?;

        let value = run_plugin(
            invocation,
            &PluginRequest::ToolCall(ToolCallRequest {
                name: tool.to_string(),
                arguments,
            }),
        )
        .await?;

        serde_json::from_value(value).map_err(|e| PluginError::InvalidResponse {
            plugin: invocation.manifest.name.clone(),
            message: e.to_string(),
        })
    }

    /// Run every applicable gate plugin and merge their verdicts.
    ///
    /// A gate that errors is reported as `Manual` rather than failing the run:
    /// a broken plugin should surface for human attention, not silently block
    /// or silently pass.
    pub async fn run_gates(&self, request: &GateRequest) -> Vec<(String, GateResult)> {
        let mut out = Vec::new();

        for invocation in self.gate_plugins() {
            let applicable: Vec<GateCriterion> = request
                .criteria
                .iter()
                .filter(|c| invocation.manifest.applies_to(&c.id))
                .cloned()
                .collect();

            if applicable.is_empty() {
                continue;
            }

            let mut scoped = request.clone();
            scoped.criteria = applicable;

            let name = invocation.manifest.name.clone();
            match run_plugin(invocation, &PluginRequest::Gate(scoped)).await {
                Ok(value) => match serde_json::from_value::<GateResponse>(value) {
                    Ok(response) => {
                        for result in response.results {
                            out.push((name.clone(), result));
                        }
                    }
                    Err(e) => {
                        tracing::warn!(plugin = %name, error = %e, "gate plugin returned invalid JSON");
                        for criterion in &request.criteria {
                            out.push((
                                name.clone(),
                                GateResult {
                                    criterion_id: criterion.id.clone(),
                                    status: GateStatus::Manual,
                                    note: format!("gate plugin returned invalid output: {e}"),
                                },
                            ));
                        }
                    }
                },
                Err(e) => {
                    tracing::warn!(plugin = %name, error = %e, "gate plugin failed");
                    for criterion in &request.criteria {
                        out.push((
                            name.clone(),
                            GateResult {
                                criterion_id: criterion.id.clone(),
                                status: GateStatus::Manual,
                                note: format!("gate plugin error: {e}"),
                            },
                        ));
                    }
                }
            }
        }

        out
    }

    fn plugin(&self, name: &str) -> Option<&PluginInvocation> {
        self.plugins.iter().find(|p| p.manifest.name == name)
    }
}

/// Spawn the plugin, write one request, read one response.
async fn run_plugin(
    invocation: &PluginInvocation,
    request: &PluginRequest,
) -> PluginResult<serde_json::Value> {
    use tokio::io::AsyncWriteExt;

    let manifest = &invocation.manifest;

    let mut child = tokio::process::Command::new(&manifest.command)
        .args(&manifest.args)
        .current_dir(&invocation.cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|source| PluginError::Spawn {
            plugin: manifest.name.clone(),
            source,
        })?;

    let payload = serde_json::to_vec(request)?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&payload).await?;
        // Dropping stdin signals EOF so the plugin knows the request is complete.
    }

    let timeout = Duration::from_secs(manifest.timeout_secs);
    let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(result) => result?,
        Err(_) => {
            return Err(PluginError::Timeout {
                plugin: manifest.name.clone(),
                seconds: manifest.timeout_secs,
            });
        }
    };

    if !output.status.success() {
        return Err(PluginError::Failed {
            plugin: manifest.name.clone(),
            code: output
                .status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".to_string()),
            stderr: String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(400)
                .collect(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).map_err(|e| PluginError::InvalidResponse {
        plugin: manifest.name.clone(),
        message: format!(
            "{e} (raw: {})",
            stdout.chars().take(200).collect::<String>()
        ),
    })
}

/// Convenience for tests and callers that just need a cwd-relative invocation.
pub fn invocation(manifest: PluginManifest, cwd: &Path) -> PluginInvocation {
    PluginInvocation {
        manifest,
        cwd: cwd.to_path_buf(),
    }
}
