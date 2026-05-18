//! Dataset use case boundaries.

mod catalog;
mod common;
mod diff;
mod evaluation;
mod export;
mod import;
pub mod port;
mod remove;
mod synth;
mod template;
mod validation;

#[cfg(test)]
mod tests;

pub use catalog::StdDatasetCatalogReadUseCase;
pub use diff::StdDatasetDiffUseCase;
pub use evaluation::StdDatasetEvaluationUseCase;
pub use export::StdDatasetExportUseCase;
pub use import::StdDatasetLocalImportUseCase;
pub use port::{
    DatasetCatalogReadUseCase, DatasetDiffRequest, DatasetDiffResult, DatasetDiffRightSelection,
    DatasetDiffUseCase, DatasetEvaluateRequest, DatasetEvaluateResult,
    DatasetEvaluationInputSelection, DatasetEvaluationUseCase, DatasetExportRequest,
    DatasetExportResult, DatasetExportUseCase, DatasetInspectRequest, DatasetInspectResult,
    DatasetListRequest, DatasetListResult, DatasetLocalImportRequest, DatasetLocalImportResult,
    DatasetLocalImportUseCase, DatasetRemoveRequest, DatasetRemoveResult, DatasetRemoveUseCase,
    DatasetSynthPromptRenderRequest, DatasetSynthPromptRenderResult, DatasetSynthesisUseCase,
    DatasetSynthesizeRequest, DatasetSynthesizeResult, DatasetTemplateRenderRequest,
    DatasetTemplateRenderResult, DatasetTemplateUseCase, DatasetUseCaseFuture,
    DatasetValidateRequest, DatasetValidateResult, DatasetValidationTargetSelection,
    DatasetValidationUseCase,
};
pub use remove::StdDatasetRemoveUseCase;
pub use synth::StdDatasetSynthesisUseCase;
pub use template::StdDatasetTemplateUseCase;
pub use validation::StdDatasetValidationUseCase;
