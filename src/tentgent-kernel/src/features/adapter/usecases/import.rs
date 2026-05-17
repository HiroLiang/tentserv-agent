//! Local adapter import use case.

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
use super::port::{AdapterLocalImportRequest, AdapterLocalImportResult, AdapterLocalImportUseCase};

/// Standard local adapter import orchestration.
pub struct StdAdapterLocalImportUseCase<'a> {
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

impl<'a> StdAdapterLocalImportUseCase<'a> {
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

impl AdapterLocalImportUseCase for StdAdapterLocalImportUseCase<'_> {
    fn import_local_adapter(
        &self,
        request: AdapterLocalImportRequest,
    ) -> KernelResult<AdapterLocalImportResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = adapter_store_layout(&layout);
        let model_store = model_store_layout(&layout);
        self.layout_initializer
            .ensure_adapter_store_layout(&store)?;

        let base_model = match &request.base_model_selector {
            Some(selector) => Some(
                self.model_catalog
                    .inspect_model(&model_store, selector)?
                    .metadata,
            ),
            None => None,
        };

        let staged = self
            .stager
            .create_staging_source(&store, AdapterSourceKind::Local)?;
        self.stager
            .copy_local_source(&request.source_path, &staged)?;
        let outcome = self.finalizer().finalize(
            &store,
            &staged,
            AdapterImportSource::Local {
                original_path: request.source_path,
            },
            base_model.as_ref(),
        )?;

        Ok(AdapterLocalImportResult {
            layout,
            store,
            outcome,
        })
    }
}

impl StdAdapterLocalImportUseCase<'_> {
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
