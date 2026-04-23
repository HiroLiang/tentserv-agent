use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use miette::{miette, IntoDiagnostic};
use tentgent_core::auth::{AuthManager, KeySource, KeyStatus, KeyValidationState, Provider};

use super::commands::{AuthCommands, AuthProviderAction};

pub async fn handle_auth_command(subject: AuthCommands) -> miette::Result<()> {
    let auth = AuthManager::new().into_diagnostic()?;

    match subject {
        AuthCommands::Hf { action } => {
            handle_provider_action(&auth, Provider::HuggingFace, action).await?
        }
        AuthCommands::Openai { action } => {
            handle_provider_action(&auth, Provider::OpenAI, action).await?
        }
        AuthCommands::Anthropic { action } => {
            handle_provider_action(&auth, Provider::Anthropic, action).await?
        }
    }

    Ok(())
}

async fn handle_provider_action(
    auth: &AuthManager,
    provider: Provider,
    action: Option<AuthProviderAction>,
) -> miette::Result<()> {
    match action {
        None => {
            let status = auth.key_status(provider).await.into_diagnostic()?;
            render_key_status(&status);
        }
        Some(AuthProviderAction::Set) => set_key(auth, provider).await?,
        Some(AuthProviderAction::Rm) => remove_key(auth, provider).await?,
    }

    Ok(())
}

async fn set_key(auth: &AuthManager, provider: Provider) -> miette::Result<()> {
    let prompt = format!("Enter {} API key: ", provider.display_name());
    let secret = rpassword::prompt_password(prompt).into_diagnostic()?;
    let secret = secret.trim().to_owned();

    if secret.is_empty() {
        return Err(miette!("No API key was entered."));
    }

    auth.set_key(provider, &secret).into_diagnostic()?;
    let validation = auth.validate_secret(provider, &secret).await;

    println!(
        "{} {} key stored in keychain.",
        style("stored").green().bold(),
        provider.display_name()
    );

    render_validation(provider, &validation);

    let status = auth.key_status(provider).await.into_diagnostic()?;
    if matches!(status.effective_source, Some(KeySource::Env)) {
        println!(
            "{} .env/env currently overrides the keychain value for {}.",
            style("note").yellow().bold(),
            provider.display_name()
        );
    }

    render_key_status(&status);
    Ok(())
}

async fn remove_key(auth: &AuthManager, provider: Provider) -> miette::Result<()> {
    let removed = auth.remove_key(provider).into_diagnostic()?;

    if removed {
        println!(
            "{} Removed {} key from keychain.",
            style("removed").yellow().bold(),
            provider.display_name()
        );
    } else {
        println!(
            "{} No stored {} key was found in keychain.",
            style("missing").yellow().bold(),
            provider.display_name()
        );
    }

    let status = auth.key_status(provider).await.into_diagnostic()?;
    render_key_status(&status);
    Ok(())
}

fn render_key_status(status: &KeyStatus) {
    println!(
        "{} {} key status",
        style("==>").cyan().bold(),
        style(status.provider.display_name()).bold()
    );

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["Field", "Value"]);

    table.add_row(vec![
        Cell::new("provider"),
        Cell::new(status.provider.display_name()),
    ]);
    table.add_row(vec![
        Cell::new("env override"),
        Cell::new(presence(status.env_present)),
    ]);
    table.add_row(vec![
        Cell::new("keychain entry"),
        Cell::new(presence(status.keychain_present)),
    ]);
    table.add_row(vec![
        Cell::new("effective source"),
        Cell::new(
            status
                .effective_source
                .map(|source| source.to_string())
                .unwrap_or_else(|| "none".to_string()),
        ),
    ]);
    table.add_row(vec![
        Cell::new("validation"),
        Cell::new(status.validation.summary()),
    ]);

    if let Some(detail) = status.validation.detail() {
        table.add_row(vec![Cell::new("detail"), Cell::new(detail)]);
    }

    println!("{table}");
    println!();
}

fn render_validation(provider: Provider, validation: &KeyValidationState) {
    match validation {
        KeyValidationState::Verified => println!(
            "{} {} key validated successfully.",
            style("verified").green().bold(),
            provider.display_name()
        ),
        KeyValidationState::Invalid { reason } => {
            println!("{} {}", style("invalid").red().bold(), reason)
        }
        KeyValidationState::Unknown { reason } => {
            println!("{} {}", style("unknown").yellow().bold(), reason)
        }
        KeyValidationState::Missing => println!(
            "{} No effective {} key is available to validate.",
            style("missing").yellow().bold(),
            provider.display_name()
        ),
    }
}

fn presence(present: bool) -> &'static str {
    if present {
        "present"
    } else {
        "absent"
    }
}
