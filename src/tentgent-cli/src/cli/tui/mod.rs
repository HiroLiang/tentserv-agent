mod app;
mod chat;
mod chat_render;
mod daemon_client;
mod jobs;
mod navigator;
mod render;
mod resource;
mod resource_render;
mod runtime_action;
mod runtime_action_render;
mod runtime_wizard;
mod session_action;
mod store_action;
mod store_action_render;
mod terminal;

use super::commands::TuiCommand;

pub async fn handle_tui_command(command: TuiCommand) -> miette::Result<()> {
    app::run_tui(command).await
}
