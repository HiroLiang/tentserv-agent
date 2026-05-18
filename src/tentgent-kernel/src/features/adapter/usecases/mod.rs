//! Adapter use case boundaries.

mod bind;
mod catalog;
mod common;
mod compatibility;
mod import;
pub mod port;
mod pull;
mod remove;
mod train_run;

#[cfg(test)]
mod tests;

pub use bind::StdAdapterBindUseCase;
pub use catalog::StdAdapterCatalogReadUseCase;
pub use compatibility::StdAdapterCompatibilityCheckUseCase;
pub use import::StdAdapterLocalImportUseCase;
pub use port::{
    AdapterBindRequest, AdapterBindResult, AdapterBindUseCase, AdapterCatalogReadUseCase,
    AdapterCompatibilityCheckRequest, AdapterCompatibilityCheckResult,
    AdapterCompatibilityCheckUseCase, AdapterHfPullRequest, AdapterHfPullResult,
    AdapterHfPullUseCase, AdapterInspectRequest, AdapterInspectResult, AdapterListRequest,
    AdapterListResult, AdapterLocalImportRequest, AdapterLocalImportResult,
    AdapterLocalImportUseCase, AdapterRemoveRequest, AdapterRemoveResult, AdapterRemoveUseCase,
    AdapterTrainRunImportRequest, AdapterTrainRunImportResult, AdapterTrainRunImportUseCase,
};
pub use pull::StdAdapterHfPullUseCase;
pub use remove::StdAdapterRemoveUseCase;
pub use train_run::StdAdapterTrainRunImportUseCase;
