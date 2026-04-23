mod auth;
mod chat;
mod model;
mod server;

pub use auth::{AuthCommands, AuthProviderAction};
pub use chat::ChatCommand;
pub use model::ModelCommands;
pub use server::{ServerCommands, ServerRunCommand};

use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Show the current daemon, runtime, model, and adapter status.
    Status,
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
    Adapter,
    /// Inspect and manage the persistent local daemon process.
    Daemon,
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
