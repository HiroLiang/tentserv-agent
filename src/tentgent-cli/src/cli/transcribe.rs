use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use miette::{miette, IntoDiagnostic, Result};
use tentgent_kernel::features::audio::{
    domain::AudioTranscriptionOutputFormat,
    infra::{PythonAudioTranscriptionBatchRuntimeClient, StdAudioTranscriptionModelResolver},
    usecases::{
        AudioTranscriptionPreparationRequest, AudioTranscriptionUseCase,
        StdAudioTranscriptionUseCase,
    },
};
use tentgent_kernel::features::model::domain::ModelRefSelector;
use tentgent_kernel::features::model::infra::FileModelCatalogStore;
use tentgent_kernel::features::model::usecases::StdModelCatalogReadUseCase;
use tentgent_kernel::features::runtime::domain::PythonRuntimeResolutionInput;
use tentgent_kernel::features::runtime::infra::{
    StdPythonRuntimeResolver, StdRuntimeExecutableResolver,
};
use tentgent_kernel::features::runtime::usecases::StdRuntimeResolutionUseCase;
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver,
};

use super::commands::TranscribeCommand;

const LARGE_AUDIO_WARNING_BYTES: u64 = 100 * 1024 * 1024;

pub async fn handle_transcribe_command(command: TranscribeCommand) -> Result<()> {
    let request = transcribe_request(&command)?;
    let output = TranscribeOutputTarget::prepare(command.output.as_deref(), request.output_format)?;
    warn_if_large_audio(&request.input_path)?;

    let kernel = CliAudioTranscriptionKernel::new();
    let runtime_resolution =
        StdRuntimeResolutionUseCase::new(&kernel.layout_resolver, &kernel.runtime_resolver);
    let model_catalog =
        StdModelCatalogReadUseCase::new(&kernel.layout_resolver, &kernel.model_catalog);
    let model_resolver = StdAudioTranscriptionModelResolver::new(&model_catalog);
    let runtime_client =
        PythonAudioTranscriptionBatchRuntimeClient::new(&kernel.executable_resolver);
    let transcriber =
        StdAudioTranscriptionUseCase::new(&runtime_resolution, &model_resolver, &runtime_client);

    let result = match transcriber
        .transcribe_audio(AudioTranscriptionPreparationRequest {
            output_path: output.runtime_path.clone(),
            ..request
        })
        .await
    {
        Ok(result) => result,
        Err(error) => {
            output.cleanup_temp();
            return Err(audio_runtime_report(error.to_string()));
        }
    };

    output.finish(&result.response.output_path)
}

struct CliAudioTranscriptionKernel {
    layout_resolver: StdRuntimeLayoutResolver,
    runtime_resolver: StdPythonRuntimeResolver,
    executable_resolver: StdRuntimeExecutableResolver,
    model_catalog: FileModelCatalogStore,
}

impl CliAudioTranscriptionKernel {
    fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            runtime_resolver: StdPythonRuntimeResolver,
            executable_resolver: StdRuntimeExecutableResolver,
            model_catalog: FileModelCatalogStore,
        }
    }
}

fn transcribe_request(command: &TranscribeCommand) -> Result<AudioTranscriptionPreparationRequest> {
    let output_format = parse_output_format(&command.format)?;
    let input_path = canonical_audio_input_path(&command.input_path)?;
    let model_selector = ModelRefSelector::parse(&command.model_ref)
        .map_err(|err| miette!("failed to parse model ref for audio transcription: {err}"))?;
    let timestamps = command.timestamps
        || matches!(
            output_format,
            AudioTranscriptionOutputFormat::Vtt | AudioTranscriptionOutputFormat::Srt
        );

    Ok(AudioTranscriptionPreparationRequest {
        layout: runtime_layout_input(command.home.as_deref()),
        runtime: PythonRuntimeResolutionInput::default(),
        model_selector,
        input_path,
        output_path: PathBuf::new(),
        output_format,
        language: command.language.clone().and_then(non_empty_string),
        timestamps,
    })
}

fn parse_output_format(value: &str) -> Result<AudioTranscriptionOutputFormat> {
    value
        .parse::<AudioTranscriptionOutputFormat>()
        .map_err(|err| miette!("{err}"))
}

fn canonical_audio_input_path(path: &Path) -> Result<PathBuf> {
    let absolute = absolutize_cli_path(path)?;
    let canonical = fs::canonicalize(&absolute).map_err(|error| {
        miette!(
            "audio input path `{}` is not readable: {error}",
            absolute.display()
        )
    })?;
    let metadata = fs::metadata(&canonical).map_err(|error| {
        miette!(
            "audio input path `{}` is not readable: {error}",
            canonical.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(miette!(
            "audio input path `{}` is not a file",
            canonical.display()
        ));
    }
    Ok(canonical)
}

fn runtime_layout_input(home: Option<&Path>) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode: LayoutResolveMode::ReadOnly,
        home_dir: home.map(Path::to_path_buf),
        data_root_dir: None,
    }
}

#[derive(Debug)]
struct TranscribeOutputTarget {
    runtime_path: PathBuf,
    final_path: Option<PathBuf>,
}

impl TranscribeOutputTarget {
    fn prepare(output_path: Option<&Path>, format: AudioTranscriptionOutputFormat) -> Result<Self> {
        if output_path.is_none()
            && matches!(
                format,
                AudioTranscriptionOutputFormat::Vtt | AudioTranscriptionOutputFormat::Srt
            )
        {
            return Err(miette!(
                "`--output` is required when `--format {}` is selected",
                format.as_str()
            ));
        }

        match output_path {
            Some(path) => {
                let final_path = absolutize_cli_path(path)?;
                ensure_output_path_available(&final_path)?;
                ensure_output_parent(&final_path)?;
                Ok(Self {
                    runtime_path: unique_temp_output_near(&final_path),
                    final_path: Some(final_path),
                })
            }
            None => Ok(Self {
                runtime_path: unique_stdout_temp_output(format),
                final_path: None,
            }),
        }
    }

    fn finish(&self, runtime_output_path: &Path) -> Result<()> {
        if let Some(final_path) = &self.final_path {
            if final_path.exists() {
                self.cleanup_temp();
                return Err(miette!(
                    "output file already exists: {}",
                    final_path.display()
                ));
            }
            fs::rename(runtime_output_path, final_path).map_err(|error| {
                miette!(
                    "failed to write transcription output `{}`: {error}",
                    final_path.display()
                )
            })?;
            println!("transcription written: {}", final_path.display());
            return Ok(());
        }

        let body = match fs::read(runtime_output_path) {
            Ok(body) => body,
            Err(error) => {
                self.cleanup_temp();
                return Err(miette!(
                    "failed to read transcription output `{}`: {error}",
                    runtime_output_path.display()
                ));
            }
        };
        let mut stdout = io::stdout().lock();
        if let Err(error) = stdout.write_all(&body) {
            self.cleanup_temp();
            return Err(error).into_diagnostic();
        }
        if !body.ends_with(b"\n") {
            if let Err(error) = writeln!(stdout) {
                self.cleanup_temp();
                return Err(error).into_diagnostic();
            }
        }
        self.cleanup_temp();
        Ok(())
    }

    fn cleanup_temp(&self) {
        let _ = fs::remove_file(&self.runtime_path);
    }
}

fn ensure_output_path_available(path: &Path) -> Result<()> {
    if path.is_dir() {
        return Err(miette!("output path is a directory: {}", path.display()));
    }
    if path.exists() {
        return Err(miette!("output file already exists: {}", path.display()));
    }
    Ok(())
}

fn ensure_output_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if parent.exists() {
        if !parent.is_dir() {
            return Err(miette!(
                "output parent path is not a directory: {}",
                parent.display()
            ));
        }
        return Ok(());
    }
    fs::create_dir_all(parent).map_err(|error| {
        miette!(
            "failed to create output directory `{}`: {error}",
            parent.display()
        )
    })
}

fn warn_if_large_audio(input_path: &Path) -> Result<()> {
    let size = fs::metadata(input_path)
        .map_err(|error| {
            miette!(
                "failed to inspect audio input `{}`: {error}",
                input_path.display()
            )
        })?
        .len();
    if size >= LARGE_AUDIO_WARNING_BYTES {
        eprintln!(
            "warning: audio input is {}; compressed audio can expand substantially during decoding, and ASR model windows may use additional memory.",
            human_bytes(size)
        );
    }
    Ok(())
}

fn audio_runtime_report(message: String) -> miette::Report {
    let lower = message.to_ascii_lowercase();
    if lower.contains("ffmpeg") || lower.contains("decode") || lower.contains("audio file") {
        return miette!(
            "audio transcription failed: {message}\n\nmedia decoder hint: ffmpeg is required for MP3, M4A, AAC, Ogg, WebM, MP4, and many other containers. Run `tentgent doctor`; on macOS install it with `brew install ffmpeg`."
        );
    }
    miette!("audio transcription failed: {message}")
}

fn unique_temp_output_near(final_path: &Path) -> PathBuf {
    let parent = final_path.parent().unwrap_or_else(|| Path::new("."));
    let filename = final_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("transcript");
    parent.join(format!(".{filename}.tentgent-{}.tmp", unique_suffix()))
}

fn unique_stdout_temp_output(format: AudioTranscriptionOutputFormat) -> PathBuf {
    env::temp_dir().join(format!(
        "tentgent-transcribe-{}.{}",
        unique_suffix(),
        format.extension()
    ))
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{}-{nanos}", std::process::id())
}

fn absolutize_cli_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(env::current_dir().into_diagnostic()?.join(path))
}

fn non_empty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn human_bytes(bytes: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / MIB)
    } else {
        format!("{bytes} bytes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_audio_transcription_output_format_alias() {
        assert_eq!(
            parse_output_format("txt").expect("txt"),
            AudioTranscriptionOutputFormat::Text
        );
    }

    #[test]
    fn vtt_and_srt_require_output_path() {
        let err = TranscribeOutputTarget::prepare(None, AudioTranscriptionOutputFormat::Vtt)
            .expect_err("vtt without output");
        assert!(err.to_string().contains("`--output` is required"));

        let err = TranscribeOutputTarget::prepare(None, AudioTranscriptionOutputFormat::Srt)
            .expect_err("srt without output");
        assert!(err.to_string().contains("`--output` is required"));
    }

    #[test]
    fn output_path_must_not_already_exist() {
        let root = unique_test_dir("transcribe-output-exists");
        fs::create_dir_all(&root).expect("root");
        let path = root.join("transcript.txt");
        fs::write(&path, b"existing").expect("existing output");

        let err =
            TranscribeOutputTarget::prepare(Some(&path), AudioTranscriptionOutputFormat::Text)
                .expect_err("existing output");

        assert!(err.to_string().contains("output file already exists"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn subtitle_formats_imply_timestamps() {
        let root = unique_test_dir("transcribe-timestamps");
        fs::create_dir_all(&root).expect("root");
        let input = root.join("audio.wav");
        fs::write(&input, b"fake audio").expect("input");

        let command = TranscribeCommand {
            input_path: input,
            model_ref: "a".repeat(64),
            output: Some(root.join("transcript.vtt")),
            format: "vtt".to_string(),
            language: None,
            timestamps: false,
            home: Some(root.join("home")),
        };
        let request = transcribe_request(&command).expect("request");

        assert!(request.timestamps);
        let _ = fs::remove_dir_all(root);
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        env::temp_dir().join(format!("tentgent-{prefix}-{}", unique_suffix()))
    }
}
