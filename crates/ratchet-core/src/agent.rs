use crate::delegation::provider_for_role;
use crate::{
    CoreResult,
    config::ProjectConfig,
    delegation::AgentRole,
    error::CoreError,
    import::{ImportFormat, SpecImporter},
    plan_parser::PlanParser,
    review::ReviewDelta,
    routing::{Router, RoutingRequest, TaskType},
    state::{AgentState, StateMachine},
    task_executor::{ExecutionResult, RunOverrides, TaskExecutor},
    verification::{
        CommandOutcome, CriterionStatus, PluginVerdict, VerificationEngine, VerificationEvidence,
        VerificationReport,
    },
};
use ratchet_mcp::{client::McpClientRegistry, types::McpTool};
use ratchet_memory::{MemoryEntry, MemoryKind, ProjectMemory};
use ratchet_observability::{MetricsStore, TaskMetrics, metrics_path};
use ratchet_plugins::{GateCriterion, GateRequest, GateStatus, PluginHost};
use ratchet_providers::{ModelProvider, RetryPolicy};
use ratchet_sandbox::{ApprovalHandler, AutoDeny, SandboxGuard};
use ratchet_spec::{
    SpecExtractor, SpecFile,
    schema::{Plan, SpecSchema},
};
use ratchet_tools::{TestRunner, ToolContext, ToolRegistry};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Outcome of a full run: what executed, whether it verified, and how the
/// result compares to the plan.
#[derive(Debug, Clone)]
pub struct RunReport {
    pub results: Vec<ExecutionResult>,
    pub verification: VerificationReport,
    pub review: ReviewDelta,
}

/// Top-level agent harness that orchestrates the spec-driven loop.
pub struct AgentHarness {
    config: ProjectConfig,
    state_machine: StateMachine,
    memory: ProjectMemory,
    verification: VerificationEngine,
    router: Router,
    mcp: Option<Arc<Mutex<McpClientRegistry>>>,
    mcp_tools: Vec<(String, McpTool)>,
    approval: Arc<dyn ApprovalHandler>,
    retry: RetryPolicy,
    metrics: MetricsStore,
    plugins: PluginHost,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentConfig {
    pub project_dir: PathBuf,
    pub ratchet_dir: PathBuf,
    pub auto_approve_safe: bool,
    pub max_retries: u32,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            project_dir: PathBuf::from("."),
            ratchet_dir: PathBuf::from(".ratchet"),
            auto_approve_safe: false,
            max_retries: 3,
        }
    }
}

impl AgentHarness {
    pub async fn new(
        config: ProjectConfig,
        providers: HashMap<String, Arc<dyn ModelProvider>>,
    ) -> CoreResult<Self> {
        let memory_path = config.ratchet_dir.join("memory.json");
        let mut memory = ProjectMemory::new(memory_path);
        memory.load().await?;

        let router = Router::new(config.routing.policy, providers);

        // Connect configured MCP servers; failures are non-fatal.
        let (mcp, mcp_tools) = Self::connect_mcp(&config).await;

        let metrics = MetricsStore::new(metrics_path(&config.ratchet_dir));
        let plugins = PluginHost::from_manifests(
            config.plugins.clone(),
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        );
        if !plugins.is_empty() {
            eprintln!(
                "🔌 Plugins: {} gate(s), {} tool(s)",
                plugins.gate_plugins().count(),
                plugins.tool_plugins().count()
            );
        }

        Ok(Self {
            config,
            state_machine: StateMachine::new(),
            memory,
            verification: VerificationEngine::new(),
            router,
            mcp,
            mcp_tools,
            approval: Arc::new(AutoDeny),
            retry: RetryPolicy::default(),
            metrics,
            plugins,
        })
    }

    pub fn plugins(&self) -> &PluginHost {
        &self.plugins
    }

    /// Replace the approval handler (CLI supplies an interactive one).
    pub fn with_approval(mut self, approval: Arc<dyn ApprovalHandler>) -> Self {
        self.approval = approval;
        self
    }

    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    async fn connect_mcp(
        config: &ProjectConfig,
    ) -> (
        Option<Arc<Mutex<McpClientRegistry>>>,
        Vec<(String, McpTool)>,
    ) {
        if config.mcp.servers.is_empty() {
            return (None, Vec::new());
        }

        let mut registry = McpClientRegistry::new();
        for (name, server) in &config.mcp.servers {
            match ratchet_mcp::McpClient::connect_stdio(&server.command, &server.args).await {
                Ok(client) => {
                    tracing::info!(server = %name, "connected MCP server");
                    registry.add(name.clone(), client);
                }
                Err(e) => {
                    tracing::warn!(server = %name, error = %e, "failed to connect MCP server");
                    eprintln!("⚠️  MCP server '{name}' failed to connect: {e}");
                }
            }
        }

        let tools = match registry.list_all_tools().await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, "failed to list MCP tools");
                Vec::new()
            }
        };
        eprintln!("🔌 MCP tools available: {}", tools.len());

        (Some(Arc::new(Mutex::new(registry))), tools)
    }

    pub fn state(&self) -> AgentState {
        self.state_machine.state()
    }

    /// Generate a plan from a spec, persist it, and return it.
    pub async fn plan(&mut self, spec: &SpecFile) -> CoreResult<Plan> {
        self.state_machine
            .transition(AgentState::Planning)
            .map_err(CoreError::Execution)?;

        let planning_preferred = provider_for_role(&self.config.delegation, AgentRole::Planner)
            .or_else(|| self.config.routing.planning_tasks.clone());

        let routing_req = RoutingRequest {
            task_type: TaskType::Planning,
            required_capabilities: AgentRole::Planner.required_capabilities(),
            preferred_model: planning_preferred,
        };

        let route = self.router.route(&routing_req)?;
        let provider = route.provider;

        let request = ratchet_providers::ChatRequest {
            messages: vec![ratchet_providers::types::Message {
                role: ratchet_providers::types::MessageRole::User,
                content: planning_prompt(spec),
                tool_calls: None,
                tool_results: None,
            }],
            tools: vec![],
            temperature: Some(0.1),
            max_tokens: Some(4096),
            model: None,
        };

        let response = provider
            .complete(request)
            .await
            .map_err(|e| CoreError::PlanGeneration(format!("{}: {e}", provider.name())))?;

        let plan = PlanParser::parse(&response.content, spec)?;
        plan.task_graph.validate()?;

        self.save_plan(&plan).await?;
        self.state_machine.transition(AgentState::Idle).ok();
        Ok(plan)
    }

    /// Load a persisted plan, or generate (and persist) one.
    pub async fn load_or_plan(&mut self, spec: &SpecFile) -> CoreResult<Plan> {
        let path = self.plan_json_path(&spec.frontmatter.id);
        if path.exists() {
            let content = tokio::fs::read_to_string(&path).await?;
            let plan: Plan = serde_json::from_str(&content)
                .map_err(|e| CoreError::Execution(format!("invalid saved plan: {e}")))?;
            plan.task_graph.validate()?;
            return Ok(plan);
        }
        self.plan(spec).await
    }

    pub async fn run(&mut self, plan: &Plan, spec: &SpecFile) -> CoreResult<RunReport> {
        self.run_with(plan, spec, RunOverrides::default()).await
    }

    /// Execute a plan, verify the result, and produce a review delta.
    pub async fn run_with(
        &mut self,
        plan: &Plan,
        spec: &SpecFile,
        overrides: RunOverrides,
    ) -> CoreResult<RunReport> {
        self.state_machine
            .transition(AgentState::Executing)
            .map_err(CoreError::Execution)?;

        let mut executor = TaskExecutor::new(self.config.clone(), self.router.clone())
            .with_overrides(overrides)
            .with_approval(Arc::clone(&self.approval))
            .with_retry(self.retry.clone())
            .with_plugins(self.plugins.clone());

        if let Some(mcp) = &self.mcp {
            executor = executor.with_mcp(Arc::clone(mcp), self.mcp_tools.clone());
        }

        let results = executor
            .execute_task_graph(&plan.task_graph, spec, &self.memory)
            .await?;

        self.state_machine.transition(AgentState::Verifying).ok();

        let verification = self.verify(spec).await?;
        self.write_verification_report(&verification).await?;

        // Persist one metric per task, now that the verification outcome is known.
        self.persist_metrics(&results, &verification).await;

        // Feed what we learned back into durable project memory.
        self.record_memory(spec, &results, &verification).await?;

        let review = ReviewDelta::compute(plan, &results, Some(&verification));
        self.write_review(&review).await?;

        if verification.overall_passed {
            self.state_machine.transition(AgentState::Done).ok();
        } else {
            self.state_machine.transition(AgentState::Failed).ok();
        }

        Ok(RunReport {
            results,
            verification,
            review,
        })
    }

    /// Verify spec conformance against the current working tree.
    pub async fn verify(&self, spec: &SpecFile) -> CoreResult<VerificationReport> {
        let schema = SpecSchema {
            id: spec.frontmatter.id.clone(),
            title: spec.frontmatter.title.clone(),
            goals: SpecExtractor::goals(spec),
            non_goals: SpecExtractor::non_goals(spec),
            acceptance_criteria: SpecExtractor::acceptance_criteria(spec),
            constraints: vec![],
        };

        let evidence = self.gather_evidence(&schema).await;
        self.verification
            .verify_spec_conformance(&schema, &evidence)
    }

    /// Run every command the spec declares for verification, plus the project
    /// test suite, and collect changed files.
    async fn gather_evidence(&self, schema: &SpecSchema) -> VerificationEvidence {
        use ratchet_spec::schema::VerificationStep;
        use std::collections::HashSet;

        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut evidence = VerificationEvidence {
            changed_files: changed_files(&cwd).await,
            ..Default::default()
        };

        // Execute each distinct command declared by a Test or Lint criterion.
        let mut commands: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for c in &schema.acceptance_criteria {
            let cmd = match &c.verification {
                Some(VerificationStep::Test { command, .. }) => Some(command.clone()),
                Some(VerificationStep::Lint { tool, .. }) => Some(tool.clone()),
                _ => None,
            };
            if let Some(cmd) = cmd {
                if seen.insert(cmd.clone()) {
                    commands.push(cmd);
                }
            }
        }

        for command in &commands {
            match run_command(&cwd, command).await {
                Some(outcome) => {
                    evidence.command_results.insert(command.clone(), outcome);
                }
                None => {
                    tracing::warn!(command = %command, "verification command could not be started");
                }
            }
        }

        // External gate plugins run last so they can see the built-in evidence.
        evidence.plugin_results = self.run_gate_plugins(schema, &evidence, &cwd).await;

        // Baseline: run the project test suite even if the spec did not ask.
        if let Ok(runner) = TestRunner::detect(&cwd) {
            let ctx = ToolContext {
                cwd: cwd.clone(),
                sandbox: SandboxGuard::new(self.config.sandbox.clone()),
                registry: ToolRegistry::new(),
            };
            if let Ok(value) = runner.execute(&ctx, None).await {
                let stdout = value["stdout"].as_str().unwrap_or("");
                let stderr = value["stderr"].as_str().unwrap_or("");
                evidence.test_output = format!("{stdout}\n{stderr}");
                evidence.test_passed = value["success"].as_bool().unwrap_or(false);
            }
        }

        evidence
    }

    /// Ask every applicable gate plugin for a verdict.
    async fn run_gate_plugins(
        &self,
        schema: &SpecSchema,
        evidence: &VerificationEvidence,
        cwd: &std::path::Path,
    ) -> HashMap<String, PluginVerdict> {
        let mut out = HashMap::new();
        if self.plugins.gate_plugins().next().is_none() {
            return out;
        }

        let request = GateRequest {
            spec_id: schema.id.clone(),
            criteria: schema
                .acceptance_criteria
                .iter()
                .map(|c| GateCriterion {
                    id: c.id.clone(),
                    description: c.description.clone(),
                })
                .collect(),
            changed_files: evidence.changed_files.clone(),
            test_passed: Some(evidence.test_passed),
            test_output: evidence.test_output.chars().take(8000).collect(),
            working_dir: cwd.display().to_string(),
        };

        for (plugin, result) in self.plugins.run_gates(&request).await {
            out.insert(
                result.criterion_id.clone(),
                PluginVerdict {
                    plugin,
                    status: match result.status {
                        GateStatus::Passed => CriterionStatus::Passed,
                        GateStatus::Failed => CriterionStatus::Failed,
                        GateStatus::Manual => CriterionStatus::Manual,
                    },
                    note: result.note,
                },
            );
        }

        out
    }

    /// Import a spec from an external format.
    pub async fn import_spec(
        &self,
        source: &std::path::Path,
        format: ImportFormat,
    ) -> CoreResult<SpecFile> {
        let importer = SpecImporter::new();
        importer.import(source, format).await
    }

    pub fn approve_blocked(&mut self) -> CoreResult<()> {
        match self.state_machine.state() {
            AgentState::Blocked(_) => {
                self.state_machine
                    .transition(AgentState::Executing)
                    .map_err(CoreError::Execution)?;
                Ok(())
            }
            other => Err(CoreError::Execution(format!(
                "cannot approve from state {other:?}"
            ))),
        }
    }

    pub fn reject_blocked(&mut self) -> CoreResult<()> {
        self.state_machine
            .transition(AgentState::Failed)
            .map_err(CoreError::Execution)?;
        Ok(())
    }

    pub fn memory(&self) -> &ProjectMemory {
        &self.memory
    }

    pub fn memory_mut(&mut self) -> &mut ProjectMemory {
        &mut self.memory
    }

    pub fn config(&self) -> &ProjectConfig {
        &self.config
    }

    // ----- persistence -----

    fn tasks_path(&self, spec_id: &str) -> PathBuf {
        self.config
            .ratchet_dir
            .join("tasks")
            .join(format!("{spec_id}.tasks.yaml"))
    }

    fn plan_json_path(&self, spec_id: &str) -> PathBuf {
        self.config
            .ratchet_dir
            .join("plan")
            .join(format!("{spec_id}.plan.json"))
    }

    fn plan_path(&self, spec_id: &str) -> PathBuf {
        self.config
            .ratchet_dir
            .join("plan")
            .join(format!("{spec_id}.plan.md"))
    }

    fn verify_path(&self, spec_id: &str) -> PathBuf {
        self.config
            .ratchet_dir
            .join("verify")
            .join(format!("{spec_id}.report.md"))
    }

    fn review_path(&self, spec_id: &str) -> PathBuf {
        self.config
            .ratchet_dir
            .join("review")
            .join(format!("{spec_id}.delta.md"))
    }

    async fn save_plan(&self, plan: &Plan) -> CoreResult<()> {
        let yaml = serde_yaml::to_string(&plan.task_graph)
            .map_err(|e| CoreError::Execution(format!("failed to serialize tasks: {e}")))?;
        write_artifact(&self.tasks_path(&plan.spec_id), yaml).await?;

        // Machine-readable plan (keeps affected_modules for the review gate).
        let json = serde_json::to_string_pretty(plan)
            .map_err(|e| CoreError::Execution(format!("failed to serialize plan: {e}")))?;
        write_artifact(&self.plan_json_path(&plan.spec_id), json).await?;

        // Human-readable plan.
        let plan_path = self.plan_path(&plan.spec_id);
        let md = format!(
            "# Plan: {}\n\n## Summary\n\n{}\n\n## Affected Modules\n\n{}\n\n## Data Model Changes\n\n{}\n\n## Risk Notes\n\n{}\n\n## Tasks\n\n{}\n",
            plan.title,
            plan.summary,
            bullet_list(&plan.affected_modules),
            bullet_list(&plan.data_model_changes),
            bullet_list(&plan.risk_notes),
            plan.task_graph
                .nodes
                .iter()
                .map(|n| format!("- **{}** — {}", n.id, n.title))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        write_artifact(&plan_path, md).await
    }

    async fn write_verification_report(&self, report: &VerificationReport) -> CoreResult<()> {
        write_artifact(&self.verify_path(&report.spec_id), render_report(report)).await
    }

    async fn write_review(&self, review: &ReviewDelta) -> CoreResult<()> {
        write_artifact(&self.review_path(&review.spec_id), review.render()).await
    }

    /// Record one metric per task, tagged with the verification outcome.
    async fn persist_metrics(
        &self,
        results: &[ExecutionResult],
        verification: &VerificationReport,
    ) {
        for result in results {
            let metric = TaskMetrics {
                task_id: result.task_id.0.clone(),
                model: result.model.clone(),
                provider: result.provider.clone(),
                started_at: chrono::Utc::now(),
                finished_at: Some(chrono::Utc::now()),
                input_tokens: result.usage.input_tokens,
                output_tokens: result.usage.output_tokens,
                cached_tokens: result.usage.cached_tokens,
                estimated_cost_usd: result.cost_usd,
                verification_passed: verification.overall_passed,
                human_interventions: 0,
                attempts: result.turns as u32,
            };
            if let Err(e) = self.metrics.append(&metric).await {
                tracing::warn!(error = %e, "failed to persist task metrics");
            }
        }
    }

    /// Distil each task's outcome into durable project memory.
    async fn record_memory(
        &mut self,
        spec: &SpecFile,
        results: &[ExecutionResult],
        verification: &VerificationReport,
    ) -> CoreResult<()> {
        for result in results {
            let summary = first_lines(&result.output, 12);
            if summary.trim().is_empty() {
                continue;
            }

            let kind = if verification.overall_passed {
                MemoryKind::VerificationReport
            } else {
                MemoryKind::Gotcha
            };

            self.memory.add(MemoryEntry {
                id: format!("{}-{}", spec.frontmatter.id, result.task_id.0),
                kind,
                content: format!(
                    "task {} ({}) via {}: {}",
                    result.task_id,
                    result.status_str(),
                    result.provider,
                    summary
                ),
                tags: vec![
                    spec.frontmatter.id.clone(),
                    result.task_id.0.clone(),
                    result.provider.clone(),
                ],
                created_at: chrono::Utc::now(),
                importance: if verification.overall_passed { 5 } else { 8 },
            });
        }

        self.memory.compact();
        self.memory.save().await?;
        Ok(())
    }
}

/// Write a file, creating parent directories as needed.
///
/// Every artifact write goes through here so a missing directory can never
/// fail a run, and errors name the offending path.
async fn write_artifact(path: &std::path::Path, contents: impl AsRef<[u8]>) -> CoreResult<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                CoreError::Execution(format!(
                    "failed to create directory {}: {e}",
                    parent.display()
                ))
            })?;
        }
    }
    tokio::fs::write(path, contents)
        .await
        .map_err(|e| CoreError::Execution(format!("failed to write {}: {e}", path.display())))
}

fn first_lines(text: &str, n: usize) -> String {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .take(n)
        .collect::<Vec<_>>()
        .join("\n")
}

trait StatusStr {
    fn status_str(&self) -> String;
}

impl StatusStr for ExecutionResult {
    fn status_str(&self) -> String {
        format!("{:?}", self.status)
    }
}

fn bullet_list(items: &[String]) -> String {
    if items.is_empty() {
        "_none_".to_string()
    } else {
        items
            .iter()
            .map(|i| format!("- {i}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub fn render_report(report: &VerificationReport) -> String {
    let mut out = format!(
        "# Verification Report: {}\n\n**Overall:** {}\n\n**Summary:** {}\n\n\
         | Criterion | Status | Note |\n|---|---|---|\n",
        report.spec_id,
        if report.overall_passed {
            "✅ PASSED"
        } else {
            "❌ FAILED"
        },
        report.summary,
    );

    for c in &report.criterion_results {
        out.push_str(&format!(
            "| {} {} | {} | {} |\n",
            c.status.icon(),
            c.criterion_id,
            format!("{:?}", c.status).to_lowercase(),
            c.note
        ));
    }

    out.push_str(&format!(
        "\n## Changed Files\n\n{}\n",
        if report.changed_files.is_empty() {
            "_none_".to_string()
        } else {
            report
                .changed_files
                .iter()
                .map(|f| format!("- `{f}`"))
                .collect::<Vec<_>>()
                .join("\n")
        }
    ));

    out
}

async fn changed_files(cwd: &std::path::Path) -> Vec<String> {
    let Ok(output) = tokio::process::Command::new("git")
        .args(["status", "--short"])
        .current_dir(cwd)
        .output()
        .await
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|l| l.get(3..).map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty())
        .collect()
}

/// Run a shell command declared by a spec, capturing combined output.
async fn run_command(cwd: &std::path::Path, command: &str) -> Option<CommandOutcome> {
    let mut cmd = ratchet_tools::shell_command(command);
    cmd.current_dir(cwd);
    let output = cmd.output().await.ok()?;

    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));

    Some(CommandOutcome {
        passed: output.status.success(),
        output: combined,
    })
}

fn planning_prompt(spec: &SpecFile) -> String {
    format!(
        r#"Generate a technical plan for the following spec.

Respond with a single JSON object and nothing else, matching this schema:

{{
  "summary": "one paragraph technical approach",
  "affected_modules": ["path/or/module"],
  "data_model_changes": ["change"],
  "risk_notes": ["risk"],
  "tasks": [
    {{
      "id": "T-1",
      "title": "short imperative title",
      "description": "what to implement and how it will be checked",
      "depends_on": [],
      "verification": {{ "kind": "test", "command": "cargo test", "expected": "test result: ok" }}
    }}
  ]
}}

Rules:
- Each task must be independently verifiable.
- `depends_on` must reference other task ids in this plan.
- Do not invent files that do not exist; inspect the spec only.

Spec:
---
{}
---"#,
        spec.raw
    )
}
