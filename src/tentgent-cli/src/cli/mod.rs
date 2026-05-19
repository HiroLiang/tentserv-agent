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
mod display;
mod doctor;
mod embed;
mod model;
mod rerank;
mod runtime;
mod runtime_footprint;
mod server;
mod session;
mod session_kernel;
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
        Commands::Embed(command) => embed::handle_embed_command(command).await?,
        Commands::Rerank(command) => rerank::handle_rerank_command(command).await?,
        Commands::Server { action } => server::handle_server_command(action).await?,
        Commands::Session { action } => session::handle_session_command(action).await?,
        Commands::Doctor(command) => doctor::handle_doctor_command(command)?,
        Commands::Runtime { action } => runtime::handle_runtime_command(action)?,
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
    fn parses_embed_command() {
        let cli = Cli::try_parse_from([
            "tentgent",
            "embed",
            "abc123",
            "--input",
            "hello",
            "--input",
            "world",
            "--home",
            "/tmp/tentgent",
            "--pretty",
        ])
        .expect("parse embed command");

        match cli.command {
            Commands::Embed(command) => {
                assert_eq!(command.model_ref, "abc123");
                assert_eq!(command.inputs, ["hello", "world"]);
                assert_eq!(
                    command.home.as_deref(),
                    Some(std::path::Path::new("/tmp/tentgent"))
                );
                assert!(command.pretty);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_rerank_command() {
        let cli = Cli::try_parse_from([
            "tentgent",
            "rerank",
            "abc123",
            "--query",
            "ownership",
            "--document",
            "Rust owns memory.",
            "--document",
            "Cake uses flour.",
            "--top-n",
            "1",
            "--home",
            "/tmp/tentgent",
            "--pretty",
        ])
        .expect("parse rerank command");

        match cli.command {
            Commands::Rerank(command) => {
                assert_eq!(command.model_ref, "abc123");
                assert_eq!(command.query, "ownership");
                assert_eq!(command.documents, ["Rust owns memory.", "Cake uses flour."]);
                assert_eq!(command.top_n, Some(1));
                assert_eq!(
                    command.home.as_deref(),
                    Some(std::path::Path::new("/tmp/tentgent"))
                );
                assert!(command.pretty);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

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

    #[test]
    fn parses_runtime_bootstrap_options() {
        let cli = Cli::try_parse_from([
            "tentgent",
            "runtime",
            "bootstrap",
            "--project",
            "/tmp/tentgent-python",
            "--env",
            "/tmp/tentgent-env",
            "--uv",
            "/tmp/uv",
            "--profile",
            "local-model",
            "--dry-run",
            "--print-plan",
        ])
        .expect("parse runtime bootstrap");

        match cli.command {
            Commands::Runtime { action } => match action {
                super::commands::RuntimeCommands::Bootstrap(command) => {
                    assert_eq!(
                        command.project.as_deref(),
                        Some(std::path::Path::new("/tmp/tentgent-python"))
                    );
                    assert_eq!(
                        command.env.as_deref(),
                        Some(std::path::Path::new("/tmp/tentgent-env"))
                    );
                    assert_eq!(command.uv.as_deref(), Some(std::path::Path::new("/tmp/uv")));
                    assert_eq!(
                        command.profile,
                        super::commands::RuntimeBootstrapProfile::LocalModel
                    );
                    assert!(command.dry_run);
                    assert!(command.print_plan);
                }
                other => panic!("unexpected runtime command: {other:?}"),
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn runtime_bootstrap_profile_defaults_to_base() {
        let cli = Cli::try_parse_from(["tentgent", "runtime", "bootstrap"])
            .expect("parse runtime bootstrap");

        match cli.command {
            Commands::Runtime { action } => match action {
                super::commands::RuntimeCommands::Bootstrap(command) => {
                    assert_eq!(
                        command.profile,
                        super::commands::RuntimeBootstrapProfile::Base
                    );
                }
                other => panic!("unexpected runtime command: {other:?}"),
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_runtime_bootstrap_profile() {
        let result =
            Cli::try_parse_from(["tentgent", "runtime", "bootstrap", "--profile", "train"]);

        assert!(result.is_err());
    }

    #[test]
    fn parses_runtime_status_options() {
        let cli = Cli::try_parse_from([
            "tentgent",
            "runtime",
            "status",
            "--project",
            "/tmp/tentgent-python",
            "--env",
            "/tmp/tentgent-env",
            "--profile",
            "full",
        ])
        .expect("parse runtime status");

        match cli.command {
            Commands::Runtime { action } => match action {
                super::commands::RuntimeCommands::Status(command) => {
                    assert_eq!(
                        command.project.as_deref(),
                        Some(std::path::Path::new("/tmp/tentgent-python"))
                    );
                    assert_eq!(
                        command.env.as_deref(),
                        Some(std::path::Path::new("/tmp/tentgent-env"))
                    );
                    assert_eq!(
                        command.profile,
                        Some(super::commands::RuntimeBootstrapProfile::Full)
                    );
                }
                other => panic!("unexpected runtime command: {other:?}"),
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }
}
