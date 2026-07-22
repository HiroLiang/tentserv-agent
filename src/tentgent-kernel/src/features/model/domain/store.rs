use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelStoreLayout {
    pub models_dir: PathBuf,
    pub store_dir: PathBuf,
    pub by_source_dir: PathBuf,
    pub hf_index_dir: PathBuf,
    pub local_index_dir: PathBuf,
    pub staging_dir: PathBuf,
}

impl ModelStoreLayout {
    pub fn from_models_dir(models_dir: impl Into<PathBuf>) -> Self {
        let models_dir = models_dir.into();
        let by_source_dir = models_dir.join(BY_SOURCE_DIRNAME);

        Self {
            store_dir: models_dir.join(STORE_DIRNAME),
            hf_index_dir: by_source_dir.join(HUGGINGFACE_SOURCE_DIRNAME),
            local_index_dir: by_source_dir.join(LOCAL_SOURCE_DIRNAME),
            staging_dir: models_dir.join(STAGING_DIRNAME),
            models_dir,
            by_source_dir,
        }
    }

    pub fn model_dir(&self, model_ref: &ModelRef) -> PathBuf {
        self.store_dir.join(model_ref.as_str())
    }

    pub fn model_metadata_path(&self, model_ref: &ModelRef) -> PathBuf {
        self.model_dir(model_ref).join(MODEL_METADATA_FILENAME)
    }

    pub fn manifest_path(&self, model_ref: &ModelRef) -> PathBuf {
        self.model_dir(model_ref).join(MODEL_MANIFEST_FILENAME)
    }

    pub fn variant_dir(&self, model_ref: &ModelRef, format: ModelFormat) -> PathBuf {
        self.model_dir(model_ref)
            .join(VARIANTS_DIRNAME)
            .join(format.as_str())
    }

    pub fn variant_metadata_path(&self, model_ref: &ModelRef, format: ModelFormat) -> PathBuf {
        self.variant_dir(model_ref, format)
            .join(VARIANT_METADATA_FILENAME)
    }

    pub fn variant_source_dir(&self, model_ref: &ModelRef, format: ModelFormat) -> PathBuf {
        self.variant_dir(model_ref, format).join(SOURCE_DIRNAME)
    }

    pub fn capability_proofs_dir(&self, model_ref: &ModelRef) -> PathBuf {
        self.model_dir(model_ref).join(CAPABILITY_PROOFS_DIRNAME)
    }

    pub fn support_proofs_dir(&self, model_ref: &ModelRef) -> PathBuf {
        self.model_dir(model_ref).join(SUPPORT_PROOFS_DIRNAME)
    }

    pub fn support_proofs_capability_dir(
        &self,
        model_ref: &ModelRef,
        capability: ModelCapability,
    ) -> PathBuf {
        self.support_proofs_dir(model_ref).join(capability.as_str())
    }

    pub fn support_proof_path(&self, key: &ModelCapabilityProofKey) -> PathBuf {
        self.support_proofs_capability_dir(&key.model_ref, key.capability)
            .join(key.filename())
    }

    pub fn capability_proof_path(
        &self,
        model_ref: &ModelRef,
        capability: ModelCapability,
    ) -> PathBuf {
        self.capability_proofs_dir(model_ref)
            .join(format!("{}.toml", capability.as_str()))
    }

    pub fn local_index_path(&self, model_ref: &ModelRef) -> PathBuf {
        self.local_index_dir
            .join(format!("{}.toml", model_ref.as_str()))
    }

    pub fn hf_index_dir_for_repo(&self, repo_id: &str) -> PathBuf {
        self.hf_index_dir.join(escape_huggingface_repo_id(repo_id))
    }

    pub fn hf_index_path(&self, repo_id: &str, resolved_revision: &str) -> PathBuf {
        self.hf_index_dir_for_repo(repo_id)
            .join(format!("{resolved_revision}.toml"))
    }
}

pub fn detect_model_formats(
    manifest: &ModelManifest,
    source_repo: Option<&str>,
) -> Vec<ModelFormat> {
    let mut formats = Vec::new();

    if source_repo.is_some_and(is_mlx_huggingface_repo) {
        formats.push(ModelFormat::Mlx);
    }

    if manifest.files.iter().any(|entry| {
        Path::new(&entry.relative_path)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "model_index.json")
    }) {
        formats.push(ModelFormat::Diffusers);
    }

    let has_safetensors = manifest.files.iter().any(|entry| {
        entry.relative_path.ends_with(".safetensors")
            || Path::new(&entry.relative_path)
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == "model.safetensors.index.json")
    });
    if has_safetensors {
        formats.push(ModelFormat::Safetensors);
    }

    if manifest
        .files
        .iter()
        .any(|entry| entry.relative_path.ends_with(".gguf"))
    {
        formats.push(ModelFormat::Gguf);
    }

    formats
}

pub fn select_primary_model_format(
    detected_formats: &[ModelFormat],
    source_repo: Option<&str>,
) -> Result<ModelFormat, ModelFormatSelectionError> {
    if source_repo.is_some_and(is_mlx_huggingface_repo) {
        return Ok(ModelFormat::Mlx);
    }

    if detected_formats.contains(&ModelFormat::Diffusers) {
        return Ok(ModelFormat::Diffusers);
    }

    if detected_formats.contains(&ModelFormat::Safetensors) {
        return Ok(ModelFormat::Safetensors);
    }

    if detected_formats.contains(&ModelFormat::Gguf) {
        return Ok(ModelFormat::Gguf);
    }

    Err(ModelFormatSelectionError::UnsupportedLayout)
}

pub fn infer_mlx_runtime_family(
    primary_format: ModelFormat,
    capabilities: &[ModelCapability],
) -> Option<MlxRuntimeFamily> {
    if primary_format != ModelFormat::Mlx {
        return None;
    }

    let mut inferred = None;
    for capability in capabilities {
        let family = MlxRuntimeFamily::for_capability(*capability)?;
        match inferred {
            Some(existing) if existing != family => return None,
            Some(_) => {}
            None => inferred = Some(family),
        }
    }
    inferred
}

pub fn is_mlx_huggingface_repo(repo_id: &str) -> bool {
    repo_id.starts_with("mlx-community/")
}

pub fn escape_huggingface_repo_id(repo_id: &str) -> String {
    repo_id.replace('/', "--")
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModelFormatSelectionError {
    #[error(
        "unsupported model layout; expected Diffusers model_index.json, safetensors files, model.safetensors.index.json, gguf files, or an mlx-community Hugging Face repository"
    )]
    UnsupportedLayout,
}
