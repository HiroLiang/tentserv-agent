use std::path::PathBuf;

use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum ClusterCommands {
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
    /// Show one stored cluster definition.
    #[command(
        name = "inspect",
        about = "Show one stored cluster definition.",
        long_about = "Show one stored cluster definition and its canonical TOML path. Cluster refs are exact lowercase slugs such as `local-assistant`."
    )]
    Inspect {
        /// Cluster ref.
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
