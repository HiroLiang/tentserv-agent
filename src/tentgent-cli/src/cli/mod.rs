mod adapter;
mod adapter_list;
mod adapter_progress;
mod adapter_render;
mod app;
mod auth;
mod chat;
mod commands;
mod daemon;
mod dataset;
mod doctor;
mod model;
mod python_runtime;
mod server;
mod status;
mod train;

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
        Commands::Adapter { action } => adapter::handle_adapter_command(action)?,
        Commands::Auth { subject } => auth::handle_auth_command(subject).await?,
        Commands::Chat(command) => chat::handle_chat_command(command).await?,
        Commands::Dataset { action } => dataset::handle_dataset_command(action).await?,
        Commands::Model { action } => model::handle_model_command(action)?,
        Commands::Server { action } => server::handle_server_command(action).await?,
        Commands::Doctor(command) => doctor::handle_doctor_command(command)?,
        Commands::Status => status::handle_status_command()?,
        Commands::Train { action } => train::handle_train_command(action)?,
        Commands::Daemon { action } => daemon::handle_daemon_command(action).await?,
    }

    Ok(())
}
