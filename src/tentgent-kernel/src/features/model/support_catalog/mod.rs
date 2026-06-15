//! Built-in model support catalog package.

mod built_in;
mod domain;
mod matching;

pub use built_in::{
    built_in_catalog_entries_for_model, built_in_model_support_catalog,
    built_in_support_hints_for_model,
};
pub use domain::{
    ModelSupportCatalogDocument, ModelSupportCatalogEntry, ModelSupportCatalogEvidence,
    ModelSupportCatalogLevel,
};

#[cfg(test)]
mod tests;
