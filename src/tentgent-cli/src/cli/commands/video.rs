use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub enum VideoCommands {
    /// Ask a local video-understanding model about one video.
    #[command(
        name = "understand",
        about = "Ask a local video-understanding model about one video.",
        long_about = "Ask a local video-understanding model about one local video in the foreground without starting the daemon. The command samples bounded frames, resolves a stored video-understanding model, calls the Python video runtime once, and prints or writes the generated answer."
    )]
    Understand(VideoUnderstandCommand),
}

#[derive(Debug, Args)]
pub struct VideoUnderstandCommand {
    /// Local video file to inspect.
    #[arg(value_name = "VIDEO_PATH")]
    pub video_path: PathBuf,
    /// Stored Tentgent video-understanding model reference to run.
    #[arg(short = 'm', long = "model-ref", value_name = "MODEL_REF")]
    pub model_ref: String,
    /// Prompt to ask about the video.
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
    /// Frames per second to sample from the video. Default is 1.0.
    #[arg(long = "sample-fps", value_name = "FPS")]
    pub sample_fps: Option<f32>,
    /// Maximum sampled frames. Default is 32.
    #[arg(long = "max-frames", value_name = "N")]
    pub max_frames: Option<u32>,
    /// Resize sampled frames so the largest edge is at most this many pixels. Default is 768.
    #[arg(long = "max-frame-edge", value_name = "PIXELS")]
    pub max_frame_edge: Option<u32>,
    /// Optional clip start offset in seconds.
    #[arg(long = "clip-start-seconds", value_name = "SECONDS")]
    pub clip_start_seconds: Option<f32>,
    /// Optional clip duration in seconds.
    #[arg(long = "clip-duration-seconds", value_name = "SECONDS")]
    pub clip_duration_seconds: Option<f32>,
    /// Optional Tentgent runtime home override.
    #[arg(short = 'H', long, value_name = "HOME")]
    pub home: Option<PathBuf>,
}
