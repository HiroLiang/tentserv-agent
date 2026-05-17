mod dto;

use axum::{
    extract::{Path, State},
    Json,
};
use tentgent_kernel::{
    features::server::{
        domain::ServerRefSelector,
        usecases::{ServerInspectRequest, ServerListRequest, ServerSpecUseCase},
    },
    foundation::{error::KernelError, layout::LayoutResolveMode},
};

use crate::transport::rest::{error::RestError, state::RestState};

use self::dto::{server_inspection_item, server_summary_item, ServerResponse, ServersResponse};

pub async fn list(State(state): State<RestState>) -> Result<Json<ServersResponse>, RestError> {
    let result = state
        .app()
        .services()
        .kernel()
        .server_usecase()
        .list_servers(ServerListRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            running_only: false,
        })
        .map_err(server_error)?;

    Ok(Json(ServersResponse {
        servers: result
            .servers
            .into_iter()
            .map(server_summary_item)
            .collect(),
    }))
}

pub async fn inspect(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<ServerResponse>, RestError> {
    let selector = ServerRefSelector::parse(&reference).map_err(|err| {
        RestError::bad_request("bad_request", format!("invalid server reference: {err}"))
    })?;
    let result = state
        .app()
        .services()
        .kernel()
        .server_usecase()
        .inspect_server(ServerInspectRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            selector,
        })
        .map_err(server_error)?;

    Ok(Json(ServerResponse {
        server: server_inspection_item(result.inspection),
    }))
}

fn server_error(error: KernelError) -> RestError {
    match error {
        KernelError::ServerStoreUnavailable(message) => {
            RestError::store_lookup("server_read_failed", message)
        }
        other => RestError::kernel("server_read_failed", other),
    }
}
