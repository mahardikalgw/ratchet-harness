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

/// Read one line. Returns `None` on EOF.
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

        say(&format!("\"{intent}\"."));
        info("A few questions first, so the spec is right.");
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
                    say("A few questions:");
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
                say("That is enough. Here is the spec I propose:");
                println!();
                self.show_spec(&draft);
                self.draft = Some(draft);
                self.phase = Phase::SpecReview;
                println!();
                info("enter = approve · type changes to revise · /cancel");
                Ok(())
            }

            DiscoveryOutcome::Unparseable(raw) => {
                warn("The model did not return a readable response. It said:");
                println!("   {DIM}{}{RESET}", raw.lines().next().unwrap_or(""));
                say("Could you describe it again, briefly?");
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
                match read_input(&format!("Choice{hint}:"))? {
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
            _ => match read_input("Answer:")? {
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
            println!("\n   {BOLD}Goals{RESET}");
            for goal in &draft.goals {
                println!("     • {goal}");
            }
        }
        if !draft.non_goals.is_empty() {
            println!("\n   {BOLD}Not included{RESET}");
            for item in &draft.non_goals {
                println!("     • {item}");
            }
        }

        println!("\n   {BOLD}Acceptance criteria{RESET}");
        for criterion in &draft.acceptance_criteria {
            let check = match draft.verification_of(criterion) {
                Some(ratchet_spec::schema::VerificationStep::Test { command, .. }) => {
                    format!("{GREEN}check: {command}{RESET}")
                }
                Some(ratchet_spec::schema::VerificationStep::Diff { pattern }) => {
                    format!("{GREEN}check: {pattern} changes{RESET}")
                }
                Some(ratchet_spec::schema::VerificationStep::Lint { tool, .. }) => {
                    format!("{GREEN}check: {tool}{RESET}")
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
            ok(&format!("all {total} criteria are machine-checkable"));
        } else {
            info(&format!(
                "{auto}/{total} criteria are machine-checkable; the rest need a human"
            ));
        }
    }

    fn set_idle(&mut self) {
        self.phase = Phase::Idle;
    }

    /// Show or change the provider for this session.
    fn handle_provider(&mut self, argument: &str) {
        let argument = argument.trim();

        if argument.is_empty() {
            let current = self
                .harness
                .session_overrides()
                .provider
                .clone()
                .unwrap_or_else(|| "(from ratchet.toml)".to_string());
            println!("   provider : {current}");
            let names = self.harness.provider_names();
            if names.is_empty() {
                warn("no providers configured");
            } else {
                info(&format!("available: {}", names.join(", ")));
            }
            return;
        }

        if argument == "default" || argument == "auto" {
            self.harness.set_provider(None);
            ok("provider reset to the configured routing");
            return;
        }

        if !self.harness.has_provider(argument) {
            // Distinguish "typo" from "configured but unusable", because the
            // fix is different and only one of them is the user's mistake.
            if self.harness.config().providers.contains_key(argument) {
                warn(&format!(
                    "'{argument}' is configured but unavailable — no credential found"
                ));
                info(&format!("run: ratchet provider login {argument}"));
            } else {
                warn(&format!("no provider named '{argument}'"));
            }
            let names = self.harness.provider_names();
            if !names.is_empty() {
                info(&format!("usable: {}", names.join(", ")));
            }
            return;
        }

        self.harness.set_provider(Some(argument.to_string()));
        ok(&format!("provider pinned to '{argument}'"));
        info("takes effect from the next request");
    }

    /// Show or change the model name for this session.
    fn handle_model(&mut self, argument: &str) {
        let argument = argument.trim();

        if argument.is_empty() {
            let current = self
                .harness
                .session_overrides()
                .model
                .clone()
                .unwrap_or_else(|| "(from ratchet.toml)".to_string());
            println!("   model    : {current}");
            info("usage: /model <name>  ·  /model default to reset");
            return;
        }

        if argument == "default" || argument == "auto" {
            self.harness.set_model(None);
            ok("model reset to the configured default");
            return;
        }

        self.harness.set_model(Some(argument.to_string()));
        ok(&format!("model pinned to '{argument}'"));
        info("takes effect from the next request");
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
            "no usable model is configured.\n\
             Run:\n  ratchet provider add <name> --kind <kind> --key-env <ENV_VAR>\n\n\
             Then check with: ratchet doctor"
        );
    }
    Ok(())
}

pub async fn run(project_dir: &Path, initial: Option<String>) -> Result<()> {
    // Not fatal: a pipe works fine (read_line returns EOF and the loop ends),
    // and this makes sessions scriptable. Risky shell commands still auto-deny
    // without a TTY, because the approval handler checks for itself.
    if !std::io::stdin().is_terminal() {
        eprintln!("{DIM}(stdin is not a terminal — reading answers from the pipe){RESET}");
    }

    if !project_dir.join("ratchet.toml").exists() {
        anyhow::bail!("no ratchet.toml here — run `ratchet init <name>` first");
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
    println!(
        "{BOLD}Ratchet{RESET} {DIM}— describe it, answer a few questions, approve, build{RESET}"
    );
    println!("{DIM}What would you like to build? /help for commands, /exit to quit.{RESET}");

    if let Some(intent) = initial {
        session.start(intent).await?;
    }

    loop {
        let input = match read_input("›")? {
            Some(i) => i,
            None => break, // EOF
        };

        // Enter with nothing typed means "approve" at either review gate.
        if input.is_empty() && !matches!(session.phase, Phase::SpecReview | Phase::PlanReview) {
            continue;
        }

        // --- slash commands ---------------------------------------------
        if let Some(command) = input.strip_prefix('/') {
            let (name, argument) = match command.split_once(char::is_whitespace) {
                Some((n, a)) => (n, a),
                None => (command, ""),
            };

            match name {
                "exit" | "quit" | "q" => break,
                "help" | "h" | "?" => print_help(),
                "spec" => match &session.draft {
                    Some(draft) => session.show_spec(draft),
                    None => info("no spec proposed yet"),
                },
                "status" => {
                    println!("   phase    : {:?}", session.phase);
                    if let Some(id) = &session.spec_id {
                        println!("   spec     : {id}");
                    }
                    let overrides = session.harness.session_overrides();
                    println!(
                        "   provider : {}",
                        overrides
                            .provider
                            .as_deref()
                            .unwrap_or("(from ratchet.toml)")
                    );
                    println!(
                        "   model    : {}",
                        overrides.model.as_deref().unwrap_or("(from ratchet.toml)")
                    );
                    let names = session.harness.provider_names();
                    if !names.is_empty() {
                        println!("   usable   : {}", names.join(", "));
                    }
                }
                "model" => session.handle_model(argument),
                "provider" => session.handle_provider(argument),
                "providers" => {
                    let names = session.harness.provider_names();
                    if names.is_empty() {
                        warn("no providers configured");
                    } else {
                        for name in names {
                            println!("   • {name}");
                        }
                        info("switch with /provider <name>");
                    }
                }
                "reset" | "cancel" => {
                    say("Starting over.");
                    session.set_idle();
                }
                other => warn(&format!("unknown command: /{other} — try /help")),
            }
            continue;
        }

        match session.phase {
            Phase::Idle => session.start(input).await?,

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
                    run_spec_approved(&mut session).await?;
                } else if input == "/cancel" {
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
                } else if input == "/cancel" {
                    session.set_idle();
                } else {
                    warn(
                        "the plan cannot be revised mid-session yet — enter to run, /cancel to abort",
                    );
                }
            }

            Phase::Done => session.start(input).await?,
        }
    }

    println!();
    println!("{DIM}bye.{RESET}");
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

    ok(&format!("spec saved: {}", path.display()));
    session.spec_id = Some(draft.id.clone());

    let spec = SpecParser::new().parse(&raw)?;
    say("Planning the work…");

    match session.harness.load_or_plan(&spec).await {
        Ok(plan) => {
            println!();
            println!("   {BOLD}Plan{RESET}");
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
                    format!("  {DIM}(after {}){RESET}", deps.join(", "))
                };
                println!("     {} {}{}", node.id, node.title, suffix);
            }

            if !plan.affected_modules.is_empty() {
                println!(
                    "\n   {DIM}modules: {}{RESET}",
                    plan.affected_modules.join(", ")
                );
            }

            session.phase = Phase::PlanReview;
            println!();
            info("enter = run · /cancel");
        }
        Err(e) => {
            warn(&format!("could not produce a plan: {e}"));
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

    say("Working on it. I will report when it is done.");
    println!();

    let report = match session
        .harness
        .run_with(&plan, &spec, RunOverrides::default())
        .await
    {
        Ok(report) => report,
        Err(e) => {
            warn(&format!("failed while working: {e}"));
            session.set_idle();
            return Ok(());
        }
    };

    // --- mechanical facts, for the model to summarise ---------------------
    let mut facts = String::new();
    for result in &report.results {
        facts.push_str(&format!(
            "- task {}: {:?}, {} turn(s), provider {}, cost ${:.4}\n",
            result.task_id, result.status, result.turns, result.provider, result.cost_usd
        ));
    }
    facts.push_str(&format!(
        "\nAutomated checks: {}\n",
        report.verification.summary
    ));
    for criterion in &report.verification.criterion_results {
        facts.push_str(&format!(
            "- {} [{:?}] {} ({})\n",
            criterion.criterion_id, criterion.status, criterion.description, criterion.note
        ));
    }
    facts.push_str(&format!(
        "\nFiles changed: {}\n",
        if report.review.changed_files.is_empty() {
            "(none)".to_string()
        } else {
            report.review.changed_files.join(", ")
        }
    ));
    if !report.review.unplanned_changes.is_empty() {
        facts.push_str(&format!(
            "Changed outside the plan: {}\n",
            report.review.unplanned_changes.join(", ")
        ));
    }

    // --- conversational summary ------------------------------------------
    match session.harness.narrate_run(&session.intent, &facts).await {
        Ok(narration) => say(&narration),
        Err(_) => {
            say("Done. Summary:");
            print!("{facts}");
        }
    }

    // --- the honest verdict ----------------------------------------------
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
        ok("All automated checks passed, no anomalies.");
    } else if report.verification.overall_passed {
        warn(
            "Automated checks passed, but something changed outside the plan — see `ratchet review`.",
        );
    } else {
        warn("Some criteria failed. Run `ratchet verify` for the details.");
    }

    session.phase = Phase::Done;
    println!("{DIM}Type a new request, or /exit.{RESET}");
    Ok(())
}

fn print_help() {
    println!();
    println!("  {BOLD}How to use{RESET}");
    println!("    Describe what you want to build, for example:");
    println!("      {DIM}build me a simple storefront{RESET}");
    println!("    I will ask a few questions, propose a spec, then plan and build it.");
    println!();
    println!("  {BOLD}Commands{RESET}");
    println!("    /spec              show the proposed spec");
    println!("    /model [name]      show or change the model for this session");
    println!("    /provider [name]   show or change the provider");
    println!("    /providers         list configured providers");
    println!("    /status            current phase, provider and model");
    println!("    /reset             discard and start over");
    println!("    /help              this help");
    println!("    /exit              quit");
    println!();
    println!("  {BOLD}At a prompt{RESET}");
    println!("    enter              approve the spec or plan");
    println!("    /cancel            abort the current spec or plan");
}
