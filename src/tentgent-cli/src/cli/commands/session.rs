use std::path::PathBuf;

use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum SessionCommands {
    /// List local chat sessions.
    #[command(
        name = "ls",
        visible_alias = "list",
        about = "List local chat sessions.",
        long_about = "List local chat sessions stored under TENTGENT_HOME/sessions. Sessions are durable transcript metadata records and are separate from training datasets."
    )]
    Ls {
        /// Optional Tentgent runtime home override for session state lookup.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
    },
    /// Inspect one local chat session.
    #[command(
        name = "inspect",
        about = "Inspect one local chat session.",
        long_about = "Inspect one local chat session by full session_ref or unique short-ref prefix."
    )]
    Inspect {
        /// Full session_ref or unique short-ref prefix.
        #[arg(value_name = "SESSION_REF")]
        reference: String,
        /// Optional Tentgent runtime home override for session state lookup.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
    },
    /// Show the recent messages from one local chat session.
    #[command(
        name = "messages",
        about = "Show recent session messages.",
        long_about = "Show recent messages from one local chat session by full session_ref or unique short-ref prefix."
    )]
    Messages {
        /// Full session_ref or unique short-ref prefix.
        #[arg(value_name = "SESSION_REF")]
        reference: String,
        /// Number of recent messages to show.
        #[arg(long, default_value_t = 100, value_name = "N")]
        tail: usize,
        /// Optional Tentgent runtime home override for session state lookup.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
    },
    /// Create a local chat session.
    #[command(
        name = "create",
        about = "Create a local chat session.",
        long_about = "Create a local chat session metadata record. This does not make chat commands session-aware yet."
    )]
    Create {
        /// Optional display title.
        #[arg(long, value_name = "TITLE")]
        title: Option<String>,
        /// Optional default server reference stored as session metadata.
        #[arg(long = "default-server", value_name = "SERVER_REF")]
        default_server: Option<String>,
        /// Optional adapter reference stored as session metadata.
        #[arg(long, value_name = "ADAPTER_REF")]
        adapter: Option<String>,
        /// Tag to attach to the session. Can be repeated.
        #[arg(long = "tag", value_name = "TAG")]
        tags: Vec<String>,
        /// Optional Tentgent runtime home override for session state lookup.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
    },
    /// Update local chat session metadata.
    #[command(
        name = "update",
        about = "Update local chat session metadata.",
        long_about = "Update local chat session metadata by full session_ref or unique short-ref prefix. Null-style clearing is exposed as explicit --clear-* flags in the CLI."
    )]
    Update {
        /// Full session_ref or unique short-ref prefix.
        #[arg(value_name = "SESSION_REF")]
        reference: String,
        /// Replace the display title.
        #[arg(long, conflicts_with = "clear_title", value_name = "TITLE")]
        title: Option<String>,
        /// Clear the display title.
        #[arg(long)]
        clear_title: bool,
        /// Replace the default server reference.
        #[arg(
            long = "default-server",
            conflicts_with = "clear_default_server",
            value_name = "SERVER_REF"
        )]
        default_server: Option<String>,
        /// Clear the default server reference.
        #[arg(long)]
        clear_default_server: bool,
        /// Replace the adapter reference.
        #[arg(long, conflicts_with = "clear_adapter", value_name = "ADAPTER_REF")]
        adapter: Option<String>,
        /// Clear the adapter reference.
        #[arg(long)]
        clear_adapter: bool,
        /// Replace the full tag list. Can be repeated.
        #[arg(long = "tag", conflicts_with = "clear_tags", value_name = "TAG")]
        tags: Vec<String>,
        /// Clear all tags.
        #[arg(long)]
        clear_tags: bool,
        /// Optional Tentgent runtime home override for session state lookup.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
    },
    /// Append one message to a local chat session.
    #[command(
        name = "append",
        about = "Append one message to a local chat session.",
        long_about = "Append one message to a local chat session by full session_ref or unique short-ref prefix. The message timestamp is assigned by Tentgent."
    )]
    Append {
        /// Full session_ref or unique short-ref prefix.
        #[arg(value_name = "SESSION_REF")]
        reference: String,
        /// Message role.
        #[arg(long, value_parser = ["system", "user", "assistant", "tool"], value_name = "ROLE")]
        role: String,
        /// Message content.
        #[arg(long, value_name = "TEXT")]
        content: String,
        /// JSON object metadata to attach to the message.
        #[arg(long = "metadata-json", default_value = "{}", value_name = "JSON")]
        metadata_json: String,
        /// Optional running server ref used to compact older session messages if the append would exceed the bounded session cap.
        #[arg(long = "compaction-server", value_name = "SERVER_REF")]
        compaction_server: Option<String>,
        /// Optional Tentgent runtime home override for session state lookup.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
    },
    /// Compact older messages in a local chat session.
    #[command(
        name = "compact",
        about = "Compact one local chat session.",
        long_about = "Compact older messages in one local chat session into a generated summary message, keeping the transcript bounded as short-term working context."
    )]
    Compact {
        /// Full session_ref or unique short-ref prefix.
        #[arg(value_name = "SESSION_REF")]
        reference: String,
        /// Optional running server ref used for summary generation. Falls back to the session default server.
        #[arg(long = "server", value_name = "SERVER_REF")]
        server: Option<String>,
        /// Number of recent raw messages to keep next to the summary.
        #[arg(long = "keep-recent", default_value_t = 49, value_name = "N")]
        keep_recent: usize,
        /// Additional summarization instructions.
        #[arg(long, value_name = "TEXT")]
        instructions: Option<String>,
        /// Optional Tentgent runtime home override for session state lookup.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
    },
    /// Permanently remove one local chat session.
    #[command(
        name = "rm",
        visible_alias = "remove",
        about = "Permanently remove one local chat session.",
        long_about = "Permanently remove one local chat session directory. There is no trash or recycle bin."
    )]
    Rm {
        /// Full session_ref or unique short-ref prefix.
        #[arg(value_name = "SESSION_REF")]
        reference: String,
        /// Optional Tentgent runtime home override for session state lookup.
        #[arg(short = 'H', long, value_name = "HOME")]
        home: Option<PathBuf>,
    },
}
