use clap::Parser;

use super::commands::Commands;

#[derive(Debug, Parser)]
#[command(
    name = "tentgent",
    version,
    about = "Agent-first CLI for local runtimes, adapters, and daemon orchestration.",
    long_about = "Tentgent is the local operator CLI for model runtimes, adapters, and the persistent Tentgent daemon.",
    arg_required_else_help = true,
    after_help = "Examples:\n  tentgent auth --help\n  tentgent auth hf\n  tentgent auth hf set\n  tentgent model --help\n  tentgent model add /path/to/model\n  tentgent model pull google/gemma-3-1b-pt\n  tentgent model inspect <short-ref>\n  tentgent chat <model-ref>"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}
