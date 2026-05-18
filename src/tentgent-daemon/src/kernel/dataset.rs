use tentgent_kernel::{
    features::{
        auth::usecases::AuthSecretResolverUseCase,
        dataset::{
            infra::{
                FileDatasetCatalogStore, FileDatasetContentStore, FileDatasetReferenceGuard,
                FileDatasetSourceIndexStore, MarkdownDatasetTemplateRenderer, StdDatasetDiffer,
                StdDatasetIdentityGenerator, StdDatasetManifestBuilder, StdDatasetPackageDetector,
                StdDatasetSourceStager, StdDatasetStoreLayoutInitializer, StdDatasetValidator,
            },
            ports::DatasetEvalRuntimeClient,
            usecases::{
                DatasetCatalogReadUseCase, DatasetInspectRequest, DatasetInspectResult,
                DatasetListRequest, DatasetListResult, StdDatasetCatalogReadUseCase,
                StdDatasetDiffUseCase, StdDatasetEvaluationUseCase, StdDatasetExportUseCase,
                StdDatasetLocalImportUseCase, StdDatasetRemoveUseCase, StdDatasetSynthesisUseCase,
                StdDatasetTemplateUseCase, StdDatasetValidationUseCase,
            },
        },
        runtime::usecases::RuntimeResolutionUseCase,
    },
    foundation::{error::KernelResult, layout::StdRuntimeLayoutResolver},
};

pub struct DatasetKernelComponent {
    layout_resolver: StdRuntimeLayoutResolver,
    layout_initializer: StdDatasetStoreLayoutInitializer,
    stager: StdDatasetSourceStager,
    manifest_builder: StdDatasetManifestBuilder,
    identity: StdDatasetIdentityGenerator,
    package_detector: StdDatasetPackageDetector,
    catalog: FileDatasetCatalogStore,
    source_indexes: FileDatasetSourceIndexStore,
    content: FileDatasetContentStore,
    validator: StdDatasetValidator,
    differ: StdDatasetDiffer,
    template_renderer: MarkdownDatasetTemplateRenderer,
    reference_guard: FileDatasetReferenceGuard,
}

impl DatasetKernelComponent {
    pub fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            layout_initializer: StdDatasetStoreLayoutInitializer,
            stager: StdDatasetSourceStager,
            manifest_builder: StdDatasetManifestBuilder,
            identity: StdDatasetIdentityGenerator,
            package_detector: StdDatasetPackageDetector,
            catalog: FileDatasetCatalogStore,
            source_indexes: FileDatasetSourceIndexStore,
            content: FileDatasetContentStore,
            validator: StdDatasetValidator,
            differ: StdDatasetDiffer,
            template_renderer: MarkdownDatasetTemplateRenderer,
            reference_guard: FileDatasetReferenceGuard,
        }
    }

    pub fn catalog_usecase(&self) -> StdDatasetCatalogReadUseCase<'_> {
        StdDatasetCatalogReadUseCase::new(&self.layout_resolver, &self.catalog)
    }

    pub fn local_import_usecase(&self) -> StdDatasetLocalImportUseCase<'_> {
        StdDatasetLocalImportUseCase::new(
            &self.layout_resolver,
            &self.layout_initializer,
            &self.stager,
            &self.manifest_builder,
            &self.identity,
            &self.package_detector,
            &self.catalog,
            &self.source_indexes,
            &self.content,
        )
    }

    pub fn remove_usecase(&self) -> StdDatasetRemoveUseCase<'_> {
        StdDatasetRemoveUseCase::new(
            &self.layout_resolver,
            &self.catalog,
            &self.source_indexes,
            &self.content,
            &self.reference_guard,
        )
    }

    pub fn template_usecase(&self) -> StdDatasetTemplateUseCase<'_> {
        StdDatasetTemplateUseCase::new(&self.template_renderer)
    }

    pub fn validation_usecase(&self) -> StdDatasetValidationUseCase<'_> {
        StdDatasetValidationUseCase::new(&self.layout_resolver, &self.catalog, &self.validator)
    }

    pub fn diff_usecase(&self) -> StdDatasetDiffUseCase<'_> {
        StdDatasetDiffUseCase::new(&self.layout_resolver, &self.differ)
    }

    pub fn export_usecase(&self) -> StdDatasetExportUseCase<'_> {
        StdDatasetExportUseCase::new(&self.layout_resolver, &self.catalog, &self.content)
    }

    pub fn synthesis_usecase<'a>(
        &'a self,
        runtime_resolution: &'a dyn RuntimeResolutionUseCase,
        auth_resolver: &'a dyn AuthSecretResolverUseCase,
        runtime_client: &'a dyn tentgent_kernel::features::dataset::ports::DatasetSynthRuntimeClient,
    ) -> StdDatasetSynthesisUseCase<'a> {
        StdDatasetSynthesisUseCase::new(runtime_resolution, auth_resolver, runtime_client)
    }

    pub fn evaluation_usecase<'a>(
        &'a self,
        runtime_resolution: &'a dyn RuntimeResolutionUseCase,
        auth_resolver: &'a dyn AuthSecretResolverUseCase,
        runtime_client: &'a dyn DatasetEvalRuntimeClient,
    ) -> StdDatasetEvaluationUseCase<'a> {
        StdDatasetEvaluationUseCase::new(
            runtime_resolution,
            auth_resolver,
            &self.catalog,
            runtime_client,
        )
    }

    pub(crate) fn catalog_store(&self) -> &FileDatasetCatalogStore {
        &self.catalog
    }
}

impl Default for DatasetKernelComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl DatasetCatalogReadUseCase for DatasetKernelComponent {
    fn list_datasets(&self, request: DatasetListRequest) -> KernelResult<DatasetListResult> {
        self.catalog_usecase().list_datasets(request)
    }

    fn inspect_dataset(
        &self,
        request: DatasetInspectRequest,
    ) -> KernelResult<DatasetInspectResult> {
        self.catalog_usecase().inspect_dataset(request)
    }
}
