use clap::{Parser, Subcommand};
use ratchet_cli::commands::*;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "ratchet")]
#[command(about = "Ratchet — A Spec-Driven Engineering Harness for Any Model")]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to project directory
    #[arg(short, long, global = true, default_value = ".")]
    project_dir: PathBuf,

    /// Configuration file path
    #[arg(short, long, global = true, default_value = "ratchet.toml")]
    config: PathBuf,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new Ratchet project
    Init {
        /// Project name
        #[arg(default_value = "my-project")]
        name: String,
    },

    /// Manage specs
    Spec {
        #[command(subcommand)]
        action: SpecAction,
    },

    /// Generate a plan from a spec
    Plan {
        /// Spec ID
        spec: String,
        /// Force a specific model for planning
        #[arg(short, long)]
        model: Option<String>,
    },

    /// Show or edit the task graph for a spec
    Tasks {
        /// Spec ID
        spec: String,
        /// Open the task graph in $EDITOR
        #[arg(short, long)]
        edit: bool,
    },

    /// Execute tasks
    Run {
        /// Spec ID
        target: String,
        /// Run a single task
        #[arg(short, long)]
        task: Option<String>,
        /// Override the model for this run: `provider:model` or `model`
        #[arg(short, long)]
        model: Option<String>,
        /// Execute the whole task graph
        #[arg(long)]
        all: bool,
    },

    /// Verify spec conformance against the working tree
    Verify {
        /// Spec ID
        spec: String,
    },

    /// Show the plan-vs-actual delta from the last run
    Review {
        /// Spec ID
        spec: String,
    },

    /// Import a spec from an external format
    Import {
        /// Source file path
        source: PathBuf,
        /// Import format (auto-detect if omitted)
        #[arg(short, long)]
        format: Option<String>,
    },

    /// Start the local dashboard (and optionally the A2A endpoint)
    Dashboard {
        /// Port to listen on
        #[arg(short, long, default_value = "8788")]
        port: u16,
        /// Accept delegated tasks from peer agents over A2A
        #[arg(long)]
        a2a: bool,
        /// Serve metrics only, never accept work
        #[arg(long)]
        read_only: bool,
    },

    /// Start the ACP server for editor integration
    Serve {
        /// Port to listen on
        #[arg(short, long, default_value = "8765")]
        port: u16,
    },

    /// Connect to an MCP server and list its tools
    Mcp {
        /// Command that starts the MCP server
        command: String,
        /// Arguments for the MCP server command
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Generate a usage report
    Report {
        /// Report format
        #[arg(short, long, value_enum, default_value = "text")]
        format: ReportFormatArg,
        /// Include records from the last N days
        #[arg(short, long, default_value = "7")]
        since: u64,
    },

    /// Manage providers and credentials
    Provider {
        #[command(subcommand)]
        action: ProviderAction,
    },
}

#[derive(Subcommand)]
enum SpecAction {
    /// Create a new spec
    New {
        /// Spec ID (kebab-case)
        id: String,
    },
    /// Edit an existing spec in $EDITOR
    Edit {
        /// Spec ID
        id: String,
    },
    /// Validate a spec file
    Validate {
        /// Path to spec file
        path: PathBuf,
    },
    /// List all specs
    List,
}

#[derive(Subcommand)]
enum ProviderAction {
    /// Add a new provider
    Add {
        /// Provider name
        name: String,
        /// Provider kind (anthropic, deepseek, mimo, ollama, openai_compatible)
        #[arg(short, long)]
        kind: String,
        /// Environment variable holding the API key
        #[arg(long)]
        key_env: Option<String>,
        /// Base URL (for OpenAI-compatible providers)
        #[arg(long)]
        base_url: Option<String>,
        /// Default model
        #[arg(long)]
        model: Option<String>,
        /// Route through a reseller such as OpenRouter
        #[arg(long)]
        via: Option<String>,
    },
    /// Store a provider credential in the OS keychain
    Login {
        /// Provider name
        name: String,
    },
    /// Remove a stored credential
    Logout {
        /// Provider name
        name: String,
    },
    /// Validate a provider's credentials with a live request
    Test {
        /// Provider name (all providers if omitted)
        name: Option<String>,
    },
    /// List configured providers and credential status
    List,
    /// Remove a provider
    Remove {
        /// Provider name
        name: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, clap::ValueEnum)]
enum ReportFormatArg {
    Text,
    Json,
    Markdown,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RATCHET_LOG")
                .unwrap_or_else(|_| if cli.verbose { "debug" } else { "warn" }.into()),
        )
        .with_target(false)
        .init();

    match cli.command {
        Commands::Init { name } => init::run(&cli.project_dir, &name).await?,
        Commands::Spec { action } => match action {
            SpecAction::New { id } => spec_cmd::new_spec(&cli.project_dir, &id).await?,
            SpecAction::Edit { id } => spec_cmd::edit_spec(&cli.project_dir, &id).await?,
            SpecAction::Validate { path } => spec_cmd::validate(&path).await?,
            SpecAction::List => spec_cmd::list_specs(&cli.project_dir).await?,
        },
        Commands::Plan { spec, model } => plan::run(&cli.project_dir, &spec, model).await?,
        Commands::Tasks { spec, edit } => tasks::run(&cli.project_dir, &spec, edit).await?,
        Commands::Run {
            target,
            task,
            model,
            all,
        } => run::run(&cli.project_dir, &target, task, model, all).await?,
        Commands::Verify { spec } => verify::run(&cli.project_dir, &spec).await?,
        Commands::Review { spec } => review::run(&cli.project_dir, &spec).await?,
        Commands::Import { source, format } => {
            import::run(&cli.project_dir, &source, format).await?
        }
        Commands::Dashboard {
            port,
            a2a,
            read_only,
        } => dashboard::run(&cli.project_dir, port, a2a, read_only).await?,
        Commands::Serve { port } => serve::run(&cli.project_dir, port).await?,
        Commands::Mcp { command, args } => mcp::connect(&cli.project_dir, &command, args).await?,
        Commands::Report { format, since } => {
            let fmt = match format {
                ReportFormatArg::Text => ratchet_observability::ReportFormat::Text,
                ReportFormatArg::Json => ratchet_observability::ReportFormat::Json,
                ReportFormatArg::Markdown => ratchet_observability::ReportFormat::Markdown,
            };
            report::run(&cli.project_dir, fmt, since).await?
        }
        Commands::Provider { action } => match action {
            ProviderAction::Add {
                name,
                kind,
                key_env,
                base_url,
                model,
                via,
            } => {
                provider_cmd::add(
                    &cli.project_dir,
                    &name,
                    &kind,
                    key_env,
                    base_url,
                    model,
                    via,
                )
                .await?
            }
            ProviderAction::Login { name } => provider_cmd::login(&cli.project_dir, &name).await?,
            ProviderAction::Logout { name } => {
                provider_cmd::logout(&cli.project_dir, &name).await?
            }
            ProviderAction::Test { name } => provider_test::run(&cli.project_dir, name).await?,
            ProviderAction::List => provider_cmd::list(&cli.project_dir).await?,
            ProviderAction::Remove { name } => {
                provider_cmd::remove(&cli.project_dir, &name).await?
            }
        },
    }

    Ok(())
}
