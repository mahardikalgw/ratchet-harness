use anyhow::Result;
use ratchet_core::{
    config::{ProjectConfig, ProjectSettings, RoutingSettings},
    detect::{detect, skill_files},
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

    let skills = skill_files(project_dir);

    println!("🔍 Memeriksa proyek…");
    println!("   bahasa        : {}", detected.summary());
    println!("   folder sumber : {}", detected.source_dirs.join(", "));
    if !detected.skill_dirs.is_empty() {
        println!(
            "   skills        : {} (bisa dibaca, tidak bisa diubah)",
            detected.skill_dirs.join(", ")
        );
    }
    if !detected.skipped_dirs.is_empty() {
        println!(
            "   diabaikan     : {} (vendor/generated)",
            detected.skipped_dirs.join(", ")
        );
    }
    if !detected.other_config_dirs.is_empty() {
        println!(
            "   tidak diizinkan: {} (tambahkan manual kalau perlu)",
            detected.other_config_dirs.join(", ")
        );
    }
    if !skills.is_empty() {
        println!();
        println!("   {} skill ditemukan:", skills.len());
        for skill in skills.iter().take(10) {
            println!("     • {}", skill.split(':').next().unwrap_or(skill));
        }
        if skills.len() > 10 {
            println!("     • … dan {} lagi", skills.len() - 10);
        }
    }

    // --- scaffold the directories --------------------------------------
    let ratchet_dir = project_dir.join(".ratchet");
    for sub in ["spec", "plan", "tasks", "verify", "review"] {
        tokio::fs::create_dir_all(ratchet_dir.join(sub)).await?;
    }

    // --- write a config that fits this repository ----------------------
    let mut allowed_paths: Vec<PathBuf> = detected.source_dirs.iter().map(PathBuf::from).collect();
    allowed_paths.push(PathBuf::from(".ratchet"));

    // Skills and agent tooling are context, not a write target: the agent must
    // not be able to rewrite its own instructions.
    let read_only_paths: Vec<PathBuf> = detected.skill_dirs.iter().map(PathBuf::from).collect();

    let config = ProjectConfig {
        project: ProjectSettings {
            name: name.to_string(),
            description: None,
            // Record what we detected so runs do not depend on guessing later,
            // and so an unrecognised language is still drivable.
            test_command: detected.test_command.clone(),
        },
        providers: Default::default(),
        routing: RoutingSettings::default(),
        sandbox: SandboxPolicy {
            allowed_paths,
            read_only_paths,
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

    // Specs, plans and reports belong in version control — they are the
    // decision record. Machine-local noise does not, and an existing project
    // should not have its own .gitignore rewritten, so scope this to .ratchet/.
    let ignore = ratchet_dir.join(".gitignore");
    if !ignore.exists() {
        tokio::fs::write(
            &ignore,
            "# Machine-local state: regenerated per machine, noisy in diffs.\n\
             metrics.jsonl\n\
             tool-output/\n",
        )
        .await?;
    }

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
    println!();
    println!("   .ratchet/ sebaiknya di-commit (spec, plan, laporan verifikasi).");
    println!("   State mesin-lokal sudah di-ignore lewat .ratchet/.gitignore.");

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
