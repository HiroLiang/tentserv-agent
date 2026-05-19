use std::path::PathBuf;

use clap::Args;

#[derive(Debug, Args)]
pub struct RerankCommand {
    /// Stored Tentgent rerank model reference to run.
    #[arg(value_name = "MODEL_REF")]
    pub model_ref: String,
    /// Query text to compare against the candidate documents.
    #[arg(short = 'q', long = "query", value_name = "TEXT")]
    pub query: String,
    /// Candidate document text. Repeat this option for multiple documents.
    #[arg(short = 'd', long = "document", value_name = "TEXT", required = true)]
    pub documents: Vec<String>,
    /// Return only the top N ranked documents.
    #[arg(long = "top-n", value_name = "N")]
    pub top_n: Option<usize>,
    /// Optional Tentgent runtime home override.
    #[arg(short = 'H', long, value_name = "HOME")]
    pub home: Option<PathBuf>,
    /// Pretty-print the JSON response.
    #[arg(long)]
    pub pretty: bool,
}
