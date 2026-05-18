use tentgent_kernel::{
    features::{
        auth::usecases::AuthSecretResolverUseCase,
        chat::{
            infra::StdChatModelResolver,
            ports::{ChatModelResolveRequest, ChatModelResolveResult, ChatModelResolver},
        },
        model::{
            infra::{
                FileModelCatalogStore, FileModelContentStore, FileModelServerReferenceProbe,
                FileModelSourceIndexStore, StdHfModelSnapshotFetcher, StdModelIdentityGenerator,
                StdModelManifestBuilder, StdModelSourceStager, StdModelStoreLayoutInitializer,
            },
            usecases::{
                ModelCatalogReadUseCase, ModelInspectRequest, ModelInspectResult, ModelListRequest,
                ModelListResult, StdModelCatalogReadUseCase, StdModelHfPullUseCase,
                StdModelLocalImportUseCase, StdModelRemoveUseCase,
            },
        },
        runtime::ports::PythonRuntimeResolver,
    },
    foundation::{error::KernelResult, layout::StdRuntimeLayoutResolver},
};

pub struct ModelKernelComponent {
    layout_resolver: StdRuntimeLayoutResolver,
    layout_initializer: StdModelStoreLayoutInitializer,
    stager: StdModelSourceStager,
    snapshot_fetcher: StdHfModelSnapshotFetcher,
    manifest_builder: StdModelManifestBuilder,
    identity: StdModelIdentityGenerator,
    catalog: FileModelCatalogStore,
    source_indexes: FileModelSourceIndexStore,
    content: FileModelContentStore,
    server_refs: FileModelServerReferenceProbe,
}

impl ModelKernelComponent {
    pub fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            layout_initializer: StdModelStoreLayoutInitializer,
            stager: StdModelSourceStager,
            snapshot_fetcher: StdHfModelSnapshotFetcher,
            manifest_builder: StdModelManifestBuilder,
            identity: StdModelIdentityGenerator,
            catalog: FileModelCatalogStore,
            source_indexes: FileModelSourceIndexStore,
            content: FileModelContentStore,
            server_refs: FileModelServerReferenceProbe,
        }
    }

    pub fn catalog_usecase(&self) -> StdModelCatalogReadUseCase<'_> {
        StdModelCatalogReadUseCase::new(&self.layout_resolver, &self.catalog)
    }

    pub fn local_import_usecase(&self) -> StdModelLocalImportUseCase<'_> {
        StdModelLocalImportUseCase::new(
            &self.layout_resolver,
            &self.layout_initializer,
            &self.stager,
            &self.manifest_builder,
            &self.identity,
            &self.catalog,
            &self.source_indexes,
            &self.content,
        )
    }

    pub fn hf_pull_usecase<'a>(
        &'a self,
        runtime_resolver: &'a dyn PythonRuntimeResolver,
        auth_resolver: &'a dyn AuthSecretResolverUseCase,
    ) -> StdModelHfPullUseCase<'a> {
        StdModelHfPullUseCase::new(
            &self.layout_resolver,
            runtime_resolver,
            auth_resolver,
            &self.layout_initializer,
            &self.stager,
            &self.snapshot_fetcher,
            &self.manifest_builder,
            &self.identity,
            &self.catalog,
            &self.source_indexes,
            &self.content,
        )
    }

    pub fn remove_usecase(&self) -> StdModelRemoveUseCase<'_> {
        StdModelRemoveUseCase::new(
            &self.layout_resolver,
            &self.catalog,
            &self.source_indexes,
            &self.content,
            &self.server_refs,
        )
    }

    pub(crate) fn catalog_store(&self) -> &FileModelCatalogStore {
        &self.catalog
    }
}

impl Default for ModelKernelComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelCatalogReadUseCase for ModelKernelComponent {
    fn list_models(&self, request: ModelListRequest) -> KernelResult<ModelListResult> {
        self.catalog_usecase().list_models(request)
    }

    fn inspect_model(&self, request: ModelInspectRequest) -> KernelResult<ModelInspectResult> {
        self.catalog_usecase().inspect_model(request)
    }
}

impl ChatModelResolver for ModelKernelComponent {
    fn resolve_chat_model(
        &self,
        request: ChatModelResolveRequest,
    ) -> KernelResult<ChatModelResolveResult> {
        StdChatModelResolver::new(self).resolve_chat_model(request)
    }
}
