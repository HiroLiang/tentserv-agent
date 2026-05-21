use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use miette::{miette, IntoDiagnostic, Result};
use tentgent_kernel::features::audio::{
    domain::AudioSpeechOutputFormat,
    infra::{PythonAudioSpeechOnceRuntimeClient, StdAudioSpeechModelResolver},
    usecases::{AudioSpeechPreparationRequest, AudioSpeechUseCase, StdAudioSpeechUseCase},
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

use super::commands::SpeakCommand;

pub async fn handle_speak_command(command: SpeakCommand) -> Result<()> {
    let request = speak_request(&command)?;
    let output = SpeechOutputTarget::prepare(&command.output)?;

    let kernel = CliAudioSpeechKernel::new();
    let runtime_resolution =
        StdRuntimeResolutionUseCase::new(&kernel.layout_resolver, &kernel.runtime_resolver);
    let model_catalog =
        StdModelCatalogReadUseCase::new(&kernel.layout_resolver, &kernel.model_catalog);
    let model_resolver = StdAudioSpeechModelResolver::new(&model_catalog);
    let runtime_client = PythonAudioSpeechOnceRuntimeClient::new(&kernel.executable_resolver);
    let speaker = StdAudioSpeechUseCase::new(&runtime_resolution, &model_resolver, &runtime_client);

    let result = match speaker
        .synthesize_speech(AudioSpeechPreparationRequest {
            output_path: output.runtime_path.clone(),
            ..request
        })
        .await
    {
        Ok(result) => result,
        Err(error) => {
            output.cleanup_temp();
            return Err(miette!("audio speech failed: {error}"));
        }
    };

    output.finish(&result.response.output_path)?;
    let sample_rate = result
        .response
        .sample_rate
        .map(|rate| format!(" at {rate} Hz"))
        .unwrap_or_default();
    println!(
        "speech written: {} ({}{}, {})",
        output.final_path.display(),
        result.response.output_format.as_str(),
        sample_rate,
        human_bytes(result.response.total_bytes)
    );
    Ok(())
}

struct CliAudioSpeechKernel {
    layout_resolver: StdRuntimeLayoutResolver,
    runtime_resolver: StdPythonRuntimeResolver,
    executable_resolver: StdRuntimeExecutableResolver,
    model_catalog: FileModelCatalogStore,
}

impl CliAudioSpeechKernel {
    fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            runtime_resolver: StdPythonRuntimeResolver,
            executable_resolver: StdRuntimeExecutableResolver,
            model_catalog: FileModelCatalogStore,
        }
    }
}

fn speak_request(command: &SpeakCommand) -> Result<AudioSpeechPreparationRequest> {
    let output_format = parse_output_format(&command.format)?;
    let text = command_text(command)?;
    let model_selector = ModelRefSelector::parse(&command.model_ref)
        .map_err(|err| miette!("failed to parse model ref for audio speech: {err}"))?;

    Ok(AudioSpeechPreparationRequest {
        layout: runtime_layout_input(command.home.as_deref()),
        runtime: PythonRuntimeResolutionInput::default(),
        model_selector,
        text,
        output_path: PathBuf::new(),
        output_format,
        language: command.language.clone().and_then(non_empty_string),
        voice: command.voice.clone().and_then(non_empty_string),
    })
}

fn command_text(command: &SpeakCommand) -> Result<String> {
    let text = match (&command.text, &command.text_file) {
        (Some(text), None) => text.clone(),
        (None, Some(path)) => fs::read_to_string(path).map_err(|error| {
            miette!(
                "failed to read speech text file `{}`: {error}",
                path.display()
            )
        })?,
        (None, None) => return Err(miette!("one of `--text` or `--text-file` is required")),
        (Some(_), Some(_)) => {
            return Err(miette!(
                "`--text` and `--text-file` cannot be used together"
            ));
        }
    };
    non_empty_string(text).ok_or_else(|| miette!("audio speech text must not be empty"))
}

fn parse_output_format(value: &str) -> Result<AudioSpeechOutputFormat> {
    value
        .parse::<AudioSpeechOutputFormat>()
        .map_err(|err| miette!("{err}"))
}

fn runtime_layout_input(home: Option<&Path>) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode: LayoutResolveMode::ReadOnly,
        home_dir: home.map(Path::to_path_buf),
        data_root_dir: None,
    }
}

#[derive(Debug)]
struct SpeechOutputTarget {
    runtime_path: PathBuf,
    final_path: PathBuf,
}

impl SpeechOutputTarget {
    fn prepare(output_path: &Path) -> Result<Self> {
        let final_path = absolutize_cli_path(output_path)?;
        ensure_output_path_available(&final_path)?;
        ensure_output_parent(&final_path)?;
        Ok(Self {
            runtime_path: unique_temp_output_near(&final_path),
            final_path,
        })
    }

    fn finish(&self, runtime_output_path: &Path) -> Result<()> {
        if self.final_path.exists() {
            self.cleanup_temp();
            return Err(miette!(
                "output file already exists: {}",
                self.final_path.display()
            ));
        }
        fs::rename(runtime_output_path, &self.final_path).map_err(|error| {
            miette!(
                "failed to write speech output `{}`: {error}",
                self.final_path.display()
            )
        })
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

fn unique_temp_output_near(final_path: &Path) -> PathBuf {
    let parent = final_path.parent().unwrap_or_else(|| Path::new("."));
    let filename = final_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("speech.wav");
    parent.join(format!(".{filename}.tentgent-{}.tmp", unique_suffix()))
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
    fn parses_audio_speech_output_format_alias() {
        assert_eq!(
            parse_output_format("wave").expect("wave"),
            AudioSpeechOutputFormat::Wav
        );
    }

    #[test]
    fn text_or_text_file_is_required() {
        let command = SpeakCommand {
            text: None,
            text_file: None,
            model_ref: "a".repeat(64),
            output: PathBuf::from("speech.wav"),
            format: "wav".to_string(),
            language: None,
            voice: None,
            home: None,
        };

        let err = command_text(&command).expect_err("missing text");
        assert!(err.to_string().contains("one of `--text` or `--text-file`"));
    }

    #[test]
    fn output_path_must_not_already_exist() {
        let root = unique_test_dir("speak-output-exists");
        fs::create_dir_all(&root).expect("root");
        let path = root.join("speech.wav");
        fs::write(&path, b"existing").expect("existing output");

        let err = SpeechOutputTarget::prepare(&path).expect_err("existing output");

        assert!(err.to_string().contains("output file already exists"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn command_text_reads_text_file() {
        let root = unique_test_dir("speak-text-file");
        fs::create_dir_all(&root).expect("root");
        let text_file = root.join("input.txt");
        fs::write(&text_file, " hello ").expect("text");
        let command = SpeakCommand {
            text: None,
            text_file: Some(text_file),
            model_ref: "a".repeat(64),
            output: root.join("speech.wav"),
            format: "wav".to_string(),
            language: None,
            voice: None,
            home: Some(root.join("home")),
        };

        assert_eq!(command_text(&command).expect("text"), "hello");
        let _ = fs::remove_dir_all(root);
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        env::temp_dir().join(format!("tentgent-{prefix}-{}", unique_suffix()))
    }
}
