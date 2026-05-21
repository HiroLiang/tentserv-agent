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
    /// Transform one image with a text prompt.
    #[command(
        name = "transform",
        about = "Transform one image with a text prompt.",
        long_about = "Transform one local image with a text prompt in the foreground without starting the daemon. The command resolves a stored image-generation model, calls the Python image-to-image runtime once, and writes the generated png or jpg output. Existing output files are never overwritten."
    )]
    Transform(ImageTransformCommand),
    /// Repaint the white area of one mask image with a text prompt.
    #[command(
        name = "inpaint",
        about = "Repaint the white area of one mask image with a text prompt.",
        long_about = "Repaint the white area of one mask image with a text prompt in the foreground without starting the daemon. The command resolves a stored image-generation model, calls the Python inpainting runtime once, and writes the generated png or jpg output. Existing output files are never overwritten."
    )]
    Inpaint(ImageInpaintCommand),
    /// Generate one image from a prompt and a typed control image.
    #[command(
        name = "control",
        about = "Generate one image from a prompt and a typed control image.",
        long_about = "Generate one image from a prompt and one typed control image in the foreground without starting the daemon. The command resolves a stored image-generation model plus a managed ControlNet-style control adapter, calls the Python controlled image-generation runtime once, and writes the generated png or jpg output. Existing output files are never overwritten."
    )]
    Control(ImageControlCommand),
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
    /// Optional managed image LoRA adapter reference.
    #[arg(long = "adapter-ref", value_name = "ADAPTER_REF")]
    pub adapter_ref: Option<String>,
    /// Optional LoRA scale. Requires --adapter-ref.
    #[arg(long = "lora-scale", value_name = "FLOAT")]
    pub lora_scale: Option<f32>,
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

#[derive(Debug, Args)]
pub struct ImageTransformCommand {
    /// Stored Tentgent image-generation model reference to run.
    #[arg(short = 'm', long = "model-ref", value_name = "MODEL_REF")]
    pub model_ref: String,
    /// Local input image path. PNG, JPEG, and WebP are supported.
    #[arg(short = 'i', long = "input-image", value_name = "PATH")]
    pub input_image: PathBuf,
    /// Prompt describing the requested transformation.
    #[arg(short = 'p', long = "prompt", value_name = "TEXT")]
    pub prompt: String,
    /// Optional negative prompt.
    #[arg(long = "negative-prompt", value_name = "TEXT")]
    pub negative_prompt: Option<String>,
    /// Optional managed image LoRA adapter reference.
    #[arg(long = "adapter-ref", value_name = "ADAPTER_REF")]
    pub adapter_ref: Option<String>,
    /// Optional LoRA scale. Requires --adapter-ref.
    #[arg(long = "lora-scale", value_name = "FLOAT")]
    pub lora_scale: Option<f32>,
    /// Diffusers-style denoising strength. 0 preserves input most; 1 regenerates most.
    #[arg(long = "strength", value_name = "FLOAT", default_value_t = 0.6)]
    pub strength: f32,
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

#[derive(Debug, Args)]
pub struct ImageInpaintCommand {
    /// Stored Tentgent image-generation model reference to run.
    #[arg(short = 'm', long = "model-ref", value_name = "MODEL_REF")]
    pub model_ref: String,
    /// Local base image path. PNG, JPEG, and WebP are supported.
    #[arg(short = 'i', long = "input-image", value_name = "PATH")]
    pub input_image: PathBuf,
    /// Local mask image path. White pixels repaint; black pixels keep.
    #[arg(long = "mask-image", value_name = "PATH")]
    pub mask_image: PathBuf,
    /// Prompt describing the requested repaint.
    #[arg(short = 'p', long = "prompt", value_name = "TEXT")]
    pub prompt: String,
    /// Optional negative prompt.
    #[arg(long = "negative-prompt", value_name = "TEXT")]
    pub negative_prompt: Option<String>,
    /// Optional managed image LoRA adapter reference.
    #[arg(long = "adapter-ref", value_name = "ADAPTER_REF")]
    pub adapter_ref: Option<String>,
    /// Optional LoRA scale. Requires --adapter-ref.
    #[arg(long = "lora-scale", value_name = "FLOAT")]
    pub lora_scale: Option<f32>,
    /// Diffusers-style denoising strength. 0 preserves masked area most; 1 repaints most.
    #[arg(long = "strength", value_name = "FLOAT", default_value_t = 1.0)]
    pub strength: f32,
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

#[derive(Debug, Args)]
pub struct ImageControlCommand {
    /// Stored Tentgent image-generation model reference to run.
    #[arg(short = 'm', long = "model-ref", value_name = "MODEL_REF")]
    pub model_ref: String,
    /// Managed ControlNet-style adapter reference.
    #[arg(long = "control-ref", value_name = "ADAPTER_REF")]
    pub control_ref: String,
    /// Local control image path. PNG, JPEG, and WebP are supported.
    #[arg(long = "control-image", value_name = "PATH")]
    pub control_image: PathBuf,
    /// Control image kind. M6O supports canny.
    #[arg(long = "control-kind", value_name = "KIND", default_value = "canny")]
    pub control_kind: String,
    /// Prompt for the generated image.
    #[arg(short = 'p', long = "prompt", value_name = "TEXT")]
    pub prompt: String,
    /// Optional negative prompt.
    #[arg(long = "negative-prompt", value_name = "TEXT")]
    pub negative_prompt: Option<String>,
    /// Optional managed image LoRA adapter reference.
    #[arg(long = "adapter-ref", value_name = "ADAPTER_REF")]
    pub adapter_ref: Option<String>,
    /// Optional LoRA scale. Requires --adapter-ref.
    #[arg(long = "lora-scale", value_name = "FLOAT")]
    pub lora_scale: Option<f32>,
    /// Control influence strength. 0 disables control; 1 is the backend default.
    #[arg(long = "control-strength", value_name = "FLOAT", default_value_t = 1.0)]
    pub control_strength: f32,
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
