//! End-to-end checks that agent-instruction directories are readable but not
//! writable. This is the property that stops an agent rewriting its own
//! instructions.

use ratchet_sandbox::{SandboxGuard, policy::SandboxPolicy};
use ratchet_tools::{ToolContext, ToolExecutor, ToolRegistry};
use serde_json::json;
use std::path::PathBuf;

struct Fixture {
    _dir: tempfile::TempDir,
    ctx: ToolContext,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join(".agents/skills/rust-best-practices")).unwrap();
    std::fs::write(
        root.join(".agents/skills/rust-best-practices/SKILL.md"),
        "# Rust best practices\n\nPrefer borrowing over cloning.\n",
    )
    .unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn version() {}\n").unwrap();

    let policy = SandboxPolicy {
        allowed_paths: vec![root.join("src"), root.join(".ratchet")],
        read_only_paths: vec![root.join(".agents")],
        shell_allowlist: vec!["cargo test".to_string()],
        network_allowed: false,
        approval_policy: Default::default(),
    };

    Fixture {
        _dir: dir,
        ctx: ToolContext {
            cwd: root,
            sandbox: SandboxGuard::new(policy),
            registry: ToolRegistry::new(),
            test_command: None,
        },
    }
}

#[tokio::test]
async fn agent_can_read_a_skill() {
    let f = fixture();
    let executor = ToolExecutor::new();

    let value = executor
        .execute(
            &f.ctx,
            "file_read",
            json!({"path": ".agents/skills/rust-best-practices/SKILL.md"}),
        )
        .await
        .expect("skills must be readable for project context");

    let content = value["content"].as_str().unwrap_or_default();
    assert!(
        content.contains("Prefer borrowing over cloning"),
        "unexpected content: {content}"
    );
}

#[tokio::test]
async fn agent_cannot_write_a_skill() {
    let f = fixture();
    let executor = ToolExecutor::new();

    let error = executor
        .execute(
            &f.ctx,
            "file_write",
            json!({
                "path": ".agents/skills/rust-best-practices/SKILL.md",
                "content": "# Rewritten by the agent"
            }),
        )
        .await
        .expect_err("writing agent instructions must be refused");

    let message = error.to_string();
    assert!(
        message.contains("readable but not writable"),
        "error should explain why: {message}"
    );

    // And the file is untouched.
    let original = std::fs::read_to_string(
        PathBuf::from(&f.ctx.cwd).join(".agents/skills/rust-best-practices/SKILL.md"),
    )
    .unwrap();
    assert!(original.contains("Prefer borrowing over cloning"));
    assert!(!original.contains("Rewritten by the agent"));
}

#[tokio::test]
async fn agent_cannot_create_a_new_skill_file() {
    let f = fixture();
    let executor = ToolExecutor::new();

    executor
        .execute(
            &f.ctx,
            "file_write",
            json!({"path": ".agents/skills/evil/SKILL.md", "content": "# injected"}),
        )
        .await
        .expect_err("writing anywhere under .agents must be refused");

    assert!(
        !PathBuf::from(&f.ctx.cwd)
            .join(".agents/skills/evil")
            .exists()
    );
}

#[tokio::test]
async fn agent_cannot_patch_a_skill() {
    let f = fixture();
    let executor = ToolExecutor::new();

    executor
        .execute(
            &f.ctx,
            "file_patch",
            json!({
                "path": ".agents/skills/rust-best-practices/SKILL.md",
                "old_text": "Prefer borrowing",
                "new_text": "Prefer cloning"
            }),
        )
        .await
        .expect_err("patching agent instructions must be refused");
}

#[tokio::test]
async fn ordinary_source_files_remain_writable() {
    let f = fixture();
    let executor = ToolExecutor::new();

    executor
        .execute(
            &f.ctx,
            "file_write",
            json!({"path": "src/lib.rs", "content": "pub fn version() -> u8 { 1 }\n"}),
        )
        .await
        .expect("source files must stay writable");

    let written = std::fs::read_to_string(PathBuf::from(&f.ctx.cwd).join("src/lib.rs")).unwrap();
    assert!(written.contains("pub fn version() -> u8"));
}

#[tokio::test]
async fn paths_outside_the_project_are_still_denied() {
    let f = fixture();
    let executor = ToolExecutor::new();

    let error = executor
        .execute(&f.ctx, "file_read", json!({"path": "../../etc/passwd"}))
        .await
        .expect_err("escaping the project must be refused");

    // A different error from the read-only case: the fix is different.
    assert!(
        error.to_string().contains("outside the allowed scope"),
        "got: {error}"
    );
}

#[tokio::test]
async fn skills_are_listed_in_the_repository_map() {
    // The agent should discover skills without being told where they are.
    let f = fixture();
    let map = ratchet_tools::repo_map(&f.ctx.cwd, 100);
    assert!(
        map.contains(".agents/skills/rust-best-practices/SKILL.md"),
        "repo map should expose skills, got:\n{map}"
    );
}
