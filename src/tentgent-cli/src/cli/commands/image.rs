use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub enum ImageCommands {
    /// Generate one image from a text prompt.
    #[command(
        name = "generate",
        about = "Generate one image from a text prompt.",
        long_about = "Generate one image from a text prompt in the foreground without starting the daemon. The command resolves a stored image-generation model, calls the Python Diffusers runtime once, and writes the generated png or jpg output. Existing output files are never overwritten."
    )]
    Generate(ImageGenerateCommand),
}

#[derive(Debug, Args)]
pub struct ImageGenerateCommand {
    /// Stored Tentgent image-generation model reference to run.
    #[arg(short = 'm', long = "model-ref", value_name = "MODEL_REF")]
    pub model_ref: String,
    /// Prompt for the generated image.
    #[arg(short = 'p', long = "prompt", value_name = "TEXT")]
    pub prompt: String,
    /// Optional negative prompt.
    #[arg(long = "negative-prompt", value_name = "TEXT")]
    pub negative_prompt: Option<String>,
    /// Local output path. Existing files are never overwritten.
    #[arg(short = 'o', long = "output", value_name = "OUTPUT_PATH")]
    pub output: PathBuf,
    /// Output image format intent: png or jpg.
    #[arg(long = "format", value_name = "FORMAT", default_value = "png")]
    pub format: String,
    /// Output image width in pixels. Must be 64..1024 and divisible by 8.
    #[arg(long = "width", value_name = "PX", default_value_t = 512)]
    pub width: u32,
    /// Output image height in pixels. Must be 64..1024 and divisible by 8.
    #[arg(long = "height", value_name = "PX", default_value_t = 512)]
    pub height: u32,
    /// Diffusion inference steps. Must be 1..100.
    #[arg(long = "steps", value_name = "N", default_value_t = 20)]
    pub steps: u32,
    /// Classifier-free guidance scale. Must be 0..30.
    #[arg(long = "guidance-scale", value_name = "FLOAT", default_value_t = 7.5)]
    pub guidance_scale: f32,
    /// Optional deterministic seed.
    #[arg(long = "seed", value_name = "N")]
    pub seed: Option<u64>,
    /// Optional Tentgent runtime home override.
    #[arg(short = 'H', long, value_name = "HOME")]
    pub home: Option<PathBuf>,
}
