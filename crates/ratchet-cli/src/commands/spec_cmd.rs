use anyhow::Result;
use ratchet_spec::{SpecParser, SpecValidator};
use std::path::Path;

const SPEC_TEMPLATE: &str = r#"---
id: {id}
title: "{title}"
status: draft
priority: normal
tags: []
dependencies: []
---

# Goals

- Describe the primary goal here.

# Non-Goals

- What is explicitly out of scope.

# Acceptance Criteria

- [ ] AC-1: Criterion description
- [ ] AC-2: Another criterion

# Constraints

- Performance, security, or compatibility constraints.

# Notes

Additional context, links, or references.
"#;

pub async fn new_spec(project_dir: &Path, id: &str) -> Result<()> {
    let spec_dir = project_dir.join(".ratchet").join("spec");
    tokio::fs::create_dir_all(&spec_dir).await?;

    let filename = format!("{}.spec.md", id);
    let path = spec_dir.join(&filename);

    if path.exists() {
        anyhow::bail!("spec '{}' already exists at {:?}", id, path);
    }

    let title = id
        .split('-')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    let content = SPEC_TEMPLATE.replace("{id}", id).replace("{title}", &title);
    tokio::fs::write(&path, content).await?;

    println!("✅ Created spec: {:?}", path);
    Ok(())
}

pub async fn edit_spec(project_dir: &Path, id: &str) -> Result<()> {
    let path = project_dir
        .join(".ratchet")
        .join("spec")
        .join(format!("{}.spec.md", id));
    if !path.exists() {
        anyhow::bail!("spec '{}' not found at {:?}", id, path);
    }

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
    let status = tokio::process::Command::new(&editor)
        .arg(&path)
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("editor exited with non-zero status");
    }

    println!("✅ Edited spec: {:?}", path);
    Ok(())
}

pub async fn validate(path: &Path) -> Result<()> {
    let parser = SpecParser::new();
    let spec = parser.parse_file(path)?;
    let validator = SpecValidator::new();
    let issues = validator.validate(&spec)?;

    let mut errors = 0;
    let mut warnings = 0;

    for issue in &issues {
        match issue.level {
            ratchet_spec::validator::ValidationLevel::Error => {
                println!("❌ [{}] {}", issue.location, issue.message);
                errors += 1;
            }
            ratchet_spec::validator::ValidationLevel::Warning => {
                println!("⚠️  [{}] {}", issue.location, issue.message);
                warnings += 1;
            }
            ratchet_spec::validator::ValidationLevel::Info => {
                println!("ℹ️  [{}] {}", issue.location, issue.message);
            }
        }
    }

    if errors == 0 && warnings == 0 {
        println!("✅ Spec is valid: {:?}", path);
    } else {
        println!("\n{} error(s), {} warning(s)", errors, warnings);
    }

    Ok(())
}

pub async fn list_specs(project_dir: &Path) -> Result<()> {
    let spec_dir = project_dir.join(".ratchet").join("spec");
    if !spec_dir.exists() {
        println!("No specs directory found.");
        return Ok(());
    }

    let mut entries = tokio::fs::read_dir(&spec_dir).await?;
    let mut count = 0;

    println!("{:<30} {:<12} {:<10} TITLE", "ID", "STATUS", "PRIORITY");
    println!("{}", "-".repeat(80));

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
        let parser = SpecParser::new();
        if let Ok(spec) = parser.parse(&content) {
            println!(
                "{:<30} {:<12?} {:<10?} {}",
                spec.frontmatter.id,
                spec.frontmatter.status,
                spec.frontmatter.priority,
                spec.frontmatter.title
            );
            count += 1;
        }
    }

    if count == 0 {
        println!("No specs found.");
    } else {
        println!("\n{} spec(s) total", count);
    }

    Ok(())
}
