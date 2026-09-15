use anyhow::Result;
use ratchet_core::import::{ImportFormat, SpecImporter};
use std::path::Path;

pub async fn run(
    project_dir: &Path,
    source: &Path,
    format: Option<String>,
) -> Result<()> {
    let format = match format.as_deref() {
        Some("agents.md") => ImportFormat::AgentsMd,
        Some("openspec") => ImportFormat::OpenSpec,
        Some("markdown") => ImportFormat::PlainMarkdown,
        Some(other) => {
            anyhow::bail!(
                "unknown import format: {}. Use: agents.md, openspec, markdown, or omit for auto-detect",
                other
            );
        }
        None => ImportFormat::AutoDetect,
    };

    println!("📥 Importing {:?}...", source);
    let importer = SpecImporter::new();
    let spec = importer.import(source, format).await?;

    let spec_dir = project_dir.join(".ratchet").join("spec");
    tokio::fs::create_dir_all(&spec_dir).await?;
    let dest_path = spec_dir.join(format!("{}.spec.md", spec.frontmatter.id));

    tokio::fs::write(&dest_path, &spec.raw).await?;

    println!("✅ Imported spec '{}' to {:?}", spec.frontmatter.id, dest_path);
    println!("   Title: {}", spec.frontmatter.title);
    println!("   Sections: {}", spec.sections.len());

    Ok(())
}
