use std::path::PathBuf;

use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum DatasetCommands {
    /// Import a local dataset file or directory.
    #[command(
        name = "add",
        about = "Import a local dataset file or directory.",
        long_about = "Import a local dataset file or directory into TENTGENT_HOME/datasets. Accepts a .jsonl file or a directory, copies it into the managed dataset store, and computes a content-derived dataset_ref.",
        override_usage = "tentgent dataset add <PATH>"
    )]
    Add {
        /// Local dataset JSONL file or directory to import.
        #[arg(value_name = "PATH")]
        path: PathBuf,
    },
    /// Validate a local dataset file or directory before import.
    #[command(
        name = "validate",
        about = "Validate a local dataset file or directory before import.",
        long_about = "Validate a local JSONL file or dataset directory against the Tentgent canonical chat dataset schema without importing it.",
        override_usage = "tentgent dataset validate <PATH>"
    )]
    Validate {
        /// Local dataset JSONL file or directory to validate.
        #[arg(value_name = "PATH")]
        path: PathBuf,
    },
    /// Print or write a paste-ready dataset generation template.
    #[command(
        name = "template",
        about = "Print or write a paste-ready dataset generation template.",
        long_about = "Print or write a deterministic Markdown prompt template for generating tentgent.chat.v1 JSONL with OpenAI, Claude, or another agent. The task and language options are prompt hints only; they do not change the dataset schema.",
        override_usage = "tentgent dataset template [-t <KIND>] [-l <LANG>] [-o <PATH>]"
    )]
    Template {
        #[arg(
            short = 't',
            long,
            value_name = "KIND",
            help = "Task/domain hint to insert into the template",
            long_help = "Task/domain hint to insert into the template, such as chat, support, summarization, tool-use, or polite-refusal. This guides generated examples but does not change the dataset schema."
        )]
        task: Option<String>,
        #[arg(
            short = 'l',
            long,
            value_name = "LANG",
            help = "Language hint for generated record content",
            long_help = "Language hint for generated record content, such as en, zh-TW, or ja. This guides the natural-language examples but does not change the dataset schema."
        )]
        language: Option<String>,
        #[arg(
            short = 'o',
            long,
            value_name = "PATH",
            help = "Write the template to a file instead of stdout"
        )]
        output: Option<PathBuf>,
    },
    /// Generate a local dataset package with OpenAI or Claude.
    #[command(
        name = "synth",
        about = "Generate a local dataset package with OpenAI or Claude.",
        long_about = "Generate a file-first Tentgent dataset package by asking OpenAI or Claude to produce tentgent.chat.v1 JSONL. The output directory is created locally and is not imported until you run dataset add. Use --print-prompt to inspect the exact provider prompt without auth or network calls.",
        override_usage = "tentgent dataset synth -p <openai|anthropic|claude> -m <MODEL> -o <DIR> (-b <TEXT> | -s <PATH>) [OPTIONS]\n       tentgent dataset synth --print-prompt (-b <TEXT> | -s <PATH>) [OPTIONS]",
        group(clap::ArgGroup::new("input").required(true).args(["brief", "spec"]))
    )]
    Synth {
        /// Cloud provider to call for dataset generation.
        #[arg(short = 'p', long, value_name = "PROVIDER", value_parser = ["openai", "anthropic", "claude"])]
        provider: Option<String>,
        /// Provider model name to use for generation.
        #[arg(short = 'm', long, value_name = "MODEL")]
        model: Option<String>,
        /// Local output directory for the generated dataset package.
        #[arg(short = 'o', long, value_name = "DIR")]
        output: Option<PathBuf>,
        /// Inline generation request to wrap in the Tentgent dataset prompt.
        #[arg(short = 'b', long, value_name = "TEXT", conflicts_with = "spec")]
        brief: Option<String>,
        /// Path to a generation spec or edited template file.
        #[arg(short = 's', long, value_name = "PATH", conflicts_with = "brief")]
        spec: Option<PathBuf>,
        /// Dataset split file to generate.
        #[arg(short = 'S', long, value_name = "SPLIT", default_value = "train", value_parser = ["train", "valid", "test", "eval_cases"])]
        split: String,
        /// Provider output token limit.
        #[arg(short = 'n', long, value_name = "TOKENS")]
        max_tokens: Option<u32>,
        /// Provider sampling temperature.
        #[arg(short = 'T', long, value_name = "FLOAT", default_value_t = 0.0)]
        temperature: f32,
        /// Provider request timeout in seconds.
        #[arg(long, value_name = "SECONDS", default_value_t = 180.0)]
        timeout_seconds: f32,
        #[arg(
            short = 'P',
            long,
            help = "Print the exact provider prompt and exit without auth or network calls"
        )]
        print_prompt: bool,
    },
    /// List managed datasets.
    #[command(
        name = "ls",
        about = "List managed datasets.",
        long_about = "List managed datasets stored under TENTGENT_HOME/datasets."
    )]
    Ls,
    /// Inspect one managed dataset.
    #[command(
        name = "inspect",
        about = "Inspect one managed dataset.",
        long_about = "Inspect one managed dataset by full dataset_ref or unique short-ref prefix.",
        override_usage = "tentgent dataset inspect <DATASET_REF>"
    )]
    Inspect {
        /// Full dataset_ref or unique short-ref prefix.
        #[arg(value_name = "DATASET_REF")]
        reference: String,
    },
    /// Export a managed dataset source into a local working directory.
    #[command(
        name = "export",
        about = "Export a managed dataset to a local working directory.",
        long_about = "Export a managed dataset source into a local working directory. The destination is created if it does not exist and must be empty when it already exists.",
        override_usage = "tentgent dataset export <DATASET_REF> <PATH>"
    )]
    Export {
        /// Full dataset_ref or unique short-ref prefix.
        #[arg(value_name = "DATASET_REF")]
        reference: String,
        /// Destination directory for exported dataset source files.
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,
    },
    /// Diff two managed datasets.
    #[command(
        name = "diff",
        about = "Diff two managed datasets.",
        long_about = "Diff two managed datasets, or diff one managed dataset against a local working copy. The MVP compares manifest entries and reports added, removed, modified, and unchanged files.",
        override_usage = "tentgent dataset diff <LEFT_REF> <RIGHT_REF>\n       tentgent dataset diff <LEFT_REF> -p <PATH>"
    )]
    Diff {
        /// Full dataset_ref or unique short-ref prefix for the left side.
        #[arg(value_name = "LEFT_REF")]
        left: String,
        /// Full dataset_ref or unique short-ref prefix for the right side.
        #[arg(value_name = "RIGHT_REF")]
        right: Option<String>,
        /// Local dataset directory or JSONL file to compare with the managed dataset.
        #[arg(short = 'p', long, value_name = "PATH", conflicts_with = "right")]
        path: Option<PathBuf>,
    },
    /// Remove one managed dataset.
    #[command(
        name = "rm",
        about = "Remove one managed dataset.",
        long_about = "Remove one managed dataset by full dataset_ref or unique short-ref prefix. This deletes the managed store record and source indexes, but does not delete any exported working copies.",
        override_usage = "tentgent dataset rm <DATASET_REF>"
    )]
    Rm {
        /// Full dataset_ref or unique short-ref prefix.
        #[arg(value_name = "DATASET_REF")]
        reference: String,
    },
}
