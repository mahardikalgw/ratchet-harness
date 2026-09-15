use ratchet_core::import::{ImportFormat, SpecImporter};

const AGENTS_MD: &str = r#"# Payment Webhook Handling

## Goals
- Receive Stripe webhooks
- Retry failed processing

## Non-Goals
- Handling refunds

## Acceptance Criteria
- [ ] AC-1: Signature validated
- [ ] AC-2: Status updates in 5s

## Constraints
- Handle 1000/min
"#;

#[tokio::test]
async fn imports_agents_md_format() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agents.md");
    tokio::fs::write(&path, AGENTS_MD).await.unwrap();

    let importer = SpecImporter::new();
    let spec = importer.import(&path, ImportFormat::AgentsMd).await.unwrap();

    assert_eq!(spec.frontmatter.id, "payment-webhook-handling");
    assert_eq!(spec.frontmatter.title, "Payment Webhook Handling");

    let goals = spec.section("Goals").expect("goals section");
    assert!(goals.body.contains("Receive Stripe webhooks"));

    let ac = spec
        .section("Acceptance Criteria")
        .expect("acceptance section");
    assert!(ac.body.contains("AC-1"));
    assert!(ac.body.contains("AC-2"));

    let ng = spec.section("Non-Goals").expect("non-goals section");
    assert!(ng.body.contains("Handling refunds"));
}

#[tokio::test]
async fn auto_detect_falls_back_to_markdown() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("spec.md");
    tokio::fs::write(&path, AGENTS_MD).await.unwrap();

    let importer = SpecImporter::new();
    let spec = importer
        .import(&path, ImportFormat::AutoDetect)
        .await
        .unwrap();

    assert_eq!(spec.frontmatter.id, "payment-webhook-handling");
}

#[tokio::test]
async fn reimporting_own_output_is_stable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agents.md");
    tokio::fs::write(&path, AGENTS_MD).await.unwrap();

    let importer = SpecImporter::new();
    let first = importer.import(&path, ImportFormat::AgentsMd).await.unwrap();

    // Write the imported raw spec and re-import it (OpenSpec path uses the native parser)
    let round_path = dir.path().join("round.spec.md");
    tokio::fs::write(&round_path, &first.raw).await.unwrap();

    let second = importer
        .import(&round_path, ImportFormat::OpenSpec)
        .await
        .unwrap();

    assert_eq!(second.frontmatter.id, first.frontmatter.id);
    assert_eq!(second.frontmatter.title, first.frontmatter.title);
}
