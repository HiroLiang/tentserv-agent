mod adapter;
mod auth;
mod chat;
mod daemon;
mod dataset;
mod embed;
mod image;
mod model;
mod rerank;
mod runtime;
mod server;
mod session;
mod speak;
mod store;
mod train;
mod transcribe;
mod video;
mod vision;

pub use adapter::AdapterCommands;
pub use auth::{AuthCommands, AuthProviderAction};
pub use chat::ChatCommand;
pub use daemon::{DaemonCommands, DaemonRunCommand, DaemonStartCommand};
pub use dataset::DatasetCommands;
pub use embed::EmbedCommand;
pub use image::{
    ImageCommands, ImageControlCommand, ImageGenerateCommand, ImageInpaintCommand,
    ImageTransformCommand,
};
pub use model::{ModelCapabilityCommands, ModelCommands};
pub use rerank::RerankCommand;
pub use runtime::RuntimeBootstrapProfile;
pub use runtime::{RuntimeBootstrapCommand, RuntimeCommands, RuntimeStatusCommand};
pub use server::{
    CloudServerRuntimeCommand, LocalServerRuntimeCommand, ServerCommands, ServerRunCommand,
};
pub use session::SessionCommands;
pub use speak::SpeakCommand;
pub use store::{StoreCommands, StoreGcCommand};
pub use train::{
    TrainCommands, TrainLoraCommands, TrainLoraPlanCommands, TrainLoraRunCommand,
    TrainLoraRunWorkerCommand,
};
pub use transcribe::TranscribeCommand;
pub use video::{VideoCommands, VideoUnderstandCommand};
pub use vision::{VisionChatCommand, VisionCommands};

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct DoctorCommand {
    /// Developer bootstrap: create or sync the managed Python environment with uv before checking health.
    #[arg(short = 'f', long)]
    pub fix: bool,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Run local installation and runtime health checks.
    #[command(
        name = "doctor",
        about = "Run local installation and runtime health checks.",
        long_about = "Run local installation and runtime health checks. Doctor checks platform, runtime-home writability, standard Tentgent directories, Python runtime assets, Python entry points, developer uv availability, media decoder availability, and backend capability states. By default it reports findings without installing dependencies. Add --fix only for the current developer bootstrap path; release installers must not require users to preinstall uv."
    )]
    Doctor(DoctorCommand),
    /// Inspect and prepare runtime support assets.
    #[command(
        name = "runtime",
        about = "Inspect and prepare runtime support assets.",
        long_about = "Inspect and prepare runtime support assets. Use `tentgent runtime bootstrap` after package-manager installs to create or sync the managed Python runtime without invoking packaged support scripts directly."
    )]
    Runtime {
        #[command(subcommand)]
        action: RuntimeCommands,
    },
    /// Inspect and manage registered models and downloaded runtimes.
    #[command(
        name = "model",
        about = "Import, pull, list, and inspect managed models.",
        long_about = "Import, pull, list, inspect, and remove managed models. Tentgent stores managed models under TENTGENT_HOME/models, computes a content-derived model reference, and reuses that reference for deduplication."
    )]
    Model {
        #[command(subcommand)]
        action: ModelCommands,
    },
    /// Run a one-shot chat request through the Python runtime harness.
    #[command(
        name = "chat",
        about = "Run a one-shot chat request through the Python runtime harness.",
        long_about = "Run a one-shot chat request through the Python runtime harness. Tentgent resolves the stored model reference, routes to the appropriate backend, and forwards generation to the Python subproject. With no --message entries, Tentgent prompts once for a user message."
    )]
    Chat(ChatCommand),
    /// Run one-shot local embedding inference.
    #[command(
        name = "embed",
        visible_alias = "embedding",
        about = "Run one-shot local embedding inference.",
        long_about = "Run one-shot local embedding inference without starting the daemon. The command resolves a stored embedding-capable model, calls the Python embedding runtime once, and prints a JSON response matching daemon /v1/embeddings."
    )]
    Embed(EmbedCommand),
    /// Run one-shot local rerank inference.
    #[command(
        name = "rerank",
        about = "Run one-shot local rerank inference.",
        long_about = "Run one-shot local rerank inference without starting the daemon. The command resolves a stored rerank-capable model, calls the Python rerank runtime once, and prints a JSON response matching daemon /v1/rerank."
    )]
    Rerank(RerankCommand),
    /// Transcribe a local audio file with a local audio-transcription model.
    #[command(
        name = "transcribe",
        about = "Transcribe a local audio file with a local audio-transcription model.",
        long_about = "Transcribe a local audio file in the foreground without starting the daemon. The command resolves a stored audio-transcription model, calls the Python audio runtime once, and writes text, JSON, WebVTT, or SRT output. With --output it writes only to the requested file and fails if that file already exists."
    )]
    Transcribe(TranscribeCommand),
    /// Synthesize a local WAV file from text with a local audio-speech model.
    #[command(
        name = "speak",
        about = "Synthesize a local WAV file from text with a local audio-speech model.",
        long_about = "Synthesize a local WAV file in the foreground without starting the daemon. The command resolves a stored audio-speech model, calls the Python audio speech runtime once, and writes a wav output file. Existing output files are never overwritten."
    )]
    Speak(SpeakCommand),
    /// Run local image-plus-text vision chat.
    #[command(
        name = "vision",
        about = "Run local image-plus-text vision workflows.",
        long_about = "Run local image-plus-text vision workflows. The first supported workflow is `tentgent vision chat`, which resolves a stored vision-chat model and asks one prompt about one local image."
    )]
    Vision {
        #[command(subcommand)]
        action: VisionCommands,
    },
    /// Run local video understanding workflows.
    #[command(
        name = "video",
        about = "Run local video understanding workflows.",
        long_about = "Run local video understanding workflows. The first supported workflow is `tentgent video understand`, which samples bounded frames from one local video, resolves a stored video-understanding model, and asks one prompt about the video."
    )]
    Video {
        #[command(subcommand)]
        action: VideoCommands,
    },
    /// Run local image generation workflows.
    #[command(
        name = "image",
        about = "Run local image generation workflows.",
        long_about = "Run local image generation workflows. The first supported workflow is `tentgent image generate`, which resolves a stored image-generation model and writes one generated image file from one text prompt."
    )]
    Image {
        #[command(subcommand)]
        action: ImageCommands,
    },
    /// Inspect and manage adapters, including LoRA selection and switching.
    #[command(
        name = "adapter",
        about = "Inspect and manage adapters.",
        long_about = "Inspect and manage adapters such as LoRA assets. Tentgent stores managed adapters under TENTGENT_HOME/adapters and later uses adapter references for server-side selection."
    )]
    Adapter {
        #[command(subcommand)]
        action: AdapterCommands,
    },
    /// Inspect and clean managed store maintenance state.
    #[command(
        name = "store",
        about = "Inspect and clean managed store maintenance state.",
        long_about = "Inspect and clean managed store maintenance state under TENTGENT_HOME. This does not remove hashed model, adapter, or dataset content; use the specific model, adapter, or dataset rm commands for canonical store objects."
    )]
    Store {
        #[command(subcommand)]
        action: StoreCommands,
    },
    /// Inspect and manage datasets for training and evaluation.
    #[command(
        name = "dataset",
        about = "Inspect and manage datasets.",
        long_about = "Inspect and manage datasets for future training and evaluation workflows. Tentgent stores local datasets under TENTGENT_HOME/datasets, computes content-derived dataset references, and reuses those references for deduplication."
    )]
    Dataset {
        #[command(subcommand)]
        action: DatasetCommands,
    },
    /// Plan and run training workflows.
    #[command(
        name = "train",
        about = "Plan and run training workflows.",
        long_about = "Plan and run training workflows. The current MVP exposes LoRA plan creation and run records. MLX plans execute through the Python MLX runner; safetensors plans execute through the Python PEFT runner."
    )]
    Train {
        #[command(subcommand)]
        action: TrainCommands,
    },
    /// Inspect and manage the persistent local daemon process.
    #[command(
        name = "daemon",
        about = "Inspect and manage the persistent local daemon process.",
        long_about = "Inspect and manage the persistent local daemon process. The daemon is the future local HTTP subsystem entry point for integrations that should not shell out to individual CLI commands."
    )]
    Daemon {
        #[command(subcommand)]
        action: DaemonCommands,
    },
    /// Define and launch stable local model server proxies.
    #[command(
        name = "server",
        about = "Define and launch stable local model server proxies.",
        long_about = "Define, launch, inspect, and control stable local model server proxies. Tentgent persists stable server specs under the runtime home, can launch a server proxy in foreground or background mode, and exposes registry-style commands such as `ls`, `ps`, `inspect`, `start`, `stop`, and `rm`."
    )]
    Server {
        #[command(subcommand)]
        action: ServerCommands,
    },
    /// Internal cloud server runtime worker.
    #[command(name = "__cloud-server-runtime", hide = true)]
    CloudServerRuntime(CloudServerRuntimeCommand),
    /// Internal local server proxy runtime worker.
    #[command(name = "__local-server-runtime", hide = true)]
    LocalServerRuntime(LocalServerRuntimeCommand),
    /// Inspect and manage local chat sessions.
    #[command(
        name = "session",
        about = "Inspect and manage local chat sessions.",
        long_about = "Inspect and manage local chat sessions. Sessions are durable transcript records stored under TENTGENT_HOME/sessions and are separate from training datasets."
    )]
    Session {
        #[command(subcommand)]
        action: SessionCommands,
    },
    /// Inspect and manage provider authentication keys.
    #[command(
        name = "auth",
        about = "Inspect and manage provider authentication keys.",
        long_about = "Inspect and manage provider authentication keys. Use this group to check whether a provider key is available, persist a key in the system keychain, or remove an existing stored key."
    )]
    Auth {
        #[command(subcommand)]
        subject: AuthCommands,
    },
}
