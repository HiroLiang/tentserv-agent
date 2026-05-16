use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use miette::{miette, IntoDiagnostic};
use tentgent_kernel::features::auth::domain::{
    AuthEnvLoadPolicy, AuthKeyStatus, AuthSecretCacheScope, AuthSecretSource, AuthValidationState,
    KeychainPresence, Provider,
};
use tentgent_kernel::features::auth::infra::{
    FileAuthMetadataStore, ProcessSessionAuthSecretCache, ReqwestAuthSecretValidator,
    StdAuthEnvSecretProbe, SystemKeychainAuthSecretStore,
};
use tentgent_kernel::features::auth::usecases::{
    AuthSecretMutationUseCase, AuthSecretResolutionRequest, AuthSecretValidationRequest,
    AuthSecretValidationUseCase, AuthStatusRequest, AuthStatusUseCase, RemoveAuthSecretRequest,
    SetAuthSecretRequest, StdAuthSecretMutationUseCase, StdAuthSecretResolverUseCase,
    StdAuthSecretValidationUseCase, StdAuthStatusUseCase,
};
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::commands::{AuthCommands, AuthProviderAction};

pub async fn handle_auth_command(subject: AuthCommands) -> miette::Result<()> {
    let auth = CliAuthKernel::new()?;

    match subject {
        AuthCommands::Status => render_all_key_statuses(&auth).await?,
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

struct CliAuthKernel {
    env_probe: StdAuthEnvSecretProbe,
    keychain_store: SystemKeychainAuthSecretStore,
    metadata_store: FileAuthMetadataStore,
    cache: ProcessSessionAuthSecretCache,
    validator: ReqwestAuthSecretValidator,
}

impl CliAuthKernel {
    fn new() -> miette::Result<Self> {
        let layout = StdRuntimeLayoutResolver
            .resolve(RuntimeLayoutInput {
                mode: LayoutResolveMode::Create,
                home_dir: None,
                data_root_dir: None,
            })
            .into_diagnostic()?;

        Ok(Self {
            env_probe: StdAuthEnvSecretProbe,
            keychain_store: SystemKeychainAuthSecretStore::new(),
            metadata_store: FileAuthMetadataStore::from_layout(&layout),
            cache: ProcessSessionAuthSecretCache::new(),
            validator: ReqwestAuthSecretValidator::new().into_diagnostic()?,
        })
    }

    fn status_usecase(&self) -> StdAuthStatusUseCase<'_> {
        StdAuthStatusUseCase::new(&self.env_probe, &self.keychain_store, &self.metadata_store)
    }

    fn resolver_usecase(&self) -> StdAuthSecretResolverUseCase<'_> {
        StdAuthSecretResolverUseCase::new(&self.env_probe, &self.keychain_store, &self.cache)
    }

    fn mutation_usecase(&self) -> StdAuthSecretMutationUseCase<'_> {
        StdAuthSecretMutationUseCase::new(&self.keychain_store, &self.metadata_store, &self.cache)
    }

    async fn validated_status(&self, provider: Provider) -> miette::Result<AuthKeyStatus> {
        let resolver = self.resolver_usecase();
        let validation =
            StdAuthSecretValidationUseCase::new(&resolver, &self.validator, &self.metadata_store);
        validation
            .validate_secret(
                AuthSecretValidationRequest::new(
                    AuthSecretResolutionRequest::for_secret_validation(
                        provider,
                        AuthEnvLoadPolicy::CwdDotenvOverride,
                    ),
                )
                .with_validated_at(now_rfc3339()?),
            )
            .await
            .into_diagnostic()?;

        self.local_status(provider)
    }

    fn local_status(&self, provider: Provider) -> miette::Result<AuthKeyStatus> {
        let report = self
            .status_usecase()
            .status(AuthStatusRequest::for_provider(
                provider,
                AuthEnvLoadPolicy::CwdDotenvOverride,
            ))
            .into_diagnostic()?;

        report
            .status_for(provider)
            .cloned()
            .ok_or_else(|| miette!("missing auth status for {}", provider.display_name()))
    }

    async fn validate_prompt_secret(
        &self,
        provider: Provider,
        secret: &str,
    ) -> miette::Result<AuthValidationState> {
        let resolver = self.resolver_usecase();
        let validation =
            StdAuthSecretValidationUseCase::new(&resolver, &self.validator, &self.metadata_store);
        let mut resolution = AuthSecretResolutionRequest::for_secret_validation(
            provider,
            AuthEnvLoadPolicy::CwdDotenvOverride,
        )
        .with_prompt_secret(secret.to_string());
        resolution.access_policy.cache_scope = AuthSecretCacheScope::None;

        validation
            .validate_secret(
                AuthSecretValidationRequest::new(resolution).with_validated_at(now_rfc3339()?),
            )
            .await
            .map(|result| result.validation)
            .into_diagnostic()
    }
}

async fn render_all_key_statuses(auth: &CliAuthKernel) -> miette::Result<()> {
    let mut statuses = Vec::new();
    for provider in Provider::ALL {
        statuses.push(auth.validated_status(provider).await?);
    }

    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Provider auth status").bold()
    );

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            "Provider",
            "Env",
            "Keychain",
            "Effective",
            "Validation",
            "Detail",
        ]);

    for status in statuses {
        table.add_row(vec![
            Cell::new(status.provider.display_name()),
            Cell::new(presence(status.env_present)),
            Cell::new(keychain_presence(status.keychain_presence)),
            Cell::new(
                status
                    .effective_source
                    .map(|source| source.to_string())
                    .unwrap_or_else(|| "none".to_string()),
            ),
            Cell::new(status.validation.summary()),
            Cell::new(status.validation.detail().unwrap_or("-")),
        ]);
    }

    println!("{table}");
    println!();
    Ok(())
}

async fn handle_provider_action(
    auth: &CliAuthKernel,
    provider: Provider,
    action: Option<AuthProviderAction>,
) -> miette::Result<()> {
    match action {
        None => {
            let status = auth.validated_status(provider).await?;
            render_key_status(&status);
        }
        Some(AuthProviderAction::Set) => set_key(auth, provider).await?,
        Some(AuthProviderAction::Rm) => remove_key(auth, provider).await?,
    }

    Ok(())
}

async fn set_key(auth: &CliAuthKernel, provider: Provider) -> miette::Result<()> {
    let prompt = format!("Enter {} API key: ", provider.display_name());
    let secret = rpassword::prompt_password(prompt).into_diagnostic()?;
    let secret = secret.trim().to_owned();

    if secret.is_empty() {
        return Err(miette!("No API key was entered."));
    }

    auth.mutation_usecase()
        .set_secret(
            SetAuthSecretRequest::new(provider, &secret)
                .with_updated_at(now_rfc3339()?)
                .with_validation(AuthValidationState::NotChecked),
        )
        .into_diagnostic()?;
    let validation = auth.validate_prompt_secret(provider, &secret).await?;

    println!(
        "{} {} key stored in keychain.",
        style("stored").green().bold(),
        provider.display_name()
    );

    render_validation(provider, &validation);

    let status = auth.validated_status(provider).await?;
    if matches!(status.effective_source, Some(AuthSecretSource::Env)) {
        println!(
            "{} .env/env currently overrides the keychain value for {}.",
            style("note").yellow().bold(),
            provider.display_name()
        );
    }

    render_key_status(&status);
    Ok(())
}

async fn remove_key(auth: &CliAuthKernel, provider: Provider) -> miette::Result<()> {
    let removed = auth
        .mutation_usecase()
        .remove_secret(RemoveAuthSecretRequest::new(provider))
        .into_diagnostic()?
        .removed;

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

    let status = auth.validated_status(provider).await?;
    render_key_status(&status);
    Ok(())
}

fn render_key_status(status: &AuthKeyStatus) {
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
        Cell::new(keychain_presence(status.keychain_presence)),
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

fn render_validation(provider: Provider, validation: &AuthValidationState) {
    match validation {
        AuthValidationState::Verified => println!(
            "{} {} key validated successfully.",
            style("verified").green().bold(),
            provider.display_name()
        ),
        AuthValidationState::Invalid { reason } => {
            println!("{} {}", style("invalid").red().bold(), reason)
        }
        AuthValidationState::Unknown { reason } => {
            println!("{} {}", style("unknown").yellow().bold(), reason)
        }
        AuthValidationState::Missing => println!(
            "{} No effective {} key is available to validate.",
            style("missing").yellow().bold(),
            provider.display_name()
        ),
        AuthValidationState::NotChecked => println!(
            "{} {} key validation was not checked.",
            style("not checked").yellow().bold(),
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

fn keychain_presence(presence: KeychainPresence) -> &'static str {
    match presence {
        KeychainPresence::Present => "present",
        KeychainPresence::Absent => "absent",
        KeychainPresence::Unknown => "unknown",
    }
}

fn now_rfc3339() -> miette::Result<String> {
    OffsetDateTime::now_utc().format(&Rfc3339).into_diagnostic()
}
