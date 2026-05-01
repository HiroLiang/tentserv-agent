mod adapter;
mod auth;
mod chat;
mod daemon;
mod dataset;
mod model;
mod server;
mod train;

pub use adapter::AdapterCommands;
pub use auth::{AuthCommands, AuthProviderAction};
pub use chat::ChatCommand;
pub use daemon::{DaemonCommands, DaemonRunCommand};
pub use dataset::DatasetCommands;
pub use model::ModelCommands;
pub use server::{ServerCommands, ServerRunCommand};
pub use train::{
    TrainCommands, TrainLoraCommands, TrainLoraPlanCommands, TrainLoraRunCommand,
    TrainLoraRunWorkerCommand,
};

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct DoctorCommand {
    /// Developer bootstrap: create or sync the managed Python environment with uv before checking health.
    #[arg(short = 'f', long)]
    pub fix: bool,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Show current runtime paths and Python asset resolution.
    #[command(
        name = "status",
        about = "Show current runtime paths and Python asset resolution.",
        long_about = "Show current runtime paths and Python asset resolution. This is the first packaging diagnostic surface: it reports the runtime home, Python project source, managed Python environment, and key Python entry points."
    )]
    Status,
    /// Run local installation and runtime health checks.
    #[command(
        name = "doctor",
        about = "Run local installation and runtime health checks.",
        long_about = "Run local installation and runtime health checks. Doctor checks platform, runtime-home writability, standard Tentgent directories, Python runtime assets, Python entry points, developer uv availability, and backend capability states. By default it reports findings without installing dependencies. Add --fix only for the current developer bootstrap path; release installers must not require users to preinstall uv."
    )]
    Doctor(DoctorCommand),
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
    /// Define and launch long-lived local model servers.
    #[command(
        name = "server",
        about = "Define and launch long-lived local model servers.",
        long_about = "Define, launch, inspect, and control long-lived local model servers. Tentgent persists stable server specs under the runtime home, can launch a server in foreground or background mode, and exposes registry-style commands such as `ls`, `ps`, `inspect`, `start`, `stop`, and `rm`."
    )]
    Server {
        #[command(subcommand)]
        action: ServerCommands,
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
