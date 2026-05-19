use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub enum VisionCommands {
    /// Ask a local vision-chat model about one image.
    #[command(
        name = "chat",
        about = "Ask a local vision-chat model about one image.",
        long_about = "Ask a local vision-chat model about one image in the foreground without starting the daemon. The command resolves a stored vision-chat model, calls the Python vision runtime once, and prints or writes the generated answer."
    )]
    Chat(VisionChatCommand),
}

#[derive(Debug, Args)]
pub struct VisionChatCommand {
    /// Local image file to inspect.
    #[arg(value_name = "IMAGE_PATH")]
    pub image_path: PathBuf,
    /// Stored Tentgent vision-chat model reference to run.
    #[arg(short = 'm', long = "model-ref", value_name = "MODEL_REF")]
    pub model_ref: String,
    /// Prompt to ask about the image.
    #[arg(short = 'p', long = "prompt", value_name = "TEXT")]
    pub prompt: String,
    /// Optional system prompt.
    #[arg(long = "system-prompt", value_name = "TEXT")]
    pub system_prompt: Option<String>,
    /// Local output path. Existing files are never overwritten.
    #[arg(short = 'o', long = "output", value_name = "OUTPUT_PATH")]
    pub output: Option<PathBuf>,
    /// Output format intent: text, json, or md.
    #[arg(long = "format", value_name = "FORMAT", default_value = "text")]
    pub format: String,
    /// Optional max generated tokens.
    #[arg(long = "max-tokens", value_name = "N")]
    pub max_tokens: Option<u32>,
    /// Optional sampling temperature.
    #[arg(long = "temperature", value_name = "FLOAT")]
    pub temperature: Option<f32>,
    /// Optional Tentgent runtime home override.
    #[arg(short = 'H', long, value_name = "HOME")]
    pub home: Option<PathBuf>,
}
