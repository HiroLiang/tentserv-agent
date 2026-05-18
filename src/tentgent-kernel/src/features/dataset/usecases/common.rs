use std::path::{Path, PathBuf};

use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::features::auth::domain::{AuthSecretMaterial, Provider};
use crate::features::auth::usecases::{AuthSecretResolutionRequest, AuthSecretResolverUseCase};
use crate::features::dataset::domain::{
    DatasetFormat, DatasetImportOutcome, DatasetMetadata, DatasetProvider, DatasetSourceKind,
    DatasetStoreLayout, LocalDatasetSourceIndex,
};
use crate::features::dataset::ports::{
    DatasetCatalogStore, DatasetContentStore, DatasetIdentityGenerator, DatasetManifestBuilder,
    DatasetPackageDetector, DatasetRuntimeAuth, DatasetSourceIndexStore, DatasetSourceStager,
    StagedDatasetSource,
};
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayout;

pub(super) struct DatasetImportSource {
    pub original_path: PathBuf,
    pub dataset_format: DatasetFormat,
}

pub(super) struct DatasetImportFinalizer<'a> {
    pub stager: &'a dyn DatasetSourceStager,
    pub manifest_builder: &'a dyn DatasetManifestBuilder,
    pub identity: &'a dyn DatasetIdentityGenerator,
    pub package_detector: &'a dyn DatasetPackageDetector,
    pub catalog: &'a dyn DatasetCatalogStore,
    pub source_indexes: &'a dyn DatasetSourceIndexStore,
    pub content: &'a dyn DatasetContentStore,
}

impl DatasetImportFinalizer<'_> {
    pub fn finalize(
        &self,
        store: &DatasetStoreLayout,
        staged: &StagedDatasetSource,
        source: DatasetImportSource,
    ) -> KernelResult<DatasetImportOutcome> {
        let result = self.finalize_inner(store, staged, source);
        let cleanup = self.stager.discard_staging(staged);

        match (result, cleanup) {
            (Ok(outcome), Ok(())) => Ok(outcome),
            (Err(err), _) => Err(err),
            (Ok(_), Err(err)) => Err(err),
        }
    }

    fn finalize_inner(
        &self,
        store: &DatasetStoreLayout,
        staged: &StagedDatasetSource,
        source: DatasetImportSource,
    ) -> KernelResult<DatasetImportOutcome> {
        let manifest = self.manifest_builder.build_manifest(&staged.source_dir)?;
        let package = self
            .package_detector
            .detect_package(&staged.source_dir, &manifest)?;
        let dataset_ref = self.identity.dataset_ref_for_manifest(&manifest)?;
        let store_path = store.dataset_dir(&dataset_ref);

        if self.content.dataset_content_exists(store, &dataset_ref)? {
            let metadata = self.catalog.load_dataset_metadata(store, &dataset_ref)?;
            let source_index_path = self.save_source_index(store, &metadata, &source)?;
            return Ok(DatasetImportOutcome {
                metadata,
                store_path,
                source_index_path,
                deduplicated: true,
            });
        }

        self.content
            .install_staged_source(store, staged, &dataset_ref)?;

        let metadata = DatasetMetadata {
            short_ref: dataset_ref.short_ref().to_string(),
            dataset_ref: dataset_ref.clone(),
            source_kind: DatasetSourceKind::Local,
            source_path: Some(source.original_path.display().to_string()),
            source_repo: None,
            source_revision: None,
            dataset_format: source.dataset_format,
            file_count: manifest.file_count(),
            total_bytes: manifest.total_bytes(),
            imported_at: imported_at_now()?,
            package,
        };

        self.catalog.save_dataset_metadata(store, &metadata)?;
        self.catalog
            .save_dataset_manifest(store, &metadata.dataset_ref, &manifest)?;
        let source_index_path = self.save_source_index(store, &metadata, &source)?;

        Ok(DatasetImportOutcome {
            metadata,
            store_path,
            source_index_path,
            deduplicated: false,
        })
    }

    fn save_source_index(
        &self,
        store: &DatasetStoreLayout,
        metadata: &DatasetMetadata,
        source: &DatasetImportSource,
    ) -> KernelResult<PathBuf> {
        self.source_indexes.save_local_source_index(
            store,
            &LocalDatasetSourceIndex {
                dataset_ref: metadata.dataset_ref.clone(),
                short_ref: metadata.short_ref.clone(),
                source_path: source.original_path.display().to_string(),
                imported_at: metadata.imported_at.clone(),
            },
        )
    }
}

pub(super) fn dataset_store_layout(layout: &RuntimeLayout) -> DatasetStoreLayout {
    DatasetStoreLayout::from_datasets_dir(layout.datasets_dir.clone())
}

pub(super) fn detect_dataset_format(path: &Path) -> KernelResult<DatasetFormat> {
    if path.is_dir() {
        return Ok(DatasetFormat::Directory);
    }

    if path.is_file()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"))
    {
        return Ok(DatasetFormat::Jsonl);
    }

    Err(dataset_store_error(format!(
        "expected a .jsonl file or a directory containing dataset files: `{}`",
        path.display()
    )))
}

pub(super) fn resolve_dataset_runtime_auth(
    auth_resolver: &dyn AuthSecretResolverUseCase,
    request: AuthSecretResolutionRequest,
    expected_provider: DatasetProvider,
    purpose: &str,
) -> KernelResult<DatasetRuntimeAuth> {
    let expected_auth_provider = auth_provider_for_dataset_provider(expected_provider);
    if request.provider != expected_auth_provider {
        return Err(dataset_runtime_error(format!(
            "{purpose} requires {} auth resolution, got {}",
            expected_auth_provider.display_name(),
            request.provider.display_name()
        )));
    }

    let resolved = auth_resolver.resolve_secret(request)?;
    let secret = resolved.secret.ok_or_else(|| {
        dataset_runtime_error(format!(
            "{} auth secret is required for {purpose}",
            expected_auth_provider.display_name()
        ))
    })?;
    ensure_secret_provider(secret, expected_auth_provider, purpose)
}

pub(super) fn dataset_store_error(message: impl Into<String>) -> KernelError {
    KernelError::DatasetStoreUnavailable(message.into())
}

fn dataset_runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::DatasetRuntimeUnavailable(message.into())
}

fn ensure_secret_provider(
    secret: AuthSecretMaterial,
    expected_provider: Provider,
    purpose: &str,
) -> KernelResult<DatasetRuntimeAuth> {
    if secret.provider != expected_provider {
        return Err(dataset_runtime_error(format!(
            "{purpose} resolved {} secret, expected {}",
            secret.provider.display_name(),
            expected_provider.display_name()
        )));
    }

    Ok(DatasetRuntimeAuth { secret })
}

fn auth_provider_for_dataset_provider(provider: DatasetProvider) -> Provider {
    match provider {
        DatasetProvider::OpenAI => Provider::OpenAI,
        DatasetProvider::Anthropic => Provider::Anthropic,
    }
}

fn imported_at_now() -> KernelResult<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|err| dataset_store_error(format!("format dataset import time failed: {err}")))
}
