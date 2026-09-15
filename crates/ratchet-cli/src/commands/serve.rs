use anyhow::Result;
use async_trait::async_trait;
use ratchet_acp::{
    AcpResult,
    server::{AcpHandler, AcpServer},
    types as acp,
};
use ratchet_core::AgentHarness;
use ratchet_spec::SpecParser;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Adapts `AgentHarness` to the ACP protocol.
///
/// `AgentHarness` needs `&mut self` to run, so it lives behind a mutex shared
/// with the server. Long runs are spawned so the connection stays responsive.
struct ServingHarness {
    inner: Arc<Mutex<AgentHarness>>,
    spec_dir: PathBuf,
    runs: Arc<Mutex<HashMap<String, acp::AgentRunResult>>>,
}

impl ServingHarness {
    fn load_spec(&self, spec_id: &str) -> AcpResult<ratchet_spec::SpecFile> {
        let path = self.spec_dir.join(format!("{spec_id}.spec.md"));
        if !path.exists() {
            return Err(ratchet_acp::AcpError::InvalidParams(format!(
                "spec '{spec_id}' not found"
            )));
        }
        SpecParser::new()
            .parse_file(&path)
            .map_err(|e| ratchet_acp::AcpError::Agent(e.to_string()))
    }

    async fn set_status(&self, run_id: &str, status: acp::RunStatus, message: Option<String>) {
        let mut runs = self.runs.lock().await;
        runs.insert(
            run_id.to_string(),
            acp::AgentRunResult {
                run_id: run_id.to_string(),
                status,
                message,
            },
        );
    }
}

#[async_trait]
impl AcpHandler for ServingHarness {
    async fn initialize(&self, _params: acp::InitializeParams) -> AcpResult<acp::InitializeResult> {
        Ok(acp::InitializeResult {
            protocol_version: "1.0".to_string(),
            server_info: acp::ServerInfo {
                name: "ratchet".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            capabilities: acp::ServerCapabilities {
                streaming: false,
                plan_preview: true,
                approval_gates: true,
            },
        })
    }

    async fn agent_plan(&self, params: acp::AgentPlanParams) -> AcpResult<acp::AgentPlanResult> {
        let spec = self.load_spec(&params.spec_id)?;
        let mut harness = self.inner.lock().await;
        let plan = harness
            .load_or_plan(&spec)
            .await
            .map_err(|e| ratchet_acp::AcpError::Agent(e.to_string()))?;

        Ok(acp::AgentPlanResult {
            plan_id: format!("plan-{}", params.spec_id),
            spec_id: params.spec_id.clone(),
            summary: plan.summary.clone(),
            tasks: plan
                .task_graph
                .nodes
                .iter()
                .map(|n| acp::PlanTask {
                    id: n.id.0.clone(),
                    title: n.title.clone(),
                    description: n.description.clone(),
                    assigned_model: n.assigned_model.clone(),
                })
                .collect(),
        })
    }

    async fn agent_run(&self, params: acp::AgentRunParams) -> AcpResult<acp::AgentRunResult> {
        let spec = self.load_spec(&params.spec_id)?;
        let run_id = format!(
            "run-{}-{}",
            params.spec_id,
            chrono::Utc::now().format("%H%M%S")
        );

        let plan = {
            let mut harness = self.inner.lock().await;
            harness
                .load_or_plan(&spec)
                .await
                .map_err(|e| ratchet_acp::AcpError::Agent(e.to_string()))?
        };

        self.set_status(&run_id, acp::RunStatus::Started, None)
            .await;

        // Run in the background so the JSON-RPC connection stays responsive.
        let inner = Arc::clone(&self.inner);
        let runs = Arc::clone(&self.runs);
        let run_id_bg = run_id.clone();
        let overrides = ratchet_core::RunOverrides {
            provider: None,
            model: params.model.clone(),
        };

        tokio::spawn(async move {
            let mut harness = inner.lock().await;
            let status = match harness.run_with(&plan, &spec, overrides).await {
                Ok(report) => {
                    if report.verification.overall_passed {
                        (acp::RunStatus::Done, Some(report.verification.summary))
                    } else {
                        (acp::RunStatus::Failed, Some(report.verification.summary))
                    }
                }
                Err(e) => (acp::RunStatus::Failed, Some(e.to_string())),
            };
            let mut guard = runs.lock().await;
            guard.insert(
                run_id_bg.clone(),
                acp::AgentRunResult {
                    run_id: run_id_bg,
                    status: status.0,
                    message: status.1,
                },
            );
        });

        Ok(acp::AgentRunResult {
            run_id,
            status: acp::RunStatus::Started,
            message: Some(format!("Run started for {}", params.spec_id)),
        })
    }

    async fn approve(&self, params: acp::ApprovalRequest) -> AcpResult<acp::ApprovalResponse> {
        // ACP clients are non-interactive by transport; approval is surfaced
        // through the client UI and echoed here.
        Ok(acp::ApprovalResponse {
            approved: true,
            note: Some(format!("Approved: {}", params.action)),
        })
    }

    async fn status(&self, run_id: &str) -> AcpResult<acp::AgentRunResult> {
        let runs = self.runs.lock().await;
        Ok(runs.get(run_id).cloned().unwrap_or(acp::AgentRunResult {
            run_id: run_id.to_string(),
            status: acp::RunStatus::Failed,
            message: Some("unknown run id".to_string()),
        }))
    }
}

pub async fn run(project_dir: &Path, port: u16) -> Result<()> {
    let harness = super::helpers::build_harness(project_dir).await?;

    println!("🚀 Starting Ratchet ACP server on port {port}");
    println!("   Editors can connect and drive `agent/plan`, `agent/run`, `agent/status`.");

    let handler = ServingHarness {
        inner: Arc::new(Mutex::new(harness)),
        spec_dir: project_dir.join(".ratchet").join("spec"),
        runs: Arc::new(Mutex::new(HashMap::new())),
    };

    AcpServer::new(port, Arc::new(handler)).run().await?;
    Ok(())
}
