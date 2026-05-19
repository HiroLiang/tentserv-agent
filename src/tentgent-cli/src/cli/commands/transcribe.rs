use std::path::PathBuf;

use clap::Args;

#[derive(Debug, Args)]
pub struct TranscribeCommand {
    /// Local audio file to transcribe.
    #[arg(value_name = "AUDIO_PATH")]
    pub input_path: PathBuf,
    /// Stored Tentgent audio-transcription model reference to run.
    #[arg(short = 'm', long = "model-ref", value_name = "MODEL_REF")]
    pub model_ref: String,
    /// Local transcript output path. Required for vtt and srt formats.
    #[arg(short = 'o', long = "output", value_name = "OUTPUT_PATH")]
    pub output: Option<PathBuf>,
    /// Transcript output format: text, json, vtt, or srt.
    #[arg(long = "format", value_name = "FORMAT", default_value = "text")]
    pub format: String,
    /// Optional model language hint, such as en.
    #[arg(long = "language", value_name = "LANGUAGE")]
    pub language: Option<String>,
    /// Ask the runtime to return timestamp chunks when supported.
    #[arg(long = "timestamps")]
    pub timestamps: bool,
    /// Optional Tentgent runtime home override.
    #[arg(short = 'H', long, value_name = "HOME")]
    pub home: Option<PathBuf>,
}
