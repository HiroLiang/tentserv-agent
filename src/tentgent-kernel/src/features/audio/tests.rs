#[cfg(any())]
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::features::audio::domain::{
    AudioSpeechBackend, AudioSpeechOutputFormat, AudioSpeechRuntimeTarget,
    AudioTranscriptionBackend, AudioTranscriptionOutputFormat, AudioTranscriptionRuntimeTarget,
};
#[cfg(any())]
use crate::features::audio::domain::{
    AudioSpeechRequest, AudioSpeechResponse, AudioTranscriptionRequest, AudioTranscriptionResponse,
    ResolvedAudioSpeechTarget, ResolvedAudioTranscriptionTarget,
};
use crate::features::audio::infra::{
    StdAudioSpeechModelResolver, StdAudioTranscriptionModelResolver,
};
use crate::features::audio::ports::{
    AudioSpeechModelResolveRequest, AudioSpeechModelResolver,
    AudioTranscriptionModelResolveRequest, AudioTranscriptionModelResolver,
};
#[cfg(any())]
use crate::features::audio::ports::{
    AudioSpeechRuntimeClient, AudioSpeechRuntimeRequest, AudioTranscriptionRuntimeClient,
    AudioTranscriptionRuntimeRequest,
};
use crate::features::model::domain::{
    default_model_capability_source, MlxRuntimeFamily, ModelCapability, ModelFormat,
    ModelInspection, ModelMetadata, ModelRef, ModelRefSelector, ModelSourceKind, ModelStoreLayout,
};
use crate::features::model::usecases::{
    ModelCatalogReadUseCase, ModelInspectRequest, ModelInspectResult, ModelListRequest,
    ModelListResult,
};
#[cfg(any())]
use crate::features::runtime::domain::{
    PythonRuntimeLayout, PythonRuntimeSource, RuntimeEntrypoint,
};
#[cfg(any())]
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::{LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput};

#[test]
fn audio_transcription_output_format_parses_aliases_and_metadata() {
    let format = "txt"
        .parse::<AudioTranscriptionOutputFormat>()
        .expect("txt alias");

    assert_eq!(format, AudioTranscriptionOutputFormat::Text);
    assert_eq!(format.default_filename(), "transcript.txt");
    assert_eq!(
        AudioTranscriptionOutputFormat::Json.media_type(),
        "application/json"
    );
    assert!("unknown"
        .parse::<AudioTranscriptionOutputFormat>()
        .expect_err("unsupported")
        .to_string()
        .contains("unsupported audio transcription output format"));
}

#[test]
fn audio_speech_output_format_parses_aliases_and_metadata() {
    let format = "wave"
        .parse::<AudioSpeechOutputFormat>()
        .expect("wave alias");

    assert_eq!(format, AudioSpeechOutputFormat::Wav);
    assert_eq!(format.default_filename(), "speech.wav");
    assert_eq!(AudioSpeechOutputFormat::Wav.media_type(), "audio/wav");
    assert!("mp3"
        .parse::<AudioSpeechOutputFormat>()
        .expect_err("unsupported")
        .to_string()
        .contains("unsupported audio speech output format"));
}

#[test]
fn std_audio_transcription_model_resolver_accepts_safetensors_model() {
    let catalog = FakeModelCatalog {
        metadata: model_metadata(
            ModelFormat::Safetensors,
            vec![ModelCapability::AudioTranscription],
        ),
    };
    let resolver = StdAudioTranscriptionModelResolver::new(&catalog);

    let result = resolver
        .resolve_audio_transcription_model(AudioTranscriptionModelResolveRequest {
            layout: layout_input(unique_path("audio-model-home")),
            selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
        })
        .expect("resolve audio transcription model");

    assert_eq!(result.model.metadata.model_ref, model_ref());
    assert_eq!(
        result.target,
        AudioTranscriptionRuntimeTarget::LocalModel {
            model_ref: model_ref(),
            backend: AudioTranscriptionBackend::TransformersAutomaticSpeechRecognition,
            source_repo: Some("org/model".to_string()),
            source_revision: Some("main".to_string()),
            model_capabilities: vec![ModelCapability::AudioTranscription],
        }
    );
}

#[test]
fn std_audio_speech_model_resolver_accepts_safetensors_model() {
    let catalog = FakeModelCatalog {
        metadata: model_metadata(ModelFormat::Safetensors, vec![ModelCapability::AudioSpeech]),
    };
    let resolver = StdAudioSpeechModelResolver::new(&catalog);

    let result = resolver
        .resolve_audio_speech_model(AudioSpeechModelResolveRequest {
            layout: layout_input(unique_path("audio-speech-model-home")),
            selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
        })
        .expect("resolve audio speech model");

    assert_eq!(result.model.metadata.model_ref, model_ref());
    assert_eq!(
        result.target,
        AudioSpeechRuntimeTarget::LocalModel {
            model_ref: model_ref(),
            backend: AudioSpeechBackend::TransformersTextToSpeech,
            source_repo: Some("org/model".to_string()),
            source_revision: Some("main".to_string()),
            model_capabilities: vec![ModelCapability::AudioSpeech],
        }
    );
}

#[test]
fn audio_transcription_backend_maps_mlx_audio_family_only() {
    assert_eq!(
        AudioTranscriptionBackend::from_model_format_and_mlx_family(
            ModelFormat::Mlx,
            Some(MlxRuntimeFamily::Audio)
        ),
        Some(AudioTranscriptionBackend::MlxAudio)
    );
    assert_eq!(
        AudioTranscriptionBackend::from_model_format_and_mlx_family(ModelFormat::Mlx, None),
        None
    );
    for family in [
        MlxRuntimeFamily::Lm,
        MlxRuntimeFamily::Vlm,
        MlxRuntimeFamily::Diffusion,
    ] {
        assert_eq!(
            AudioTranscriptionBackend::from_model_format_and_mlx_family(
                ModelFormat::Mlx,
                Some(family)
            ),
            None
        );
    }
}

#[test]
fn std_audio_transcription_model_resolver_accepts_mlx_audio_model() {
    let catalog = FakeModelCatalog {
        metadata: mlx_model_metadata(
            Some(MlxRuntimeFamily::Audio),
            vec![ModelCapability::AudioTranscription],
        ),
    };
    let resolver = StdAudioTranscriptionModelResolver::new(&catalog);

    let result = resolver
        .resolve_audio_transcription_model(AudioTranscriptionModelResolveRequest {
            layout: layout_input(unique_path("audio-mlx-audio-home")),
            selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
        })
        .expect("resolve mlx audio transcription model");

    assert_eq!(
        result.target,
        AudioTranscriptionRuntimeTarget::LocalModel {
            model_ref: model_ref(),
            backend: AudioTranscriptionBackend::MlxAudio,
            source_repo: Some("org/model".to_string()),
            source_revision: Some("main".to_string()),
            model_capabilities: vec![ModelCapability::AudioTranscription],
        }
    );
}

#[test]
fn std_audio_speech_model_resolver_accepts_mlx_audio_model() {
    let catalog = FakeModelCatalog {
        metadata: mlx_model_metadata(
            Some(MlxRuntimeFamily::Audio),
            vec![ModelCapability::AudioSpeech],
        ),
    };
    let resolver = StdAudioSpeechModelResolver::new(&catalog);

    let result = resolver
        .resolve_audio_speech_model(AudioSpeechModelResolveRequest {
            layout: layout_input(unique_path("audio-speech-mlx-audio-home")),
            selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
        })
        .expect("resolve mlx audio speech model");

    assert_eq!(
        result.target,
        AudioSpeechRuntimeTarget::LocalModel {
            model_ref: model_ref(),
            backend: AudioSpeechBackend::MlxAudio,
            source_repo: Some("org/model".to_string()),
            source_revision: Some("main".to_string()),
            model_capabilities: vec![ModelCapability::AudioSpeech],
        }
    );
}

#[test]
fn std_audio_transcription_model_resolver_rejects_non_audio_models() {
    for capability in [
        ModelCapability::Chat,
        ModelCapability::Embedding,
        ModelCapability::Rerank,
        ModelCapability::AudioSpeech,
        ModelCapability::ImageGeneration,
    ] {
        let catalog = FakeModelCatalog {
            metadata: model_metadata(ModelFormat::Safetensors, vec![capability]),
        };
        let resolver = StdAudioTranscriptionModelResolver::new(&catalog);

        let err = resolver
            .resolve_audio_transcription_model(AudioTranscriptionModelResolveRequest {
                layout: layout_input(unique_path("audio-non-audio-home")),
                selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
            })
            .expect_err("non-audio model");

        assert!(matches!(err, KernelError::UnsupportedTarget(_)));
        assert!(err
            .to_string()
            .contains("requires model capability `audio-transcription`"));
    }
}

#[test]
fn std_audio_speech_model_resolver_rejects_non_speech_models() {
    for capability in [
        ModelCapability::Chat,
        ModelCapability::Embedding,
        ModelCapability::Rerank,
        ModelCapability::AudioTranscription,
        ModelCapability::ImageGeneration,
    ] {
        let catalog = FakeModelCatalog {
            metadata: model_metadata(ModelFormat::Safetensors, vec![capability]),
        };
        let resolver = StdAudioSpeechModelResolver::new(&catalog);

        let err = resolver
            .resolve_audio_speech_model(AudioSpeechModelResolveRequest {
                layout: layout_input(unique_path("audio-non-speech-home")),
                selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
            })
            .expect_err("non-speech model");

        assert!(matches!(err, KernelError::UnsupportedTarget(_)));
        assert!(err
            .to_string()
            .contains("requires model capability `audio-speech`"));
    }
}

#[test]
fn std_audio_transcription_model_resolver_rejects_unsupported_format() {
    let catalog = FakeModelCatalog {
        metadata: model_metadata(ModelFormat::Gguf, vec![ModelCapability::AudioTranscription]),
    };
    let resolver = StdAudioTranscriptionModelResolver::new(&catalog);

    let err = resolver
        .resolve_audio_transcription_model(AudioTranscriptionModelResolveRequest {
            layout: layout_input(unique_path("audio-gguf-home")),
            selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
        })
        .expect_err("unsupported backend");

    assert!(matches!(err, KernelError::UnsupportedTarget(_)));
    assert!(err.to_string().contains("does not support `gguf`"));
}

#[test]
fn std_audio_transcription_model_resolver_rejects_non_audio_mlx_families() {
    for family in [
        None,
        Some(MlxRuntimeFamily::Lm),
        Some(MlxRuntimeFamily::Vlm),
        Some(MlxRuntimeFamily::Diffusion),
    ] {
        let catalog = FakeModelCatalog {
            metadata: mlx_model_metadata(family, vec![ModelCapability::AudioTranscription]),
        };
        let resolver = StdAudioTranscriptionModelResolver::new(&catalog);

        let err = resolver
            .resolve_audio_transcription_model(AudioTranscriptionModelResolveRequest {
                layout: layout_input(unique_path("audio-non-audio-mlx-home")),
                selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
            })
            .expect_err("unsupported mlx audio family");

        assert!(matches!(err, KernelError::UnsupportedTarget(_)));
        assert!(err.to_string().contains("does not support `mlx`"));
    }
}

#[cfg(any())]
#[cfg(unix)]
#[tokio::test]
async fn python_audio_transcription_batch_client_runs_entrypoint_with_path_arguments() {
    use std::os::unix::fs::PermissionsExt;

    let root = unique_path("python-audio-transcribe");
    let home = root.join("home");
    let project = root.join("project");
    let env = root.join("env");
    fs::create_dir_all(&home).expect("home");
    fs::create_dir_all(&project).expect("project");
    fs::create_dir_all(&env).expect("env");
    let entrypoint = root.join("tentgent-model-runtime-daemon");
    fs::write(
        &entrypoint,
        "#!/bin/sh\nprintf '%s\\n' \"$PWD\" > \"$TENTGENT_HOME/cwd.txt\"\nprintf '%s\\n' \"$@\" > \"$TENTGENT_HOME/args.txt\"\nprintf '{\"output_format\":\"vtt\",\"media_type\":\"text/vtt\",\"output_path\":\"%s/out.vtt\",\"total_bytes\":42,\"text\":\"hello\"}' \"$TENTGENT_HOME\"\n",
    )
    .expect("script");
    let mut permissions = fs::metadata(&entrypoint).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&entrypoint, permissions).expect("chmod");

    let executable_resolver = FakeExecutableResolver {
        entrypoint,
        expected_entrypoint: RuntimeEntrypoint::ModelRuntimeDaemon,
    };
    let client = PythonAudioTranscriptionBatchRuntimeClient::new(&executable_resolver);
    let response = client
        .transcribe_audio(AudioTranscriptionRuntimeRequest {
            layout: runtime_layout(&home),
            runtime: python_runtime(&project, &env),
            request: audio_request(&home),
        })
        .await
        .expect("transcribe");

    assert_eq!(
        response,
        AudioTranscriptionResponse {
            output_format: AudioTranscriptionOutputFormat::Vtt,
            media_type: "text/vtt".to_string(),
            output_path: home.join("out.vtt"),
            total_bytes: 42,
            text: Some("hello".to_string()),
        }
    );
    let observed_cwd = PathBuf::from(
        fs::read_to_string(home.join("cwd.txt"))
            .expect("cwd")
            .trim(),
    );
    assert_eq!(
        fs::canonicalize(observed_cwd).expect("observed cwd"),
        fs::canonicalize(&project).expect("project")
    );
    let args = fs::read_to_string(home.join("args.txt")).expect("args");
    assert!(args.contains("--model-ref\n"));
    assert!(args.contains(&format!("{}\n", model_ref())));
    assert!(args.contains("--input-path\n"));
    assert!(args.contains("input.wav\n"));
    assert!(args.contains("--output-path\n"));
    assert!(args.contains("out.vtt\n"));
    assert!(args.contains("--format\nvtt\n"));
    assert!(args.contains("--language\nen\n"));
    assert!(args.contains("--timestamps\n"));
}

#[cfg(any())]
#[cfg(unix)]
#[tokio::test]
async fn python_audio_speech_once_client_runs_entrypoint_with_text_arguments() {
    use std::os::unix::fs::PermissionsExt;

    let root = unique_path("python-audio-speech");
    let home = root.join("home");
    let project = root.join("project");
    let env = root.join("env");
    fs::create_dir_all(&home).expect("home");
    fs::create_dir_all(&project).expect("project");
    fs::create_dir_all(&env).expect("env");
    let entrypoint = root.join("tentgent-model-runtime-daemon");
    fs::write(
        &entrypoint,
        "#!/bin/sh\nprintf '%s\\n' \"$PWD\" > \"$TENTGENT_HOME/speech-cwd.txt\"\nprintf '%s\\n' \"$@\" > \"$TENTGENT_HOME/speech-args.txt\"\nprintf '{\"output_format\":\"wav\",\"media_type\":\"audio/wav\",\"output_path\":\"%s/speech.wav\",\"total_bytes\":44,\"sample_rate\":16000}' \"$TENTGENT_HOME\"\n",
    )
    .expect("script");
    let mut permissions = fs::metadata(&entrypoint).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&entrypoint, permissions).expect("chmod");

    let executable_resolver = FakeExecutableResolver {
        entrypoint,
        expected_entrypoint: RuntimeEntrypoint::ModelRuntimeDaemon,
    };
    let client = PythonAudioSpeechOnceRuntimeClient::new(&executable_resolver);
    let response = client
        .synthesize_speech(AudioSpeechRuntimeRequest {
            layout: runtime_layout(&home),
            runtime: python_runtime(&project, &env),
            request: audio_speech_request(&home),
        })
        .await
        .expect("synthesize");

    assert_eq!(
        response,
        AudioSpeechResponse {
            output_format: AudioSpeechOutputFormat::Wav,
            media_type: "audio/wav".to_string(),
            output_path: home.join("speech.wav"),
            total_bytes: 44,
            sample_rate: Some(16000),
        }
    );
    let observed_cwd = PathBuf::from(
        fs::read_to_string(home.join("speech-cwd.txt"))
            .expect("cwd")
            .trim(),
    );
    assert_eq!(
        fs::canonicalize(observed_cwd).expect("observed cwd"),
        fs::canonicalize(&project).expect("project")
    );
    let args = fs::read_to_string(home.join("speech-args.txt")).expect("args");
    assert!(args.contains("--model-ref\n"));
    assert!(args.contains(&format!("{}\n", model_ref())));
    assert!(args.contains("--text\nhello world\n"));
    assert!(args.contains("--output-path\n"));
    assert!(args.contains("speech.wav\n"));
    assert!(args.contains("--format\nwav\n"));
    assert!(args.contains("--language\nen\n"));
    assert!(args.contains("--voice\ndefault\n"));
}

#[derive(Clone)]
struct FakeModelCatalog {
    metadata: ModelMetadata,
}

impl ModelCatalogReadUseCase for FakeModelCatalog {
    fn list_models(&self, request: ModelListRequest) -> KernelResult<ModelListResult> {
        let layout = runtime_layout(
            request
                .layout
                .home_dir
                .as_deref()
                .unwrap_or(Path::new("/tmp")),
        );
        Ok(ModelListResult {
            store: ModelStoreLayout::from_models_dir(layout.models_dir.clone()),
            layout,
            models: Vec::new(),
        })
    }

    fn inspect_model(&self, request: ModelInspectRequest) -> KernelResult<ModelInspectResult> {
        let layout = runtime_layout(
            request
                .layout
                .home_dir
                .as_deref()
                .unwrap_or(Path::new("/tmp")),
        );
        Ok(ModelInspectResult {
            store: ModelStoreLayout::from_models_dir(layout.models_dir.clone()),
            model: ModelInspection {
                metadata: self.metadata.clone(),
                store_path: layout.models_dir.join("store").join(model_ref().as_str()),
                manifest_path: layout
                    .models_dir
                    .join("store")
                    .join(model_ref().as_str())
                    .join("manifest.json"),
                variant_source_path: layout
                    .models_dir
                    .join("store")
                    .join(model_ref().as_str())
                    .join("source"),
            },
            layout,
        })
    }
}

#[cfg(any())]
struct FakeExecutableResolver {
    entrypoint: PathBuf,
    expected_entrypoint: RuntimeEntrypoint,
}

#[cfg(any())]
impl RuntimeExecutableResolver for FakeExecutableResolver {
    fn python_binary_path(&self, _runtime: &PythonRuntimeLayout) -> KernelResult<PathBuf> {
        Ok(PathBuf::from("python"))
    }

    fn entrypoint_path(
        &self,
        _runtime: &PythonRuntimeLayout,
        entrypoint: RuntimeEntrypoint,
    ) -> KernelResult<PathBuf> {
        assert_eq!(entrypoint, self.expected_entrypoint);
        Ok(self.entrypoint.clone())
    }
}

#[cfg(any())]
fn audio_request(home: &Path) -> AudioTranscriptionRequest {
    AudioTranscriptionRequest {
        target: ResolvedAudioTranscriptionTarget {
            runtime: AudioTranscriptionRuntimeTarget::LocalModel {
                model_ref: model_ref(),
                backend: AudioTranscriptionBackend::TransformersAutomaticSpeechRecognition,
                source_repo: Some("org/model".to_string()),
                source_revision: Some("main".to_string()),
                model_capabilities: vec![ModelCapability::AudioTranscription],
            },
        },
        input_path: home.join("input.wav"),
        output_path: home.join("out.vtt"),
        output_format: AudioTranscriptionOutputFormat::Vtt,
        language: Some("en".to_string()),
        timestamps: true,
    }
}

#[cfg(any())]
fn audio_speech_request(home: &Path) -> AudioSpeechRequest {
    AudioSpeechRequest {
        target: ResolvedAudioSpeechTarget {
            runtime: AudioSpeechRuntimeTarget::LocalModel {
                model_ref: model_ref(),
                backend: AudioSpeechBackend::TransformersTextToSpeech,
                source_repo: Some("org/model".to_string()),
                source_revision: Some("main".to_string()),
                model_capabilities: vec![ModelCapability::AudioSpeech],
            },
        },
        text: "hello world".to_string(),
        output_path: home.join("speech.wav"),
        output_format: AudioSpeechOutputFormat::Wav,
        language: Some("en".to_string()),
        voice: Some("default".to_string()),
    }
}

fn model_metadata(format: ModelFormat, capabilities: Vec<ModelCapability>) -> ModelMetadata {
    ModelMetadata {
        model_ref: model_ref(),
        short_ref: model_ref().short_ref().to_string(),
        source_kind: ModelSourceKind::HuggingFace,
        source_repo: Some("org/model".to_string()),
        source_revision: Some("main".to_string()),
        source_path: None,
        primary_format: format,
        detected_formats: vec![format],
        mlx_runtime_family: None,
        model_capabilities: capabilities,
        model_capability_source: default_model_capability_source(),
        file_count: 1,
        total_bytes: 10,
        imported_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

fn mlx_model_metadata(
    family: Option<MlxRuntimeFamily>,
    capabilities: Vec<ModelCapability>,
) -> ModelMetadata {
    let mut metadata = model_metadata(ModelFormat::Mlx, capabilities);
    metadata.mlx_runtime_family = family;
    metadata
}

fn model_ref() -> ModelRef {
    ModelRef::parse("6".repeat(64)).expect("model ref")
}

fn layout_input(home: PathBuf) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode: LayoutResolveMode::ReadOnly,
        home_dir: Some(home),
        data_root_dir: None,
    }
}

fn runtime_layout(home: &Path) -> RuntimeLayout {
    RuntimeLayout {
        home_dir: home.to_path_buf(),
        data_root_dir: home.join("data"),
        config_path: home.join("config.toml"),
        models_dir: home.join("models"),
        adapters_dir: home.join("adapters"),
        datasets_dir: home.join("datasets"),
        sessions_dir: home.join("sessions"),
        servers_dir: home.join("servers"),
        train_dir: home.join("training"),
        cache_dir: home.join("cache"),
        runtime_dir: home.join("runtime"),
        logs_dir: home.join("logs"),
        locks_dir: home.join("locks"),
        python_env_dir: home.join("runtime/python"),
        bootstrap_dir: home.join("bootstrap"),
        bootstrap_uv_dir: home.join("bootstrap/uv"),
        bootstrap_uv_cache_dir: home.join("bootstrap/uv-cache"),
        capabilities_path: home.join("runtime/capabilities.toml"),
        auth_metadata_path: home.join("runtime/auth.toml"),
    }
}

#[cfg(any())]
fn python_runtime(project: &Path, env: &Path) -> PythonRuntimeLayout {
    PythonRuntimeLayout {
        project_dir: project.to_path_buf(),
        env_dir: env.to_path_buf(),
        source: PythonRuntimeSource::DevelopmentSource,
    }
}

fn unique_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("tentgent-{prefix}-{nanos}"))
}
