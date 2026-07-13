use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub enum ClusterCommands {
    /// Create and launch a server backed by one stored cluster definition.
    #[command(
        name = "run",
        about = "Create and launch a server backed by one stored cluster definition.",
        long_about = "Create or reuse one stored server spec targeting a cluster and launch it immediately. The cluster must declare a local-model `routes.chat` target. Other configured routes are selected by HTTP endpoint family. The resulting process remains visible through `tentgent server ls`, `inspect`, `stop`, and `rm`."
    )]
    Run(ClusterRunCommand),
    /// Validate and replace one stored cluster definition from TOML.
    #[command(
        name = "apply",
        about = "Validate and replace one stored cluster definition from TOML.",
        long_about = "Validate and replace one stored cluster definition from a TOML file. The file must contain `schema_version`, `cluster_ref`, and route tables such as `[routes.chat]`. Apply is a full-definition replacement: routes omitted from the file are unbound in stored state."
    )]
    Apply {
        /// Path to a cluster TOML definition file.
        #[arg(value_name = "CLUSTER_TOML")]
        path: PathBuf,
        /// Optional Tentgent runtime home override.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
        /// Allow reading TOML from an obvious secret-bearing path.
        #[arg(long)]
        force: bool,
    },
    /// Validate one cluster TOML definition without writing it.
    #[command(
        name = "validate",
        about = "Validate one cluster TOML definition without writing it.",
        long_about = "Validate one cluster TOML definition without writing it. This checks schema, cluster_ref, route keys, referenced models or providers, capability compatibility, and optional runtime profile references."
    )]
    Validate {
        /// Path to a cluster TOML definition file.
        #[arg(value_name = "CLUSTER_TOML")]
        path: PathBuf,
        /// Optional Tentgent runtime home override.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
        /// Allow reading TOML from an obvious secret-bearing path.
        #[arg(long)]
        force: bool,
    },
    /// List stored cluster definitions.
    #[command(
        name = "ls",
        visible_alias = "list",
        about = "List stored cluster definitions.",
        long_about = "List stored cluster definitions under TENTGENT_HOME/clusters."
    )]
    Ls {
        /// Optional Tentgent runtime home override.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
    },
    /// Show one stored cluster definition and route readiness.
    #[command(
        name = "inspect",
        about = "Show one stored cluster definition and route readiness.",
        long_about = "Show one stored cluster definition, its canonical TOML path, and read-only route readiness. Inspect reports support/auth status, inferred runtime profiles, problem flags, and next actions without starting runtimes or reading provider secrets. Cluster refs are exact lowercase slugs such as `local-assistant`."
    )]
    Inspect {
        /// Stored cluster ref to inspect, for example `local-assistant`.
        ///
        /// This is the `cluster_ref` from the stored cluster TOML, not a file
        /// path and not a model ref.
        #[arg(value_name = "CLUSTER_REF")]
        cluster_ref: String,
        /// Optional Tentgent runtime home override.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
    },
    /// Remove one stored cluster definition.
    #[command(
        name = "rm",
        visible_alias = "remove",
        about = "Remove one stored cluster definition.",
        long_about = "Remove one stored cluster definition under TENTGENT_HOME/clusters/<cluster_ref>."
    )]
    Rm {
        /// Cluster ref.
        #[arg(value_name = "CLUSTER_REF")]
        cluster_ref: String,
        /// Optional Tentgent runtime home override.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
    },
}

#[derive(Debug, Args)]
pub struct ClusterRunCommand {
    /// Stored cluster ref, for example `local-assistant`.
    #[arg(value_name = "CLUSTER_REF")]
    pub cluster_ref: String,
    /// Optional Tentgent runtime home override for cluster and server state.
    #[arg(short = 'H', long, value_name = "HOME")]
    pub home: Option<PathBuf>,
    /// Host interface for the cluster HTTP listener.
    #[arg(short = 'a', long, value_name = "HOST")]
    pub host: Option<String>,
    /// Fixed TCP port. Omit to auto-scan from 8780.
    #[arg(short = 'p', long, value_name = "PORT")]
    pub port: Option<u16>,
    /// Record the shared server lazy-load preference in the stored spec.
    #[arg(short = 'l', long)]
    pub lazy_load: bool,
    /// Auto-release each loaded model runtime after N idle seconds.
    #[arg(short = 'i', long = "idle-seconds", value_name = "N")]
    pub idle_seconds: Option<u64>,
    /// Allow unknown or stale local route support evidence for this launch.
    #[arg(long)]
    pub allow_unverified: bool,
    /// Launch the initial cluster server process in background mode.
    #[arg(short = 'd', long)]
    pub detach: bool,
}

#[derive(Debug, Args)]
pub struct ClusterServerRuntimeCommand {
    #[arg(long, value_name = "SERVER_REF")]
    pub server_ref: String,
    #[arg(long, value_name = "CLUSTER_REF")]
    pub cluster_ref: String,
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
    #[arg(long)]
    pub allow_unverified: bool,
}
