use crate::CoreResult;
use ratchet_spec::{
    format::{Priority, SpecFile, SpecFrontmatter, SpecSection, SpecStatus},
    SpecParser,
};
use std::path::Path;

/// Import specs from external SDD formats.
pub struct SpecImporter;

impl SpecImporter {
    pub fn new() -> Self {
        Self
    }

    pub async fn import(&self, source: &Path, format: ImportFormat) -> CoreResult<SpecFile> {
        let content = tokio::fs::read_to_string(source).await?;

        match format {
            ImportFormat::AutoDetect => self.auto_detect(&content),
            ImportFormat::AgentsMd => self.import_agents_md(&content),
            ImportFormat::OpenSpec => self.import_open_spec(&content),
            ImportFormat::PlainMarkdown => self.import_plain_markdown(&content),
        }
    }

    fn auto_detect(&self, content: &str) -> CoreResult<SpecFile> {
        if content.contains("agents.md") || content.contains("# Agent") {
            self.import_agents_md(content)
        } else if content.contains("openapi") || content.contains("# OpenSpec") {
            self.import_open_spec(content)
        } else {
            self.import_plain_markdown(content)
        }
    }

    /// Import from agents.md-style files.
    fn import_agents_md(&self, content: &str) -> CoreResult<SpecFile> {
        let mut title = "Imported Spec".to_string();
        let mut goals = Vec::new();
        let mut non_goals = Vec::new();
        let mut acceptance_criteria = Vec::new();
        let mut constraints = Vec::new();
        let mut current_section: Option<&str> = None;
        let mut body_lines: Vec<String> = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with("# ") && title == "Imported Spec" {
                title = trimmed[2..].to_string();
                continue;
            }

            if trimmed.starts_with("## ") || trimmed.starts_with("### ") {
                let heading = trimmed.to_lowercase();
                if heading.contains("goal") && !heading.contains("non") {
                    current_section = Some("goals");
                } else if heading.contains("non-goal") || heading.contains("non goal") {
                    current_section = Some("non_goals");
                } else if heading.contains("acceptance") || heading.contains("criteria") {
                    current_section = Some("acceptance");
                } else if heading.contains("constraint") {
                    current_section = Some("constraints");
                } else {
                    current_section = None;
                }
                body_lines.push(line.to_string());
                continue;
            }

            match current_section {
                Some("goals")
                    if (trimmed.starts_with("- ") || trimmed.starts_with("* ")) => {
                        goals.push(trimmed[2..].to_string());
                    }
                Some("non_goals")
                    if (trimmed.starts_with("- ") || trimmed.starts_with("* ")) => {
                        non_goals.push(trimmed[2..].to_string());
                    }
                Some("acceptance") => {
                    if trimmed.starts_with("- [") || trimmed.starts_with("* [") {
                        let desc = if trimmed.contains("] ") {
                            trimmed.split("] ").nth(1).unwrap_or(trimmed).to_string()
                        } else {
                            trimmed.to_string()
                        };
                        acceptance_criteria.push(desc);
                    } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                        acceptance_criteria.push(trimmed[2..].to_string());
                    }
                }
                Some("constraints")
                    if (trimmed.starts_with("- ") || trimmed.starts_with("* ")) => {
                        constraints.push(trimmed[2..].to_string());
                    }
                _ => {}
            }

            body_lines.push(line.to_string());
        }

        let id = Self::to_kebab_case(&title);

        // Build sections
        let mut sections = Vec::new();
        if !goals.is_empty() {
            sections.push(SpecSection {
                heading: Some("Goals".to_string()),
                level: 2,
                body: goals.iter().map(|g| format!("- {}", g)).collect::<Vec<_>>().join("\n"),
                metadata: Default::default(),
            });
        }
        if !non_goals.is_empty() {
            sections.push(SpecSection {
                heading: Some("Non-Goals".to_string()),
                level: 2,
                body: non_goals.iter().map(|g| format!("- {}", g)).collect::<Vec<_>>().join("\n"),
                metadata: Default::default(),
            });
        }
        if !acceptance_criteria.is_empty() {
            sections.push(SpecSection {
                heading: Some("Acceptance Criteria".to_string()),
                level: 2,
                body: acceptance_criteria
                    .iter()
                    .enumerate()
                    .map(|(i, ac)| format!("- [ ] AC-{}: {}", i + 1, ac))
                    .collect::<Vec<_>>()
                    .join("\n"),
                metadata: Default::default(),
            });
        }
        if !constraints.is_empty() {
            sections.push(SpecSection {
                heading: Some("Constraints".to_string()),
                level: 2,
                body: constraints.iter().map(|c| format!("- {}", c)).collect::<Vec<_>>().join("\n"),
                metadata: Default::default(),
            });
        }

        let frontmatter = SpecFrontmatter {
            id: id.clone(),
            title: title.clone(),
            status: SpecStatus::Draft,
            tags: vec![],
            assigned_model: None,
            priority: Priority::Normal,
            dependencies: vec![],
            estimate: None,
        };

        let raw = format!(
            "---\nid: {}\ntitle: \"{}\"\nstatus: draft\npriority: normal\ntags: []\ndependencies: []\n---\n\n{}",
            id,
            title,
            body_lines.join("\n")
        );

        Ok(SpecFile {
            frontmatter,
            sections,
            raw,
        })
    }

    /// Import from OpenSpec-style YAML frontmatter.
    fn import_open_spec(&self, content: &str) -> CoreResult<SpecFile> {
        // OpenSpec is close to our native format — try direct parse first
        let parser = SpecParser::new();
        parser.parse(content).map_err(crate::CoreError::Spec)
    }

    /// Import plain markdown with heuristics.
    fn import_plain_markdown(&self, content: &str) -> CoreResult<SpecFile> {
        self.import_agents_md(content)
    }

    fn to_kebab_case(s: &str) -> String {
        s.to_lowercase()
            .replace(" ", "-")
            .replace("_", "-")
            .replace(|c: char| !c.is_alphanumeric() && c != '-', "")
            .replace("--", "-")
    }
}

impl Default for SpecImporter {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImportFormat {
    AutoDetect,
    AgentsMd,
    OpenSpec,
    PlainMarkdown,
}
