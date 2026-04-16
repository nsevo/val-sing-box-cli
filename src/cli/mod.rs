use clap::{CommandFactory, Parser, Subcommand};

const MAIN_AFTER_HELP: &str = "\
Quick start:
  valsb sub add
  valsb sub list
  valsb sub use 0
  valsb node use
  valsb completion bash
";

const SUB_AFTER_HELP: &str = "\
Examples:
  valsb sub add
  valsb sub add https://api.example.com/sub?token=abc
  valsb sub use 0
  valsb sub use hk-main
  valsb sub remove 1
";

const NODE_AFTER_HELP: &str = "\
Examples:
  valsb node use
  valsb node use HK
";

const CONFIG_AFTER_HELP: &str = "\
Examples:
  valsb config path
  valsb config list
";

#[derive(Parser)]
#[command(
    name = "valsb",
    about = "A modern CLI for managing sing-box",
    version,
    propagate_version = true,
    arg_required_else_help = true,
    after_help = MAIN_AFTER_HELP
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Override config directory
    #[arg(long, global = true)]
    pub config_dir: Option<String>,

    /// Output in JSON format
    #[arg(long, global = true)]
    pub json: bool,

    /// Enable verbose output
    #[arg(long, global = true)]
    pub verbose: bool,

    /// Skip interactive confirmations
    #[arg(long, global = true)]
    pub yes: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start sing-box service
    Start,

    /// Stop sing-box service
    Stop,

    /// Restart sing-box service
    Restart,

    /// Show service status and connection info
    Status,

    /// Hot-reload configuration (graceful, via SIGHUP)
    Reload,

    /// View sing-box service logs
    Logs {
        /// Follow log output
        #[arg(short, long)]
        follow: bool,
        /// Number of log lines to show
        #[arg(short = 'n', long, default_value = "50")]
        lines: u32,
    },

    /// Install sing-box kernel and service unit
    Install,

    /// Check for updates and upgrade valsb + sing-box
    Update,

    /// Uninstall valsb and all managed components
    Uninstall,

    /// Manage subscriptions
    #[command(subcommand, visible_alias = "subscription")]
    Sub(SubCommands),

    /// Manage proxy nodes
    #[command(subcommand)]
    Node(NodeCommands),

    /// Manage configuration
    #[command(subcommand)]
    Config(ConfigCommands),

    /// Generate shell completion scripts
    Completion {
        /// Target shell
        shell: clap_complete::Shell,
    },

    /// Check environment and diagnose issues
    Doctor,

    /// Show version information
    Version,

    /// Windows Service worker (called by SCM, not for direct use)
    #[command(hide = true, name = "service-worker")]
    ServiceWorker {
        #[arg(long)]
        sing_box_bin: String,
        #[arg(long)]
        config: String,
        #[arg(long)]
        log_dir: String,
    },
}

#[derive(Subcommand)]
#[command(arg_required_else_help = true, after_help = SUB_AFTER_HELP)]
pub enum SubCommands {
    /// Add a new subscription (or update existing by URL)
    Add {
        /// Subscription URL (omit to paste interactively)
        url: Option<String>,
        /// Custom remark for this subscription
        #[arg(long)]
        remark: Option<String>,
    },
    /// List all subscriptions
    List,
    /// Update subscriptions
    Update {
        /// Target subscription (remark, id, or index). Omit to update all
        target: Option<String>,
    },
    /// Switch the active subscription
    #[command(visible_alias = "switch")]
    Use {
        /// Target subscription (remark, id, or index)
        target: Option<String>,
    },
    /// Remove a subscription (interactive if no target given)
    Remove {
        /// Target subscription (remark, id, or index)
        target: Option<String>,
    },
}

#[derive(Subcommand)]
#[command(arg_required_else_help = true, after_help = NODE_AFTER_HELP)]
pub enum NodeCommands {
    /// Browse nodes and switch active proxy (interactive if no target given)
    #[command(visible_alias = "list")]
    Use {
        /// Node name or index for direct switch
        target: Option<String>,
    },
}

#[derive(Subcommand)]
#[command(arg_required_else_help = true, after_help = CONFIG_AFTER_HELP)]
pub enum ConfigCommands {
    /// Initialize config directories
    Init,
    /// Show config paths
    Path,
    /// List config profiles
    List,
}

pub fn print_completion(shell: clap_complete::Shell) {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "valsb", &mut std::io::stdout());
}
