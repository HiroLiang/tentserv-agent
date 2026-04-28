mod diff;
mod error;
mod hash;
mod index;
mod manifest;
mod package;
mod service;
mod store;
mod template;
mod validate;

pub use diff::{DatasetDiffFile, DatasetDiffStatus, DatasetDiffSummary, DatasetManifestDiff};
pub use error::DatasetError;
pub use service::{
    DatasetDiffOutcome, DatasetDiffSide, DatasetExportOutcome, DatasetImportOutcome,
    DatasetInspection, DatasetManager, DatasetRemovalOutcome, DatasetSummary,
};
pub use store::{
    imported_at_now, read_dataset_metadata, write_dataset_metadata, DatasetFormat, DatasetMetadata,
    DatasetPackageMetadata, DatasetSourceKind, DatasetSplits, DatasetStorePaths,
};
pub use template::{
    render_dataset_template, write_dataset_template, DatasetTemplateRequest,
    DATASET_TEMPLATE_VERSION, DEFAULT_TEMPLATE_LANGUAGE, DEFAULT_TEMPLATE_TASK,
};
pub use validate::{
    validate_dataset_path, DatasetValidationIssue, DatasetValidationOutcome,
    DatasetValidationSplit, DatasetValidationTargetKind, CANONICAL_CHAT_SCHEMA,
};
