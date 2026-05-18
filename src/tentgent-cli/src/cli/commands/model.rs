use std::path::PathBuf;

use clap::Subcommand;
use tentgent_kernel::features::model::domain::ModelCapability;

#[derive(Debug, Subcommand)]
pub enum ModelCommands {
    /// Import a local file or directory into the managed model store.
    #[command(
        name = "add",
        about = "Import a local file or directory into the managed model store.",
        long_about = "Import a local file or directory into the managed model store. Tentgent copies the source into staging, builds a manifest, computes a content-derived model reference, and deduplicates if the same content already exists."
    )]
    Add {
        /// Local model file or directory to import.
        #[arg(value_name = "PATH")]
        path: PathBuf,
        /// Serving capability to assign to this model.
        #[arg(long, value_name = "chat|embedding|rerank")]
        capability: Option<ModelCapability>,
    },
    /// Pull a full model snapshot from Hugging Face into the managed store.
    #[command(
        name = "pull",
        about = "Pull a full model snapshot from Hugging Face into the managed store.",
        long_about = "Pull a full model snapshot from Hugging Face into the managed store. Tentgent resolves the repository revision first, downloads that exact snapshot, computes a content-derived model reference, and deduplicates identical content."
    )]
    Pull {
        /// Hugging Face model repository id, such as owner/name.
        #[arg(value_name = "MODEL_FULL_NAME")]
        repo_id: String,
        /// Hugging Face revision to resolve before downloading.
        #[arg(short = 'r', long, value_name = "REV")]
        revision: Option<String>,
        /// Serving capability to assign to this model.
        #[arg(long, value_name = "chat|embedding|rerank")]
        capability: Option<ModelCapability>,
    },
    /// List managed models in the local Tentgent model store.
    #[command(
        name = "ls",
        visible_alias = "list",
        about = "List managed models in the local Tentgent model store.",
        long_about = "List managed models in the local Tentgent model store. Each row shows the short reference, primary format, source kind, and source summary."
    )]
    Ls,
    /// Remove one managed model and its source indexes by hash reference.
    #[command(
        name = "rm",
        visible_alias = "remove",
        about = "Remove one managed model and its source indexes by hash reference.",
        long_about = "Remove one managed model and its source indexes by hash reference. Tentgent accepts either the full model_ref hash or a unique short_ref hash prefix, deletes the canonical store directory under TENTGENT_HOME/models/store/<model_ref>, and removes matching local and Hugging Face source index entries."
    )]
    Rm {
        /// Full model_ref or unique short-ref prefix.
        #[arg(value_name = "HASH")]
        hash: String,
    },
    /// Inspect one managed model by full or short reference.
    #[command(
        name = "inspect",
        about = "Inspect one managed model by full or short reference.",
        long_about = "Inspect one managed model by full or short reference. Tentgent accepts either the full model_ref or a unique short_ref prefix and prints metadata, source details, and store paths."
    )]
    Inspect {
        /// Full model_ref or unique short-ref prefix.
        #[arg(value_name = "REF")]
        reference: String,
    },
    /// Correct stored model capability metadata.
    #[command(
        name = "set-capability",
        about = "Correct stored model capability metadata.",
        long_about = "Correct stored model capability metadata without changing model content or model_ref. Tentgent accepts either the full model_ref or a unique short_ref prefix."
    )]
    SetCapability {
        /// Full model_ref or unique short-ref prefix.
        #[arg(value_name = "REF")]
        reference: String,
        /// Serving capability to assign to this model.
        #[arg(value_name = "chat|embedding|rerank")]
        capability: ModelCapability,
    },
}
