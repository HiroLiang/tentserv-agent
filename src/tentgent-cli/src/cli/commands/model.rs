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
        /// Capability metadata to assign to this model.
        #[arg(long, value_name = "CAPABILITY")]
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
        /// Capability metadata to assign to this model.
        #[arg(long, value_name = "CAPABILITY")]
        capability: Option<ModelCapability>,
    },
    /// List built-in model support catalog entries.
    #[command(
        name = "catalog",
        visible_alias = "recommend",
        about = "List built-in model support catalog entries.",
        long_about = "List built-in model support catalog entries. The catalog includes curated local fixtures, known model families, large external-runtime candidates, and MLX conversion patterns. Use filters such as `--capability chat`, `--publisher Qwen`, `--support-level fixture-supported`, `--local`, or `--query nemotron` to narrow the list. The output includes a pull command template and descriptions for the capabilities present in the filtered results."
    )]
    Catalog {
        /// Filter by Tentgent endpoint capability.
        #[arg(long, value_name = "CAPABILITY")]
        capability: Option<ModelCapability>,
        /// Filter by publisher text, case-insensitive.
        #[arg(long, value_name = "PUBLISHER")]
        publisher: Option<String>,
        /// Filter by catalog support level.
        #[arg(
            long,
            value_name = "LEVEL",
            help = "Filter by support level: fixture-supported, local-runtime-supported, catalog-known, requires-external-runtime, known-unsupported, deprecated"
        )]
        support_level: Option<String>,
        /// Show only entries that can become local support hints.
        #[arg(long)]
        local: bool,
        /// Search publisher, family, source, tags, and recommendation text.
        #[arg(short = 'q', long, value_name = "TEXT")]
        query: Option<String>,
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
    /// Show or mutate stored model capability metadata.
    #[command(
        name = "capability",
        about = "Show or mutate stored model capability metadata.",
        long_about = "Show or mutate stored model capability metadata without changing model content or model_ref. Tentgent accepts either the full model_ref or a unique short_ref prefix."
    )]
    Capability {
        #[command(subcommand)]
        action: ModelCapabilityCommands,
    },
    /// Correct stored model capability metadata.
    #[command(
        name = "set-capability",
        hide = true,
        about = "Correct stored model capability metadata.",
        long_about = "Correct stored model capability metadata without changing model content or model_ref. Tentgent accepts either the full model_ref or a unique short_ref prefix."
    )]
    SetCapability {
        /// Full model_ref or unique short-ref prefix.
        #[arg(value_name = "REF")]
        reference: String,
        /// Capability metadata to assign to this model.
        #[arg(value_name = "CAPABILITY")]
        capability: ModelCapability,
    },
}

#[derive(Debug, Subcommand)]
pub enum ModelCapabilityCommands {
    /// Show stored capability metadata for one model.
    #[command(
        name = "show",
        about = "Show stored capability metadata for one model.",
        long_about = "Show stored capability metadata for one model without changing model content."
    )]
    Show {
        /// Full model_ref or unique short-ref prefix.
        #[arg(value_name = "REF")]
        reference: String,
    },
    /// Replace stored capability metadata for one model.
    #[command(
        name = "set",
        about = "Replace stored capability metadata for one model.",
        long_about = "Replace stored capability metadata for one model without changing model content or model_ref."
    )]
    Set {
        /// Full model_ref or unique short-ref prefix.
        #[arg(value_name = "REF")]
        reference: String,
        /// Capability metadata to assign to this model.
        #[arg(value_name = "CAPABILITY", num_args = 1..)]
        capabilities: Vec<ModelCapability>,
    },
    /// Add stored capability metadata to one model.
    #[command(
        name = "add",
        about = "Add stored capability metadata to one model.",
        long_about = "Add stored capability metadata to one model without changing model content or model_ref."
    )]
    Add {
        /// Full model_ref or unique short-ref prefix.
        #[arg(value_name = "REF")]
        reference: String,
        /// Capability metadata to add to this model.
        #[arg(value_name = "CAPABILITY", num_args = 1..)]
        capabilities: Vec<ModelCapability>,
    },
    /// Remove stored capability metadata from one model.
    #[command(
        name = "remove",
        visible_alias = "rm",
        about = "Remove stored capability metadata from one model.",
        long_about = "Remove stored capability metadata from one model without changing model content or model_ref. Tentgent rejects mutations that would leave the model with no capabilities."
    )]
    Remove {
        /// Full model_ref or unique short-ref prefix.
        #[arg(value_name = "REF")]
        reference: String,
        /// Capability metadata to remove from this model.
        #[arg(value_name = "CAPABILITY", num_args = 1..)]
        capabilities: Vec<ModelCapability>,
    },
    /// List latest proof records for one model.
    #[command(
        name = "proofs",
        about = "List latest proof records for one model.",
        long_about = "List latest proof records for one model capability set. Proofs are local metadata records written by manual probes and runtime events such as server start."
    )]
    Proofs {
        /// Full model_ref or unique short-ref prefix.
        #[arg(value_name = "REF")]
        reference: String,
    },
    /// Manage stored proof records for one model capability.
    #[command(
        name = "proof",
        about = "Manage stored proof records for one model capability.",
        long_about = "Manage stored proof records for one model capability. Use this when a failed local proof should be cleared after fixing the runtime environment."
    )]
    Proof {
        #[command(subcommand)]
        action: ModelCapabilityProofCommands,
    },
    /// Run a local capability probe and write a proof record.
    #[command(
        name = "verify",
        about = "Run a local capability probe and write a proof record.",
        long_about = "Run a local metadata-level capability probe and write a proof record without changing model content or model_ref. Runtime endpoint smoke proofs are recorded by server/runtime flows."
    )]
    Verify {
        /// Full model_ref or unique short-ref prefix.
        #[arg(value_name = "REF")]
        reference: String,
        /// Capability metadata to probe.
        #[arg(value_name = "CAPABILITY")]
        capability: ModelCapability,
    },
}

#[derive(Debug, Subcommand)]
pub enum ModelCapabilityProofCommands {
    /// Clear stored proof records for one model capability.
    #[command(
        name = "clear",
        visible_alias = "rm",
        about = "Clear stored proof records for one model capability.",
        long_about = "Clear stored proof records for one model capability. This removes local verified or failed proof evidence for the selected capability without changing model capability metadata or model content."
    )]
    Clear {
        /// Full model_ref or unique short-ref prefix.
        #[arg(value_name = "REF")]
        reference: String,
        /// Capability proof records to clear.
        #[arg(value_name = "CAPABILITY")]
        capability: ModelCapability,
    },
}
