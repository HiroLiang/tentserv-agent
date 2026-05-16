use std::path::PathBuf;

use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::features::model::domain::{
    detect_model_formats, select_primary_model_format, HfModelSourceIndex, LocalModelSourceIndex,
    ModelImportMethod, ModelImportOutcome, ModelMetadata, ModelSourceKind, ModelStoreLayout,
    ModelVariantMetadata, ModelVariantStatus, SOURCE_DIRNAME,
};
use crate::features::model::ports::{
    ModelCatalogStore, ModelContentStore, ModelIdentityGenerator, ModelManifestBuilder,
    ModelSourceIndexStore, ModelSourceStager, StagedModelSource,
};
use crate::foundation::error::{KernelError, KernelResult};

pub(super) enum ModelImportSource {
    Local {
        original_path: PathBuf,
    },
    HuggingFace {
        repo_id: String,
        resolved_revision: String,
    },
}

impl ModelImportSource {
    fn kind(&self) -> ModelSourceKind {
        match self {
            Self::Local { .. } => ModelSourceKind::Local,
            Self::HuggingFace { .. } => ModelSourceKind::HuggingFace,
        }
    }

    fn repo_id(&self) -> Option<&str> {
        match self {
            Self::Local { .. } => None,
            Self::HuggingFace { repo_id, .. } => Some(repo_id.as_str()),
        }
    }

    fn resolved_revision(&self) -> Option<&str> {
        match self {
            Self::Local { .. } => None,
            Self::HuggingFace {
                resolved_revision, ..
            } => Some(resolved_revision.as_str()),
        }
    }

    fn original_path(&self) -> Option<&PathBuf> {
        match self {
            Self::Local { original_path } => Some(original_path),
            Self::HuggingFace { .. } => None,
        }
    }
}

pub(super) struct ModelImportFinalizer<'a> {
    pub stager: &'a dyn ModelSourceStager,
    pub manifest_builder: &'a dyn ModelManifestBuilder,
    pub identity: &'a dyn ModelIdentityGenerator,
    pub catalog: &'a dyn ModelCatalogStore,
    pub source_indexes: &'a dyn ModelSourceIndexStore,
    pub content: &'a dyn ModelContentStore,
}

impl ModelImportFinalizer<'_> {
    pub fn finalize(
        &self,
        store: &ModelStoreLayout,
        staged: &StagedModelSource,
        source: ModelImportSource,
        method: ModelImportMethod,
    ) -> KernelResult<ModelImportOutcome> {
        let result = self.finalize_inner(store, staged, source, method);
        let cleanup = self.stager.discard_staging(staged);

        match (result, cleanup) {
            (Ok(outcome), Ok(())) => Ok(outcome),
            (Err(err), _) => Err(err),
            (Ok(_), Err(err)) => Err(err),
        }
    }

    fn finalize_inner(
        &self,
        store: &ModelStoreLayout,
        staged: &StagedModelSource,
        source: ModelImportSource,
        method: ModelImportMethod,
    ) -> KernelResult<ModelImportOutcome> {
        let manifest = self.manifest_builder.build_manifest(&staged.source_dir)?;
        let model_ref = self.identity.model_ref_for_manifest(&manifest)?;
        let detected_formats = detect_model_formats(&manifest, source.repo_id());
        let primary_format = select_primary_model_format(&detected_formats, source.repo_id())
            .map_err(|err| {
                KernelError::ModelStoreUnavailable(format!("model format selection failed: {err}"))
            })?;
        let store_path = store.model_dir(&model_ref);

        if self.content.model_content_exists(store, &model_ref)? {
            let metadata = self.catalog.load_model_metadata(store, &model_ref)?;
            let source_index_path = self.save_source_index(store, &metadata, &source)?;
            return Ok(ModelImportOutcome {
                metadata,
                store_path,
                source_index_path,
                deduplicated: true,
            });
        }

        self.content
            .install_staged_source(store, staged, &model_ref, primary_format)?;

        let imported_at = imported_at_now()?;
        let metadata = ModelMetadata {
            model_ref: model_ref.clone(),
            short_ref: model_ref.short_ref().to_string(),
            source_kind: source.kind(),
            source_repo: source.repo_id().map(str::to_string),
            source_revision: source.resolved_revision().map(str::to_string),
            source_path: source
                .original_path()
                .map(|path| path.display().to_string()),
            primary_format,
            detected_formats,
            file_count: manifest.file_count(),
            total_bytes: manifest.total_bytes(),
            imported_at,
        };
        let variant = ModelVariantMetadata {
            format: primary_format,
            status: ModelVariantStatus::Imported,
            import_method: method,
            relative_source_path: SOURCE_DIRNAME.to_string(),
        };

        self.catalog.save_model_metadata(store, &metadata)?;
        self.catalog
            .save_model_manifest(store, &metadata.model_ref, &manifest)?;
        self.catalog
            .save_variant_metadata(store, &metadata.model_ref, &variant)?;
        let source_index_path = self.save_source_index(store, &metadata, &source)?;

        Ok(ModelImportOutcome {
            metadata,
            store_path,
            source_index_path,
            deduplicated: false,
        })
    }

    fn save_source_index(
        &self,
        store: &ModelStoreLayout,
        metadata: &ModelMetadata,
        source: &ModelImportSource,
    ) -> KernelResult<PathBuf> {
        match source {
            ModelImportSource::Local { original_path } => {
                self.source_indexes.save_local_source_index(
                    store,
                    &LocalModelSourceIndex {
                        model_ref: metadata.model_ref.clone(),
                        short_ref: metadata.short_ref.clone(),
                        source_path: original_path.display().to_string(),
                        imported_at: metadata.imported_at.clone(),
                    },
                )
            }
            ModelImportSource::HuggingFace {
                repo_id,
                resolved_revision,
            } => self.source_indexes.save_hf_source_index(
                store,
                &HfModelSourceIndex {
                    model_ref: metadata.model_ref.clone(),
                    short_ref: metadata.short_ref.clone(),
                    source_repo: repo_id.clone(),
                    source_revision: resolved_revision.clone(),
                    imported_at: metadata.imported_at.clone(),
                },
            ),
        }
    }
}

pub(super) fn model_store_layout(
    layout: &crate::foundation::layout::RuntimeLayout,
) -> ModelStoreLayout {
    ModelStoreLayout::from_models_dir(layout.models_dir.clone())
}

fn imported_at_now() -> KernelResult<String> {
    OffsetDateTime::now_utc().format(&Rfc3339).map_err(|err| {
        KernelError::ModelStoreUnavailable(format!("format import time failed: {err}"))
    })
}
