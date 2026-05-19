use std::path::PathBuf;

use clap::Args;

#[derive(Debug, Args)]
pub struct EmbedCommand {
    /// Stored Tentgent embedding model reference to run.
    #[arg(value_name = "MODEL_REF")]
    pub model_ref: String,
    /// Text input to embed. Repeat this option to embed multiple strings.
    #[arg(short = 'i', long = "input", value_name = "TEXT", required = true)]
    pub inputs: Vec<String>,
    /// Optional Tentgent runtime home override.
    #[arg(short = 'H', long, value_name = "HOME")]
    pub home: Option<PathBuf>,
    /// Pretty-print the JSON response.
    #[arg(long)]
    pub pretty: bool,
}
