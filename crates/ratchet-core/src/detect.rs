//! Project detection, so `init` fits the repository it is dropped into.
//!
//! The default sandbox policy is Rust-shaped. Dropping that into an existing
//! Go or Python repo produces a config whose allow-list permits the wrong
//! commands and whose path list omits the real source directories — the agent
//! then gets blocked for reasons the user cannot see.
//!
//! Detection is deliberately shallow and literal: it looks for manifest files
//! and checks which conventional directories exist. It never guesses.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// An ecosystem Ratchet recognised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Ecosystem {
    Rust,
    Node,
    Python,
    Go,
    Ruby,
    Java,
    Php,
    Make,
}

impl Ecosystem {
    pub fn label(&self) -> &'static str {
        match self {
            Ecosystem::Rust => "Rust",
            Ecosystem::Node => "Node/TypeScript",
            Ecosystem::Python => "Python",
            Ecosystem::Go => "Go",
            Ecosystem::Ruby => "Ruby",
            Ecosystem::Java => "Java",
            Ecosystem::Php => "PHP",
            Ecosystem::Make => "Make",
        }
    }

    /// Commands that are safe to run without asking.
    pub fn safe_commands(&self) -> &'static [&'static str] {
        match self {
            Ecosystem::Rust => &[
                "cargo test",
                "cargo build",
                "cargo check",
                "cargo fmt",
                "cargo clippy",
            ],
            Ecosystem::Node => &[
                "npm test",
                "npm run build",
                "npm run lint",
                "npm run typecheck",
                "pnpm test",
                "yarn test",
                "npx tsc",
            ],
            Ecosystem::Python => &[
                "pytest",
                "python -m pytest",
                "python -m unittest",
                "ruff check",
                "mypy",
                "black --check",
            ],
            Ecosystem::Go => &[
                "go test ./...",
                "go build ./...",
                "go vet ./...",
                "gofmt -l .",
            ],
            Ecosystem::Ruby => &["bundle exec rspec", "bundle exec rake", "rubocop"],
            Ecosystem::Java => &[
                "mvn test",
                "mvn verify",
                "./gradlew test",
                "./gradlew build",
            ],
            Ecosystem::Php => &[
                "composer test",
                "vendor/bin/phpunit",
                "vendor/bin/pint --test",
            ],
            Ecosystem::Make => &["make test", "make build", "make lint"],
        }
    }
}

/// What `init` found in the repository.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectedProject {
    pub ecosystems: Vec<Ecosystem>,
    /// Directories that exist and look like they hold project content.
    pub source_dirs: Vec<String>,
    /// Directory names that exist but are usually not hand-edited.
    pub skipped_dirs: Vec<String>,
    /// Readable but never written: agent instructions and skills.
    pub skill_dirs: Vec<String>,
    /// Versioned config that exists but is not in the write allow-list.
    pub other_config_dirs: Vec<String>,
    /// Combined, de-duplicated allow-list.
    pub shell_allowlist: Vec<String>,
    /// Best guess at the project's test command, if any.
    pub test_command: Option<String>,
}

impl DetectedProject {
    pub fn primary(&self) -> Option<Ecosystem> {
        self.ecosystems.first().copied()
    }

    pub fn summary(&self) -> String {
        if self.ecosystems.is_empty() {
            return "not detected (using generic defaults)".to_string();
        }
        self.ecosystems
            .iter()
            .map(|e| e.label())
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

/// Directories that are safe to let the agent write to when they exist.
const CANDIDATE_SOURCE_DIRS: &[&str] = &[
    "src",
    "lib",
    "app",
    "apps",
    "packages",
    "cmd",
    "internal",
    "pkg",
    "tests",
    "test",
    "spec",
    "docs",
    "examples",
    "migrations",
    "scripts",
    "public",
    "assets",
    "styles",
    "components",
];

/// Directories that are vendored, generated, or otherwise off-limits.
const ALWAYS_SKIPPED: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "vendor",
    ".venv",
    "venv",
    "__pycache__",
    ".next",
    ".nuxt",
    "coverage",
    ".gradle",
];

/// Directories holding agent instructions, skills and tooling configuration.
///
/// These are read so the agent picks up project conventions, but never
/// written: allowing writes would let an agent rewrite its own instructions.
const SKILL_DIRS: &[&str] = &[
    ".agents",
    ".claude",
    ".cursor",
    ".continue",
    ".codex",
    ".gemini",
    ".aider",
    ".pi",
];

/// Versioned project configuration that exists but is not a source directory.
/// Reported so the user can opt in deliberately.
const OTHER_CONFIG_DIRS: &[&str] = &[".github", ".vscode", ".idea", ".devcontainer"];

/// Inspect a repository root.
pub fn detect(root: &Path) -> DetectedProject {
    let has = |name: &str| root.join(name).exists();

    // --- ecosystems, in priority order ---------------------------------
    let mut ecosystems = Vec::new();

    if has("Cargo.toml") {
        ecosystems.push(Ecosystem::Rust);
    }
    if has("package.json") {
        ecosystems.push(Ecosystem::Node);
    }
    if has("pyproject.toml") || has("setup.py") || has("requirements.txt") || has("Pipfile") {
        ecosystems.push(Ecosystem::Python);
    }
    if has("go.mod") {
        ecosystems.push(Ecosystem::Go);
    }
    if has("Gemfile") {
        ecosystems.push(Ecosystem::Ruby);
    }
    if has("pom.xml") || has("build.gradle") || has("build.gradle.kts") {
        ecosystems.push(Ecosystem::Java);
    }
    if has("composer.json") {
        ecosystems.push(Ecosystem::Php);
    }
    if has("Makefile") && ecosystems.is_empty() {
        // Only fall back to Make when nothing more specific matched.
        ecosystems.push(Ecosystem::Make);
    }

    // --- directories ----------------------------------------------------
    let mut source_dirs: Vec<String> = CANDIDATE_SOURCE_DIRS
        .iter()
        .filter(|d| root.join(d).is_dir())
        .map(|d| d.to_string())
        .collect();

    let skipped_dirs: Vec<String> = ALWAYS_SKIPPED
        .iter()
        .filter(|d| root.join(d).is_dir())
        .map(|d| d.to_string())
        .collect();

    // Skills are readable context, not write targets.
    let skill_dirs: Vec<String> = SKILL_DIRS
        .iter()
        .filter(|d| root.join(d).is_dir())
        .map(|d| d.to_string())
        .collect();

    let other_config_dirs: Vec<String> = OTHER_CONFIG_DIRS
        .iter()
        .filter(|d| root.join(d).is_dir())
        .map(|d| d.to_string())
        .collect();

    // `src` is near-universal; include it even in an empty repo so the first
    // run has somewhere to write.
    if source_dirs.is_empty() {
        source_dirs.push("src".to_string());
    }

    // --- allow-lists ----------------------------------------------------
    let mut shell_allowlist: Vec<String> = Vec::new();
    for ecosystem in &ecosystems {
        for command in ecosystem.safe_commands() {
            let command = command.to_string();
            if !shell_allowlist.contains(&command) {
                shell_allowlist.push(command);
            }
        }
    }

    // Read-only git is always safe, and the review gate depends on it.
    for command in ["git status", "git diff", "git log", "git show"] {
        let command = command.to_string();
        if !shell_allowlist.contains(&command) {
            shell_allowlist.push(command);
        }
    }

    // If nothing matched, offer the most common generic entries.
    if ecosystems.is_empty() {
        shell_allowlist.extend(
            ["make test", "make build", "make lint"]
                .iter()
                .map(|s| s.to_string()),
        );
    }

    let test_command = ecosystems
        .first()
        .and_then(|e| e.safe_commands().first().map(|c| c.to_string()));

    DetectedProject {
        ecosystems,
        source_dirs,
        skipped_dirs,
        skill_dirs,
        other_config_dirs,
        shell_allowlist,
        test_command,
    }
}

/// Skill files worth telling the model about, as `name: relative/path` pairs.
///
/// Only `SKILL.md` under `*/skills/*/` is considered, which is the convention
/// these directories use; anything else is left to the agent to discover.
pub fn skill_files(root: &Path) -> Vec<String> {
    let mut found = Vec::new();

    for dir in SKILL_DIRS {
        let skills_root = root.join(dir).join("skills");
        let Ok(entries) = std::fs::read_dir(&skills_root) else {
            continue;
        };

        for entry in entries.flatten() {
            let candidate = entry.path().join("SKILL.md");
            if candidate.is_file() {
                if let Some(name) = entry.file_name().to_str() {
                    found.push(format!("{name}: {}", short_path(root, &candidate)));
                }
            }
        }
    }

    found.sort();
    found
}

fn short_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn detects_rust() {
        let dir = tmp();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();

        let found = detect(dir.path());
        assert_eq!(found.primary(), Some(Ecosystem::Rust));
        assert!(found.source_dirs.contains(&"src".to_string()));
        assert!(found.shell_allowlist.iter().any(|c| c == "cargo test"));
        assert_eq!(found.test_command.as_deref(), Some("cargo test"));
    }

    #[test]
    fn detects_python_without_a_cargo_manifest() {
        let dir = tmp();
        std::fs::write(dir.path().join("pyproject.toml"), "").unwrap();
        std::fs::create_dir(dir.path().join("tests")).unwrap();

        let found = detect(dir.path());
        assert_eq!(found.primary(), Some(Ecosystem::Python));
        assert!(found.shell_allowlist.iter().any(|c| c == "pytest"));
        assert!(!found.shell_allowlist.iter().any(|c| c == "cargo test"));
    }

    #[test]
    fn detects_go() {
        let dir = tmp();
        std::fs::write(dir.path().join("go.mod"), "module x").unwrap();
        std::fs::create_dir(dir.path().join("cmd")).unwrap();
        std::fs::create_dir(dir.path().join("internal")).unwrap();

        let found = detect(dir.path());
        assert_eq!(found.primary(), Some(Ecosystem::Go));
        assert!(found.source_dirs.contains(&"cmd".to_string()));
        assert!(found.source_dirs.contains(&"internal".to_string()));
        assert!(found.shell_allowlist.iter().any(|c| c == "go test ./..."));
    }

    #[test]
    fn detects_a_mixed_repository() {
        // e.g. a Tauri app: a Rust backend plus a JS frontend.
        let dir = tmp();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();

        let found = detect(dir.path());
        assert_eq!(found.ecosystems.len(), 2);
        assert!(found.shell_allowlist.iter().any(|c| c == "cargo test"));
        assert!(found.shell_allowlist.iter().any(|c| c == "npm test"));
        assert!(found.summary().contains("Rust"));
        assert!(found.summary().contains("Node"));
    }

    #[test]
    fn unknown_project_falls_back_to_something_usable() {
        let dir = tmp();
        let found = detect(dir.path());

        assert!(found.ecosystems.is_empty());
        assert_eq!(found.source_dirs, vec!["src".to_string()]);
        // git commands are always allowed, and the list is never empty.
        assert!(found.shell_allowlist.iter().any(|c| c == "git diff"));
        assert!(!found.shell_allowlist.is_empty());
        assert!(found.summary().contains("not detected"));
    }

    #[test]
    fn reports_vendored_directories_it_ignore() {
        let dir = tmp();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        std::fs::create_dir(dir.path().join("node_modules")).unwrap();

        let found = detect(dir.path());
        assert!(found.skipped_dirs.contains(&"node_modules".to_string()));
        // Vendored code must never enter the write allow-list.
        assert!(!found.source_dirs.contains(&"node_modules".to_string()));
    }

    #[test]
    fn detects_agent_skill_directories() {
        let dir = tmp();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        std::fs::create_dir_all(dir.path().join(".agents/skills/rust-best-practices")).unwrap();
        std::fs::write(
            dir.path()
                .join(".agents/skills/rust-best-practices/SKILL.md"),
            "# skill",
        )
        .unwrap();

        let found = detect(dir.path());
        assert!(found.skill_dirs.contains(&".agents".to_string()));
        // Skills must never enter the write allow-list.
        assert!(!found.source_dirs.contains(&".agents".to_string()));

        let skills = skill_files(dir.path());
        assert_eq!(skills.len(), 1);
        assert!(skills[0].starts_with("rust-best-practices: "));
        assert!(skills[0].ends_with(".agents/skills/rust-best-practices/SKILL.md"));
    }

    #[test]
    fn finds_skills_across_several_tooling_directories() {
        let dir = tmp();
        for (tool, skill) in [(".agents", "alpha"), (".claude", "beta")] {
            let path = dir.path().join(tool).join("skills").join(skill);
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("SKILL.md"), "# skill").unwrap();
        }

        let skills = skill_files(dir.path());
        assert_eq!(skills.len(), 2);
        assert!(skills[0].starts_with("alpha:"));
        assert!(skills[1].starts_with("beta:"));
    }

    #[test]
    fn no_skill_directory_means_no_skills() {
        let dir = tmp();
        assert!(skill_files(dir.path()).is_empty());
        assert!(detect(dir.path()).skill_dirs.is_empty());
    }

    #[test]
    fn reports_versioned_config_directories() {
        let dir = tmp();
        std::fs::create_dir_all(dir.path().join(".github/workflows")).unwrap();
        std::fs::create_dir_all(dir.path().join(".vscode")).unwrap();

        let found = detect(dir.path());
        assert!(found.other_config_dirs.contains(&".github".to_string()));
        assert!(found.other_config_dirs.contains(&".vscode".to_string()));
        // Present, but not silently added to the write scope.
        assert!(!found.source_dirs.contains(&".github".to_string()));
    }

    #[test]
    fn allowlist_has_no_duplicates() {
        let dir = tmp();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        std::fs::write(dir.path().join("Makefile"), "").unwrap();

        let found = detect(dir.path());
        let mut sorted = found.shell_allowlist.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), found.shell_allowlist.len());
    }
}
