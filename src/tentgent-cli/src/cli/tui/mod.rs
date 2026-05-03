mod app;
mod daemon_client;
mod navigator;
mod render;
mod resource;
mod resource_render;
mod terminal;

use super::commands::TuiCommand;

pub async fn handle_tui_command(command: TuiCommand) -> miette::Result<()> {
    app::run_tui(command).await
}
