use anyhow::Result;
use ratchet_core::{
    config::{ProjectConfig, ProjectSettings, RoutingSettings},
    detect::detect,
};
use ratchet_sandbox::policy::SandboxPolicy;
use std::path::{Path, PathBuf};

pub async fn run(project_dir: &Path, name: &str) -> Result<()> {
    let config_path = project_dir.join("ratchet.toml");
    if config_path.exists() {
        anyhow::bail!(
            "ratchet.toml sudah ada di {}.\nHapus dulu kalau memang mau generate ulang.",
            config_path.display()
        );
    }

    // --- look at what is actually here ---------------------------------
    let detected = detect(project_dir);

    println!("🔍 Memeriksa proyek…");
    println!("   bahasa        : {}", detected.summary());
    println!("   folder sumber : {}", detected.source_dirs.join(", "));
    if !detected.skipped_dirs.is_empty() {
        println!(
            "   diabaikan     : {} (vendor/generated)",
            detected.skipped_dirs.join(", ")
        );
    }

    // --- scaffold the directories --------------------------------------
    let ratchet_dir = project_dir.join(".ratchet");
    for sub in ["spec", "plan", "tasks", "verify", "review"] {
        tokio::fs::create_dir_all(ratchet_dir.join(sub)).await?;
    }

    // --- write a config that fits this repository ----------------------
    let mut allowed_paths: Vec<PathBuf> = detected.source_dirs.iter().map(PathBuf::from).collect();
    allowed_paths.push(PathBuf::from(".ratchet"));

    let config = ProjectConfig {
        project: ProjectSettings {
            name: name.to_string(),
            description: None,
        },
        providers: Default::default(),
        routing: RoutingSettings::default(),
        sandbox: SandboxPolicy {
            allowed_paths,
            shell_allowlist: detected.shell_allowlist.clone(),
            network_allowed: false,
            approval_policy: Default::default(),
        },
        mcp: Default::default(),
        delegation: Default::default(),
        plugins: Vec::new(),
        ratchet_dir: PathBuf::from(".ratchet"),
    };
    config.save(&config_path)?;

    let intent_md = ratchet_dir.join("intent.md");
    if !intent_md.exists() {
        tokio::fs::write(
            &intent_md,
            format!("# Intent: {name}\n\nTulis di sini apa yang mau dibangun.\n"),
        )
        .await?;
    }

    // --- report --------------------------------------------------------
    println!();
    println!("✅ Siap. Dibuat:");
    println!("   {}", config_path.display());
    println!("   {}/", ratchet_dir.display());

    if let Some(test) = &detected.test_command {
        println!();
        println!("   Perintah test yang dideteksi: {test}");
    }

    println!();
    println!("Langkah berikutnya:");
    println!("  ratchet provider add <nama> --kind <jenis> --key-env <ENV_VAR>");
    println!("  ratchet                      # mulai ngobrol");
    println!();
    println!("Cek kapan saja dengan: ratchet doctor");
    println!();
    println!("Catatan: kode dan file yang sudah ada tidak diubah sama sekali.");

    Ok(())
}
