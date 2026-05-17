mod dto;

use axum::{
    extract::{Path, State},
    Json,
};
use tentgent_kernel::{
    features::model::{
        domain::ModelRefSelector,
        usecases::{ModelCatalogReadUseCase, ModelInspectRequest, ModelListRequest},
    },
    foundation::{error::KernelError, layout::LayoutResolveMode},
};

use crate::transport::rest::{error::RestError, state::RestState};

use self::dto::{model_inspection_item, model_summary_item, ModelResponse, ModelsResponse};

pub async fn list(State(state): State<RestState>) -> Result<Json<ModelsResponse>, RestError> {
    let result = state
        .app()
        .services()
        .kernel()
        .models()
        .catalog_usecase()
        .list_models(ModelListRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
        })
        .map_err(model_error)?;

    Ok(Json(ModelsResponse {
        models: result.models.into_iter().map(model_summary_item).collect(),
    }))
}

pub async fn inspect(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<ModelResponse>, RestError> {
    let selector = ModelRefSelector::parse(&reference).map_err(|err| {
        RestError::bad_request("bad_request", format!("invalid model reference: {err}"))
    })?;
    let result = state
        .app()
        .services()
        .kernel()
        .models()
        .catalog_usecase()
        .inspect_model(ModelInspectRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            selector,
        })
        .map_err(model_error)?;

    Ok(Json(ModelResponse {
        model: model_inspection_item(result.model),
    }))
}

fn model_error(error: KernelError) -> RestError {
    match error {
        KernelError::ModelStoreUnavailable(message) => {
            RestError::store_lookup("model_read_failed", message)
        }
        other => RestError::kernel("model_read_failed", other),
    }
}
