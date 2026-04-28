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
        #[arg(value_name = "PATH")]
        path: PathBuf,
    },
    /// Print or write a paste-ready dataset generation template.
    #[command(
        name = "template",
        about = "Print or write a paste-ready dataset generation template.",
        long_about = "Print or write a deterministic Markdown prompt template for generating tentgent.chat.v1 JSONL with OpenAI, Claude, or another agent.",
        override_usage = "tentgent dataset template [--task <KIND>] [--language <LANG>] [--output <PATH>]"
    )]
    Template {
        #[arg(long, value_name = "KIND")]
        task: Option<String>,
        #[arg(long, value_name = "LANG")]
        language: Option<String>,
        #[arg(long, value_name = "PATH")]
        output: Option<PathBuf>,
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
        #[arg(value_name = "DATASET_REF")]
        reference: String,
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,
    },
    /// Diff two managed datasets.
    #[command(
        name = "diff",
        about = "Diff two managed datasets.",
        long_about = "Diff two managed datasets, or diff one managed dataset against a local working copy. The MVP compares manifest entries and reports added, removed, modified, and unchanged files.",
        override_usage = "tentgent dataset diff <LEFT_REF> <RIGHT_REF>\n       tentgent dataset diff <LEFT_REF> --path <PATH>"
    )]
    Diff {
        #[arg(value_name = "LEFT_REF")]
        left: String,
        #[arg(value_name = "RIGHT_REF")]
        right: Option<String>,
        #[arg(long, value_name = "PATH", conflicts_with = "right")]
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
        #[arg(value_name = "DATASET_REF")]
        reference: String,
    },
}
