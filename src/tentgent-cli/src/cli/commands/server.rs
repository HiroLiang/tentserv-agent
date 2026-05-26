use std::path::PathBuf;

use clap::{Args, Subcommand};
use tentgent_kernel::features::server::domain::ServerCapability;

#[derive(Debug, Args)]
pub struct CloudServerRuntimeCommand {
    #[arg(long, value_name = "SERVER_REF")]
    pub server_ref: String,
    #[arg(long, value_name = "PROVIDER")]
    pub provider: String,
    #[arg(long, value_name = "MODEL")]
    pub provider_model: String,
    #[arg(long, value_name = "HOST")]
    pub host: String,
    #[arg(long, value_name = "PORT")]
    pub port: u16,
    #[arg(long, value_name = "HOME")]
    pub home: Option<PathBuf>,
    #[arg(long)]
    pub lazy_load: bool,
    #[arg(long = "idle-seconds", value_name = "N")]
    pub idle_seconds: Option<u64>,
}

#[derive(Debug, Args)]
pub struct LocalServerRuntimeCommand {
    #[arg(long, value_name = "SERVER_REF")]
    pub server_ref: String,
    #[arg(long, value_name = "CAPABILITY")]
    pub capability: String,
    #[arg(long, value_name = "MODEL_REF")]
    pub model_ref: String,
    #[arg(long, value_name = "HOST")]
    pub host: String,
    #[arg(long, value_name = "PORT")]
    pub port: u16,
    #[arg(long, value_name = "HOME")]
    pub home: Option<PathBuf>,
    #[arg(long)]
    pub lazy_load: bool,
    #[arg(long = "idle-seconds", value_name = "N")]
    pub idle_seconds: Option<u64>,
}

#[derive(Debug, Subcommand)]
pub enum ServerCommands {
    /// Create a server spec and launch it in foreground mode by default.
    #[command(
        name = "run",
        about = "Create a server spec and launch it in foreground mode by default.",
        long_about = "Create or reuse one stored server spec for a local model reference or cloud runtime reference and launch it immediately. `RUNTIME_REF` can be a full Tentgent model reference, a unique short-ref prefix, `openai:<MODEL_NAME>`, `anthropic:<MODEL_NAME>`, or `claude:<MODEL_NAME>`.\n\n`--home` points to the Tentgent runtime home, not the repository workspace.\n`--host` and `--port` define the HTTP bind address. When `--port` is omitted, Tentgent starts scanning at 8780 and records the actual bound port in process metadata.\n`--capability` selects the endpoint family. When omitted for a local model, Tentgent infers it from stored model capabilities.\n`--lazy-load` preserves the server spec preference while the local proxy starts or reuses the shared Python runtime on demand.\n`--idle-seconds` becomes the shared Python runtime idle shutdown policy if this proxy starts that runtime.\n`--detach` launches the initial server process in background mode and returns immediately."
    )]
    Run(ServerRunCommand),
    /// List registered server specs and their current runtime state.
    #[command(
        name = "ls",
        visible_alias = "list",
        about = "List registered server specs and their current runtime state.",
        long_about = "List registered server specs and their current runtime state. This includes stored servers that are currently stopped."
    )]
    Ls {
        /// Optional Tentgent runtime home override for server state lookup.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
    },
    /// List only live server processes.
    #[command(
        name = "ps",
        about = "List only live server processes.",
        long_about = "List only live server processes. This command reads Tentgent server specs, checks process metadata, and filters to currently running servers."
    )]
    Ps {
        /// Optional Tentgent runtime home override for server state lookup.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
    },
    /// Show one stored server spec together with runtime state and log paths.
    #[command(
        name = "inspect",
        about = "Show one stored server spec together with runtime state and log paths.",
        long_about = "Show one stored server spec together with runtime state and log paths. Tentgent accepts either the full server_ref or a unique short_ref prefix."
    )]
    Inspect {
        /// Full server_ref or unique short-ref prefix.
        #[arg(value_name = "SERVER_REF")]
        reference: String,
        /// Optional Tentgent runtime home override for server state lookup.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
    },
    /// Launch one stored server spec in background mode.
    #[command(
        name = "start",
        about = "Launch one stored server spec in background mode.",
        long_about = "Launch one stored server spec in background mode. Tentgent accepts either the full server_ref or a unique short_ref prefix, starts the server process with stdout and stderr redirected to server-local log files, and returns immediately.\n\nBy default `start` prints a concise status summary. Add `--details` to include the full inspection table."
    )]
    Start {
        /// Full server_ref or unique short-ref prefix.
        #[arg(value_name = "SERVER_REF")]
        reference: String,
        /// Optional Tentgent runtime home override for server state lookup.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
        /// Show the full inspection table after the server starts.
        #[arg(short = 'd', long)]
        details: bool,
    },
    /// Stop one live server process without deleting its stored spec.
    #[command(
        name = "stop",
        about = "Stop one live server process without deleting its stored spec.",
        long_about = "Stop one live server process without deleting its stored spec. Tentgent accepts either the full server_ref or a unique short_ref prefix.\n\nBy default `stop` prints a concise status summary. Add `--details` to include the full inspection table after shutdown."
    )]
    Stop {
        /// Full server_ref or unique short-ref prefix.
        #[arg(value_name = "SERVER_REF")]
        reference: String,
        /// Optional Tentgent runtime home override for server state lookup.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
        /// Show the full inspection table after the server stops.
        #[arg(short = 'd', long)]
        details: bool,
    },
    /// Remove one stored server spec after it is stopped.
    #[command(
        name = "rm",
        visible_alias = "remove",
        about = "Remove one stored server spec after it is stopped.",
        long_about = "Remove one stored server spec after it is stopped. Tentgent accepts either the full server_ref or a unique short_ref prefix and removes the full server directory under TENTGENT_HOME/servers/<server_ref>.\n\nBy default `rm` prints a concise status summary. Add `--details` to include the full inspection table before removal."
    )]
    Rm {
        /// Full server_ref or unique short-ref prefix.
        #[arg(value_name = "SERVER_REF")]
        reference: String,
        /// Optional Tentgent runtime home override for server state lookup.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
        /// Show the full inspection table captured before the server is removed.
        #[arg(short = 'd', long)]
        details: bool,
    },
}

#[derive(Debug, Args)]
pub struct ServerRunCommand {
    /// Local model ref, or cloud runtime ref such as openai:gpt-4.1-mini.
    #[arg(value_name = "RUNTIME_REF")]
    pub runtime_ref: String,
    /// Optional Tentgent runtime home override for server state and model lookup.
    #[arg(short = 'H', long, value_name = "HOME")]
    pub home: Option<PathBuf>,
    /// Host interface for the future HTTP listener.
    #[arg(short = 'a', long, value_name = "HOST")]
    pub host: Option<String>,
    /// Fixed TCP port for the HTTP listener. Omit to auto-scan from 8780.
    #[arg(short = 'p', long, value_name = "PORT")]
    pub port: Option<u16>,
    /// Delay model loading until the first request arrives.
    #[arg(short = 'l', long)]
    pub lazy_load: bool,
    /// Auto-release the loaded model after N idle seconds.
    #[arg(short = 'i', long = "idle-seconds", value_name = "N")]
    pub idle_seconds: Option<u64>,
    /// Endpoint family to serve from the selected runtime.
    #[arg(long, value_name = "CAPABILITY")]
    pub capability: Option<ServerCapability>,
    /// Launch the initial server process in background mode and return immediately.
    #[arg(short = 'd', long)]
    pub detach: bool,
}
