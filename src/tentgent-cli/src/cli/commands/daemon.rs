use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub enum DaemonCommands {
    /// Run the local Tentgent daemon process in the foreground.
    #[command(
        name = "run",
        about = "Run the local Tentgent daemon process in the foreground.",
        long_about = "Run the local Tentgent daemon process in the foreground.\n\n`--home` points to the Tentgent runtime home, not the repository workspace.\n`--host` and `--port` define the HTTP bind address. Loopback binds can run without auth for development. Non-loopback or wildcard binds require `TENTGENT_DAEMON_TOKEN` unless `--allow-unsafe-bind` is passed.\n\nAdd `--detach` to launch the daemon in background mode and return after readiness checks."
    )]
    Run(DaemonRunCommand),
    /// Start the local Tentgent daemon process in background mode.
    #[command(
        name = "start",
        about = "Start the local Tentgent daemon process in background mode.",
        long_about = "Start the local Tentgent daemon process in background mode and return after readiness checks.\n\n`start` uses the same detached-launch implementation as `daemon run --detach`. `--home` points to the Tentgent runtime home, not the repository workspace. `--host` and `--port` define the HTTP bind address. Loopback binds can run without auth for development. Non-loopback or wildcard binds require `TENTGENT_DAEMON_TOKEN` unless `--allow-unsafe-bind` is passed."
    )]
    Start(DaemonStartCommand),
    /// Show the current local daemon process state.
    #[command(
        name = "status",
        about = "Show the current local daemon process state.",
        long_about = "Show the current local daemon process state. This checks the daemon metadata under the Tentgent runtime directory and cleans up stale pid metadata when the process has exited."
    )]
    Status {
        /// Optional Tentgent runtime home override for daemon state lookup.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
    },
    /// Stop the running local daemon process.
    #[command(
        name = "stop",
        about = "Stop the running local daemon process.",
        long_about = "Stop the running local daemon process. This sends TERM to the process recorded under the Tentgent runtime directory and removes matching process metadata after shutdown."
    )]
    Stop {
        /// Optional Tentgent runtime home override for daemon state lookup.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
    },
}

#[derive(Debug, Args)]
pub struct DaemonRunCommand {
    /// Optional Tentgent runtime home override for daemon state.
    #[arg(short = 'H', long, value_name = "HOME")]
    pub home: Option<PathBuf>,
    /// Host interface for the future HTTP listener.
    #[arg(short = 'a', long, value_name = "HOST")]
    pub host: Option<String>,
    /// TCP port for the future HTTP listener.
    #[arg(short = 'p', long, value_name = "PORT")]
    pub port: Option<u16>,
    /// Allow binding to non-loopback or wildcard hosts without a daemon token.
    #[arg(long)]
    pub allow_unsafe_bind: bool,
    /// Launch the daemon in background mode and return after readiness checks.
    #[arg(long)]
    pub detach: bool,
}

#[derive(Debug, Args)]
pub struct DaemonStartCommand {
    /// Optional Tentgent runtime home override for daemon state.
    #[arg(short = 'H', long, value_name = "HOME")]
    pub home: Option<PathBuf>,
    /// Host interface for the future HTTP listener.
    #[arg(short = 'a', long, value_name = "HOST")]
    pub host: Option<String>,
    /// TCP port for the future HTTP listener.
    #[arg(short = 'p', long, value_name = "PORT")]
    pub port: Option<u16>,
    /// Allow binding to non-loopback or wildcard hosts without a daemon token.
    #[arg(long)]
    pub allow_unsafe_bind: bool,
}
