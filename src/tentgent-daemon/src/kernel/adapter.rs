use tentgent_kernel::{
    features::{
        adapter::{
            infra::{
                FileAdapterBaseIndexStore, FileAdapterCatalogStore, FileAdapterContentStore,
                FileAdapterServerReferenceProbe, FileAdapterSourceIndexStore,
                StdAdapterIdentityGenerator, StdAdapterManifestBuilder,
                StdAdapterSourceMetadataReader, StdAdapterSourceStager,
                StdAdapterStoreLayoutInitializer, StdHfAdapterSnapshotFetcher,
            },
            usecases::{
                AdapterCatalogReadUseCase, AdapterCompatibilityCheckRequest,
                AdapterCompatibilityCheckResult, AdapterCompatibilityCheckUseCase,
                AdapterInspectRequest, AdapterInspectResult, AdapterListRequest, AdapterListResult,
                StdAdapterBindUseCase, StdAdapterCatalogReadUseCase,
                StdAdapterCompatibilityCheckUseCase, StdAdapterHfPullUseCase,
                StdAdapterLocalImportUseCase, StdAdapterRemoveUseCase,
                StdAdapterTrainRunImportUseCase,
            },
        },
        auth::usecases::AuthSecretResolverUseCase,
        chat::{
            infra::StdChatAdapterResolver,
            ports::{ChatAdapterResolveRequest, ChatAdapterResolveResult, ChatAdapterResolver},
        },
        image_generation::{
            infra::StdImageGenerationAdapterResolver,
            ports::{
                ImageGenerationAdapterResolveRequest, ImageGenerationAdapterResolveResult,
                ImageGenerationAdapterResolver, ImageGenerationControlResolveRequest,
                ImageGenerationControlResolveResult,
            },
        },
        model::ports::ModelCatalogStore,
        runtime::ports::PythonRuntimeResolver,
    },
    foundation::{error::KernelResult, layout::StdRuntimeLayoutResolver},
};

pub struct AdapterKernelComponent {
    layout_resolver: StdRuntimeLayoutResolver,
    layout_initializer: StdAdapterStoreLayoutInitializer,
    stager: StdAdapterSourceStager,
    snapshot_fetcher: StdHfAdapterSnapshotFetcher,
    manifest_builder: StdAdapterManifestBuilder,
    identity: StdAdapterIdentityGenerator,
    source_metadata_reader: StdAdapterSourceMetadataReader,
    catalog: FileAdapterCatalogStore,
    source_indexes: FileAdapterSourceIndexStore,
    base_indexes: FileAdapterBaseIndexStore,
    content: FileAdapterContentStore,
    server_refs: FileAdapterServerReferenceProbe,
}

impl AdapterKernelComponent {
    pub fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            layout_initializer: StdAdapterStoreLayoutInitializer,
            stager: StdAdapterSourceStager,
            snapshot_fetcher: StdHfAdapterSnapshotFetcher,
            manifest_builder: StdAdapterManifestBuilder,
            identity: StdAdapterIdentityGenerator,
            source_metadata_reader: StdAdapterSourceMetadataReader,
            catalog: FileAdapterCatalogStore,
            source_indexes: FileAdapterSourceIndexStore,
            base_indexes: FileAdapterBaseIndexStore,
            content: FileAdapterContentStore,
            server_refs: FileAdapterServerReferenceProbe,
        }
    }

    pub fn catalog_usecase(&self) -> StdAdapterCatalogReadUseCase<'_> {
        StdAdapterCatalogReadUseCase::new(&self.layout_resolver, &self.catalog)
    }

    pub fn local_import_usecase<'a>(
        &'a self,
        model_catalog: &'a dyn ModelCatalogStore,
    ) -> StdAdapterLocalImportUseCase<'a> {
        StdAdapterLocalImportUseCase::new(
            &self.layout_resolver,
            &self.layout_initializer,
            &self.stager,
            &self.manifest_builder,
            &self.identity,
            &self.source_metadata_reader,
            &self.catalog,
            &self.source_indexes,
            &self.base_indexes,
            &self.content,
            model_catalog,
        )
    }

    pub fn hf_pull_usecase<'a>(
        &'a self,
        runtime_resolver: &'a dyn PythonRuntimeResolver,
        auth_resolver: &'a dyn AuthSecretResolverUseCase,
        model_catalog: &'a dyn ModelCatalogStore,
    ) -> StdAdapterHfPullUseCase<'a> {
        StdAdapterHfPullUseCase::new(
            &self.layout_resolver,
            runtime_resolver,
            auth_resolver,
            &self.layout_initializer,
            &self.stager,
            &self.snapshot_fetcher,
            &self.manifest_builder,
            &self.identity,
            &self.source_metadata_reader,
            &self.catalog,
            &self.source_indexes,
            &self.base_indexes,
            &self.content,
            model_catalog,
        )
    }

    pub fn bind_usecase<'a>(
        &'a self,
        model_catalog: &'a dyn ModelCatalogStore,
    ) -> StdAdapterBindUseCase<'a> {
        StdAdapterBindUseCase::new(
            &self.layout_resolver,
            &self.catalog,
            &self.source_metadata_reader,
            &self.base_indexes,
            model_catalog,
        )
    }

    pub fn compatibility_usecase(&self) -> StdAdapterCompatibilityCheckUseCase<'_> {
        StdAdapterCompatibilityCheckUseCase::new(&self.layout_resolver, &self.catalog)
    }

    pub fn train_run_import_usecase<'a>(
        &'a self,
        model_catalog: &'a dyn ModelCatalogStore,
    ) -> StdAdapterTrainRunImportUseCase<'a> {
        StdAdapterTrainRunImportUseCase::new(
            &self.layout_resolver,
            &self.layout_initializer,
            &self.stager,
            &self.manifest_builder,
            &self.identity,
            &self.source_metadata_reader,
            &self.catalog,
            &self.source_indexes,
            &self.base_indexes,
            &self.content,
            model_catalog,
        )
    }

    pub fn remove_usecase(&self) -> StdAdapterRemoveUseCase<'_> {
        StdAdapterRemoveUseCase::new(
            &self.layout_resolver,
            &self.catalog,
            &self.source_indexes,
            &self.base_indexes,
            &self.content,
            &self.server_refs,
        )
    }
}

impl Default for AdapterKernelComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl AdapterCatalogReadUseCase for AdapterKernelComponent {
    fn list_adapters(&self, request: AdapterListRequest) -> KernelResult<AdapterListResult> {
        self.catalog_usecase().list_adapters(request)
    }

    fn inspect_adapter(
        &self,
        request: AdapterInspectRequest,
    ) -> KernelResult<AdapterInspectResult> {
        self.catalog_usecase().inspect_adapter(request)
    }
}

impl AdapterCompatibilityCheckUseCase for AdapterKernelComponent {
    fn check_adapter_compatibility(
        &self,
        request: AdapterCompatibilityCheckRequest,
    ) -> KernelResult<AdapterCompatibilityCheckResult> {
        self.compatibility_usecase()
            .check_adapter_compatibility(request)
    }
}

impl ChatAdapterResolver for AdapterKernelComponent {
    fn resolve_chat_adapter(
        &self,
        request: ChatAdapterResolveRequest,
    ) -> KernelResult<ChatAdapterResolveResult> {
        StdChatAdapterResolver::new(self).resolve_chat_adapter(request)
    }
}

impl ImageGenerationAdapterResolver for AdapterKernelComponent {
    fn resolve_image_generation_adapter(
        &self,
        request: ImageGenerationAdapterResolveRequest,
    ) -> KernelResult<ImageGenerationAdapterResolveResult> {
        StdImageGenerationAdapterResolver::new(self).resolve_image_generation_adapter(request)
    }

    fn resolve_image_generation_control(
        &self,
        request: ImageGenerationControlResolveRequest,
    ) -> KernelResult<ImageGenerationControlResolveResult> {
        StdImageGenerationAdapterResolver::new(self).resolve_image_generation_control(request)
    }
}
