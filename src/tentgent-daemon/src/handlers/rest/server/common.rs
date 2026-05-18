use tentgent_kernel::{
    features::server::{
        domain::{ServerInspection, ServerRefSelector},
        usecases::{ServerInspectRequest, ServerSpecUseCase},
    },
    foundation::layout::{LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput},
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::transport::rest::{error::RestError, state::RestState};

use super::error::server_error;

pub(super) fn inspect_server(
    state: &RestState,
    reference: &str,
) -> Result<ServerInspection, RestError> {
    let selector = parse_server_selector(reference)?;
    Ok(state
        .app()
        .services()
        .kernel()
        .server_usecase()
        .inspect_server(ServerInspectRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            selector,
        })
        .map_err(server_error)?
        .inspection)
}

pub(super) fn parse_server_selector(reference: &str) -> Result<ServerRefSelector, RestError> {
    ServerRefSelector::parse(reference).map_err(|err| {
        RestError::bad_request("bad_request", format!("invalid server reference: {err}"))
    })
}

pub(super) fn layout_input_from_layout(
    layout: &RuntimeLayout,
    mode: LayoutResolveMode,
) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode,
        home_dir: Some(layout.home_dir.clone()),
        data_root_dir: Some(layout.data_root_dir.clone()),
    }
}

pub(super) fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}
