//! Interactive session: talk your way to a spec, then let Ratchet build it.
//!
//! The point is that the user never edits a markdown file. They describe what
//! they want, answer questions, approve a spec, approve a plan, and watch it
//! run — with the conversation doing the work that file editing used to.

use anyhow::Result;
use ratchet_core::{
    AgentHarness, RunOverrides,
    discovery::{DiscoveryOutcome, Exchange, Question, QuestionKind, SpecDraft},
};
use ratchet_spec::SpecParser;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::helpers::load_project;
use crate::approval::CliApprovalHandler;

// ---------------------------------------------------------------------------
// Terminal helpers
// ---------------------------------------------------------------------------

const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
const CYAN: &str = "\x1b[36m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RESET: &str = "\x1b[0m";

fn say(text: &str) {
    println!("\n{CYAN}🤖{RESET} {text}");
}

fn info(text: &str) {
    println!("   {DIM}{text}{RESET}");
}

fn ok(text: &str) {
    println!("   {GREEN}✓{RESET} {text}");
}

fn warn(text: &str) {
    println!("   {YELLOW}!{RESET} {text}");
}

/// Read one line of input. Returns `None` on EOF.
fn read_input(prompt: &str) -> Result<Option<String>> {
    print!("{BOLD}{prompt}{RESET} ");
    std::io::stdout().flush()?;

    let mut line = String::new();
    if std::io::stdin().read_line(&mut line)? == 0 {
        return Ok(None);
    }
    Ok(Some(line.trim().to_string()))
}

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    /// Waiting for the opening request.
    Idle,
    /// Asking clarifying questions.
    Discovery,
    /// Spec proposed, awaiting approval or changes.
    SpecReview,
    /// Plan proposed, awaiting approval.
    PlanReview,
    /// Finished a run.
    Done,
}

struct Session {
    project_dir: PathBuf,
    harness: AgentHarness,
    phase: Phase,

    intent: String,
    transcript: Vec<Exchange>,
    round: usize,

    draft: Option<SpecDraft>,
    spec_id: Option<String>,
}

impl Session {
    /// Kick off elicitation for a fresh request.
    async fn start(&mut self, intent: String) -> Result<()> {
        self.intent = intent.clone();
        self.transcript.clear();
        self.round = 0;
        self.draft = None;
        self.spec_id = None;
        self.phase = Phase::Discovery;

        say(&format!("Baik — \"{intent}\"."));
        info("Saya tanya beberapa hal dulu supaya spec-nya tepat.");
        self.advance_discovery().await
    }

    /// One elicitation turn: ask questions, or receive the spec.
    async fn advance_discovery(&mut self) -> Result<()> {
        self.round += 1;

        let outcome = self
            .harness
            .draft_spec(&self.intent, &self.transcript, self.round)
            .await?;

        match outcome {
            DiscoveryOutcome::Questions {
                rationale,
                questions,
            } => {
                if !rationale.trim().is_empty() {
                    say(&rationale);
                } else {
                    say("Beberapa pertanyaan:");
                }
                for (i, question) in questions.iter().enumerate() {
                    let answer = match self.ask(question, i + 1, questions.len())? {
                        Some(a) => a,
                        None => return Ok(()), // EOF
                    };
                    self.transcript.push(Exchange {
                        question: question.question.clone(),
                        answer,
                    });
                }
                Box::pin(self.advance_discovery()).await
            }

            DiscoveryOutcome::Spec(draft) => {
                say("Saya sudah cukup. Ini spec yang saya usulkan:");
                println!();
                self.show_spec(&draft);
                self.draft = Some(draft);
                self.phase = Phase::SpecReview;
                println!();
                println!("   {DIM}enter = setuju · ketik perubahan untuk revisi · /batal{RESET}");
                Ok(())
            }

            DiscoveryOutcome::Unparseable(raw) => {
                warn("Model tidak memberi format yang bisa dibaca. Responsnya:");
                println!("   {DIM}{}{RESET}", raw.lines().next().unwrap_or(""));
                say("Coba jelaskan lagi secara singkat?");
                self.phase = Phase::Idle;
                Ok(())
            }
        }
    }

    /// Present one question and collect an answer.
    fn ask(&self, question: &Question, index: usize, total: usize) -> Result<Option<String>> {
        println!();
        println!("{BOLD}[{index}/{total}]{RESET} {}", question.question);

        match question.kind {
            QuestionKind::Choice if !question.options.is_empty() => {
                for (i, option) in question.options.iter().enumerate() {
                    println!("   {}. {}", i + 1, option);
                }
                let hint = match &question.default {
                    Some(d) => format!(" (enter = {d})"),
                    None => String::new(),
                };
                match read_input(&format!("Pilih{hint}:"))? {
                    None => Ok(None),
                    Some(input) => {
                        if input.is_empty() {
                            return Ok(Some(question.default.clone().unwrap_or_else(|| {
                                question.options.first().cloned().unwrap_or_default()
                            })));
                        }
                        // Accept either the number or the text.
                        if let Ok(n) = input.parse::<usize>() {
                            if let Some(option) = question.options.get(n.saturating_sub(1)) {
                                return Ok(Some(option.clone()));
                            }
                        }
                        Ok(Some(input))
                    }
                }
            }
            QuestionKind::Confirm => {
                let hint = question.default.as_deref().unwrap_or("y");
                match read_input(&format!("(y/n, enter = {hint}):"))? {
                    None => Ok(None),
                    Some(input) => {
                        if input.is_empty() {
                            return Ok(Some(hint.to_string()));
                        }
                        Ok(Some(input))
                    }
                }
            }
            _ => match read_input("Jawab:")? {
                None => Ok(None),
                Some(input) => {
                    if input.is_empty() {
                        if let Some(default) = &question.default {
                            return Ok(Some(default.clone()));
                        }
                    }
                    Ok(Some(input))
                }
            },
        }
    }

    fn show_spec(&self, draft: &SpecDraft) {
        println!(
            "   {BOLD}{}{RESET}  {DIM}(id: {}){RESET}",
            draft.title, draft.id
        );

        if !draft.goals.is_empty() {
            println!("\n   {BOLD}Tujuan{RESET}");
            for goal in &draft.goals {
                println!("     • {goal}");
            }
        }
        if !draft.non_goals.is_empty() {
            println!("\n   {BOLD}Bukan termasuk{RESET}");
            for item in &draft.non_goals {
                println!("     • {item}");
            }
        }

        println!("\n   {BOLD}Kriteria diterima{RESET}");
        for criterion in &draft.acceptance_criteria {
            let check = match draft.verification_of(criterion) {
                Some(ratchet_spec::schema::VerificationStep::Test { command, .. }) => {
                    format!("{GREEN}cek: {command}{RESET}")
                }
                Some(ratchet_spec::schema::VerificationStep::Diff { pattern }) => {
                    format!("{GREEN}cek: {pattern} berubah{RESET}")
                }
                Some(ratchet_spec::schema::VerificationStep::Lint { tool, .. }) => {
                    format!("{GREEN}cek: {tool}{RESET}")
                }
                _ => format!("{YELLOW}manual{RESET}"),
            };
            println!(
                "     {} {}  {DIM}({check}{DIM}){RESET}",
                criterion.id, criterion.description
            );
        }

        let auto = draft.auto_verifiable();
        let total = draft.acceptance_criteria.len();
        println!();
        if auto == total {
            ok(&format!("{total} kriteria bisa dicek otomatis"));
        } else {
            info(&format!(
                "{auto}/{total} kriteria bisa dicek otomatis, sisanya manual"
            ));
        }
    }

    fn set_idle(&mut self) {
        self.phase = Phase::Idle;
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Does this project have a usable model configured?
fn ensure_usable(project_dir: &Path) -> Result<()> {
    let (_, providers) = load_project(project_dir)?;
    if providers.is_empty() {
        anyhow::bail!(
            "belum ada model yang bisa dipakai.\n\
             Jalankan dulu:\n  \
             ratchet provider add <nama> --kind <jenis> --key-env <ENV_VAR>\n\n\
             Cek dengan: ratchet doctor"
        );
    }
    Ok(())
}

pub async fn run(project_dir: &Path, initial: Option<String>) -> Result<()> {
    // Not fatal: a pipe works fine (read_line returns EOF and the loop ends),
    // and this makes sessions scriptable. Risky shell commands still auto-deny
    // without a TTY, because the approval handler checks for itself.
    if !std::io::stdin().is_terminal() {
        eprintln!(
            "{DIM}(stdin bukan terminal — menjawab dari pipe. Gunakan `ratchet run` untuk CI.){RESET}"
        );
    }

    if !project_dir.join("ratchet.toml").exists() {
        anyhow::bail!("belum ada ratchet.toml — jalankan `ratchet init <nama>` dulu");
    }

    ensure_usable(project_dir)?;

    let (config, providers) = load_project(project_dir)?;
    let harness = AgentHarness::new(config, providers)
        .await?
        .with_approval(Arc::new(CliApprovalHandler));

    let mut session = Session {
        project_dir: project_dir.to_path_buf(),
        harness,
        phase: Phase::Idle,
        intent: String::new(),
        transcript: Vec::new(),
        round: 0,
        draft: None,
        spec_id: None,
    };

    println!();
    println!("{BOLD}Ratchet{RESET} {DIM}— ngobrol dulu, baru dikerjakan{RESET}");
    println!(
        "{DIM}Ketuk apa yang mau kamu buat. /help untuk bantuan, /keluar untuk keluar.{RESET}"
    );

    if let Some(intent) = initial {
        session.start(intent).await?;
    }

    loop {
        // --- nothing pending: get the next thing to do -------------------
        let input = match read_input("›")? {
            Some(i) => i,
            None => break, // EOF
        };

        // Enter with nothing typed means "approve" at either review gate.
        if input.is_empty() && !matches!(session.phase, Phase::SpecReview | Phase::PlanReview) {
            continue;
        }

        // --- slash commands ---------------------------------------------
        if input.starts_with('/') {
            match input.as_str() {
                "/keluar" | "/exit" | "/quit" => break,
                "/help" | "/bantuan" => print_help(),
                "/spec" => match &session.draft {
                    Some(draft) => session.show_spec(draft),
                    None => info("belum ada spec"),
                },
                "/status" => {
                    println!("   fase: {:?}", session.phase);
                    if let Some(id) = &session.spec_id {
                        println!("   spec: {id}");
                    }
                }
                "/reset" => {
                    say("Oke, mulai dari awal.");
                    session.set_idle();
                }
                other => warn(&format!("perintah tidak dikenal: {other}")),
            }
            continue;
        }

        match session.phase {
            Phase::Idle => {
                session.start(input).await?;
            }

            Phase::Discovery => {
                // Free text while questions are pending is treated as guidance.
                session.transcript.push(Exchange {
                    question: "(additional guidance)".to_string(),
                    answer: input,
                });
                session.advance_discovery().await?;
            }

            Phase::SpecReview => {
                if input.is_empty() {
                    // Approved: persist the spec and move on to planning. The
                    // user still reviews the plan before anything runs.
                    run_spec_approved(&mut session).await?;
                } else if input == "/batal" {
                    session.set_idle();
                } else {
                    session.transcript.push(Exchange {
                        question: "Feedback on the proposed spec".to_string(),
                        answer: input,
                    });
                    session.advance_discovery().await?;
                }
            }

            Phase::PlanReview => {
                if input.is_empty() {
                    execute(&mut session).await?;
                } else if input == "/batal" {
                    session.set_idle();
                } else {
                    warn(
                        "belum bisa revisi plan lewat chat — enter untuk jalan, /batal untuk batal",
                    );
                }
            }

            Phase::Done => {
                // Anything typed after a run starts a new request.
                session.start(input).await?;
            }
        }
    }

    println!();
    println!("{DIM}sampai jumpa.{RESET}");
    Ok(())
}

/// Persist the approved spec and produce a plan for review.
async fn run_spec_approved(session: &mut Session) -> Result<()> {
    let draft = match &session.draft {
        Some(d) => d.clone(),
        None => return Ok(()),
    };

    let raw = ratchet_core::discovery::render_spec(&draft, &session.intent);
    let spec_dir = session.project_dir.join(".ratchet").join("spec");
    tokio::fs::create_dir_all(&spec_dir).await?;
    let path = spec_dir.join(format!("{}.spec.md", draft.id));
    tokio::fs::write(&path, &raw).await?;

    ok(&format!("spec disimpan: {}", path.display()));
    session.spec_id = Some(draft.id.clone());

    // Plan from the spec we just wrote.
    let spec = SpecParser::new().parse(&raw)?;
    say("Sekarang saya susun rencana kerjanya…");

    match session.harness.load_or_plan(&spec).await {
        Ok(plan) => {
            println!();
            println!("   {BOLD}Rencana{RESET}");
            for node in &plan.task_graph.nodes {
                let deps: Vec<String> = plan
                    .task_graph
                    .dependencies_of(&node.id)
                    .iter()
                    .map(|d| d.id.0.clone())
                    .collect();
                let suffix = if deps.is_empty() {
                    String::new()
                } else {
                    format!("  {DIM}(setelah {}){RESET}", deps.join(", "))
                };
                println!("     {} {}{}", node.id, node.title, suffix);
            }

            if !plan.affected_modules.is_empty() {
                println!(
                    "\n   {DIM}modul: {}{RESET}",
                    plan.affected_modules.join(", ")
                );
            }

            session.phase = Phase::PlanReview;
            println!();
            println!("   {DIM}enter = jalankan · /batal{RESET}");
        }
        Err(e) => {
            warn(&format!("gagal menyusun rencana: {e}"));
            session.set_idle();
        }
    }

    Ok(())
}

/// Execute the plan and report back in plain language.
async fn execute(session: &mut Session) -> Result<()> {
    let Some(spec_id) = session.spec_id.clone() else {
        return Ok(());
    };

    let spec_path = session
        .project_dir
        .join(".ratchet")
        .join("spec")
        .join(format!("{spec_id}.spec.md"));
    let raw = tokio::fs::read_to_string(&spec_path).await?;
    let spec = SpecParser::new().parse(&raw)?;
    let plan = session.harness.load_or_plan(&spec).await?;

    say("Mulai kerja. Saya laporkan kalau sudah selesai.");
    println!();

    let report = session
        .harness
        .run_with(&plan, &spec, RunOverrides::default())
        .await;

    let report = match report {
        Ok(report) => report,
        Err(e) => {
            warn(&format!("gagal saat mengerjakan: {e}"));
            session.set_idle();
            return Ok(());
        }
    };

    // --- mechanical summary ------------------------------------------------
    let mut facts = String::new();
    for result in &report.results {
        facts.push_str(&format!(
            "- task {}: {}, {} turn(s), provider {}, biaya ${:.4}\n",
            result.task_id,
            result.status_str(),
            result.turns,
            result.provider,
            result.cost_usd
        ));
    }
    facts.push_str(&format!(
        "\nUji otomatis: {}\n",
        report.verification.summary
    ));
    for criterion in &report.verification.criterion_results {
        facts.push_str(&format!(
            "- {} [{}] {} ({})\n",
            criterion.criterion_id,
            format!("{:?}", criterion.status).to_lowercase(),
            criterion.description,
            criterion.note
        ));
    }
    facts.push_str(&format!(
        "\nFile berubah: {}\n",
        if report.review.changed_files.is_empty() {
            "(tidak ada)".to_string()
        } else {
            report.review.changed_files.join(", ")
        }
    ));
    if !report.review.unplanned_changes.is_empty() {
        facts.push_str(&format!(
            "Di luar rencana: {}\n",
            report.review.unplanned_changes.join(", ")
        ));
    }

    // --- conversational summary -------------------------------------------
    match session.harness.narrate_run(&session.intent, &facts).await {
        Ok(narration) => say(&narration),
        Err(_) => {
            say("Selesai. Ringkasannya:");
            print!("{facts}");
        }
    }

    // --- the honest check --------------------------------------------------
    println!();
    for criterion in &report.verification.criterion_results {
        println!(
            "   {} {} — {}",
            criterion.status.icon(),
            criterion.criterion_id,
            criterion.description
        );
    }

    println!();
    if report.verification.overall_passed && report.review.is_clean() {
        ok("Semua kriteria otomatis lolos, tidak ada anomali.");
    } else if report.verification.overall_passed {
        warn("Kriteria otomatis lolos, tapi ada yang di luar rencana — cek `ratchet review`.");
    } else {
        warn("Ada kriteria yang gagal. Cek `ratchet verify` untuk detailnya.");
    }

    session.phase = Phase::Done;
    println!("{DIM}Ketik permintaan baru, atau /keluar.{RESET}");
    Ok(())
}

fn print_help() {
    println!();
    println!("  {BOLD}Cara pakai{RESET}");
    println!("    Ketik apa yang mau kamu buat, contoh:");
    println!("      {DIM}buatkan aku ecommerce sederhana{RESET}");
    println!("    Saya akan tanya beberapa hal, lalu mengusulkan spec.");
    println!("    Setelah kamu setuju, saya susun rencana dan kerjakan.");
    println!();
    println!("  {BOLD}Perintah{RESET}");
    println!("    /spec     lihat spec yang diusulkan");
    println!("    /status   fase saat ini");
    println!("    /reset    batalkan dan mulai dari awal");
    println!("    /help     bantuan ini");
    println!("    /keluar   keluar");
}

/// Let the caller render a task status without importing the enum.
trait StatusStr {
    fn status_str(&self) -> String;
}

impl StatusStr for ratchet_core::ExecutionResult {
    fn status_str(&self) -> String {
        format!("{:?}", self.status)
    }
}
