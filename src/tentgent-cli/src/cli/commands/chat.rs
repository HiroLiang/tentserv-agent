use clap::Args;

#[derive(Debug, Args)]
pub struct ChatCommand {
    /// Stored Tentgent model reference to run.
    #[arg(value_name = "MODEL_REF")]
    pub model_ref: String,
    /// Message content in order. Use role:content for explicit system, user, or assistant context.
    #[arg(long = "message", value_name = "MESSAGE")]
    pub messages: Vec<String>,
    /// Optional Tentgent runtime home override passed through to the Python harness.
    #[arg(long, value_name = "HOME")]
    pub home: Option<String>,
    /// Maximum number of tokens to generate.
    #[arg(long = "max-tokens", value_name = "N")]
    pub max_tokens: Option<u32>,
    /// Sampling temperature. Omit or use 0 for deterministic decoding.
    #[arg(long, value_name = "TEMP")]
    pub temperature: Option<f32>,
    /// Optional compatible adapter reference for PEFT-backed LoRA chat.
    #[arg(long = "adapter-ref", value_name = "REF")]
    pub adapter_ref: Option<String>,
    /// Stream generated text to stdout when the selected backend supports streaming.
    #[arg(long)]
    pub stream: bool,
}
