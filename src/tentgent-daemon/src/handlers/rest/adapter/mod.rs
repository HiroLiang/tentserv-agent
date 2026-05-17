mod dto;

use axum::{
    extract::{Path, State},
    Json,
};
use tentgent_kernel::{
    features::adapter::{
        domain::AdapterRefSelector,
        usecases::{AdapterCatalogReadUseCase, AdapterInspectRequest, AdapterListRequest},
    },
    foundation::{error::KernelError, layout::LayoutResolveMode},
};

use crate::transport::rest::{error::RestError, state::RestState};

use self::dto::{adapter_inspection_item, adapter_summary_item, AdapterResponse, AdaptersResponse};

pub async fn list(State(state): State<RestState>) -> Result<Json<AdaptersResponse>, RestError> {
    let result = state
        .app()
        .services()
        .kernel()
        .adapters()
        .catalog_usecase()
        .list_adapters(AdapterListRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
        })
        .map_err(adapter_error)?;

    Ok(Json(AdaptersResponse {
        adapters: result
            .adapters
            .into_iter()
            .map(adapter_summary_item)
            .collect(),
    }))
}

pub async fn inspect(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<AdapterResponse>, RestError> {
    let selector = AdapterRefSelector::parse(&reference).map_err(|err| {
        RestError::bad_request("bad_request", format!("invalid adapter reference: {err}"))
    })?;
    let result = state
        .app()
        .services()
        .kernel()
        .adapters()
        .catalog_usecase()
        .inspect_adapter(AdapterInspectRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            selector,
        })
        .map_err(adapter_error)?;

    Ok(Json(AdapterResponse {
        adapter: adapter_inspection_item(result.adapter),
    }))
}

fn adapter_error(error: KernelError) -> RestError {
    match error {
        KernelError::AdapterStoreUnavailable(message) => {
            RestError::store_lookup("adapter_read_failed", message)
        }
        other => RestError::kernel("adapter_read_failed", other),
    }
}
