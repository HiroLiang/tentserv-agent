//! Standard dataset-store infrastructure implementations.

mod catalog;
mod content;
mod diff;
mod error;
mod identity;
mod index;
mod layout;
mod manifest;
mod package;
mod reference_guard;
mod runtime;
mod staging;
mod template;
mod validator;

#[cfg(test)]
mod tests;

pub use catalog::FileDatasetCatalogStore;
pub use content::FileDatasetContentStore;
pub use diff::StdDatasetDiffer;
pub use identity::StdDatasetIdentityGenerator;
pub use index::FileDatasetSourceIndexStore;
pub use layout::StdDatasetStoreLayoutInitializer;
pub use manifest::StdDatasetManifestBuilder;
pub use package::StdDatasetPackageDetector;
pub use reference_guard::FileDatasetReferenceGuard;
pub use runtime::{PythonDatasetEvalRuntimeClient, PythonDatasetSynthRuntimeClient};
pub use staging::StdDatasetSourceStager;
pub use template::MarkdownDatasetTemplateRenderer;
pub use validator::StdDatasetValidator;
