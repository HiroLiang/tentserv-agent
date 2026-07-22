use clap::{CommandFactory, Parser};
use miette::IntoDiagnostic;

use super::{app, commands::Commands};

pub async fn run() -> miette::Result<()> {
    if std::env::args_os().len() == 1 {
        let mut command = app::Cli::command();
        command.print_help().into_diagnostic()?;
        println!();
        return Ok(());
    }

    let cli = app::Cli::parse();

    match cli.command {
        Commands::Adapter { action } => super::adapter::handle_adapter_command(action)?,
        Commands::Auth { subject } => super::auth::handle_auth_command(subject).await?,
        Commands::Chat(command) => super::chat::handle_chat_command(command).await?,
        Commands::Cluster { action } => super::cluster::handle_cluster_command(action).await?,
        Commands::Dataset { action } => super::dataset::handle_dataset_command(action).await?,
        Commands::Model { action } => super::model::handle_model_command(action)?,
        Commands::Embed(command) => super::embed::handle_embed_command(command).await?,
        Commands::Rerank(command) => super::rerank::handle_rerank_command(command).await?,
        Commands::Transcribe(command) => {
            super::transcribe::handle_transcribe_command(command).await?
        }
        Commands::Speak(command) => super::speak::handle_speak_command(command).await?,
        Commands::Vision { action } => super::vision::handle_vision_command(action).await?,
        Commands::Video { action } => super::video::handle_video_command(action).await?,
        Commands::Image { action } => super::image::handle_image_command(action).await?,
        Commands::Server { action } => super::server::handle_server_command(action).await?,
        Commands::CloudServerRuntime(command) => {
            super::server::handle_cloud_server_runtime(command).await?
        }
        Commands::LocalServerRuntime(command) => {
            super::server::handle_local_server_runtime(command).await?
        }
        Commands::ClusterServerRuntime(command) => {
            super::server::handle_cluster_server_runtime(command).await?
        }
        Commands::Session { action } => super::session::handle_session_command(action).await?,
        Commands::Store { action } => super::store::handle_store_command(action)?,
        Commands::Doctor(command) => super::doctor::handle_doctor_command(command)?,
        Commands::Runtime { action } => super::runtime::handle_runtime_command(action)?,
        Commands::Train { action } => super::train::handle_train_command(action)?,
        Commands::Daemon { action } => super::daemon::handle_daemon_command(action).await?,
    }

    Ok(())
}
