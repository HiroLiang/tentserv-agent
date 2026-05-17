mod dto;

use axum::{
    extract::{Path, State},
    Json,
};
use tentgent_kernel::{
    features::dataset::{
        domain::DatasetRefSelector,
        usecases::{DatasetCatalogReadUseCase, DatasetInspectRequest, DatasetListRequest},
    },
    foundation::{error::KernelError, layout::LayoutResolveMode},
};

use crate::transport::rest::{error::RestError, state::RestState};

use self::dto::{dataset_inspection_item, dataset_summary_item, DatasetResponse, DatasetsResponse};

pub async fn list(State(state): State<RestState>) -> Result<Json<DatasetsResponse>, RestError> {
    let result = state
        .app()
        .services()
        .kernel()
        .datasets()
        .catalog_usecase()
        .list_datasets(DatasetListRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
        })
        .map_err(dataset_error)?;

    Ok(Json(DatasetsResponse {
        datasets: result
            .datasets
            .into_iter()
            .map(dataset_summary_item)
            .collect(),
    }))
}

pub async fn inspect(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<DatasetResponse>, RestError> {
    let selector = DatasetRefSelector::parse(&reference).map_err(|err| {
        RestError::bad_request("bad_request", format!("invalid dataset reference: {err}"))
    })?;
    let result = state
        .app()
        .services()
        .kernel()
        .datasets()
        .catalog_usecase()
        .inspect_dataset(DatasetInspectRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            selector,
        })
        .map_err(dataset_error)?;

    Ok(Json(DatasetResponse {
        dataset: dataset_inspection_item(result.dataset),
    }))
}

fn dataset_error(error: KernelError) -> RestError {
    match error {
        KernelError::DatasetStoreUnavailable(message) => {
            RestError::store_lookup("dataset_read_failed", message)
        }
        other => RestError::kernel("dataset_read_failed", other),
    }
}
