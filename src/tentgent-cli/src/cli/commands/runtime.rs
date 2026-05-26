use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};

#[derive(Debug, Subcommand)]
pub enum RuntimeCommands {
    /// Create or sync the managed Python runtime.
    #[command(
        name = "bootstrap",
        about = "Create or sync the managed Python runtime.",
        long_about = "Create or sync Tentgent's managed Python runtime using the packaged bootstrap script and pinned uv tooling. This is the package-manager friendly runtime setup command for Homebrew and manual installs."
    )]
    Bootstrap(RuntimeBootstrapCommand),
    /// Show managed Python runtime status.
    #[command(
        name = "status",
        about = "Show managed Python runtime status.",
        long_about = "Show managed Python runtime status, including resolved runtime-home paths, Python environment presence, Python version, and bootstrap profile readiness."
    )]
    Status(RuntimeStatusCommand),
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
    /// Runtime dependency profile to install.
    #[arg(long, value_enum, default_value_t = RuntimeBootstrapProfile::Base)]
    pub profile: RuntimeBootstrapProfile,
    /// Ask uv to plan the sync without modifying the Python environment.
    #[arg(long)]
    pub dry_run: bool,
    /// Print resolved paths without syncing.
    #[arg(long)]
    pub print_plan: bool,
}

#[derive(Debug, Args)]
pub struct RuntimeStatusCommand {
    /// Python daemon project directory override for status resolution.
    #[arg(long, value_name = "PATH")]
    pub project: Option<PathBuf>,
    /// Managed Python environment path override for status resolution.
    #[arg(long, value_name = "PATH")]
    pub env: Option<PathBuf>,
    /// Show readiness for one runtime dependency profile.
    #[arg(long, value_enum)]
    pub profile: Option<RuntimeBootstrapProfile>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum RuntimeBootstrapProfile {
    Base,
    #[value(name = "local-model")]
    LocalModel,
    Training,
    #[value(alias = "all")]
    Full,
}

impl RuntimeBootstrapProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::LocalModel => "local-model",
            Self::Training => "training",
            Self::Full => "full",
        }
    }
}

impl std::fmt::Display for RuntimeBootstrapProfile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}
