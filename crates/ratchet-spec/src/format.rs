use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// A parsed spec file with frontmatter and markdown body sections.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpecFile {
    pub frontmatter: SpecFrontmatter,
    pub sections: Vec<SpecSection>,
    pub raw: String,
}

/// YAML frontmatter embedded in a `.spec.md` file.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SpecFrontmatter {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub status: SpecStatus,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub assigned_model: Option<String>,
    #[serde(default)]
    pub priority: Priority,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimate: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpecStatus {
    #[default]
    Draft,
    Review,
    Approved,
    InProgress,
    Implemented,
    Verified,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Priority {
    #[default]
    Normal,
    Low,
    High,
    Critical,
}

/// A single markdown section within a spec file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpecSection {
    pub heading: Option<String>,
    pub level: u8,
    pub body: String,
    #[serde(default)]
    pub metadata: IndexMap<String, String>,
}

impl SpecFile {
    pub fn section(&self, title: &str) -> Option<&SpecSection> {
        self.sections
            .iter()
            .find(|s| s.heading.as_deref() == Some(title))
    }

    pub fn section_contains(&self, title: &str) -> Vec<&SpecSection> {
        self.sections
            .iter()
            .filter(|s| {
                s.heading
                    .as_deref()
                    .map(|h| h.to_lowercase().contains(&title.to_lowercase()))
                    .unwrap_or(false)
            })
            .collect()
    }

    pub fn goals_section(&self) -> Option<&SpecSection> {
        self.section("Goals")
            .or_else(|| self.section(" goals"))
            .or_else(|| self.section_contains("goal").first().copied())
    }

    pub fn acceptance_criteria_section(&self) -> Option<&SpecSection> {
        self.section("Acceptance Criteria")
            .or_else(|| self.section_contains("acceptance").first().copied())
    }

    pub fn non_goals_section(&self) -> Option<&SpecSection> {
        self.section("Non-Goals")
            .or_else(|| self.section_contains("non-goal").first().copied())
    }
}
