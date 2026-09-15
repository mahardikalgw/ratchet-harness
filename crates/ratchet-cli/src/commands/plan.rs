use anyhow::Result;
use ratchet_core::AgentHarness;
use ratchet_spec::SpecParser;
use std::path::Path;

use super::helpers::load_project;

pub async fn run(project_dir: &Path, spec_id: &str, _model: Option<String>) -> Result<()> {
    let (config, providers) = load_project(project_dir)?;

    if providers.is_empty() {
        anyhow::bail!(
            "no usable providers configured — set an API key env var (see `ratchet provider list`)"
        );
    }

    let mut harness = AgentHarness::new(config, providers).await?;

    let spec_path = project_dir
        .join(".ratchet")
        .join("spec")
        .join(format!("{}.spec.md", spec_id));

    if !spec_path.exists() {
        anyhow::bail!("spec '{}' not found at {:?}", spec_id, spec_path);
    }

    let parser = SpecParser::new();
    let spec = parser.parse_file(&spec_path)?;

    println!("🧠 Generating plan for '{}'...", spec_id);
    let plan = harness.plan(&spec).await?;

    let plan_dir = project_dir.join(".ratchet").join("plan");
    tokio::fs::create_dir_all(&plan_dir).await?;
    let plan_path = plan_dir.join(format!("{}.plan.md", spec_id));

    let plan_content = format!(
        "# Plan: {}\n\n## Summary\n\n{}\n\n## Affected Modules\n\n{}\n\n## Data Model Changes\n\n{}\n\n## Risk Notes\n\n{}\n\n## Task Graph\n\n{} tasks, {} edges\n",
        plan.title,
        plan.summary,
        plan.affected_modules.join("\n- "),
        plan.data_model_changes.join("\n- "),
        plan.risk_notes.join("\n- "),
        plan.task_graph.nodes.len(),
        plan.task_graph.edges.len()
    );

    tokio::fs::write(&plan_path, plan_content).await?;

    println!("✅ Plan generated and saved to {:?}", plan_path);
    println!("   Tasks: {}", plan.task_graph.nodes.len());

    Ok(())
}
