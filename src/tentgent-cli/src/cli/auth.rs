use std::path::PathBuf;

use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use miette::{miette, IntoDiagnostic};
use tentgent_kernel::features::auth::domain::{
    AuthEnvLoadPolicy, AuthKeyStatus, AuthProviderPreference, AuthSecretCacheScope,
    AuthSecretSource, AuthSourceMode, AuthValidationState, KeychainPresence, Provider,
};
use tentgent_kernel::features::auth::infra::{
    FileAuthMetadataStore, ProcessSessionAuthSecretCache, ReqwestAuthSecretValidator,
    StdAuthEnvSecretProbe, SystemKeychainAuthSecretStore,
};
use tentgent_kernel::features::auth::usecases::{
    AuthPreferenceListRequest, AuthPreferenceRequest, AuthPreferenceUseCase,
    AuthSecretMutationUseCase, AuthSecretResolutionRequest, AuthSecretValidationRequest,
    AuthSecretValidationUseCase, AuthStatusRequest, AuthStatusUseCase, RemoveAuthSecretRequest,
    SetAuthPreferenceRequest, SetAuthSecretRequest, StdAuthPreferenceUseCase,
    StdAuthSecretMutationUseCase, StdAuthSecretResolverUseCase, StdAuthSecretValidationUseCase,
    StdAuthStatusUseCase,
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
        AuthCommands::Mode {
            provider,
            mode,
            path,
        } => handle_mode_action(&auth, provider, mode, path)?,
        AuthCommands::Hf { action } => {
            handle_provider_action(&auth, Provider::HuggingFace, action).await?
        }
        AuthCommands::Openai { action } => {
            handle_provider_action(&auth, Provider::OpenAI, action).await?
        }
        AuthCommands::Anthropic { action } => {
            handle_provider_action(&auth, Provider::Anthropic, action).await?
        }
        AuthCommands::Gemini { action } => {
            handle_provider_action(&auth, Provider::Gemini, action).await?
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
        StdAuthSecretResolverUseCase::new(
            &self.env_probe,
            &self.keychain_store,
            &self.metadata_store,
            &self.cache,
        )
    }

    fn mutation_usecase(&self) -> StdAuthSecretMutationUseCase<'_> {
        StdAuthSecretMutationUseCase::new(&self.keychain_store, &self.metadata_store, &self.cache)
    }

    fn preference_usecase(&self) -> StdAuthPreferenceUseCase<'_> {
        StdAuthPreferenceUseCase::new(&self.metadata_store)
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

    async fn best_effort_validated_status(
        &self,
        provider: Provider,
    ) -> miette::Result<AuthKeyStatus> {
        match self.validated_status(provider).await {
            Ok(status) => Ok(status),
            Err(_) => {
                let mut status = self.local_status(provider)?;
                status.validation = AuthValidationState::Unknown {
                    reason: format!(
                        "validation unavailable; run `tentgent auth {}` for details",
                        provider.cli_name()
                    ),
                };
                Ok(status)
            }
        }
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
        statuses.push(auth.best_effort_validated_status(provider).await?);
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
            "Mode",
            "Env",
            "Keychain",
            "Effective",
            "Validation",
            "Detail",
        ]);

    for status in statuses {
        table.add_row(vec![
            Cell::new(status.provider.display_name()),
            Cell::new(status.preference.source_mode.as_str()),
            Cell::new(env_presence(&status)),
            Cell::new(keychain_status(&status)),
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

fn handle_mode_action(
    auth: &CliAuthKernel,
    provider: Option<String>,
    mode: Option<String>,
    path: Option<PathBuf>,
) -> miette::Result<()> {
    match (provider, mode) {
        (None, None) => {
            let report = auth
                .preference_usecase()
                .list_preferences(AuthPreferenceListRequest::all())
                .into_diagnostic()?;
            render_auth_preferences(&report.preferences);
            Ok(())
        }
        (Some(provider), None) => {
            let provider = parse_provider_arg("auth mode", &provider)?;
            if path.is_some() {
                return Err(miette!("`--path` can only be used when setting file mode"));
            }
            let preference = auth
                .preference_usecase()
                .get_preference(AuthPreferenceRequest::new(provider))
                .into_diagnostic()?;
            render_auth_preferences(&[preference]);
            Ok(())
        }
        (Some(provider), Some(mode)) => {
            let provider = parse_provider_arg("auth mode", &provider)?;
            let source_mode = parse_auth_source_mode(&mode)?;
            let request = match (source_mode, path) {
                (AuthSourceMode::File, Some(path)) => {
                    SetAuthPreferenceRequest::new(provider, source_mode)
                        .with_env_file(absolutize_auth_path(path)?)
                }
                (AuthSourceMode::File, None) => {
                    return Err(miette!("file auth mode requires `--path <ENV_FILE>`"));
                }
                (_, Some(_)) => {
                    return Err(miette!("`--path` is only valid with file auth mode"));
                }
                (_, None) => SetAuthPreferenceRequest::new(provider, source_mode),
            };
            let preference = auth
                .preference_usecase()
                .set_preference(request)
                .into_diagnostic()?;
            println!(
                "{} {} auth mode set to {}.",
                style("updated").green().bold(),
                provider.display_name(),
                preference.source_mode
            );
            render_auth_preferences(&[preference]);
            Ok(())
        }
        (None, Some(_)) => Err(miette!(
            "missing provider for auth mode update; use `tentgent auth mode <provider> <mode>`"
        )),
    }
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

fn render_auth_preferences(preferences: &[AuthProviderPreference]) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Provider auth modes").bold()
    );

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["Provider", "Mode", "File"]);

    for preference in preferences {
        table.add_row(vec![
            Cell::new(preference.provider.display_name()),
            Cell::new(preference.source_mode.as_str()),
            Cell::new(
                preference
                    .env_file
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "-".to_string()),
            ),
        ]);
    }

    println!("{table}");
    println!();
}

fn parse_provider_arg(command: &str, value: &str) -> miette::Result<Provider> {
    Provider::ALL
        .into_iter()
        .find(|provider| provider.cli_name() == value)
        .ok_or_else(|| {
            miette!(
                "invalid provider `{value}` for `{command}`; expected one of hf, openai, anthropic, gemini"
            )
        })
}

fn parse_auth_source_mode(value: &str) -> miette::Result<AuthSourceMode> {
    match value {
        "auto" => Ok(AuthSourceMode::Auto),
        "keychain" => Ok(AuthSourceMode::Keychain),
        "file" => Ok(AuthSourceMode::File),
        "env" => Ok(AuthSourceMode::Env),
        "none" => Ok(AuthSourceMode::None),
        other => Err(miette!(
            "invalid auth mode `{other}`; expected one of auto, keychain, file, env, none"
        )),
    }
}

fn absolutize_auth_path(path: PathBuf) -> miette::Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path);
    }
    Ok(std::env::current_dir().into_diagnostic()?.join(path))
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

    let status = auth.local_status(provider)?;
    if matches!(status.effective_source, Some(AuthSecretSource::Env)) {
        println!(
            "{} .env/env currently overrides the keychain value for {}.",
            style("note").yellow().bold(),
            provider.display_name()
        );
    }
    if !matches!(
        status.preference.source_mode,
        AuthSourceMode::Auto | AuthSourceMode::Keychain
    ) {
        println!(
            "{} Current {} auth mode is `{}`; the stored keychain value will not be used until mode is changed.",
            style("note").yellow().bold(),
            provider.display_name(),
            status.preference.source_mode
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
        Cell::new("mode"),
        Cell::new(status.preference.source_mode.as_str()),
    ]);
    if let Some(path) = status.preference.env_file.as_ref() {
        table.add_row(vec![
            Cell::new("file"),
            Cell::new(path.display().to_string()),
        ]);
    }
    table.add_row(vec![Cell::new("env/file"), Cell::new(env_presence(status))]);
    table.add_row(vec![
        Cell::new("keychain entry"),
        Cell::new(keychain_status(status)),
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

fn env_presence(status: &AuthKeyStatus) -> &'static str {
    if !status.preference.source_mode.can_probe_env() {
        "skipped"
    } else {
        presence(status.env_present)
    }
}

fn keychain_status(status: &AuthKeyStatus) -> &'static str {
    if !status.preference.source_mode.can_probe_keychain() {
        "skipped"
    } else {
        keychain_presence(status.keychain_presence)
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
