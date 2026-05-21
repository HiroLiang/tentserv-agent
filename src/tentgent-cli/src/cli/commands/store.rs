use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub enum StoreCommands {
    /// Inspect or remove abandoned managed-store staging directories.
    #[command(
        name = "gc",
        about = "Inspect or remove abandoned managed-store staging directories.",
        long_about = "Inspect or remove abandoned managed-store staging directories under models/staging, adapters/staging, and datasets/staging. By default this is a dry run; add --apply to delete the listed staging directories."
    )]
    Gc(StoreGcCommand),
}

#[derive(Debug, Args)]
pub struct StoreGcCommand {
    /// Runtime home override for store lookup.
    #[arg(short = 'H', long, value_name = "HOME")]
    pub home: Option<PathBuf>,
    /// Delete listed staging directories. Without this flag, only prints what would be removed.
    #[arg(long)]
    pub apply: bool,
}
