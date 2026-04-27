use std::path::PathBuf;

use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum AdapterCommands {
    /// Import a local adapter directory into the managed adapter store.
    #[command(
        name = "add",
        about = "Import a local adapter directory into the managed adapter store.",
        long_about = "Import a local adapter directory into the managed adapter store. The first implementation targets PEFT-style LoRA adapter directories and will store managed adapter metadata under TENTGENT_HOME/adapters."
    )]
    Add {
        #[arg(value_name = "PATH")]
        path: PathBuf,
        /// Local base model reference this adapter was trained for.
        #[arg(long = "base-model-ref", value_name = "MODEL_REF")]
        base_model_ref: Option<String>,
    },
    /// Pull an adapter snapshot from Hugging Face into the managed adapter store.
    #[command(
        name = "pull",
        about = "Pull an adapter snapshot from Hugging Face into the managed adapter store.",
        long_about = "Pull an adapter snapshot from Hugging Face into the managed adapter store. Tentgent resolves the repository revision first, downloads that exact snapshot, computes a content-derived adapter reference, and deduplicates identical content.",
        override_usage = "tentgent adapter pull <HF_REPO> [--revision <REV>] [--base-model-ref <MODEL_REF>]"
    )]
    Pull {
        #[arg(value_name = "HF_REPO")]
        repo_id: String,
        #[arg(long, value_name = "REV")]
        revision: Option<String>,
        /// Local base model reference this adapter was trained for.
        #[arg(long = "base-model-ref", value_name = "MODEL_REF")]
        base_model_ref: Option<String>,
    },
    /// List managed adapters.
    #[command(
        name = "ls",
        about = "List managed adapters.",
        long_about = "List managed adapters stored under TENTGENT_HOME/adapters."
    )]
    Ls,
    /// Inspect one managed adapter.
    #[command(
        name = "inspect",
        about = "Inspect one managed adapter.",
        long_about = "Inspect one managed adapter by full adapter_ref or unique short-ref prefix.",
        override_usage = "tentgent adapter inspect <ADAPTER_REF>"
    )]
    Inspect {
        #[arg(value_name = "ADAPTER_REF")]
        reference: String,
    },
    /// Bind an adapter to one local managed base model.
    #[command(
        name = "bind",
        about = "Bind an adapter to one local managed base model.",
        long_about = "Bind an adapter to one local managed base model. Tentgent validates adapter_config.json base-model hints when available and writes a by-base index for later server compatibility checks.",
        override_usage = "tentgent adapter bind <ADAPTER_REF> --base-model-ref <MODEL_REF>"
    )]
    Bind {
        #[arg(value_name = "ADAPTER_REF")]
        adapter_ref: String,
        /// Local managed base model reference this adapter should target.
        #[arg(long = "base-model-ref", value_name = "MODEL_REF")]
        base_model_ref: Option<String>,
    },
    /// Remove one managed adapter by hash reference.
    #[command(
        name = "rm",
        visible_alias = "remove",
        about = "Remove one managed adapter by hash reference.",
        long_about = "Remove one managed adapter by hash reference. Tentgent accepts either the full adapter_ref hash or a unique short_ref hash prefix, deletes the canonical store directory under TENTGENT_HOME/adapters/store/<adapter_ref>, and removes matching source and base-model index entries.",
        override_usage = "tentgent adapter rm <ADAPTER_REF>"
    )]
    Rm {
        #[arg(value_name = "ADAPTER_REF")]
        reference: String,
    },
}
