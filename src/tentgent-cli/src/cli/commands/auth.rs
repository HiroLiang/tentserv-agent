use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum AuthCommands {
    /// Show auth status for every provider.
    #[command(
        name = "status",
        visible_alias = "ls",
        about = "Show auth status for every provider.",
        long_about = "Show auth status for every provider. Tentgent reports .env/env override state, keychain presence, effective source, and validation status without printing secret values."
    )]
    Status,
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
