use anyhow::Result;
use std::path::Path;

/// Show the plan-vs-actual delta produced by the last run.
pub async fn run(project_dir: &Path, spec_id: &str) -> Result<()> {
    let path = project_dir
        .join(".ratchet")
        .join("review")
        .join(format!("{}.delta.md", spec_id));

    if !path.exists() {
        anyhow::bail!("no review delta for '{spec_id}' — run `ratchet run {spec_id} --all` first");
    }

    let content = tokio::fs::read_to_string(&path).await?;
    println!("{content}");
    Ok(())
}
