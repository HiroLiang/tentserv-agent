use clap::Parser;

use super::commands::Commands;

#[derive(Debug, Parser)]
#[command(
    name = "tentgent",
    version,
    about = "Agent-first CLI for local runtimes, adapters, and daemon orchestration.",
    long_about = "Tentgent is the local operator CLI for model runtimes, adapters, and the persistent Tentgent daemon.",
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}
