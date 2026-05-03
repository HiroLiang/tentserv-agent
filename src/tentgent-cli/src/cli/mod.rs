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
mod session;
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
        Commands::Session { action } => session::handle_session_command(action).await?,
        Commands::Doctor(command) => doctor::handle_doctor_command(command)?,
        Commands::Status => status::handle_status_command()?,
        Commands::Train { action } => train::handle_train_command(action)?,
        Commands::Daemon { action } => daemon::handle_daemon_command(action).await?,
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser as _;

    use super::{
        app::Cli,
        commands::{Commands, DaemonCommands},
    };

    #[test]
    fn parses_daemon_start() {
        let cli = Cli::try_parse_from([
            "tentgent",
            "daemon",
            "start",
            "--home",
            "/tmp/tentgent",
            "--host",
            "127.0.0.1",
            "--port",
            "8790",
            "--allow-unsafe-bind",
        ])
        .expect("parse daemon start");

        match cli.command {
            Commands::Daemon {
                action: DaemonCommands::Start(command),
            } => {
                assert_eq!(
                    command.home.as_deref(),
                    Some(std::path::Path::new("/tmp/tentgent"))
                );
                assert_eq!(command.host.as_deref(), Some("127.0.0.1"));
                assert_eq!(command.port, Some(8790));
                assert!(command.allow_unsafe_bind);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_daemon_run_detach() {
        let cli = Cli::try_parse_from(["tentgent", "daemon", "run", "--detach"])
            .expect("parse daemon run --detach");

        match cli.command {
            Commands::Daemon {
                action: DaemonCommands::Run(command),
            } => {
                assert!(command.detach);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }
}
