use anyhow::Result;
use ratchet_core::{AgentHarness, render_report};
use ratchet_spec::SpecParser;
use std::path::Path;

use super::helpers::load_project;

pub async fn run(project_dir: &Path, spec_id: &str) -> Result<()> {
    let (config, providers) = load_project(project_dir)?;
    let harness = AgentHarness::new(config, providers).await?;

    let spec_path = project_dir
        .join(".ratchet")
        .join("spec")
        .join(format!("{}.spec.md", spec_id));

    if !spec_path.exists() {
        anyhow::bail!("spec '{}' not found", spec_id);
    }

    let parser = SpecParser::new();
    let spec = parser.parse_file(&spec_path)?;

    println!("🔍 Verifying spec '{}'...", spec_id);
    let report = harness.verify(&spec).await?;

    let verify_dir = project_dir.join(".ratchet").join("verify");
    tokio::fs::create_dir_all(&verify_dir).await?;
    let report_path = verify_dir.join(format!("{}.report.md", spec_id));
    tokio::fs::write(&report_path, render_report(&report)).await?;

    println!("\n{}", report.summary);
    for c in &report.criterion_results {
        println!(
            "  {} {} — {}{}",
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

    println!("\n📄 Report: {:?}", report_path);
    println!(
        "   Result: {}",
        if report.overall_passed {
            "PASSED"
        } else {
            "FAILED"
        }
    );

    // Non-zero exit on failure makes this usable as a CI gate.
    if !report.overall_passed {
        std::process::exit(1);
    }

    Ok(())
}
