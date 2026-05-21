use std::path::PathBuf;

use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum AdapterCommands {
    /// Import a local adapter directory into the managed adapter store.
    #[command(
        name = "add",
        about = "Import a local adapter directory into the managed adapter store.",
        long_about = "Import a local adapter directory into the managed adapter store. Tentgent stores managed adapter metadata under TENTGENT_HOME/adapters and supports PEFT chat adapters plus explicitly tagged image-generation LoRA adapters."
    )]
    Add {
        /// Local adapter directory to import.
        #[arg(value_name = "PATH")]
        path: PathBuf,
        /// Local base model reference this adapter was trained for.
        #[arg(short = 'b', long = "base-model-ref", value_name = "MODEL_REF")]
        base_model_ref: Option<String>,
        /// Target model capability this adapter is intended for, such as image-generation.
        #[arg(long = "target-capability", value_name = "CAPABILITY")]
        target_capability: Option<String>,
        /// Adapter type override, such as lora or controlnet.
        #[arg(long = "adapter-type", value_name = "TYPE")]
        adapter_type: Option<String>,
        /// Adapter format override, such as diffusers-lora, diffusers-controlnet, or mlx-diffusion-lora.
        #[arg(long = "adapter-format", value_name = "FORMAT")]
        adapter_format: Option<String>,
        /// Runtime backend support override. May be repeated.
        #[arg(long = "backend-support", value_name = "BACKEND")]
        backend_support: Vec<String>,
        /// Control kind for ControlNet-style adapters, such as canny.
        #[arg(long = "control-kind", value_name = "KIND")]
        control_kind: Option<String>,
        /// Relative path to the LoRA weight file inside the adapter source.
        #[arg(long = "weight-file", value_name = "RELATIVE_PATH")]
        weight_file: Option<String>,
        /// Trigger word hint for image LoRA prompts. May be repeated.
        #[arg(long = "trigger-word", value_name = "TEXT")]
        trigger_word: Vec<String>,
        /// Recommended LoRA scale to store as adapter metadata.
        #[arg(long = "recommended-scale", value_name = "FLOAT")]
        recommended_scale: Option<f32>,
    },
    /// Pull an adapter snapshot from Hugging Face into the managed adapter store.
    #[command(
        name = "pull",
        about = "Pull an adapter snapshot from Hugging Face into the managed adapter store.",
        long_about = "Pull an adapter snapshot from Hugging Face into the managed adapter store. Tentgent resolves the repository revision first, downloads that exact snapshot, computes a content-derived adapter reference, and deduplicates identical content.",
        override_usage = "tentgent adapter pull <HF_REPO> [-r <REV>] [-b <MODEL_REF>]"
    )]
    Pull {
        /// Hugging Face adapter repository id, such as owner/name.
        #[arg(value_name = "HF_REPO")]
        repo_id: String,
        /// Hugging Face revision to resolve before downloading.
        #[arg(short = 'r', long, value_name = "REV")]
        revision: Option<String>,
        /// Local base model reference this adapter was trained for.
        #[arg(short = 'b', long = "base-model-ref", value_name = "MODEL_REF")]
        base_model_ref: Option<String>,
        /// Target model capability this adapter is intended for, such as image-generation.
        #[arg(long = "target-capability", value_name = "CAPABILITY")]
        target_capability: Option<String>,
        /// Adapter type override, such as lora or controlnet.
        #[arg(long = "adapter-type", value_name = "TYPE")]
        adapter_type: Option<String>,
        /// Adapter format override, such as diffusers-lora, diffusers-controlnet, or mlx-diffusion-lora.
        #[arg(long = "adapter-format", value_name = "FORMAT")]
        adapter_format: Option<String>,
        /// Runtime backend support override. May be repeated.
        #[arg(long = "backend-support", value_name = "BACKEND")]
        backend_support: Vec<String>,
        /// Control kind for ControlNet-style adapters, such as canny.
        #[arg(long = "control-kind", value_name = "KIND")]
        control_kind: Option<String>,
        /// Relative path to the LoRA weight file inside the adapter source.
        #[arg(long = "weight-file", value_name = "RELATIVE_PATH")]
        weight_file: Option<String>,
        /// Trigger word hint for image LoRA prompts. May be repeated.
        #[arg(long = "trigger-word", value_name = "TEXT")]
        trigger_word: Vec<String>,
        /// Recommended LoRA scale to store as adapter metadata.
        #[arg(long = "recommended-scale", value_name = "FLOAT")]
        recommended_scale: Option<f32>,
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
        /// Full adapter_ref or unique short-ref prefix.
        #[arg(value_name = "ADAPTER_REF")]
        reference: String,
    },
    /// Bind an adapter to one local managed base model.
    #[command(
        name = "bind",
        about = "Bind an adapter to one local managed base model.",
        long_about = "Bind an adapter to one local managed base model. Tentgent validates adapter_config.json base-model hints when available and writes a by-base index for later server compatibility checks.",
        override_usage = "tentgent adapter bind <ADAPTER_REF> -b <MODEL_REF>"
    )]
    Bind {
        /// Full adapter_ref or unique short-ref prefix.
        #[arg(value_name = "ADAPTER_REF")]
        adapter_ref: String,
        /// Local managed base model reference this adapter should target.
        #[arg(short = 'b', long = "base-model-ref", value_name = "MODEL_REF")]
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
        /// Full adapter_ref or unique short-ref prefix.
        #[arg(value_name = "ADAPTER_REF")]
        reference: String,
    },
}
