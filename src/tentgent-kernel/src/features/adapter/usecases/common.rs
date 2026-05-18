use std::path::{Path, PathBuf};

use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::features::adapter::domain::{
    backend_support_for_format, detect_adapter_format, AdapterImportOutcome, AdapterMetadata,
    AdapterSourceKind, AdapterStoreLayout, AdapterType, BaseModelAdapterIndex,
    HfAdapterSourceIndex, LocalAdapterSourceIndex, TrainRunAdapterSourceIndex,
};
use crate::features::adapter::ports::{
    AdapterBaseIndexStore, AdapterCatalogStore, AdapterContentStore, AdapterIdentityGenerator,
    AdapterManifestBuilder, AdapterSourceIndexStore, AdapterSourceMetadata,
    AdapterSourceMetadataReader, AdapterSourceStager, StagedAdapterSource,
};
use crate::features::model::domain::{ModelMetadata, ModelStoreLayout};
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayout;

pub(super) enum AdapterImportSource {
    Local {
        original_path: PathBuf,
    },
    HuggingFace {
        repo_id: String,
        resolved_revision: String,
    },
    TrainRun {
        output_path: PathBuf,
        run_ref: String,
        dataset_ref: String,
        config_ref: String,
    },
}

impl AdapterImportSource {
    fn kind(&self) -> AdapterSourceKind {
        match self {
            Self::Local { .. } => AdapterSourceKind::Local,
            Self::HuggingFace { .. } => AdapterSourceKind::HuggingFace,
            Self::TrainRun { .. } => AdapterSourceKind::TrainRun,
        }
    }

    fn repo_id(&self) -> Option<&str> {
        match self {
            Self::Local { .. } | Self::TrainRun { .. } => None,
            Self::HuggingFace { repo_id, .. } => Some(repo_id.as_str()),
        }
    }

    fn resolved_revision(&self) -> Option<&str> {
        match self {
            Self::Local { .. } | Self::TrainRun { .. } => None,
            Self::HuggingFace {
                resolved_revision, ..
            } => Some(resolved_revision.as_str()),
        }
    }

    fn original_path(&self) -> Option<&PathBuf> {
        match self {
            Self::Local { original_path } => Some(original_path),
            Self::TrainRun { output_path, .. } => Some(output_path),
            Self::HuggingFace { .. } => None,
        }
    }
}

pub(super) struct AdapterImportFinalizer<'a> {
    pub stager: &'a dyn AdapterSourceStager,
    pub manifest_builder: &'a dyn AdapterManifestBuilder,
    pub identity: &'a dyn AdapterIdentityGenerator,
    pub source_metadata_reader: &'a dyn AdapterSourceMetadataReader,
    pub catalog: &'a dyn AdapterCatalogStore,
    pub source_indexes: &'a dyn AdapterSourceIndexStore,
    pub base_indexes: &'a dyn AdapterBaseIndexStore,
    pub content: &'a dyn AdapterContentStore,
}

impl AdapterImportFinalizer<'_> {
    pub fn finalize(
        &self,
        store: &AdapterStoreLayout,
        staged: &StagedAdapterSource,
        source: AdapterImportSource,
        base_model: Option<&ModelMetadata>,
    ) -> KernelResult<AdapterImportOutcome> {
        let result = self.finalize_inner(store, staged, source, base_model);
        let cleanup = self.stager.discard_staging(staged);

        match (result, cleanup) {
            (Ok(outcome), Ok(())) => Ok(outcome),
            (Err(err), _) => Err(err),
            (Ok(_), Err(err)) => Err(err),
        }
    }

    fn finalize_inner(
        &self,
        store: &AdapterStoreLayout,
        staged: &StagedAdapterSource,
        source: AdapterImportSource,
        base_model: Option<&ModelMetadata>,
    ) -> KernelResult<AdapterImportOutcome> {
        let manifest = self.manifest_builder.build_manifest(&staged.source_dir)?;
        let source_metadata = self
            .source_metadata_reader
            .read_source_metadata(&staged.source_dir)?;
        validate_source_metadata(&source_metadata, base_model)?;

        let adapter_format = detect_adapter_format(&manifest).map_err(|err| {
            adapter_store_error(format!("adapter format detection failed: {err}"))
        })?;
        let adapter_ref = self.identity.adapter_ref_for_manifest(&manifest)?;
        let store_path = store.adapter_dir(&adapter_ref);

        if self.content.adapter_content_exists(store, &adapter_ref)? {
            let mut metadata = self.catalog.load_adapter_metadata(store, &adapter_ref)?;
            apply_base_metadata(&mut metadata, &source_metadata, base_model);
            apply_training_metadata(&mut metadata, &source);
            self.catalog.save_adapter_metadata(store, &metadata)?;
            let source_index_path = self.save_source_index(store, &metadata, &source)?;
            let base_index_path = self.save_base_index_if_needed(store, &metadata)?;
            return Ok(AdapterImportOutcome {
                metadata,
                store_path,
                source_index_path,
                base_index_path,
                deduplicated: true,
            });
        }

        self.content
            .install_staged_source(store, staged, &adapter_ref)?;

        let imported_at = imported_at_now()?;
        let mut metadata = AdapterMetadata {
            adapter_ref: adapter_ref.clone(),
            short_ref: adapter_ref.short_ref().to_string(),
            adapter_format,
            adapter_type: AdapterType::Lora,
            base_model_ref: None,
            base_model_source_repo: None,
            base_model_source_revision: None,
            model_family: None,
            backend_support: backend_support_for_format(adapter_format),
            source_kind: source.kind(),
            source_repo: source.repo_id().map(str::to_string),
            source_revision: source.resolved_revision().map(str::to_string),
            source_path: source
                .original_path()
                .map(|path| path.display().to_string()),
            training_dataset_ref: None,
            training_run_ref: None,
            training_config_ref: None,
            file_count: manifest.file_count(),
            total_bytes: manifest.total_bytes(),
            imported_at,
        };
        apply_base_metadata(&mut metadata, &source_metadata, base_model);
        apply_training_metadata(&mut metadata, &source);

        self.catalog.save_adapter_metadata(store, &metadata)?;
        self.catalog
            .save_adapter_manifest(store, &metadata.adapter_ref, &manifest)?;
        let source_index_path = self.save_source_index(store, &metadata, &source)?;
        let base_index_path = self.save_base_index_if_needed(store, &metadata)?;

        Ok(AdapterImportOutcome {
            metadata,
            store_path,
            source_index_path,
            base_index_path,
            deduplicated: false,
        })
    }

    fn save_source_index(
        &self,
        store: &AdapterStoreLayout,
        metadata: &AdapterMetadata,
        source: &AdapterImportSource,
    ) -> KernelResult<PathBuf> {
        match source {
            AdapterImportSource::Local { original_path } => {
                self.source_indexes.save_local_source_index(
                    store,
                    &LocalAdapterSourceIndex {
                        adapter_ref: metadata.adapter_ref.clone(),
                        short_ref: metadata.short_ref.clone(),
                        source_path: original_path.display().to_string(),
                        imported_at: metadata.imported_at.clone(),
                    },
                )
            }
            AdapterImportSource::HuggingFace {
                repo_id,
                resolved_revision,
            } => self.source_indexes.save_hf_source_index(
                store,
                &HfAdapterSourceIndex {
                    adapter_ref: metadata.adapter_ref.clone(),
                    short_ref: metadata.short_ref.clone(),
                    source_repo: repo_id.clone(),
                    source_revision: resolved_revision.clone(),
                    imported_at: metadata.imported_at.clone(),
                },
            ),
            AdapterImportSource::TrainRun {
                run_ref,
                dataset_ref,
                config_ref,
                ..
            } => self.source_indexes.save_train_run_source_index(
                store,
                &TrainRunAdapterSourceIndex {
                    adapter_ref: metadata.adapter_ref.clone(),
                    short_ref: metadata.short_ref.clone(),
                    training_run_ref: run_ref.clone(),
                    training_dataset_ref: dataset_ref.clone(),
                    training_config_ref: config_ref.clone(),
                    imported_at: metadata.imported_at.clone(),
                },
            ),
        }
    }

    pub fn save_base_index_if_needed(
        &self,
        store: &AdapterStoreLayout,
        metadata: &AdapterMetadata,
    ) -> KernelResult<Option<PathBuf>> {
        let Some(base_model_ref) = &metadata.base_model_ref else {
            return Ok(None);
        };

        Ok(Some(self.base_indexes.save_base_model_index(
            store,
            &BaseModelAdapterIndex {
                adapter_ref: metadata.adapter_ref.clone(),
                short_ref: metadata.short_ref.clone(),
                base_model_ref: base_model_ref.clone(),
                adapter_format: metadata.adapter_format,
                imported_at: metadata.imported_at.clone(),
            },
        )?))
    }
}

pub(super) fn adapter_store_layout(layout: &RuntimeLayout) -> AdapterStoreLayout {
    AdapterStoreLayout::from_adapters_dir(layout.adapters_dir.clone())
}

pub(super) fn model_store_layout(layout: &RuntimeLayout) -> ModelStoreLayout {
    ModelStoreLayout::from_models_dir(layout.models_dir.clone())
}

pub(super) fn validate_source_metadata(
    source_metadata: &AdapterSourceMetadata,
    base_model: Option<&ModelMetadata>,
) -> KernelResult<()> {
    let Some(base_model) = base_model else {
        return Ok(());
    };

    if let (Some(adapter_base), Some(model_base)) = (
        source_metadata.base_model_source_repo.as_deref(),
        base_model.source_repo.as_deref(),
    ) {
        if !is_local_path_hint(adapter_base) && adapter_base != model_base {
            return Err(adapter_store_error(format!(
                "adapter base model `{adapter_base}` does not match local model `{model_base}`"
            )));
        }
    }

    if let (Some(adapter_revision), Some(model_revision)) = (
        source_metadata.base_model_source_revision.as_deref(),
        base_model.source_revision.as_deref(),
    ) {
        if adapter_revision != model_revision {
            return Err(adapter_store_error(format!(
                "adapter base revision `{adapter_revision}` does not match local model revision `{model_revision}`"
            )));
        }
    }

    Ok(())
}

pub(super) fn apply_base_metadata(
    metadata: &mut AdapterMetadata,
    source_metadata: &AdapterSourceMetadata,
    base_model: Option<&ModelMetadata>,
) {
    if let Some(base_model) = base_model {
        metadata.base_model_ref = Some(base_model.model_ref.clone());
        metadata.base_model_source_repo = base_model.source_repo.clone();
        metadata.base_model_source_revision = base_model.source_revision.clone();
    }

    if let Some(repo) = source_metadata.base_model_source_repo.as_deref() {
        if !is_local_path_hint(repo) {
            metadata.base_model_source_repo = Some(repo.to_string());
        }
    }
    if let Some(revision) = source_metadata.base_model_source_revision.as_deref() {
        metadata.base_model_source_revision = Some(revision.to_string());
    }
    if let Some(model_family) = source_metadata.model_family.as_deref() {
        metadata.model_family = Some(model_family.to_string());
    }
}

pub(super) fn base_index_for_metadata(
    metadata: &AdapterMetadata,
    base_model_ref: crate::features::model::domain::ModelRef,
) -> BaseModelAdapterIndex {
    BaseModelAdapterIndex {
        adapter_ref: metadata.adapter_ref.clone(),
        short_ref: metadata.short_ref.clone(),
        base_model_ref,
        adapter_format: metadata.adapter_format,
        imported_at: metadata.imported_at.clone(),
    }
}

fn apply_training_metadata(metadata: &mut AdapterMetadata, source: &AdapterImportSource) {
    if let AdapterImportSource::TrainRun {
        run_ref,
        dataset_ref,
        config_ref,
        ..
    } = source
    {
        metadata.training_run_ref = Some(run_ref.clone());
        metadata.training_dataset_ref = Some(dataset_ref.clone());
        metadata.training_config_ref = Some(config_ref.clone());
    }
}

fn imported_at_now() -> KernelResult<String> {
    OffsetDateTime::now_utc().format(&Rfc3339).map_err(|err| {
        KernelError::AdapterStoreUnavailable(format!("format import time failed: {err}"))
    })
}

fn is_local_path_hint(value: &str) -> bool {
    let trimmed = value.trim();
    Path::new(trimmed).is_absolute() || trimmed.starts_with("./") || trimmed.starts_with("../")
}

pub(super) fn adapter_store_error(message: impl Into<String>) -> KernelError {
    KernelError::AdapterStoreUnavailable(message.into())
}
