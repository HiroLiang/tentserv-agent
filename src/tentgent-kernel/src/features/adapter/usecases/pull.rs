//! Hugging Face adapter pull use case.

use std::path::Path;

use crate::features::adapter::domain::{AdapterSourceKind, HfAdapterPullProgress};
use crate::features::adapter::ports::{
    AdapterBaseIndexStore, AdapterCatalogStore, AdapterContentStore, AdapterIdentityGenerator,
    AdapterManifestBuilder, AdapterSourceIndexStore, AdapterSourceMetadataReader,
    AdapterSourceStager, AdapterStoreLayoutInitializer, HfAdapterSnapshotFetcher,
    HfAdapterSnapshotRequest,
};
use crate::features::auth::domain::Provider;
use crate::features::auth::usecases::AuthSecretResolverUseCase;
use crate::features::model::ports::ModelCatalogStore;
use crate::features::runtime::ports::PythonRuntimeResolver;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::{
    adapter_store_error, adapter_store_layout, model_store_layout, AdapterImportFinalizer,
    AdapterImportSource,
};
use super::port::{AdapterHfPullRequest, AdapterHfPullResult, AdapterHfPullUseCase};

/// Standard Hugging Face adapter pull orchestration.
pub struct StdAdapterHfPullUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    runtime_resolver: &'a dyn PythonRuntimeResolver,
    auth_resolver: &'a dyn AuthSecretResolverUseCase,
    layout_initializer: &'a dyn AdapterStoreLayoutInitializer,
    stager: &'a dyn AdapterSourceStager,
    snapshot_fetcher: &'a dyn HfAdapterSnapshotFetcher,
    manifest_builder: &'a dyn AdapterManifestBuilder,
    identity: &'a dyn AdapterIdentityGenerator,
    source_metadata_reader: &'a dyn AdapterSourceMetadataReader,
    adapter_catalog: &'a dyn AdapterCatalogStore,
    source_indexes: &'a dyn AdapterSourceIndexStore,
    base_indexes: &'a dyn AdapterBaseIndexStore,
    content: &'a dyn AdapterContentStore,
    model_catalog: &'a dyn ModelCatalogStore,
}

impl<'a> StdAdapterHfPullUseCase<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        runtime_resolver: &'a dyn PythonRuntimeResolver,
        auth_resolver: &'a dyn AuthSecretResolverUseCase,
        layout_initializer: &'a dyn AdapterStoreLayoutInitializer,
        stager: &'a dyn AdapterSourceStager,
        snapshot_fetcher: &'a dyn HfAdapterSnapshotFetcher,
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
            runtime_resolver,
            auth_resolver,
            layout_initializer,
            stager,
            snapshot_fetcher,
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

impl AdapterHfPullUseCase for StdAdapterHfPullUseCase<'_> {
    fn pull_hf_adapter(
        &self,
        request: AdapterHfPullRequest,
        progress: &mut dyn FnMut(HfAdapterPullProgress),
    ) -> KernelResult<AdapterHfPullResult> {
        if request.auth.provider != Provider::HuggingFace {
            return Err(adapter_store_error(format!(
                "Hugging Face adapter pull requires Hugging Face auth resolution, got {:?}",
                request.auth.provider
            )));
        }

        let layout = self.layout_resolver.resolve(request.layout)?;
        let runtime = self
            .runtime_resolver
            .resolve_python_runtime(&layout, request.runtime)?;
        let auth = self.auth_resolver.resolve_secret(request.auth)?;
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
            .create_staging_source(&store, AdapterSourceKind::HuggingFace)?;
        let snapshot = match self.snapshot_fetcher.fetch_hf_snapshot(
            HfAdapterSnapshotRequest {
                runtime: runtime.clone(),
                repo_id: request.repo_id,
                revision: request.revision,
                destination_dir: staged.source_dir.clone(),
                secret: auth.secret,
            },
            progress,
        ) {
            Ok(snapshot) => snapshot,
            Err(err) => {
                let _ = self.stager.discard_staging(&staged);
                return Err(err);
            }
        };

        if !same_path(&snapshot.local_dir, &staged.source_dir) {
            let _ = self.stager.discard_staging(&staged);
            return Err(adapter_store_error(format!(
                "Hugging Face snapshot helper wrote to `{}` instead of requested staging source `{}`",
                snapshot.local_dir.display(),
                staged.source_dir.display()
            )));
        }

        let outcome = self.finalizer().finalize(
            &store,
            &staged,
            AdapterImportSource::HuggingFace {
                repo_id: snapshot.repo_id,
                resolved_revision: snapshot.resolved_revision,
            },
            base_model.as_ref(),
            &request.options,
        )?;

        Ok(AdapterHfPullResult {
            layout,
            store,
            runtime,
            outcome,
        })
    }
}

impl StdAdapterHfPullUseCase<'_> {
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

fn same_path(left: &Path, right: &Path) -> bool {
    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left == right
}
