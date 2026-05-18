//! Hugging Face model pull use case.

use std::path::Path;

use crate::features::auth::domain::Provider;
use crate::features::auth::usecases::AuthSecretResolverUseCase;
use crate::features::model::domain::{HfModelPullProgress, ModelImportMethod};
use crate::features::model::ports::{
    HfModelSnapshotFetcher, HfModelSnapshotRequest, ModelCatalogStore, ModelContentStore,
    ModelIdentityGenerator, ModelManifestBuilder, ModelSourceIndexStore, ModelSourceStager,
    ModelStoreLayoutInitializer,
};
use crate::features::runtime::ports::PythonRuntimeResolver;
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::{model_store_layout, ModelImportFinalizer, ModelImportSource};
use super::port::{ModelHfPullRequest, ModelHfPullResult, ModelHfPullUseCase};

/// Standard Hugging Face model pull orchestration.
pub struct StdModelHfPullUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    runtime_resolver: &'a dyn PythonRuntimeResolver,
    auth_resolver: &'a dyn AuthSecretResolverUseCase,
    layout_initializer: &'a dyn ModelStoreLayoutInitializer,
    stager: &'a dyn ModelSourceStager,
    snapshot_fetcher: &'a dyn HfModelSnapshotFetcher,
    manifest_builder: &'a dyn ModelManifestBuilder,
    identity: &'a dyn ModelIdentityGenerator,
    catalog: &'a dyn ModelCatalogStore,
    source_indexes: &'a dyn ModelSourceIndexStore,
    content: &'a dyn ModelContentStore,
}

impl<'a> StdModelHfPullUseCase<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        runtime_resolver: &'a dyn PythonRuntimeResolver,
        auth_resolver: &'a dyn AuthSecretResolverUseCase,
        layout_initializer: &'a dyn ModelStoreLayoutInitializer,
        stager: &'a dyn ModelSourceStager,
        snapshot_fetcher: &'a dyn HfModelSnapshotFetcher,
        manifest_builder: &'a dyn ModelManifestBuilder,
        identity: &'a dyn ModelIdentityGenerator,
        catalog: &'a dyn ModelCatalogStore,
        source_indexes: &'a dyn ModelSourceIndexStore,
        content: &'a dyn ModelContentStore,
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
            catalog,
            source_indexes,
            content,
        }
    }
}

impl ModelHfPullUseCase for StdModelHfPullUseCase<'_> {
    fn pull_hf_model(
        &self,
        request: ModelHfPullRequest,
        progress: &mut dyn FnMut(HfModelPullProgress),
    ) -> KernelResult<ModelHfPullResult> {
        if request.auth.provider != Provider::HuggingFace {
            return Err(KernelError::ModelStoreUnavailable(format!(
                "Hugging Face model pull requires Hugging Face auth resolution, got {:?}",
                request.auth.provider
            )));
        }

        let layout = self.layout_resolver.resolve(request.layout)?;
        let runtime = self
            .runtime_resolver
            .resolve_python_runtime(&layout, request.runtime)?;
        let auth = self.auth_resolver.resolve_secret(request.auth)?;
        let store = model_store_layout(&layout);
        self.layout_initializer.ensure_model_store_layout(&store)?;

        let staged = self
            .stager
            .create_staging_source(&store, ModelImportMethod::Pull)?;
        let snapshot = match self.snapshot_fetcher.fetch_hf_snapshot(
            HfModelSnapshotRequest {
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
            return Err(KernelError::ModelStoreUnavailable(format!(
                "Hugging Face snapshot helper wrote to `{}` instead of requested staging source `{}`",
                snapshot.local_dir.display(),
                staged.source_dir.display()
            )));
        }

        let outcome = self.finalizer().finalize(
            &store,
            &staged,
            ModelImportSource::HuggingFace {
                repo_id: snapshot.repo_id,
                resolved_revision: snapshot.resolved_revision,
            },
            ModelImportMethod::Pull,
            request.capability,
        )?;

        Ok(ModelHfPullResult {
            layout,
            store,
            runtime,
            outcome,
        })
    }
}

impl StdModelHfPullUseCase<'_> {
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

fn same_path(left: &Path, right: &Path) -> bool {
    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left == right
}
