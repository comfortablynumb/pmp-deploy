use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "pmp-deploy")]
#[command(author, version, about = "Simplified multi-infrastructure application deployments")]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Path to the configuration file
    #[arg(short, long, global = true)]
    pub file: Option<PathBuf>,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Suppress all output except errors
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Output format (text, json)
    #[arg(long, global = true, default_value = "text")]
    pub output: OutputFormat,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Deploy to a specified environment
    Deploy(DeployArgs),

    /// Check deployment status
    Status(StatusArgs),

    /// Rollback to the previous version
    Rollback(RollbackArgs),

    /// Stream or fetch logs
    Logs(LogsArgs),

    /// List available environments
    List,

    /// Validate the configuration file
    Validate,

    /// Initialize a new configuration file
    Init(InitArgs),

    /// Provision infrastructure (create if not exists, update if exists)
    Provision(ProvisionArgs),

    /// Manage projects (for multi-project setup)
    #[command(subcommand)]
    Projects(ProjectsCommands),

    /// Manage deployment hooks
    #[command(subcommand)]
    Hooks(HooksCommands),

    /// Start the web UI server
    Ui(UiArgs),
}

#[derive(Parser, Debug)]
pub struct DeployArgs {
    /// The environment to deploy to
    pub environment: String,

    /// Perform a dry run without making changes
    #[arg(long)]
    pub dry_run: bool,

    /// Skip confirmation prompt
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Override the image to deploy
    #[arg(long)]
    pub image: Option<String>,

    /// Deploy mode: 'app-only' (default) updates only the app image, 'full' updates app + infrastructure
    #[arg(long, default_value = "app-only")]
    pub deploy_mode: String,

    /// Skip all hooks (pre-deploy and post-deploy)
    #[arg(long)]
    pub skip_hooks: bool,

    /// Skip only pre-deploy hooks
    #[arg(long)]
    pub skip_pre_hooks: bool,

    /// Skip only post-deploy hooks
    #[arg(long)]
    pub skip_post_hooks: bool,
}

#[derive(Parser, Debug)]
pub struct StatusArgs {
    /// The environment to check
    pub environment: String,

    /// Watch for changes
    #[arg(short, long)]
    pub watch: bool,
}

#[derive(Parser, Debug)]
pub struct RollbackArgs {
    /// The environment to rollback
    pub environment: String,

    /// Target version to rollback to (defaults to previous)
    #[arg(long = "to-version", id = "target_version")]
    pub target_version: Option<String>,

    /// Skip confirmation prompt
    #[arg(short = 'y', long)]
    pub yes: bool,
}

#[derive(Parser, Debug)]
pub struct LogsArgs {
    /// The environment to get logs from
    pub environment: String,

    /// Follow log output
    #[arg(short = 'F', long)]
    pub follow: bool,

    /// Number of lines to show
    #[arg(short = 'n', long, default_value = "100")]
    pub lines: usize,
}

#[derive(Parser, Debug)]
pub struct InitArgs {
    /// Infrastructure type to initialize with
    #[arg(long)]
    pub infrastructure: Option<String>,

    /// Force overwrite existing config
    #[arg(long)]
    pub force: bool,
}

#[derive(Subcommand, Debug)]
pub enum ProjectsCommands {
    /// List all configured projects
    List,

    /// Add a project to the global config
    Add(ProjectAddArgs),

    /// Remove a project from the global config
    Remove(ProjectRemoveArgs),
}

#[derive(Parser, Debug)]
pub struct ProjectAddArgs {
    /// Path to the project
    pub path: PathBuf,

    /// Optional name for the project
    #[arg(short, long)]
    pub name: Option<String>,
}

#[derive(Parser, Debug)]
pub struct ProjectRemoveArgs {
    /// Path or name of the project to remove
    pub project: String,
}

#[derive(Parser, Debug)]
pub struct UiArgs {
    /// Port to run the UI server on
    #[arg(short, long, default_value = "8080")]
    pub port: u16,

    /// Host to bind to
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Open browser automatically
    #[arg(long)]
    pub open: bool,

    /// Enable HTTPS with the specified certificate file
    #[arg(long, requires = "tls_key")]
    pub tls_cert: Option<PathBuf>,

    /// TLS private key file (required with --tls-cert)
    #[arg(long, requires = "tls_cert")]
    pub tls_key: Option<PathBuf>,

    /// Generate self-signed development certificate
    #[arg(long, conflicts_with_all = ["tls_cert", "tls_key"])]
    pub dev_tls: bool,
}

#[derive(Subcommand, Debug)]
pub enum HooksCommands {
    /// Run a specific hook manually
    Run(HooksRunArgs),

    /// List all hooks for an environment
    List(HooksListArgs),
}

#[derive(Parser, Debug)]
pub struct HooksRunArgs {
    /// The environment containing the hook
    pub environment: String,

    /// Name of the hook to run
    pub hook_name: String,

    /// Perform a dry run without executing the hook
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Parser, Debug)]
pub struct HooksListArgs {
    /// The environment to list hooks for
    pub environment: String,
}

#[derive(Parser, Debug)]
pub struct ProvisionArgs {
    /// The environment to provision
    pub environment: String,

    /// Show planned changes without applying (dry run)
    #[arg(long)]
    pub plan: bool,

    /// Perform a dry run without making changes
    #[arg(long)]
    pub dry_run: bool,

    /// Skip confirmation prompt
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Override the image to deploy
    #[arg(long)]
    pub image: Option<String>,
}
