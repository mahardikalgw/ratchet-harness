use crate::{
    CoreResult,
    config::ProjectConfig,
    delegation::{
        AgentRole, ReviewVerdict, parse_review_verdict, provider_for_role, review_request,
        revision_request,
    },
    error::CoreError,
    routing::{Router, RoutingRequest, TaskType},
};
use ratchet_mcp::{client::McpClientRegistry, types::McpTool};
use ratchet_memory::{ContextAssembler, ProjectMemory};
use ratchet_observability::{CostTracker, TaskMetrics};
use ratchet_plugins::PluginHost;
use ratchet_providers::{
    ChatRequest, FailoverProvider, Message, MessageRole, ModelProvider, RetryPolicy,
    ToolDefinition, recover_tool_calls, traits::TokenUsage,
};
use ratchet_sandbox::{ApprovalDecision, ApprovalHandler, ApprovalPolicy, AutoDeny, SandboxGuard};
use ratchet_spec::{SpecFile, TaskId, TaskStatus, schema::TaskGraph};
use ratchet_tools::{ToolContext, ToolExecutor, ToolRegistry};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Hard cap on model↔tool round-trips per task, so a looping model cannot
/// burn unbounded tokens or wall-clock time.
pub const MAX_TURNS: usize = 12;

/// Tool output larger than this is spilled to disk instead of being inlined
/// into the model's context (the "context firewall" from the design).
pub const MAX_INLINE_TOOL_OUTPUT: usize = 8_000;

/// Per-run overrides that take precedence over routing config.
#[derive(Debug, Clone, Default)]
pub struct RunOverrides {
    /// Force a specific provider for this run.
    pub provider: Option<String>,
    /// Force a specific model name (passed through to the provider).
    pub model: Option<String>,
}

/// What one agent-loop invocation produced.
#[derive(Debug, Clone)]
struct LoopOutcome {
    final_text: String,
    tool_calls: Vec<ratchet_providers::types::ToolCall>,
    usage: TokenUsage,
    turns: usize,
    provider: String,
    model: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionResult {
    pub task_id: TaskId,
    pub status: TaskStatus,
    pub output: String,
    pub tool_calls: Vec<ratchet_providers::types::ToolCall>,
    pub usage: TokenUsage,
    pub cost_usd: f64,
    pub turns: usize,
    pub changed_files: Vec<String>,
    /// Provider that actually served the final turn.
    pub provider: String,
    pub model: String,
    /// Roles that participated, in order (e.g. `["implementer", "reviewer"]`).
    pub roles: Vec<String>,
    /// Reviewer verdict, when review was enabled.
    pub review: Option<ReviewVerdict>,
}

/// Executes a task graph end-to-end, optionally delegating to reviewer agents.
pub struct TaskExecutor {
    config: ProjectConfig,
    sandbox: SandboxGuard,
    tool_executor: ToolExecutor,
    cost_tracker: CostTracker,
    router: Router,
    mcp: Option<Arc<Mutex<McpClientRegistry>>>,
    mcp_tools: Vec<(String, McpTool)>,
    retry: RetryPolicy,
    overrides: RunOverrides,
    approval: Arc<dyn ApprovalHandler>,
    session_allow: HashSet<String>,
    session_deny: HashSet<String>,
    tool_output_dir: PathBuf,
    plugins: PluginHost,
}

impl TaskExecutor {
    pub fn new(config: ProjectConfig, router: Router) -> Self {
        let sandbox = SandboxGuard::new(config.sandbox.clone());
        let tool_output_dir = config.ratchet_dir.join("tool-output");
        Self {
            config,
            sandbox,
            tool_executor: ToolExecutor::new(),
            cost_tracker: CostTracker::new(),
            router,
            mcp: None,
            mcp_tools: Vec::new(),
            retry: RetryPolicy::default(),
            overrides: RunOverrides::default(),
            approval: Arc::new(AutoDeny),
            session_allow: HashSet::new(),
            session_deny: HashSet::new(),
            tool_output_dir,
            plugins: PluginHost::default(),
        }
    }

    pub fn with_mcp(
        mut self,
        registry: Arc<Mutex<McpClientRegistry>>,
        tools: Vec<(String, McpTool)>,
    ) -> Self {
        self.mcp = Some(registry);
        self.mcp_tools = tools;
        self
    }

    pub fn with_overrides(mut self, overrides: RunOverrides) -> Self {
        self.overrides = overrides;
        self
    }

    pub fn with_approval(mut self, approval: Arc<dyn ApprovalHandler>) -> Self {
        self.approval = approval;
        self
    }

    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    pub fn with_plugins(mut self, plugins: PluginHost) -> Self {
        self.plugins = plugins;
        self
    }

    pub async fn execute_task_graph(
        &mut self,
        graph: &TaskGraph,
        spec: &SpecFile,
        memory: &ProjectMemory,
    ) -> CoreResult<Vec<ExecutionResult>> {
        let mut results = Vec::new();
        let mut completed = std::collections::HashSet::new();

        while completed.len() < graph.nodes.len() {
            let ready: Vec<_> = graph
                .nodes
                .iter()
                .filter(|n| {
                    !completed.contains(&n.id)
                        && graph
                            .dependencies_of(&n.id)
                            .iter()
                            .all(|dep| completed.contains(&dep.id))
                })
                .collect();

            if ready.is_empty() && completed.len() < graph.nodes.len() {
                return Err(CoreError::Execution(
                    "deadlock in task graph — cyclic dependency?".into(),
                ));
            }

            for node in ready {
                let result = self.execute_single_task(node, spec, memory).await?;
                completed.insert(node.id.clone());
                results.push(result);
            }
        }

        Ok(results)
    }

    pub async fn execute_single_task(
        &mut self,
        node: &ratchet_spec::schema::TaskNode,
        spec: &SpecFile,
        memory: &ProjectMemory,
    ) -> CoreResult<ExecutionResult> {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        let (registry, tools) = self.build_toolset().await;

        let working_ctx = ContextAssembler::new().assemble(spec, &node.id, memory, &[])?;
        let repo_map = ratchet_tools::repo_map(&cwd, 120);

        let initial_task = format!(
            "Task: {}\n\nDescription: {}\n\n\
             Repository layout (paths are relative to the project root):\n{}\n\n\
             Context:\n{}",
            node.title, node.description, repo_map, working_ctx.content
        );

        let started_at = chrono::Utc::now();
        let mut roles = vec![AgentRole::Implementer.as_str().to_string()];

        // ---- Implementer ----
        let implementer = self.provider_for(AgentRole::Implementer, node.assigned_model.clone())?;
        let mut outcome = self
            .run_agent_loop(
                AgentRole::Implementer,
                &implementer,
                &registry,
                &tools,
                vec![
                    Message {
                        role: MessageRole::System,
                        content: AgentRole::Implementer.system_prompt().to_string(),
                        tool_calls: None,
                        tool_results: None,
                    },
                    Message {
                        role: MessageRole::User,
                        content: initial_task.clone(),
                        tool_calls: None,
                        tool_results: None,
                    },
                ],
            )
            .await?;

        let mut total_usage = outcome.usage.clone();
        let mut total_turns = outcome.turns;
        let mut all_tool_calls = outcome.tool_calls.clone();

        // ---- Reviewer (multi-agent delegation) ----
        let mut review: Option<ReviewVerdict> = None;
        if self.config.delegation.review {
            let max_rounds = self.config.delegation.max_review_rounds;
            let mut round = 0u32;

            loop {
                roles.push(AgentRole::Reviewer.as_str().to_string());
                let diff = git_diff(&cwd).await;
                let reviewer = self.provider_for(AgentRole::Reviewer, None)?;

                let verdict = self
                    .ask_reviewer(&reviewer, &node.title, &node.description, &diff)
                    .await?;

                let retry = verdict.should_retry() && round < max_rounds;
                review = Some(verdict.clone());

                if !retry {
                    break;
                }

                round += 1;
                roles.push(AgentRole::Implementer.as_str().to_string());
                tracing::info!(
                    round,
                    issues = verdict.issues.len(),
                    "reviewer rejected; requesting revision"
                );

                let implementer =
                    self.provider_for(AgentRole::Implementer, node.assigned_model.clone())?;
                let revision = self
                    .run_agent_loop(
                        AgentRole::Implementer,
                        &implementer,
                        &registry,
                        &tools,
                        vec![
                            Message {
                                role: MessageRole::System,
                                content: AgentRole::Implementer.system_prompt().to_string(),
                                tool_calls: None,
                                tool_results: None,
                            },
                            Message {
                                role: MessageRole::User,
                                content: initial_task.clone(),
                                tool_calls: None,
                                tool_results: None,
                            },
                            Message {
                                role: MessageRole::User,
                                content: revision_request(&verdict.issues, &verdict.summary),
                                tool_calls: None,
                                tool_results: None,
                            },
                        ],
                    )
                    .await?;

                total_usage.add(&revision.usage);
                total_turns += revision.turns;
                all_tool_calls.extend(revision.tool_calls.clone());
                outcome = revision;
            }
        }

        let finished_at = chrono::Utc::now();
        let primary = self.provider_for(AgentRole::Implementer, node.assigned_model.clone())?;
        let cost = primary.cost_model().estimate_cost(
            total_usage.input_tokens,
            total_usage.output_tokens,
            total_usage.cached_tokens,
        );

        let changed_files = changed_files(&cwd).await;

        self.cost_tracker.record(TaskMetrics {
            task_id: node.id.0.clone(),
            model: outcome.model.clone(),
            provider: outcome.provider.clone(),
            started_at,
            finished_at: Some(finished_at),
            input_tokens: total_usage.input_tokens,
            output_tokens: total_usage.output_tokens,
            cached_tokens: total_usage.cached_tokens,
            estimated_cost_usd: cost,
            verification_passed: false,
            human_interventions: 0,
            attempts: total_turns as u32,
        });

        Ok(ExecutionResult {
            task_id: node.id.clone(),
            status: TaskStatus::Done,
            output: outcome.final_text,
            tool_calls: all_tool_calls,
            usage: total_usage,
            cost_usd: cost,
            turns: total_turns,
            changed_files,
            provider: outcome.provider,
            model: outcome.model,
            roles,
            review,
        })
    }

    /// Build the tool registry: built-ins, plugin tools, and MCP tools.
    async fn build_toolset(&self) -> (ToolRegistry, Vec<ToolDefinition>) {
        let mut registry = ToolRegistry::new();
        let mut tools: Vec<ToolDefinition> = registry
            .list()
            .iter()
            .map(|t| ToolDefinition {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            })
            .collect();

        for (name, tool) in &self.mcp_tools {
            registry.register(ratchet_tools::ToolDefinition {
                name: name.clone(),
                description: tool.description.clone().unwrap_or_default(),
                parameters: tool.input_schema.clone(),
            });
            tools.push(ToolDefinition {
                name: name.clone(),
                description: tool.description.clone().unwrap_or_default(),
                parameters: tool.input_schema.clone(),
            });
        }

        // Plugin-provided tools, namespaced as `plugin.tool`.
        for invocation in self.plugins.tool_plugins() {
            let plugin_name = &invocation.manifest.name;
            match self.plugins.describe(plugin_name).await {
                Ok(descriptors) => {
                    for descriptor in descriptors {
                        let qualified = format!("{plugin_name}.{}", descriptor.name);
                        registry.register(ratchet_tools::ToolDefinition {
                            name: qualified.clone(),
                            description: descriptor.description.clone(),
                            parameters: descriptor.parameters.clone(),
                        });
                        tools.push(ToolDefinition {
                            name: qualified,
                            description: descriptor.description,
                            parameters: descriptor.parameters,
                        });
                    }
                }
                Err(e) => {
                    tracing::warn!(plugin = %plugin_name, error = %e, "tool plugin describe failed");
                }
            }
        }

        (registry, tools)
    }

    /// Resolve a retry+failover provider for a role.
    fn provider_for(
        &self,
        role: AgentRole,
        task_pinned: Option<String>,
    ) -> CoreResult<FailoverProvider> {
        let preferred = self
            .overrides
            .provider
            .clone()
            .or_else(|| provider_for_role(&self.config.delegation, role))
            .or(task_pinned)
            .or_else(|| self.config.routing.default.clone());

        let request = RoutingRequest {
            task_type: match role {
                AgentRole::Planner => TaskType::Planning,
                AgentRole::Reviewer => TaskType::Review,
                AgentRole::Tester => TaskType::Testing,
                AgentRole::Implementer => TaskType::Coding,
            },
            required_capabilities: role.required_capabilities(),
            preferred_model: preferred,
        };

        let candidates = self.router.route_all(&request)?;
        let providers: Vec<Arc<dyn ModelProvider>> =
            candidates.iter().map(|c| Arc::clone(&c.provider)).collect();

        Ok(FailoverProvider::new(providers, self.retry.clone()))
    }

    /// The model↔tool loop for a single agent.
    #[allow(clippy::too_many_arguments)]
    async fn run_agent_loop(
        &mut self,
        role: AgentRole,
        provider: &FailoverProvider,
        registry: &ToolRegistry,
        tools: &[ToolDefinition],
        mut messages: Vec<Message>,
    ) -> CoreResult<LoopOutcome> {
        // Reviewers reason over supplied context; they get no tools.
        let offered_tools: Vec<ToolDefinition> = if role.uses_tools() {
            tools.to_vec()
        } else {
            Vec::new()
        };

        let mut total_usage = TokenUsage::default();
        let mut all_tool_calls = Vec::new();
        let mut turns = 0usize;
        let mut served_by: String;
        let mut served_model: String;
        let final_text: String;

        loop {
            turns += 1;

            let response = provider
                .complete(ChatRequest {
                    messages: messages.clone(),
                    tools: offered_tools.clone(),
                    temperature: Some(0.2),
                    max_tokens: Some(4096),
                    model: self.overrides.model.clone(),
                })
                .await
                .map_err(|e| {
                    CoreError::Provider(format!("{} [{}]: {e}", provider.name(), role.as_str()))
                })?;

            total_usage.add(&response.usage);
            served_by = response.provider.clone();
            served_model = response.model.clone();

            tracing::debug!(
                role = role.as_str(),
                provider = %response.provider,
                tool_calls = response.tool_calls.len(),
                finish_reason = ?response.finish_reason,
                content = %truncate_for_log(&response.content),
                "model response"
            );

            // Recover tool calls a model emitted as text rather than natively.
            let tool_calls = if response.tool_calls.is_empty() {
                let known: Vec<String> = offered_tools.iter().map(|t| t.name.clone()).collect();
                let recovered = recover_tool_calls(&response.content, &known);
                if recovered.is_empty() {
                    final_text = response.content;
                    break;
                }
                tracing::info!(
                    role = role.as_str(),
                    count = recovered.len(),
                    "recovered tool calls emitted as text"
                );
                recovered
                    .into_iter()
                    .enumerate()
                    .map(|(i, r)| ratchet_providers::types::ToolCall {
                        id: format!("recovered-{i}"),
                        name: r.name,
                        arguments: r.arguments,
                    })
                    .collect()
            } else {
                response.tool_calls.clone()
            };

            messages.push(Message {
                role: MessageRole::Assistant,
                content: response.content.clone(),
                tool_calls: Some(tool_calls.clone()),
                tool_results: None,
            });

            let mut tool_results = Vec::new();
            for tc in &tool_calls {
                all_tool_calls.push(tc.clone());
                let (content, is_error) = self.run_tool(registry, tc).await;
                let content = self.firewall(&tc.name, content).await;
                tool_results.push(ratchet_providers::types::ToolResult {
                    tool_call_id: tc.id.clone(),
                    content,
                    is_error,
                });
            }

            messages.push(Message {
                role: MessageRole::Tool,
                content: String::new(),
                tool_calls: None,
                tool_results: Some(tool_results),
            });

            if turns >= MAX_TURNS {
                final_text = format!(
                    "{}\n\n[ratchet] stopped after {MAX_TURNS} turns without a final answer",
                    response.content
                );
                break;
            }
        }

        Ok(LoopOutcome {
            final_text,
            tool_calls: all_tool_calls,
            usage: total_usage,
            turns,
            provider: served_by,
            model: served_model,
        })
    }

    /// Ask the reviewer agent to judge a change.
    async fn ask_reviewer(
        &self,
        provider: &FailoverProvider,
        title: &str,
        description: &str,
        diff: &str,
    ) -> CoreResult<ReviewVerdict> {
        let response = provider
            .complete(ChatRequest {
                messages: vec![
                    Message {
                        role: MessageRole::System,
                        content: AgentRole::Reviewer.system_prompt().to_string(),
                        tool_calls: None,
                        tool_results: None,
                    },
                    Message {
                        role: MessageRole::User,
                        content: review_request(title, description, diff),
                        tool_calls: None,
                        tool_results: None,
                    },
                ],
                tools: vec![],
                temperature: Some(0.0),
                max_tokens: Some(1024),
                model: self.overrides.model.clone(),
            })
            .await
            .map_err(|e| CoreError::Provider(format!("reviewer: {e}")))?;

        Ok(parse_review_verdict(&response.content))
    }

    /// Run a single tool call, enforcing the sandbox and dispatching MCP tools.
    async fn run_tool(
        &mut self,
        registry: &ToolRegistry,
        tc: &ratchet_providers::types::ToolCall,
    ) -> (String, bool) {
        tracing::info!(
            tool = %tc.name,
            args = %truncate_for_log(&tc.arguments.to_string()),
            "tool call"
        );

        if let Some((namespace, tool)) = tc.name.split_once('.') {
            // Plugin tools are declared locally and take precedence.
            if self
                .plugins
                .tool_plugins()
                .any(|p| p.manifest.name == namespace)
            {
                return self
                    .call_plugin_tool(namespace, tool, tc.arguments.clone())
                    .await;
            }
            if self.mcp.is_some() {
                return self
                    .call_mcp_tool(namespace, tool, tc.arguments.clone())
                    .await;
            }
        }

        if tc.name == "shell_exec" {
            if let Some(decision) = self.authorize_shell(tc) {
                if !decision.is_approved() {
                    let cmd = tc
                        .arguments
                        .get("command")
                        .and_then(|v| v.as_str())
                        .unwrap_or("<unknown>");
                    return (
                        format!(
                            "command not permitted: `{cmd}`. Choose an allow-listed \
                             command (see ratchet.toml `shell_allowlist`) or ask the user."
                        ),
                        true,
                    );
                }
            }
        }

        let ctx = ToolContext {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            sandbox: self.sandbox.clone(),
            registry: registry.clone(),
        };

        let outcome = match self
            .tool_executor
            .execute(&ctx, &tc.name, tc.arguments.clone())
            .await
        {
            Ok(value) => (value.to_string(), false),
            Err(e) => (format!("tool error: {e}"), true),
        };

        tracing::info!(
            tool = %tc.name,
            is_error = outcome.1,
            result = %truncate_for_log(&outcome.0),
            "tool result"
        );

        outcome
    }

    fn authorize_shell(
        &mut self,
        tc: &ratchet_providers::types::ToolCall,
    ) -> Option<ApprovalDecision> {
        let cmd = tc
            .arguments
            .get("command")
            .and_then(|v| v.as_str())?
            .to_string();

        if self.sandbox.check_shell(&cmd).is_ok() {
            return None;
        }

        match self.config.sandbox.approval_policy {
            ApprovalPolicy::AutoApproveAll => return Some(ApprovalDecision::Approve),
            ApprovalPolicy::DenyAll => return Some(ApprovalDecision::Reject),
            ApprovalPolicy::AutoApproveSafe | ApprovalPolicy::Interactive => {}
        }

        if self.session_deny.contains(&cmd) {
            return Some(ApprovalDecision::Reject);
        }
        if self.session_allow.contains(&cmd) {
            return Some(ApprovalDecision::Approve);
        }

        let decision = self.approval.request("shell_exec", &cmd);
        if decision.is_sticky() {
            if decision.is_approved() {
                self.session_allow.insert(cmd);
            } else {
                self.session_deny.insert(cmd);
            }
        }
        Some(decision)
    }

    async fn firewall(&self, tool: &str, output: String) -> String {
        if output.len() <= MAX_INLINE_TOOL_OUTPUT {
            return output;
        }

        let safe_tool = tool.replace(['/', '.'], "_");
        let filename = format!(
            "{}-{}.txt",
            chrono::Utc::now().format("%Y%m%d-%H%M%S-%3f"),
            safe_tool
        );
        let path = self.tool_output_dir.join(filename);

        if tokio::fs::create_dir_all(&self.tool_output_dir)
            .await
            .is_err()
        {
            return preview(&output);
        }
        if tokio::fs::write(&path, &output).await.is_err() {
            return preview(&output);
        }

        format!(
            "{}\n\n[ratchet] output truncated: {} bytes total. \
             Full output saved to {}. Use `file_read` with offset/limit to inspect it.",
            preview(&output),
            output.len(),
            path.display()
        )
    }

    async fn call_plugin_tool(
        &self,
        plugin: &str,
        tool: &str,
        arguments: serde_json::Value,
    ) -> (String, bool) {
        match self.plugins.call_tool(plugin, tool, arguments).await {
            Ok(response) => (response.content, response.is_error),
            Err(e) => (format!("plugin error: {e}"), true),
        }
    }

    async fn call_mcp_tool(
        &self,
        server: &str,
        tool: &str,
        arguments: serde_json::Value,
    ) -> (String, bool) {
        let Some(registry) = &self.mcp else {
            return ("mcp: no servers connected".to_string(), true);
        };
        let mut guard = registry.lock().await;
        let Some(client) = guard.get(server) else {
            return (format!("mcp: unknown server `{server}`"), true);
        };
        match client.call_tool(tool, Some(arguments)).await {
            Ok(resp) => {
                let text = resp
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        ratchet_mcp::types::ToolContent::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                (text, resp.is_error)
            }
            Err(e) => (format!("mcp call failed: {e}"), true),
        }
    }

    pub fn cost_tracker(&self) -> &CostTracker {
        &self.cost_tracker
    }

    pub fn all_metrics(&self) -> &[TaskMetrics] {
        self.cost_tracker.all_metrics()
    }
}

fn truncate_for_log(s: &str) -> String {
    s.chars().take(400).collect()
}

fn preview(output: &str) -> String {
    output.chars().take(MAX_INLINE_TOOL_OUTPUT).collect()
}

/// Files touched in the working tree, excluding harness/build artifacts.
async fn changed_files(cwd: &Path) -> Vec<String> {
    parse_git_status(&git_status_short(cwd).await)
}

fn parse_git_status(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|l| l.get(3..).map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty())
        .filter(|s| !is_harness_artifact(s))
        .collect()
}

async fn git_status_short(cwd: &Path) -> String {
    let Ok(output) = tokio::process::Command::new("git")
        .args(["status", "--short"])
        .current_dir(cwd)
        .output()
        .await
    else {
        return String::new();
    };
    if !output.status.success() {
        return String::new();
    }
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Unified diff of the working tree, used as reviewer evidence.
async fn git_diff(cwd: &Path) -> String {
    let output = tokio::process::Command::new("git")
        .args(["diff", "--", ".", ":!.ratchet"])
        .current_dir(cwd)
        .output()
        .await;

    let Ok(output) = output else {
        return String::new();
    };

    let mut diff = String::from_utf8_lossy(&output.stdout).to_string();

    // Include untracked files, which `git diff` omits.
    let untracked = tokio::process::Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(cwd)
        .output()
        .await;

    if let Ok(untracked) = untracked {
        for file in String::from_utf8_lossy(&untracked.stdout).lines() {
            if is_harness_artifact(file) {
                continue;
            }
            if let Ok(content) = tokio::fs::read_to_string(cwd.join(file)).await {
                diff.push_str(&format!(
                    "\n--- new file: {file} ---\n{}\n",
                    content.chars().take(4000).collect::<String>()
                ));
            }
        }
    }

    // Keep the reviewer payload bounded.
    diff.chars().take(20_000).collect()
}

/// Paths the harness itself owns, or that are pure build/lock output rather
/// than model-authored source changes.
///
/// These are excluded from the review delta because flagging them would train
/// reviewers to ignore the anomaly marker. `ratchet.toml` is harness
/// configuration (written by `init`/`provider add`), not model output.
fn is_harness_artifact(path: &str) -> bool {
    const IGNORED_PREFIXES: &[&str] = &[".ratchet/", ".ratchet", "target/", "target"];
    const IGNORED_FILES: &[&str] = &[
        "ratchet.toml",
        "Cargo.lock",
        "package-lock.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "poetry.lock",
    ];

    IGNORED_PREFIXES
        .iter()
        .any(|p| path == *p || path.starts_with(p))
        || IGNORED_FILES.contains(&path)
}
