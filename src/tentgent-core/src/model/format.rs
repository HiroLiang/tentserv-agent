use std::fmt;

use serde::{Deserialize, Serialize};

use super::{error::ModelError, manifest::ManifestDocument};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelFormat {
    Safetensors,
    Gguf,
    Mlx,
}

impl ModelFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Safetensors => "safetensors",
            Self::Gguf => "gguf",
            Self::Mlx => "mlx",
        }
    }
}

impl fmt::Display for ModelFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

pub fn detect_formats(manifest: &ManifestDocument, source_repo: Option<&str>) -> Vec<ModelFormat> {
    let mut formats = Vec::new();

    if source_repo.is_some_and(is_mlx_repo) {
        formats.push(ModelFormat::Mlx);
    }

    let has_safetensors = manifest.files.iter().any(|entry| {
        entry.relative_path.ends_with(".safetensors")
            || entry
                .relative_path
                .rsplit('/')
                .next()
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

pub fn select_primary_format(
    detected_formats: &[ModelFormat],
    source_repo: Option<&str>,
) -> Result<ModelFormat, ModelError> {
    if source_repo.is_some_and(is_mlx_repo) {
        return Ok(ModelFormat::Mlx);
    }

    if detected_formats.contains(&ModelFormat::Safetensors) {
        return Ok(ModelFormat::Safetensors);
    }

    if detected_formats.contains(&ModelFormat::Gguf) {
        return Ok(ModelFormat::Gguf);
    }

    Err(ModelError::UnsupportedLayout {
        reason: "Tentgent could not detect a supported primary format. Supported MVP layouts must contain .safetensors files, model.safetensors.index.json, .gguf files, or come from mlx-community/* on Hugging Face.".to_string(),
    })
}

pub fn is_mlx_repo(repo: &str) -> bool {
    repo.starts_with("mlx-community/")
}
