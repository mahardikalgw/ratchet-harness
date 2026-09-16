use anyhow::Result;
use ratchet_core::{AgentHarness, RunOverrides};
use ratchet_spec::SpecParser;
use std::path::Path;
use std::sync::Arc;

use super::helpers::load_project;
use crate::approval::CliApprovalHandler;

pub async fn run(
    project_dir: &Path,
    target: &str,
    task: Option<String>,
    model: Option<String>,
    all: bool,
) -> Result<()> {
    let (config, providers) = load_project(project_dir)?;

    if providers.is_empty() {
        anyhow::bail!(
            "no usable providers configured — set an API key or run \
             `ratchet provider login <name>` (see `ratchet provider list`)"
        );
    }

    let mut harness = AgentHarness::new(config, providers)
        .await?
        .with_approval(Arc::new(CliApprovalHandler))
        .with_project_dir(project_dir.to_path_buf());

    let spec_path = project_dir
        .join(".ratchet")
        .join("spec")
        .join(format!("{}.spec.md", target));

    if !spec_path.exists() {
        anyhow::bail!("spec '{}' not found at {:?}", target, spec_path);
    }

    let parser = SpecParser::new();
    let spec = parser.parse_file(&spec_path)?;

    // `--model provider:model` pins both; `--model model` pins just the model.
    let overrides = match model {
        Some(spec_str) => parse_model_override(&spec_str),
        None => RunOverrides::default(),
    };
    if overrides.provider.is_some() || overrides.model.is_some() {
        println!(
            "🔄 Override → provider={:?} model={:?}",
            overrides.provider, overrides.model
        );
    }

    let mut plan = harness.load_or_plan(&spec).await?;

    if let Some(task_id) = task {
        let Some(node) = plan
            .task_graph
            .nodes
            .iter()
            .find(|n| n.id.0 == task_id)
            .cloned()
        else {
            anyhow::bail!("task '{}' not found in plan", task_id);
        };
        println!("▶️  Running task {task_id}");
        plan.task_graph.nodes = vec![node];
        plan.task_graph.edges.clear();
    } else if !all {
        println!("📋 Task graph: {} task(s)", plan.task_graph.nodes.len());
        println!("   Run with --all to execute, or --task <id> for a single task.");
        for node in &plan.task_graph.nodes {
            let deps: Vec<_> = plan
                .task_graph
                .dependencies_of(&node.id)
                .iter()
                .map(|d| d.id.0.clone())
                .collect();
            println!(
                "   - {} {} (deps: {})",
                node.id,
                node.title,
                if deps.is_empty() {
                    "none".to_string()
                } else {
                    deps.join(", ")
                }
            );
        }
        return Ok(());
    } else {
        println!("▶️  Running all tasks for '{}'...", target);
    }

    let report = harness.run_with(&plan, &spec, overrides).await?;

    println!("\n─── Execution ───");
    for r in &report.results {
        println!(
            "   {} | {} | {} turn(s) | ${:.4} | {} in / {} out | {}",
            r.task_id,
            r.status_str(),
            r.turns,
            r.cost_usd,
            r.usage.input_tokens,
            r.usage.output_tokens,
            r.provider
        );
    }

    println!("\n─── Verification ───");
    println!("   {}", report.verification.summary);
    for c in &report.verification.criterion_results {
        println!(
            "   {} {} — {}{}",
            c.status.icon(),
            c.criterion_id,
            c.description,
            if c.note.is_empty() {
                String::new()
            } else {
                format!("  ({})", c.note)
            }
        );
    }

    println!("\n─── Review ───");
    println!("   {}", report.review.render().lines().last().unwrap_or(""));
    let review_path = project_dir
        .join(".ratchet")
        .join("review")
        .join(format!("{}.delta.md", target));
    println!("   Delta report: {review_path:?}");

    let tool_calls: usize = report.results.iter().map(|r| r.tool_calls.len()).sum();
    if tool_calls == 0 {
        println!(
            "\n⚠️  The model made no tool calls, so nothing was changed.\n   \
             If this is a local model, it likely has no tool-calling support — \
             try a tool-trained one (qwen2.5, llama3.1, mistral-nemo)."
        );
    }

    if !report.review.is_clean() {
        println!("\n⚠️  Anomalies need review (see the delta report).");
        // Non-zero exit so this is usable as a CI/automation gate.
        std::process::exit(1);
    }

    Ok(())
}

fn parse_model_override(spec: &str) -> RunOverrides {
    match spec.split_once(':') {
        Some((provider, model)) => RunOverrides {
            provider: Some(provider.trim().to_string()),
            model: Some(model.trim().to_string()),
        },
        None => RunOverrides {
            provider: None,
            model: Some(spec.trim().to_string()),
        },
    }
}

/// Small helper so callers can render a task status without importing the enum.
trait StatusStr {
    fn status_str(&self) -> String;
}

impl StatusStr for ratchet_core::ExecutionResult {
    fn status_str(&self) -> String {
        format!("{:?}", self.status)
    }
}
