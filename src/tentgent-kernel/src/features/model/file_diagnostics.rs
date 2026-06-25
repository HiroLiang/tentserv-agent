//! Stored model file diagnostics.

use std::fs;
use std::path::{Path, PathBuf};

use super::domain::{
    ModelCapability, ModelFileDiagnostic, ModelFileDiagnosticCode, ModelFileDiagnosticSeverity,
    ModelFormat, ModelMetadata, ModelStoreLayout, ModelVariantMetadata,
};

pub fn model_file_diagnostics(
    layout: &ModelStoreLayout,
    metadata: &ModelMetadata,
) -> Vec<ModelFileDiagnostic> {
    let mut diagnostics = Vec::new();

    let manifest_path = layout.manifest_path(&metadata.model_ref);
    if !manifest_path.is_file() {
        diagnostics.push(diagnostic(
            ModelFileDiagnosticSeverity::Warning,
            ModelFileDiagnosticCode::MissingManifest,
            manifest_path,
            "model manifest is missing; integrity details may be stale or unavailable",
            reinstall_action(metadata),
        ));
    }

    let variant_metadata_path =
        layout.variant_metadata_path(&metadata.model_ref, metadata.primary_format);
    let source_path = match read_variant_source_path(layout, metadata, &variant_metadata_path) {
        Ok(source_path) => source_path,
        Err(diagnostic) => {
            diagnostics.push(diagnostic);
            layout.variant_source_dir(&metadata.model_ref, metadata.primary_format)
        }
    };

    if !source_path.exists() {
        diagnostics.push(diagnostic(
            ModelFileDiagnosticSeverity::Blocking,
            ModelFileDiagnosticCode::MissingSourcePath,
            source_path,
            "model source path is missing; the runtime cannot load this model",
            reinstall_action(metadata),
        ));
        return diagnostics;
    }

    if source_path.is_dir() && directory_is_empty(&source_path) {
        diagnostics.push(diagnostic(
            ModelFileDiagnosticSeverity::Blocking,
            ModelFileDiagnosticCode::EmptySourceDirectory,
            source_path.clone(),
            "model source directory is empty; the runtime cannot load this model",
            reinstall_action(metadata),
        ));
        return diagnostics;
    }

    match metadata.primary_format {
        ModelFormat::Gguf => check_gguf_source(metadata, &source_path, &mut diagnostics),
        ModelFormat::Safetensors | ModelFormat::Mlx => {
            check_config_source(metadata, &source_path, &mut diagnostics);
            if requires_tokenizer_assets(&metadata.model_capabilities) {
                check_tokenizer_assets(metadata, &source_path, &mut diagnostics);
            }
            if requires_generation_metadata(&metadata.model_capabilities) {
                check_generation_metadata(&source_path, &mut diagnostics);
            }
            if requires_processor_assets(&metadata.model_capabilities) {
                check_processor_assets(metadata, &source_path, &mut diagnostics);
            }
        }
        ModelFormat::Diffusers => check_diffusers_source(metadata, &source_path, &mut diagnostics),
    }

    diagnostics
}

pub fn model_file_diagnostics_block_execution(diagnostics: &[ModelFileDiagnostic]) -> bool {
    diagnostics
        .iter()
        .any(ModelFileDiagnostic::blocks_execution)
}

pub fn model_file_diagnostics_summary(diagnostics: &[ModelFileDiagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| {
            format!(
                "{} {} at `{}`: {}; next action: {}",
                diagnostic.severity,
                diagnostic.code,
                diagnostic.path.display(),
                diagnostic.message,
                diagnostic.next_action
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn read_variant_source_path(
    layout: &ModelStoreLayout,
    metadata: &ModelMetadata,
    path: &Path,
) -> Result<PathBuf, ModelFileDiagnostic> {
    if !path.is_file() {
        return Err(diagnostic(
            ModelFileDiagnosticSeverity::Blocking,
            ModelFileDiagnosticCode::MissingVariantMetadata,
            path.to_path_buf(),
            "model variant metadata is missing; the runtime cannot resolve the stored source path",
            reinstall_action(metadata),
        ));
    }

    let body = fs::read_to_string(path).map_err(|err| {
        diagnostic(
            ModelFileDiagnosticSeverity::Blocking,
            ModelFileDiagnosticCode::InvalidVariantMetadata,
            path.to_path_buf(),
            format!("model variant metadata cannot be read: {err}"),
            reinstall_action(metadata),
        )
    })?;
    let variant: ModelVariantMetadata = toml::from_str(&body).map_err(|err| {
        diagnostic(
            ModelFileDiagnosticSeverity::Blocking,
            ModelFileDiagnosticCode::InvalidVariantMetadata,
            path.to_path_buf(),
            format!("model variant metadata cannot be parsed: {err}"),
            reinstall_action(metadata),
        )
    })?;
    Ok(layout
        .variant_dir(&metadata.model_ref, metadata.primary_format)
        .join(variant.relative_source_path))
}

fn check_gguf_source(
    metadata: &ModelMetadata,
    source_path: &Path,
    diagnostics: &mut Vec<ModelFileDiagnostic>,
) {
    if source_path.is_file() {
        if source_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("gguf"))
        {
            return;
        }
        diagnostics.push(diagnostic(
            ModelFileDiagnosticSeverity::Blocking,
            ModelFileDiagnosticCode::MissingGgufFile,
            source_path.to_path_buf(),
            "GGUF model source is not a .gguf file",
            reinstall_action(metadata),
        ));
        return;
    }

    let matches = direct_child_files_with_extension(source_path, "gguf");
    match matches.len() {
        0 => diagnostics.push(diagnostic(
            ModelFileDiagnosticSeverity::Blocking,
            ModelFileDiagnosticCode::MissingGgufFile,
            source_path.to_path_buf(),
            "GGUF model source does not contain a .gguf file",
            reinstall_action(metadata),
        )),
        1 => {}
        _ => diagnostics.push(diagnostic(
            ModelFileDiagnosticSeverity::Blocking,
            ModelFileDiagnosticCode::MultipleGgufFiles,
            source_path.to_path_buf(),
            "GGUF model source contains multiple .gguf files and the runtime expects exactly one",
            reinstall_action(metadata),
        )),
    }
}

fn check_config_source(
    metadata: &ModelMetadata,
    source_path: &Path,
    diagnostics: &mut Vec<ModelFileDiagnostic>,
) {
    if !source_path.join("config.json").is_file() {
        diagnostics.push(diagnostic(
            ModelFileDiagnosticSeverity::Blocking,
            ModelFileDiagnosticCode::MissingModelConfig,
            source_path.join("config.json"),
            "model config.json is missing; the runtime cannot load this model",
            reinstall_action(metadata),
        ));
    }
}

fn check_tokenizer_assets(
    metadata: &ModelMetadata,
    source_path: &Path,
    diagnostics: &mut Vec<ModelFileDiagnostic>,
) {
    if any_child_file(
        source_path,
        &[
            "tokenizer.json",
            "tokenizer.model",
            "spiece.model",
            "vocab.json",
            "vocab.txt",
            "merges.txt",
        ],
    ) {
        return;
    }

    diagnostics.push(diagnostic(
        ModelFileDiagnosticSeverity::Blocking,
        ModelFileDiagnosticCode::MissingTokenizerAssets,
        source_path.to_path_buf(),
        "tokenizer assets are missing; text model workflows cannot tokenize requests",
        reinstall_action(metadata),
    ));
}

fn check_generation_metadata(source_path: &Path, diagnostics: &mut Vec<ModelFileDiagnostic>) {
    if source_path.join("generation_config.json").is_file() {
        return;
    }

    diagnostics.push(diagnostic(
        ModelFileDiagnosticSeverity::Warning,
        ModelFileDiagnosticCode::MissingGenerationConfig,
        source_path.join("generation_config.json"),
        "generation_config.json is missing; generation may rely on backend defaults",
        "re-pull or re-import the model if generation settings behave unexpectedly",
    ));
}

fn check_processor_assets(
    metadata: &ModelMetadata,
    source_path: &Path,
    diagnostics: &mut Vec<ModelFileDiagnostic>,
) {
    if any_child_file(
        source_path,
        &[
            "processor_config.json",
            "preprocessor_config.json",
            "feature_extractor_config.json",
            "image_processor_config.json",
        ],
    ) {
        return;
    }

    diagnostics.push(diagnostic(
        ModelFileDiagnosticSeverity::Blocking,
        ModelFileDiagnosticCode::MissingProcessorAssets,
        source_path.to_path_buf(),
        "processor or preprocessing metadata is missing; media model workflows cannot prepare inputs",
        reinstall_action(metadata),
    ));
}

fn check_diffusers_source(
    metadata: &ModelMetadata,
    source_path: &Path,
    diagnostics: &mut Vec<ModelFileDiagnostic>,
) {
    if source_path.join("model_index.json").is_file() {
        return;
    }

    diagnostics.push(diagnostic(
        ModelFileDiagnosticSeverity::Blocking,
        ModelFileDiagnosticCode::MissingDiffusersIndex,
        source_path.join("model_index.json"),
        "Diffusers model_index.json is missing; image generation workflows cannot load this model",
        reinstall_action(metadata),
    ));
}

fn requires_tokenizer_assets(capabilities: &[ModelCapability]) -> bool {
    capabilities.iter().any(|capability| {
        matches!(
            capability,
            ModelCapability::Chat | ModelCapability::Embedding | ModelCapability::Rerank
        )
    })
}

fn requires_generation_metadata(capabilities: &[ModelCapability]) -> bool {
    capabilities
        .iter()
        .any(|capability| matches!(capability, ModelCapability::Chat))
}

fn requires_processor_assets(capabilities: &[ModelCapability]) -> bool {
    capabilities.iter().any(|capability| {
        matches!(
            capability,
            ModelCapability::AudioTranscription
                | ModelCapability::AudioSpeech
                | ModelCapability::VisionChat
                | ModelCapability::VideoUnderstanding
        )
    })
}

fn directory_is_empty(path: &Path) -> bool {
    path.read_dir()
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false)
}

fn direct_child_files_with_extension(path: &Path, extension: &str) -> Vec<PathBuf> {
    let Ok(entries) = path.read_dir() else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|actual| actual.to_str())
                    .is_some_and(|actual| actual.eq_ignore_ascii_case(extension))
        })
        .collect()
}

fn any_child_file(path: &Path, names: &[&str]) -> bool {
    names.iter().any(|name| path.join(name).is_file())
}

fn reinstall_action(metadata: &ModelMetadata) -> String {
    format!(
        "remove the corrupted model with `tentgent model rm {}`, then pull or import it again",
        metadata.short_ref
    )
}

fn diagnostic(
    severity: ModelFileDiagnosticSeverity,
    code: ModelFileDiagnosticCode,
    path: PathBuf,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> ModelFileDiagnostic {
    ModelFileDiagnostic {
        severity,
        code,
        path,
        message: message.into(),
        next_action: next_action.into(),
    }
}
