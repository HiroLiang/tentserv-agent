use std::path::PathBuf;

use clap::Args;

#[derive(Debug, Args)]
pub struct SpeakCommand {
    /// Text to synthesize.
    #[arg(long = "text", value_name = "TEXT", conflicts_with = "text_file")]
    pub text: Option<String>,
    /// UTF-8 text file to synthesize.
    #[arg(long = "text-file", value_name = "PATH", conflicts_with = "text")]
    pub text_file: Option<PathBuf>,
    /// Stored Tentgent audio-speech model reference to run.
    #[arg(short = 'm', long = "model-ref", value_name = "MODEL_REF")]
    pub model_ref: String,
    /// Local speech audio output path.
    #[arg(short = 'o', long = "output", value_name = "OUTPUT_PATH")]
    pub output: PathBuf,
    /// Speech output format. M6P supports wav.
    #[arg(long = "format", value_name = "FORMAT", default_value = "wav")]
    pub format: String,
    /// Optional model language hint, such as en.
    #[arg(long = "language", value_name = "LANGUAGE")]
    pub language: Option<String>,
    /// Optional model voice or speaker hint.
    #[arg(long = "voice", value_name = "VOICE")]
    pub voice: Option<String>,
    /// Optional Tentgent runtime home override.
    #[arg(short = 'H', long, value_name = "HOME")]
    pub home: Option<PathBuf>,
}
