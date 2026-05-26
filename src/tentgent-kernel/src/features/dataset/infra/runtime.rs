use serde_json::Value;

use crate::features::dataset::domain::{DatasetRuntimeDebug, DatasetSynthRuntimeOutput};
use crate::features::dataset::ports::{
    DatasetEvalRuntimeClient, DatasetEvalRuntimeRequest, DatasetPortFuture,
    DatasetSynthPromptRuntimeRequest, DatasetSynthRuntimeClient, DatasetSynthRuntimeRequest,
};
use crate::features::runtime::ports::RuntimeExecutableResolver;

use super::error::dataset_runtime_error;

const DATASET_RUNTIME_HTTP_TODO: &str =
    "dataset provider runtimes must be served by the model runtime HTTP daemon; the dataset HTTP endpoints have not been ported yet";

/// Placeholder for provider-backed dataset synthesis after the model runtime
/// migration. The old Python command entrypoints were removed; callers now fail
/// closed instead of invoking a stale process path.
pub struct PythonDatasetSynthRuntimeClient;

impl PythonDatasetSynthRuntimeClient {
    pub fn new(_executable_resolver: &dyn RuntimeExecutableResolver) -> Self {
        Self
    }
}

impl DatasetSynthRuntimeClient for PythonDatasetSynthRuntimeClient {
    fn render_synth_prompt(
        &self,
        _request: DatasetSynthPromptRuntimeRequest,
    ) -> DatasetPortFuture<'_, String> {
        Box::pin(async move { Err(dataset_runtime_error(DATASET_RUNTIME_HTTP_TODO)) })
    }

    fn synthesize_dataset(
        &self,
        _request: DatasetSynthRuntimeRequest,
    ) -> DatasetPortFuture<'_, DatasetSynthRuntimeOutput> {
        Box::pin(async move { Err(dataset_runtime_error(DATASET_RUNTIME_HTTP_TODO)) })
    }
}

/// Placeholder for provider-backed dataset evaluation after the model runtime
/// migration. The old Python command entrypoints were removed; callers now fail
/// closed instead of invoking a stale process path.
pub struct PythonDatasetEvalRuntimeClient;

impl PythonDatasetEvalRuntimeClient {
    pub fn new(_executable_resolver: &dyn RuntimeExecutableResolver) -> Self {
        Self
    }
}

impl DatasetEvalRuntimeClient for PythonDatasetEvalRuntimeClient {
    fn evaluate_dataset(
        &self,
        _request: DatasetEvalRuntimeRequest,
    ) -> DatasetPortFuture<'_, Value> {
        Box::pin(async move { Err(dataset_runtime_error(DATASET_RUNTIME_HTTP_TODO)) })
    }

    fn runtime_debug(&self, _error_detail: &str) -> Option<DatasetRuntimeDebug> {
        None
    }
}
