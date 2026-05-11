use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub enum RuntimeCommands {
    /// Create or sync the managed Python runtime.
    #[command(
        name = "bootstrap",
        about = "Create or sync the managed Python runtime.",
        long_about = "Create or sync Tentgent's managed Python runtime using the packaged bootstrap script and pinned uv tooling. This is the package-manager friendly runtime setup command for Homebrew and manual installs."
    )]
    Bootstrap(RuntimeBootstrapCommand),
}

#[derive(Debug, Args)]
pub struct RuntimeBootstrapCommand {
    /// Python daemon project directory. Defaults to the resolved packaged or source project.
    #[arg(long, value_name = "PATH")]
    pub project: Option<PathBuf>,
    /// Managed Python environment path. Defaults to the resolved Tentgent Python env.
    #[arg(long, value_name = "PATH")]
    pub env: Option<PathBuf>,
    /// Use an explicit pinned uv executable path.
    #[arg(long, value_name = "PATH")]
    pub uv: Option<PathBuf>,
    /// Ask uv to plan the sync without modifying the Python environment.
    #[arg(long)]
    pub dry_run: bool,
    /// Print resolved paths without syncing.
    #[arg(long)]
    pub print_plan: bool,
}
