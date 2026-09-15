use anyhow::Result;
use ratchet_spec::TaskGraph;
use std::path::Path;

pub async fn run(project_dir: &Path, spec_id: &str, edit: bool) -> Result<()> {
    let tasks_dir = project_dir.join(".ratchet").join("tasks");
    let task_path = tasks_dir.join(format!("{}.tasks.yaml", spec_id));

    if !task_path.exists() {
        let plan_path = project_dir
            .join(".ratchet")
            .join("plan")
            .join(format!("{}.plan.md", spec_id));
        if plan_path.exists() {
            println!("ℹ️  No task graph yet for '{spec_id}'.");
            println!("   Run `ratchet plan {spec_id}` to generate one.");
            return Ok(());
        }
        anyhow::bail!("no tasks or plan found for '{spec_id}'");
    }

    // Validate before handing to the editor, so we never open a broken file.
    let content = tokio::fs::read_to_string(&task_path).await?;
    let graph: TaskGraph = serde_yaml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("invalid task graph at {task_path:?}: {e}"))?;

    if edit {
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
        let status = tokio::process::Command::new(&editor)
            .arg(&task_path)
            .status()
            .await?;
        if !status.success() {
            anyhow::bail!("editor exited with non-zero status");
        }

        // Re-validate what the user saved.
        let edited = tokio::fs::read_to_string(&task_path).await?;
        let edited_graph: TaskGraph = serde_yaml::from_str(&edited)
            .map_err(|e| anyhow::anyhow!("saved task graph is not valid YAML: {e}"))?;
        edited_graph
            .validate()
            .map_err(|e| anyhow::anyhow!("saved task graph is invalid: {e}"))?;

        println!(
            "✅ Saved {} task(s), {} edge(s) to {:?}",
            edited_graph.nodes.len(),
            edited_graph.edges.len(),
            task_path
        );
        print_task_graph(&edited_graph);
        return Ok(());
    }

    print_task_graph(&graph);
    println!("\n💡 `ratchet tasks {spec_id} --edit` to modify the graph.");
    Ok(())
}

fn print_task_graph(graph: &TaskGraph) {
    println!(
        "Tasks ({} nodes, {} edges)",
        graph.nodes.len(),
        graph.edges.len()
    );
    println!("{}", "-".repeat(80));

    for node in &graph.nodes {
        let deps: Vec<_> = graph
            .dependencies_of(&node.id)
            .iter()
            .map(|d| d.id.0.clone())
            .collect();
        let status = if deps.is_empty() {
            "ready".to_string()
        } else {
            format!("blocked by: {}", deps.join(", "))
        };
        println!(
            "  {:<6} | {:<40} | model={:<12} | {}",
            node.id.0,
            truncate(&node.title, 40),
            node.assigned_model.as_deref().unwrap_or("-"),
            status
        );
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
