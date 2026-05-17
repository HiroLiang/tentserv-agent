//! Adapter base-model binding use case.

use crate::features::adapter::domain::AdapterBindOutcome;
use crate::features::adapter::ports::{
    AdapterBaseIndexStore, AdapterCatalogStore, AdapterSourceMetadataReader,
};
use crate::features::model::ports::ModelCatalogStore;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::{
    adapter_store_layout, apply_base_metadata, base_index_for_metadata, model_store_layout,
    validate_source_metadata,
};
use super::port::{AdapterBindRequest, AdapterBindResult, AdapterBindUseCase};

/// Standard adapter-to-model binding orchestration.
pub struct StdAdapterBindUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    adapter_catalog: &'a dyn AdapterCatalogStore,
    source_metadata_reader: &'a dyn AdapterSourceMetadataReader,
    base_indexes: &'a dyn AdapterBaseIndexStore,
    model_catalog: &'a dyn ModelCatalogStore,
}

impl<'a> StdAdapterBindUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        adapter_catalog: &'a dyn AdapterCatalogStore,
        source_metadata_reader: &'a dyn AdapterSourceMetadataReader,
        base_indexes: &'a dyn AdapterBaseIndexStore,
        model_catalog: &'a dyn ModelCatalogStore,
    ) -> Self {
        Self {
            layout_resolver,
            adapter_catalog,
            source_metadata_reader,
            base_indexes,
            model_catalog,
        }
    }
}

impl AdapterBindUseCase for StdAdapterBindUseCase<'_> {
    fn bind_adapter(&self, request: AdapterBindRequest) -> KernelResult<AdapterBindResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = adapter_store_layout(&layout);
        let model_store = model_store_layout(&layout);
        let inspection = self
            .adapter_catalog
            .inspect_adapter(&store, &request.adapter_selector)?;
        let base_model = self
            .model_catalog
            .inspect_model(&model_store, &request.base_model_selector)?
            .metadata;
        let source_metadata = self
            .source_metadata_reader
            .read_source_metadata(&inspection.source_path)?;
        validate_source_metadata(&source_metadata, Some(&base_model))?;

        let mut metadata = inspection.metadata;
        let previous_base_model_ref = metadata.base_model_ref.clone();
        apply_base_metadata(&mut metadata, &source_metadata, Some(&base_model));
        self.adapter_catalog
            .save_adapter_metadata(&store, &metadata)?;

        let removed_base_index_path =
            match (previous_base_model_ref, metadata.base_model_ref.clone()) {
                (Some(previous), Some(current)) if previous != current => {
                    self.base_indexes.remove_base_model_index(
                        &store,
                        &base_index_for_metadata(&metadata, previous),
                    )?
                }
                _ => None,
            };
        let base_index_path = self.base_indexes.save_base_model_index(
            &store,
            &base_index_for_metadata(&metadata, base_model.model_ref),
        )?;

        Ok(AdapterBindResult {
            layout,
            store,
            outcome: AdapterBindOutcome {
                metadata,
                store_path: inspection.store_path,
                base_index_path,
                removed_base_index_path,
            },
        })
    }
}
