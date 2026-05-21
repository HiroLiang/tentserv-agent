//! Training-run adapter import use case.

use crate::features::adapter::domain::AdapterSourceKind;
use crate::features::adapter::ports::{
    AdapterBaseIndexStore, AdapterCatalogStore, AdapterContentStore, AdapterIdentityGenerator,
    AdapterManifestBuilder, AdapterSourceIndexStore, AdapterSourceMetadataReader,
    AdapterSourceStager, AdapterStoreLayoutInitializer,
};
use crate::features::model::ports::ModelCatalogStore;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::{
    adapter_store_layout, model_store_layout, AdapterImportFinalizer, AdapterImportSource,
};
use super::port::{
    AdapterTrainRunImportRequest, AdapterTrainRunImportResult, AdapterTrainRunImportUseCase,
};

/// Standard training-run adapter import orchestration.
pub struct StdAdapterTrainRunImportUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    layout_initializer: &'a dyn AdapterStoreLayoutInitializer,
    stager: &'a dyn AdapterSourceStager,
    manifest_builder: &'a dyn AdapterManifestBuilder,
    identity: &'a dyn AdapterIdentityGenerator,
    source_metadata_reader: &'a dyn AdapterSourceMetadataReader,
    adapter_catalog: &'a dyn AdapterCatalogStore,
    source_indexes: &'a dyn AdapterSourceIndexStore,
    base_indexes: &'a dyn AdapterBaseIndexStore,
    content: &'a dyn AdapterContentStore,
    model_catalog: &'a dyn ModelCatalogStore,
}

impl<'a> StdAdapterTrainRunImportUseCase<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        layout_initializer: &'a dyn AdapterStoreLayoutInitializer,
        stager: &'a dyn AdapterSourceStager,
        manifest_builder: &'a dyn AdapterManifestBuilder,
        identity: &'a dyn AdapterIdentityGenerator,
        source_metadata_reader: &'a dyn AdapterSourceMetadataReader,
        adapter_catalog: &'a dyn AdapterCatalogStore,
        source_indexes: &'a dyn AdapterSourceIndexStore,
        base_indexes: &'a dyn AdapterBaseIndexStore,
        content: &'a dyn AdapterContentStore,
        model_catalog: &'a dyn ModelCatalogStore,
    ) -> Self {
        Self {
            layout_resolver,
            layout_initializer,
            stager,
            manifest_builder,
            identity,
            source_metadata_reader,
            adapter_catalog,
            source_indexes,
            base_indexes,
            content,
            model_catalog,
        }
    }
}

impl AdapterTrainRunImportUseCase for StdAdapterTrainRunImportUseCase<'_> {
    fn import_train_run_adapter(
        &self,
        request: AdapterTrainRunImportRequest,
    ) -> KernelResult<AdapterTrainRunImportResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = adapter_store_layout(&layout);
        let model_store = model_store_layout(&layout);
        self.layout_initializer
            .ensure_adapter_store_layout(&store)?;

        let base_model = self
            .model_catalog
            .inspect_model(&model_store, &request.base_model_selector)?
            .metadata;

        let staged = self
            .stager
            .create_staging_source(&store, AdapterSourceKind::TrainRun)?;
        self.stager
            .copy_local_source(&request.output_path, &staged)?;
        let outcome = self.finalizer().finalize(
            &store,
            &staged,
            AdapterImportSource::TrainRun {
                output_path: request.output_path,
                run_ref: request.training_run_ref,
                dataset_ref: request.training_dataset_ref,
                config_ref: request.training_config_ref,
            },
            Some(&base_model),
            &request.options,
        )?;

        Ok(AdapterTrainRunImportResult {
            layout,
            store,
            outcome,
        })
    }
}

impl StdAdapterTrainRunImportUseCase<'_> {
    fn finalizer(&self) -> AdapterImportFinalizer<'_> {
        AdapterImportFinalizer {
            stager: self.stager,
            manifest_builder: self.manifest_builder,
            identity: self.identity,
            source_metadata_reader: self.source_metadata_reader,
            catalog: self.adapter_catalog,
            source_indexes: self.source_indexes,
            base_indexes: self.base_indexes,
            content: self.content,
        }
    }
}
