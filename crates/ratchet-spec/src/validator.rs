use crate::{
    format::SpecFile,
    schema::SpecSchema,
    SpecResult,
};

/// Validates spec files against structural and semantic rules.
pub struct SpecValidator;

impl SpecValidator {
    pub fn new() -> Self {
        Self
    }

    pub fn validate(&self, spec: &SpecFile) -> SpecResult<Vec<ValidationIssue>> {
        let mut issues = Vec::new();

        // Required frontmatter fields
        if spec.frontmatter.id.is_empty() {
            issues.push(ValidationIssue::error(
                "frontmatter",
                "missing required field: id",
            ));
        }
        if spec.frontmatter.title.is_empty() {
            issues.push(ValidationIssue::error(
                "frontmatter",
                "missing required field: title",
            ));
        }

        // Must have at least goals or acceptance criteria
        if spec.goals_section().is_none() && spec.acceptance_criteria_section().is_none() {
            issues.push(ValidationIssue::warning(
                "structure",
                "spec should contain either Goals or Acceptance Criteria section",
            ));
        }

        // ID should be kebab-case
        if !spec.frontmatter.id.is_empty() && !is_kebab_case(&spec.frontmatter.id) {
            issues.push(ValidationIssue::warning(
                "frontmatter.id",
                "spec id should be kebab-case (e.g., 'billing-reminders')",
            ));
        }

        Ok(issues)
    }

    pub fn validate_schema(&self, schema: &SpecSchema) -> SpecResult<Vec<ValidationIssue>> {
        let mut issues = Vec::new();

        if schema.id.is_empty() {
            issues.push(ValidationIssue::error("schema.id", "id is required"));
        }
        if schema.title.is_empty() {
            issues.push(ValidationIssue::error("schema.title", "title is required"));
        }
        if schema.acceptance_criteria.is_empty() {
            issues.push(ValidationIssue::warning(
                "schema.acceptance_criteria",
                "no acceptance criteria defined",
            ));
        }

        // Check for duplicate criterion IDs
        let mut seen = std::collections::HashSet::new();
        for ac in &schema.acceptance_criteria {
            if !seen.insert(ac.id.clone()) {
                issues.push(ValidationIssue::error(
                    "schema.acceptance_criteria",
                    format!("duplicate criterion id: {}", ac.id),
                ));
            }
        }

        Ok(issues)
    }

    pub fn is_valid(&self, spec: &SpecFile) -> SpecResult<bool> {
        let issues = self.validate(spec)?;
        Ok(issues.iter().all(|i| !i.is_error()))
    }
}

impl Default for SpecValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidationIssue {
    pub level: ValidationLevel,
    pub location: String,
    pub message: String,
}

impl ValidationIssue {
    pub fn error(location: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: ValidationLevel::Error,
            location: location.into(),
            message: message.into(),
        }
    }

    pub fn warning(location: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: ValidationLevel::Warning,
            location: location.into(),
            message: message.into(),
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self.level, ValidationLevel::Error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValidationLevel {
    Error,
    Warning,
    Info,
}

fn is_kebab_case(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_lowercase() || c == '-' || c.is_numeric())
        && !s.starts_with('-')
        && !s.ends_with('-')
        && !s.contains("--")
}
