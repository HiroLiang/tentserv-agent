mod app;
mod auth;
mod chat;
mod commands;
mod model;

use clap::{CommandFactory, Parser};
use commands::Commands;
use miette::IntoDiagnostic;

pub async fn run() -> miette::Result<()> {
    if std::env::args_os().len() == 1 {
        let mut command = app::Cli::command();
        command.print_help().into_diagnostic()?;
        println!();
        return Ok(());
    }

    let cli = app::Cli::parse();

    match cli.command {
        Commands::Auth { subject } => auth::handle_auth_command(subject).await?,
        Commands::Chat(command) => chat::handle_chat_command(command).await?,
        Commands::Model { action } => model::handle_model_command(action)?,
        Commands::Status | Commands::Adapter | Commands::Daemon => {
            println!("Command scaffold is present. Use --help to inspect available groups.");
        }
    }

    Ok(())
}
