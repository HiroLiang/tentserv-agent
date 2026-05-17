//! Local dataset import use case.

use crate::features::dataset::ports::{
    DatasetCatalogStore, DatasetContentStore, DatasetIdentityGenerator, DatasetManifestBuilder,
    DatasetPackageDetector, DatasetSourceIndexStore, DatasetSourceStager,
    DatasetStoreLayoutInitializer,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::{
    dataset_store_layout, detect_dataset_format, DatasetImportFinalizer, DatasetImportSource,
};
use super::port::{DatasetLocalImportRequest, DatasetLocalImportResult, DatasetLocalImportUseCase};

/// Standard local dataset import orchestration.
pub struct StdDatasetLocalImportUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    layout_initializer: &'a dyn DatasetStoreLayoutInitializer,
    stager: &'a dyn DatasetSourceStager,
    manifest_builder: &'a dyn DatasetManifestBuilder,
    identity: &'a dyn DatasetIdentityGenerator,
    package_detector: &'a dyn DatasetPackageDetector,
    catalog: &'a dyn DatasetCatalogStore,
    source_indexes: &'a dyn DatasetSourceIndexStore,
    content: &'a dyn DatasetContentStore,
}

impl<'a> StdDatasetLocalImportUseCase<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        layout_initializer: &'a dyn DatasetStoreLayoutInitializer,
        stager: &'a dyn DatasetSourceStager,
        manifest_builder: &'a dyn DatasetManifestBuilder,
        identity: &'a dyn DatasetIdentityGenerator,
        package_detector: &'a dyn DatasetPackageDetector,
        catalog: &'a dyn DatasetCatalogStore,
        source_indexes: &'a dyn DatasetSourceIndexStore,
        content: &'a dyn DatasetContentStore,
    ) -> Self {
        Self {
            layout_resolver,
            layout_initializer,
            stager,
            manifest_builder,
            identity,
            package_detector,
            catalog,
            source_indexes,
            content,
        }
    }
}

impl DatasetLocalImportUseCase for StdDatasetLocalImportUseCase<'_> {
    fn import_local_dataset(
        &self,
        request: DatasetLocalImportRequest,
    ) -> KernelResult<DatasetLocalImportResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = dataset_store_layout(&layout);
        self.layout_initializer
            .ensure_dataset_store_layout(&store)?;

        let dataset_format = detect_dataset_format(&request.source_path)?;
        let staged = self.stager.create_staging_source(&store, "add")?;
        self.stager
            .copy_local_source(&request.source_path, &staged)?;
        let outcome = self.finalizer().finalize(
            &store,
            &staged,
            DatasetImportSource {
                original_path: request.source_path,
                dataset_format,
            },
        )?;

        Ok(DatasetLocalImportResult {
            layout,
            store,
            outcome,
        })
    }
}

impl StdDatasetLocalImportUseCase<'_> {
    fn finalizer(&self) -> DatasetImportFinalizer<'_> {
        DatasetImportFinalizer {
            stager: self.stager,
            manifest_builder: self.manifest_builder,
            identity: self.identity,
            package_detector: self.package_detector,
            catalog: self.catalog,
            source_indexes: self.source_indexes,
            content: self.content,
        }
    }
}
