use clap::Subcommand;
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum AuthCommands {
    /// Show auth status for every provider.
    #[command(
        name = "status",
        visible_alias = "ls",
        about = "Show auth status for every provider.",
        long_about = "Show auth status for every provider. Tentgent reports auth mode, .env/env override state, keychain presence, effective source, and validation status without printing secret values."
    )]
    Status,
    /// Show or set provider auth source modes.
    #[command(
        name = "mode",
        about = "Show or set provider auth source modes.",
        long_about = "Show or set provider auth source modes. Modes are non-secret preferences that control where Tentgent may resolve provider credentials from: auto, keychain, file, env, or none.",
        after_help = "Examples:\n  tentgent auth mode\n  tentgent auth mode openai\n  tentgent auth mode openai env\n  tentgent auth mode gemini file --path ~/.config/tentgent/provider.env\n  tentgent auth mode anthropic none"
    )]
    Mode {
        /// Provider to inspect or configure: hf, openai, anthropic, or gemini.
        provider: Option<String>,
        /// Auth mode to set: auto, keychain, file, env, or none.
        mode: Option<String>,
        /// Explicit env file path for file mode.
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Show, store, or remove the Hugging Face API key.
    #[command(
        name = "hf",
        about = "Show, store, or remove the Hugging Face API key.",
        long_about = "Show, store, or remove the Hugging Face API key. With no action, Tentgent prints the current Hugging Face auth status, including .env/env override state, keychain presence, and validation status."
    )]
    Hf {
        #[command(subcommand)]
        action: Option<AuthProviderAction>,
    },
    /// Show, store, or remove the OpenAI API key.
    #[command(
        name = "openai",
        about = "Show, store, or remove the OpenAI API key.",
        long_about = "Show, store, or remove the OpenAI API key. With no action, Tentgent prints the current OpenAI auth status, including .env/env override state, keychain presence, and validation status."
    )]
    Openai {
        #[command(subcommand)]
        action: Option<AuthProviderAction>,
    },
    /// Show, store, or remove the Anthropic API key.
    #[command(
        name = "anthropic",
        about = "Show, store, or remove the Anthropic API key.",
        long_about = "Show, store, or remove the Anthropic API key. With no action, Tentgent prints the current Anthropic auth status, including .env/env override state, keychain presence, and validation status."
    )]
    Anthropic {
        #[command(subcommand)]
        action: Option<AuthProviderAction>,
    },
    /// Show, store, or remove the Gemini API key.
    #[command(
        name = "gemini",
        about = "Show, store, or remove the Gemini API key.",
        long_about = "Show, store, or remove the Gemini API key. With no action, Tentgent prints the current Gemini auth status, including .env/env override state, keychain presence, and validation status."
    )]
    Gemini {
        #[command(subcommand)]
        action: Option<AuthProviderAction>,
    },
}

#[derive(Debug, Subcommand)]
pub enum AuthProviderAction {
    /// Prompt for an API key and store it in the system keychain.
    #[command(
        about = "Prompt for an API key and store it in the system keychain.",
        long_about = "Prompt for an API key and store it in the system keychain. The key is read from secure terminal input, written to the system keychain, and then validated against the provider's official API endpoint.",
        after_help = "Input format:\n  Paste the raw API key only.\n  Do not include quotes, spaces, or a Bearer prefix."
    )]
    Set,
    /// Remove the stored API key from the system keychain.
    #[command(
        name = "rm",
        visible_alias = "remove",
        about = "Remove the stored API key from the system keychain.",
        long_about = "Remove the stored API key from the system keychain. This only deletes the persisted keychain entry. It does not change any .env or environment-variable override."
    )]
    Rm,
}
