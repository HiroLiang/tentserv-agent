use clap::CommandFactory;
use miette::{miette, IntoDiagnostic, Result};
use tentgent_kernel::features::adapter::domain::{
    AdapterBackendSupport, AdapterFormat, AdapterRefSelector, AdapterType, LoraScale,
};
use tentgent_kernel::features::adapter::infra::{
    FileAdapterBaseIndexStore, FileAdapterCatalogStore, FileAdapterContentStore,
    FileAdapterServerReferenceProbe, FileAdapterSourceIndexStore, StdAdapterIdentityGenerator,
    StdAdapterManifestBuilder, StdAdapterSourceMetadataReader, StdAdapterSourceStager,
    StdAdapterStoreLayoutInitializer, StdHfAdapterSnapshotFetcher,
};
use tentgent_kernel::features::adapter::usecases::{
    AdapterBindRequest, AdapterBindUseCase, AdapterCatalogReadUseCase, AdapterHfPullRequest,
    AdapterHfPullUseCase, AdapterImportOptions, AdapterInspectRequest, AdapterListRequest,
    AdapterLocalImportRequest, AdapterLocalImportUseCase, AdapterRemoveRequest,
    AdapterRemoveUseCase, StdAdapterBindUseCase, StdAdapterCatalogReadUseCase,
    StdAdapterHfPullUseCase, StdAdapterLocalImportUseCase, StdAdapterRemoveUseCase,
};
use tentgent_kernel::features::auth::domain::{AuthEnvLoadPolicy, Provider};
use tentgent_kernel::features::auth::infra::{
    ProcessSessionAuthSecretCache, StdAuthEnvSecretProbe, SystemKeychainAuthSecretStore,
};
use tentgent_kernel::features::auth::usecases::{
    AuthSecretResolutionRequest, StdAuthSecretResolverUseCase,
};
use tentgent_kernel::features::model::domain::{ModelCapability, ModelRefSelector};
use tentgent_kernel::features::model::infra::FileModelCatalogStore;
use tentgent_kernel::features::runtime::domain::PythonRuntimeResolutionInput;
use tentgent_kernel::features::runtime::infra::StdPythonRuntimeResolver;
use tentgent_kernel::foundation::error::KernelError;
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver,
};

use super::adapter_list::render_adapter_list;
use super::adapter_progress::PullProgress;
use super::adapter_render::{
    render_adapter_inspection, render_bind_outcome, render_import_outcome, render_removal_outcome,
};
use super::app::Cli;
use super::commands::AdapterCommands;

pub fn handle_adapter_command(action: AdapterCommands) -> Result<()> {
    let adapter = CliAdapterKernel::new();

    match action {
        AdapterCommands::Add {
            path,
            base_model_ref,
            target_capability,
            adapter_type,
            adapter_format,
            backend_support,
            control_kind,
            weight_file,
            trigger_word,
            recommended_scale,
        } => {
            let base_model_selector = parse_optional_model_selector(
                "add",
                "--base-model-ref <MODEL_REF>",
                base_model_ref.as_deref(),
            )?;
            let result = adapter
                .local_import_usecase()
                .import_local_adapter(AdapterLocalImportRequest {
                    layout: runtime_layout_input(LayoutResolveMode::Create),
                    source_path: path,
                    base_model_selector,
                    options: adapter_import_options(
                        target_capability.as_deref(),
                        adapter_type.as_deref(),
                        adapter_format.as_deref(),
                        &backend_support,
                        control_kind,
                        weight_file,
                        trigger_word,
                        recommended_scale,
                    )?,
                })
                .into_diagnostic()?;
            render_import_outcome("Adapter imported", &result.outcome);
        }
        AdapterCommands::Pull {
            repo_id,
            revision,
            base_model_ref,
            target_capability,
            adapter_type,
            adapter_format,
            backend_support,
            control_kind,
            weight_file,
            trigger_word,
            recommended_scale,
        } => {
            if is_help_token(&repo_id) {
                print_adapter_subcommand_help("pull")?;
                return Ok(());
            }

            let base_model_selector = parse_optional_model_selector(
                "pull",
                "--base-model-ref <MODEL_REF>",
                base_model_ref.as_deref(),
            )?;
            let mut progress = PullProgress::new(&repo_id, revision.as_deref());
            let auth_resolver = adapter.auth_resolver_usecase();
            let outcome = adapter.hf_pull_usecase(&auth_resolver).pull_hf_adapter(
                AdapterHfPullRequest {
                    layout: runtime_layout_input(LayoutResolveMode::Create),
                    runtime: PythonRuntimeResolutionInput::default(),
                    repo_id: repo_id.clone(),
                    revision,
                    base_model_selector,
                    options: adapter_import_options(
                        target_capability.as_deref(),
                        adapter_type.as_deref(),
                        adapter_format.as_deref(),
                        &backend_support,
                        control_kind,
                        weight_file,
                        trigger_word,
                        recommended_scale,
                    )?,
                    auth: AuthSecretResolutionRequest::for_secret_use(
                        Provider::HuggingFace,
                        AuthEnvLoadPolicy::CwdDotenvOverride,
                    ),
                },
                &mut |event| progress.update(event),
            );
            progress.finish();

            let outcome = outcome.into_diagnostic()?;
            render_import_outcome("Adapter pulled", &outcome.outcome);
        }
        AdapterCommands::Ls => {
            let result = adapter
                .catalog_usecase()
                .list_adapters(AdapterListRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                })
                .into_diagnostic()?;
            render_adapter_list(&result.adapters);
        }
        AdapterCommands::Inspect { reference } => {
            if is_help_token(&reference) {
                print_adapter_subcommand_help("inspect")?;
                return Ok(());
            }

            let selector = parse_adapter_selector("inspect", "ADAPTER_REF", &reference)?;
            let inspection =
                match adapter
                    .catalog_usecase()
                    .inspect_adapter(AdapterInspectRequest {
                        layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                        selector,
                    }) {
                    Ok(result) => result.adapter,
                    Err(err) => {
                        return Err(explain_adapter_lookup_error("inspect", "ADAPTER_REF", err))
                    }
                };
            render_adapter_inspection(&inspection);
        }
        AdapterCommands::Bind {
            adapter_ref,
            base_model_ref,
        } => {
            if is_help_token(&adapter_ref) {
                print_adapter_subcommand_help("bind")?;
                return Ok(());
            }

            let Some(base_model_ref) = base_model_ref else {
                return Err(miette!(
                    "missing required option `--base-model-ref <MODEL_REF>`\n\nUsage: tentgent adapter bind <ADAPTER_REF> --base-model-ref <MODEL_REF>"
                ));
            };

            let adapter_selector = parse_adapter_selector("bind", "ADAPTER_REF", &adapter_ref)?;
            let base_model_selector =
                parse_model_selector("bind", "MODEL_REF", base_model_ref.as_str())?;
            let outcome = match adapter.bind_usecase().bind_adapter(AdapterBindRequest {
                layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                adapter_selector,
                base_model_selector,
            }) {
                Ok(result) => result.outcome,
                Err(err) => return Err(explain_adapter_lookup_error("bind", "ADAPTER_REF", err)),
            };
            render_bind_outcome(&outcome);
        }
        AdapterCommands::Rm { reference } => {
            if is_help_token(&reference) {
                print_adapter_subcommand_help("rm")?;
                return Ok(());
            }

            let selector = parse_adapter_selector("rm", "ADAPTER_REF", &reference)?;
            let outcome = match adapter
                .remove_usecase()
                .remove_adapter(AdapterRemoveRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                    selector,
                }) {
                Ok(result) => result.outcome,
                Err(err) => return Err(explain_adapter_lookup_error("rm", "ADAPTER_REF", err)),
            };
            render_removal_outcome(&outcome);
        }
    }

    Ok(())
}

struct CliAdapterKernel {
    layout_resolver: StdRuntimeLayoutResolver,
    runtime_resolver: StdPythonRuntimeResolver,
    env_probe: StdAuthEnvSecretProbe,
    keychain_store: SystemKeychainAuthSecretStore,
    cache: ProcessSessionAuthSecretCache,
    layout_initializer: StdAdapterStoreLayoutInitializer,
    stager: StdAdapterSourceStager,
    snapshot_fetcher: StdHfAdapterSnapshotFetcher,
    manifest_builder: StdAdapterManifestBuilder,
    identity: StdAdapterIdentityGenerator,
    source_metadata_reader: StdAdapterSourceMetadataReader,
    adapter_catalog: FileAdapterCatalogStore,
    source_indexes: FileAdapterSourceIndexStore,
    base_indexes: FileAdapterBaseIndexStore,
    content: FileAdapterContentStore,
    server_refs: FileAdapterServerReferenceProbe,
    model_catalog: FileModelCatalogStore,
}

impl CliAdapterKernel {
    fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            runtime_resolver: StdPythonRuntimeResolver,
            env_probe: StdAuthEnvSecretProbe,
            keychain_store: SystemKeychainAuthSecretStore::new(),
            cache: ProcessSessionAuthSecretCache::new(),
            layout_initializer: StdAdapterStoreLayoutInitializer,
            stager: StdAdapterSourceStager,
            snapshot_fetcher: StdHfAdapterSnapshotFetcher,
            manifest_builder: StdAdapterManifestBuilder,
            identity: StdAdapterIdentityGenerator,
            source_metadata_reader: StdAdapterSourceMetadataReader,
            adapter_catalog: FileAdapterCatalogStore,
            source_indexes: FileAdapterSourceIndexStore,
            base_indexes: FileAdapterBaseIndexStore,
            content: FileAdapterContentStore,
            server_refs: FileAdapterServerReferenceProbe,
            model_catalog: FileModelCatalogStore,
        }
    }

    fn catalog_usecase(&self) -> StdAdapterCatalogReadUseCase<'_> {
        StdAdapterCatalogReadUseCase::new(&self.layout_resolver, &self.adapter_catalog)
    }

    fn local_import_usecase(&self) -> StdAdapterLocalImportUseCase<'_> {
        StdAdapterLocalImportUseCase::new(
            &self.layout_resolver,
            &self.layout_initializer,
            &self.stager,
            &self.manifest_builder,
            &self.identity,
            &self.source_metadata_reader,
            &self.adapter_catalog,
            &self.source_indexes,
            &self.base_indexes,
            &self.content,
            &self.model_catalog,
        )
    }

    fn hf_pull_usecase<'a>(
        &'a self,
        auth_resolver: &'a StdAuthSecretResolverUseCase<'a>,
    ) -> StdAdapterHfPullUseCase<'a> {
        StdAdapterHfPullUseCase::new(
            &self.layout_resolver,
            &self.runtime_resolver,
            auth_resolver,
            &self.layout_initializer,
            &self.stager,
            &self.snapshot_fetcher,
            &self.manifest_builder,
            &self.identity,
            &self.source_metadata_reader,
            &self.adapter_catalog,
            &self.source_indexes,
            &self.base_indexes,
            &self.content,
            &self.model_catalog,
        )
    }

    fn bind_usecase(&self) -> StdAdapterBindUseCase<'_> {
        StdAdapterBindUseCase::new(
            &self.layout_resolver,
            &self.adapter_catalog,
            &self.source_metadata_reader,
            &self.base_indexes,
            &self.model_catalog,
        )
    }

    fn remove_usecase(&self) -> StdAdapterRemoveUseCase<'_> {
        StdAdapterRemoveUseCase::new(
            &self.layout_resolver,
            &self.adapter_catalog,
            &self.source_indexes,
            &self.base_indexes,
            &self.content,
            &self.server_refs,
        )
    }

    fn auth_resolver_usecase(&self) -> StdAuthSecretResolverUseCase<'_> {
        StdAuthSecretResolverUseCase::new(&self.env_probe, &self.keychain_store, &self.cache)
    }
}

fn runtime_layout_input(mode: LayoutResolveMode) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode,
        home_dir: None,
        data_root_dir: None,
    }
}

fn is_help_token(value: &str) -> bool {
    matches!(value, "help" | "--help" | "-h")
}

fn parse_adapter_selector(
    command: &str,
    value_name: &str,
    value: &str,
) -> Result<AdapterRefSelector> {
    AdapterRefSelector::parse(value).map_err(|err| usage_error(command, value_name, err))
}

fn parse_model_selector(command: &str, value_name: &str, value: &str) -> Result<ModelRefSelector> {
    ModelRefSelector::parse(value).map_err(|err| usage_error(command, value_name, err))
}

fn parse_optional_model_selector(
    command: &str,
    value_name: &str,
    value: Option<&str>,
) -> Result<Option<ModelRefSelector>> {
    value
        .map(|value| parse_model_selector(command, value_name, value))
        .transpose()
}

fn adapter_import_options(
    target_capability: Option<&str>,
    adapter_type: Option<&str>,
    adapter_format: Option<&str>,
    backend_support: &[String],
    control_kind: Option<String>,
    weight_file: Option<String>,
    trigger_word: Vec<String>,
    recommended_scale: Option<f32>,
) -> Result<AdapterImportOptions> {
    let target_capability = target_capability
        .map(|value| {
            value
                .parse::<ModelCapability>()
                .map_err(|err| miette!("invalid --target-capability: {err}"))
        })
        .transpose()?;
    let adapter_type = adapter_type
        .map(|value| {
            value
                .parse::<AdapterType>()
                .map_err(|err| miette!("invalid --adapter-type: {err}"))
        })
        .transpose()?;
    let adapter_format = adapter_format
        .map(|value| {
            value
                .parse::<AdapterFormat>()
                .map_err(|err| miette!("invalid --adapter-format: {err}"))
        })
        .transpose()?;
    let backend_support = backend_support
        .iter()
        .map(|value| {
            value
                .parse::<AdapterBackendSupport>()
                .map_err(|err| miette!("invalid --backend-support: {err}"))
        })
        .collect::<Result<Vec<_>>>()?;
    let trigger_words = trigger_word
        .into_iter()
        .filter_map(non_empty_string)
        .collect::<Vec<_>>();
    let recommended_scale = recommended_scale
        .map(|value| LoraScale::new(value).map_err(|err| miette!("{err}")))
        .transpose()?;

    Ok(AdapterImportOptions {
        adapter_type,
        target_capability,
        adapter_format,
        backend_support,
        control_kind: control_kind.and_then(non_empty_string),
        weight_file: weight_file.and_then(non_empty_string),
        trigger_words,
        recommended_scale,
    })
}

fn non_empty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn print_adapter_subcommand_help(name: &str) -> Result<()> {
    let mut root = Cli::command();
    let adapter = root
        .find_subcommand_mut("adapter")
        .ok_or_else(|| miette!("adapter command metadata is unavailable"))?;
    let subcommand = adapter
        .find_subcommand_mut(name)
        .ok_or_else(|| miette!("adapter subcommand `{name}` is unavailable"))?;
    subcommand.print_help().into_diagnostic()?;
    println!();
    Ok(())
}

fn explain_adapter_lookup_error(
    command: &str,
    value_name: &str,
    err: KernelError,
) -> miette::Report {
    let message = err.to_string();
    if message.contains(" was not found") || message.contains(" is ambiguous") {
        return usage_error(command, value_name, message);
    }

    miette!("{message}")
}

fn usage_error(command: &str, value_name: &str, message: impl std::fmt::Display) -> miette::Report {
    let usage = match command {
        "bind" => "tentgent adapter bind <ADAPTER_REF> --base-model-ref <MODEL_REF>".to_string(),
        "rm" => "tentgent adapter rm <ADAPTER_REF>".to_string(),
        _ => format!("tentgent adapter {command} <{value_name}>"),
    };
    miette!(
        "{message}\n\nUsage: {usage}\nHint: use `tentgent adapter {command} --help` for the command template."
    )
}
