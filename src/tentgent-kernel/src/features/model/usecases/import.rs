//! Local model import use case.

use crate::features::model::domain::ModelImportMethod;
use crate::features::model::ports::{
    ModelCatalogStore, ModelContentStore, ModelIdentityGenerator, ModelManifestBuilder,
    ModelSourceIndexStore, ModelSourceStager, ModelStoreLayoutInitializer,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::{model_store_layout, ModelImportFinalizer, ModelImportSource};
use super::port::{ModelLocalImportRequest, ModelLocalImportResult, ModelLocalImportUseCase};

/// Standard local model import orchestration.
pub struct StdModelLocalImportUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    layout_initializer: &'a dyn ModelStoreLayoutInitializer,
    stager: &'a dyn ModelSourceStager,
    manifest_builder: &'a dyn ModelManifestBuilder,
    identity: &'a dyn ModelIdentityGenerator,
    catalog: &'a dyn ModelCatalogStore,
    source_indexes: &'a dyn ModelSourceIndexStore,
    content: &'a dyn ModelContentStore,
}

impl<'a> StdModelLocalImportUseCase<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        layout_initializer: &'a dyn ModelStoreLayoutInitializer,
        stager: &'a dyn ModelSourceStager,
        manifest_builder: &'a dyn ModelManifestBuilder,
        identity: &'a dyn ModelIdentityGenerator,
        catalog: &'a dyn ModelCatalogStore,
        source_indexes: &'a dyn ModelSourceIndexStore,
        content: &'a dyn ModelContentStore,
    ) -> Self {
        Self {
            layout_resolver,
            layout_initializer,
            stager,
            manifest_builder,
            identity,
            catalog,
            source_indexes,
            content,
        }
    }
}

impl ModelLocalImportUseCase for StdModelLocalImportUseCase<'_> {
    fn import_local_model(
        &self,
        request: ModelLocalImportRequest,
    ) -> KernelResult<ModelLocalImportResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = model_store_layout(&layout);
        self.layout_initializer.ensure_model_store_layout(&store)?;

        let staged = self
            .stager
            .create_staging_source(&store, ModelImportMethod::Add)?;
        self.stager
            .copy_local_source(&request.source_path, &staged)?;
        let outcome = self.finalizer().finalize(
            &store,
            &staged,
            ModelImportSource::Local {
                original_path: request.source_path,
            },
            ModelImportMethod::Add,
            request.capability,
        )?;

        Ok(ModelLocalImportResult {
            layout,
            store,
            outcome,
        })
    }
}

impl StdModelLocalImportUseCase<'_> {
    fn finalizer(&self) -> ModelImportFinalizer<'_> {
        ModelImportFinalizer {
            stager: self.stager,
            manifest_builder: self.manifest_builder,
            identity: self.identity,
            catalog: self.catalog,
            source_indexes: self.source_indexes,
            content: self.content,
        }
    }
}
