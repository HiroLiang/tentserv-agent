use clap::{Args, Subcommand, ValueEnum};
use tentgent_core::train::{LoraTrainBackendRequest, LoraTrainOverrides};

#[derive(Debug, Subcommand)]
pub enum TrainCommands {
    /// Plan or run LoRA training workflows.
    #[command(
        name = "lora",
        about = "Plan or run LoRA training workflows.",
        long_about = "Plan or run LoRA training workflows. Plan commands validate model, dataset, backend selection, and output paths. The run command creates durable run records and launches the selected Python training runner."
    )]
    Lora {
        #[command(subcommand)]
        action: TrainLoraCommands,
    },
}

#[derive(Debug, Subcommand)]
pub enum TrainLoraCommands {
    /// Create, list, and inspect managed LoRA training plans.
    #[command(
        name = "plan",
        about = "Create, list, and inspect managed LoRA training plans.",
        long_about = "Create, list, and inspect managed LoRA training plans. A plan is a persistent recipe; each future run will create a separate run record and successful runs will create new adapter refs instead of overwriting prior adapters."
    )]
    Plan {
        #[command(subcommand)]
        action: TrainLoraPlanCommands,
    },
    /// Run one managed LoRA training plan.
    #[command(
        name = "run",
        about = "Run one managed LoRA training plan.",
        long_about = "Run one managed LoRA training plan by full ref or unique short-ref prefix. Tentgent creates run.toml, metrics.jsonl, raw.log, keeps raw backend logs out of the default CLI output, executes MLX plans through mlx_lm.lora, and executes safetensors plans through Transformers plus PEFT.",
        override_usage = "tentgent train lora run <PLAN_REF> [-v] [-d]"
    )]
    Run(TrainLoraRunCommand),
}

#[derive(Debug, Subcommand)]
pub enum TrainLoraPlanCommands {
    /// Create a managed LoRA train plan without running training.
    #[command(
        name = "create",
        about = "Create a managed LoRA train plan.",
        long_about = "Create a managed LoRA train plan without running training. Tentgent resolves the model and dataset, chooses backend defaults, stores a plan.toml, and returns the plan ref. Use --review to preview before saving, or --interactive to edit common overrides before the final review.",
        override_usage = "tentgent train lora plan create -m <MODEL_REF> -d <DATASET_REF> [OPTIONS]"
    )]
    Create(TrainLoraPlanCreateCommand),
    /// List managed LoRA train plans.
    #[command(
        name = "ls",
        about = "List managed LoRA train plans.",
        override_usage = "tentgent train lora plan ls"
    )]
    Ls,
    /// Inspect a managed LoRA train plan.
    #[command(
        name = "inspect",
        about = "Inspect a managed LoRA train plan.",
        long_about = "Inspect a managed LoRA train plan by full ref or unique short-ref prefix.",
        override_usage = "tentgent train lora plan inspect <PLAN_REF>"
    )]
    Inspect {
        /// Managed LoRA train plan ref or unique short-ref prefix.
        #[arg(value_name = "PLAN_REF")]
        reference: String,
    },
    /// Remove a managed LoRA train plan and its run records.
    #[command(
        name = "rm",
        about = "Remove a managed LoRA train plan.",
        long_about = "Remove a managed LoRA train plan and any run records stored under that plan. This does not remove adapters that were already imported into the adapter store.",
        override_usage = "tentgent train lora plan rm <PLAN_REF>"
    )]
    Rm {
        /// Managed LoRA train plan ref or unique short-ref prefix.
        #[arg(value_name = "PLAN_REF")]
        reference: String,
    },
}

#[derive(Debug, Args)]
pub struct TrainLoraPlanCreateCommand {
    /// Managed model ref or unique short-ref prefix.
    #[arg(short = 'm', long, value_name = "MODEL_REF")]
    pub model: String,
    /// Managed dataset ref or unique short-ref prefix.
    #[arg(short = 'd', long, value_name = "DATASET_REF")]
    pub dataset: String,
    /// Optional human-readable plan name.
    #[arg(short = 'n', long)]
    pub name: Option<String>,
    /// Preview the generated plan and ask before saving it.
    #[arg(short = 'R', long)]
    pub review: bool,
    /// Prompt for common overrides, then review before saving.
    #[arg(short = 'i', long)]
    pub interactive: bool,
    /// Override dataset max sequence length.
    #[arg(short = 'L', long, value_name = "TOKENS")]
    pub max_seq_length: Option<u32>,
    /// Explicitly keep the default behavior: train only assistant output tokens while keeping prompt/context tokens visible.
    #[arg(short = 'p', long, conflicts_with = "no_mask_prompt")]
    pub mask_prompt: bool,
    /// Opt out of prompt masking and train full rendered text, including prompt/context framing tokens.
    #[arg(long, conflicts_with = "mask_prompt")]
    pub no_mask_prompt: bool,
    /// Override LoRA rank.
    #[arg(short = 'r', long, value_name = "RANK")]
    pub rank: Option<u32>,
    /// Override optimization learning rate.
    #[arg(short = 'l', long, value_name = "LR")]
    pub learning_rate: Option<f64>,
    /// Override per-device batch size.
    #[arg(short = 'b', long, value_name = "N")]
    pub batch_size: Option<u32>,
    /// Override gradient accumulation steps.
    #[arg(short = 'g', long, value_name = "N")]
    pub grad_accum: Option<u32>,
    /// Override max training steps.
    #[arg(short = 's', long, value_name = "STEPS")]
    pub max_steps: Option<u32>,
    /// Override random seed.
    #[arg(short = 'S', long, value_name = "SEED")]
    pub seed: Option<u64>,
    /// Override MLX tuned layer count.
    #[arg(short = 'N', long, value_name = "LAYERS")]
    pub num_layers: Option<u32>,
    /// Enable MLX gradient checkpointing.
    #[arg(short = 'c', long)]
    pub grad_checkpoint: bool,
    /// Enable PEFT 4-bit loading.
    #[arg(long)]
    pub load_in_4bit: bool,
    /// Enable PEFT 8-bit loading.
    #[arg(long)]
    pub load_in_8bit: bool,
    /// Backend selection. Use auto unless you need to verify an explicit backend.
    #[arg(short = 'B', long, value_enum, default_value_t = TrainBackendArg::Auto)]
    pub backend: TrainBackendArg,
}

#[derive(Debug, Args)]
pub struct TrainLoraRunCommand {
    /// Managed LoRA train plan ref or unique short-ref prefix.
    #[arg(value_name = "PLAN_REF")]
    pub reference: String,
    /// Show eval, checkpoint, and backend summary events.
    #[arg(short = 'v', long)]
    pub verbose: bool,
    /// Stream raw backend output in addition to writing raw.log.
    #[arg(short = 'd', long)]
    pub debug: bool,
}

impl TrainLoraPlanCreateCommand {
    pub fn overrides(&self) -> LoraTrainOverrides {
        LoraTrainOverrides {
            max_seq_length: self.max_seq_length,
            mask_prompt: if self.no_mask_prompt {
                Some(false)
            } else {
                self.mask_prompt.then_some(true)
            },
            rank: self.rank,
            learning_rate: self.learning_rate,
            batch_size: self.batch_size,
            gradient_accumulation_steps: self.grad_accum,
            max_steps: self.max_steps,
            seed: self.seed,
            mlx_num_layers: self.num_layers,
            mlx_grad_checkpoint: self.grad_checkpoint.then_some(true),
            peft_load_in_4bit: self.load_in_4bit.then_some(true),
            peft_load_in_8bit: self.load_in_8bit.then_some(true),
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TrainBackendArg {
    Auto,
    Mlx,
    Peft,
}

impl From<TrainBackendArg> for LoraTrainBackendRequest {
    fn from(value: TrainBackendArg) -> Self {
        match value {
            TrainBackendArg::Auto => Self::Auto,
            TrainBackendArg::Mlx => Self::Mlx,
            TrainBackendArg::Peft => Self::Peft,
        }
    }
}
