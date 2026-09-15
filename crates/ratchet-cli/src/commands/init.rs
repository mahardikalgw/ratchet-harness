use anyhow::Result;
use ratchet_core::ProjectConfig;
use std::path::Path;

pub async fn run(project_dir: &Path, name: &str) -> Result<()> {
    let ratchet_dir = project_dir.join(".ratchet");
    tokio::fs::create_dir_all(&ratchet_dir).await?;
    tokio::fs::create_dir_all(ratchet_dir.join("spec")).await?;
    tokio::fs::create_dir_all(ratchet_dir.join("plan")).await?;
    tokio::fs::create_dir_all(ratchet_dir.join("tasks")).await?;
    tokio::fs::create_dir_all(ratchet_dir.join("verify")).await?;

    let config = ProjectConfig::scaffold(name);
    let config_path = project_dir.join("ratchet.toml");
    config.save(&config_path)?;

    let intent_md = project_dir.join(".ratchet").join("intent.md");
    tokio::fs::write(
        &intent_md,
        format!(
            "# Intent: {}\n\nDescribe what you want to build here.\n",
            name
        ),
    )
    .await?;

    println!(
        "✅ Initialized Ratchet project '{}' at {:?}",
        name, project_dir
    );
    println!("   Config: {:?}", config_path);
    println!("   Ratchet dir: {:?}", ratchet_dir);
    println!("\nNext steps:");
    println!("  ratchet spec new <feature-id>   — create a spec");
    println!("  ratchet plan <feature-id>       — generate a plan");
    println!("  ratchet run <feature-id>        — execute tasks");

    Ok(())
}
