use std::{
    env,
    path::{Path, PathBuf},
};

use clap::CommandFactory;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use miette::{miette, IntoDiagnostic, Result};
use serde_json::Value;
use tentgent_kernel::{
    features::{
        auth::{
            domain::{AuthEnvLoadPolicy, AuthSecretSource, AuthValidationState, Provider},
            infra::{
                FileAuthMetadataStore, ProcessSessionAuthSecretCache, ReqwestAuthSecretValidator,
                StdAuthEnvSecretProbe, SystemKeychainAuthSecretStore,
            },
            usecases::{
                AuthSecretResolutionRequest, AuthSecretValidationRequest,
                AuthSecretValidationUseCase, StdAuthSecretResolverUseCase,
                StdAuthSecretValidationUseCase,
            },
        },
        dataset::{
            domain::{
                DatasetDiffOutcome, DatasetDiffStatus, DatasetEvalSplit, DatasetExportOutcome,
                DatasetImportOutcome, DatasetInspection, DatasetMetadata, DatasetPromptSource,
                DatasetProvider, DatasetRefSelector, DatasetRemovalOutcome, DatasetSplitKind,
                DatasetSummary, DatasetSynthCounts, DatasetSynthPromptRequest, DatasetSynthRequest,
                DatasetTemplateRequest, DatasetValidationOutcome,
            },
            infra::{
                CloudDatasetEvalRuntimeClient, CloudDatasetSynthRuntimeClient,
                FileDatasetCatalogStore, FileDatasetContentStore, FileDatasetReferenceGuard,
                FileDatasetSourceIndexStore, MarkdownDatasetTemplateRenderer, StdDatasetDiffer,
                StdDatasetIdentityGenerator, StdDatasetManifestBuilder, StdDatasetPackageDetector,
                StdDatasetSourceStager, StdDatasetStoreLayoutInitializer, StdDatasetValidator,
            },
            usecases::{
                DatasetCatalogReadUseCase, DatasetDiffRequest, DatasetDiffRightSelection,
                DatasetDiffUseCase, DatasetEvaluateRequest, DatasetEvaluationInputSelection,
                DatasetEvaluationUseCase, DatasetExportRequest, DatasetExportUseCase,
                DatasetInspectRequest, DatasetListRequest, DatasetLocalImportRequest,
                DatasetLocalImportUseCase, DatasetRemoveRequest, DatasetSynthPromptRenderRequest,
                DatasetSynthesisUseCase, DatasetSynthesizeRequest, DatasetTemplateRenderRequest,
                DatasetTemplateUseCase, DatasetValidateRequest, DatasetValidationTargetSelection,
                DatasetValidationUseCase, StdDatasetCatalogReadUseCase, StdDatasetDiffUseCase,
                StdDatasetEvaluationUseCase, StdDatasetExportUseCase, StdDatasetLocalImportUseCase,
                StdDatasetRemoveUseCase, StdDatasetSynthesisUseCase, StdDatasetTemplateUseCase,
                StdDatasetValidationUseCase,
            },
        },
        runtime::{
            domain::PythonRuntimeResolutionInput, infra::StdPythonRuntimeResolver,
            usecases::StdRuntimeResolutionUseCase,
        },
    },
    foundation::{
        error::KernelError,
        layout::{
            LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
        },
    },
};

use super::app::Cli;
use super::commands::DatasetCommands;
use super::display::{format_bytes, format_size_transition};
use super::resource_mutation::project_resource_mutation;

pub async fn handle_dataset_command(action: DatasetCommands) -> Result<()> {
    let dataset = CliDatasetKernel::new();

    match action {
        DatasetCommands::Add { path } => {
            if is_help_path(&path) {
                print_dataset_subcommand_help("add")?;
                return Ok(());
            }

            let outcome = dataset
                .local_import_usecase()
                .import_local_dataset(DatasetLocalImportRequest {
                    layout: runtime_layout_input(LayoutResolveMode::Create),
                    source_path: path,
                })
                .into_diagnostic()?;
            render_import_outcome(&outcome.outcome);
        }
        DatasetCommands::Validate { path } => {
            if is_help_path(&path) {
                print_dataset_subcommand_help("validate")?;
                return Ok(());
            }

            let result = dataset
                .validation_usecase()
                .validate_dataset(DatasetValidateRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                    target: DatasetValidationTargetSelection::LocalPath(path),
                })
                .into_diagnostic()?;
            render_validation_outcome(&result.outcome);
            if !result.outcome.is_valid() {
                return Err(miette!(
                    "dataset validation failed with {} error(s)",
                    result.outcome.errors.len()
                ));
            }
        }
        DatasetCommands::Template {
            task,
            language,
            output,
        } => {
            let request = DatasetTemplateRequest::new(task, language);
            let result = dataset
                .template_usecase()
                .render_dataset_template(DatasetTemplateRenderRequest {
                    template: request.clone(),
                    output_path: output,
                })
                .into_diagnostic()?;
            if let Some(path) = result.output_path {
                render_template_written(&path, &request);
            } else {
                print!("{}", result.rendered.body);
            }
        }
        DatasetCommands::Synth {
            provider,
            model,
            output,
            brief,
            spec,
            split,
            count,
            train_count,
            valid_count,
            test_count,
            eval_count,
            max_tokens,
            temperature,
            timeout_seconds,
            retries,
            print_prompt,
        } => {
            let counts = DatasetSynthCounts {
                count,
                train_count,
                valid_count,
                test_count,
                eval_count,
            };
            let split = parse_dataset_split("synth", "--split <SPLIT>", &split)?;
            let prompt_source = dataset_prompt_source(brief.clone(), spec.as_deref())?;
            if print_prompt {
                let runtime_resolution = dataset.runtime_resolution_usecase();
                let auth_resolver = dataset.auth_resolver_usecase();
                let runtime_client = CloudDatasetSynthRuntimeClient::new();
                let synthesis = StdDatasetSynthesisUseCase::new(
                    &runtime_resolution,
                    &auth_resolver,
                    &runtime_client,
                );
                let result = synthesis
                    .render_synth_prompt(DatasetSynthPromptRenderRequest {
                        layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                        runtime: PythonRuntimeResolutionInput::default(),
                        prompt: DatasetSynthPromptRequest {
                            prompt_source,
                            split,
                            counts,
                        },
                    })
                    .await
                    .into_diagnostic()?;
                print!("{}", result.prompt);
                return Ok(());
            }

            let provider = provider
                .as_deref()
                .ok_or_else(|| {
                    miette!(
                        "missing required option `--provider`; use `--print-prompt` to inspect the prompt without provider settings"
                    )
                })
                .and_then(|provider| parse_dataset_provider("synth", "--provider <PROVIDER>", provider))?;
            let model = model.ok_or_else(|| {
                miette!(
                    "missing required option `--model`; use `--print-prompt` to inspect the prompt without provider settings"
                )
            })?;
            let output = output.ok_or_else(|| {
                miette!(
                    "missing required option `--output`; use `--print-prompt` to inspect the prompt without an output directory"
                )
            })?;
            let auth = dataset
                .validate_dataset_provider_auth(provider, "dataset synth")
                .await?;
            render_dataset_provider_auth_preflight(auth.provider, auth.source, "dataset synth");
            let output_dir = absolutize_cli_path(&output)?;
            render_dataset_synth_started(provider, &model, &output_dir, split, &counts, retries);
            let runtime_resolution = dataset.runtime_resolution_usecase();
            let auth_resolver = dataset.auth_resolver_usecase();
            let runtime_client = CloudDatasetSynthRuntimeClient::new();
            let synthesis = StdDatasetSynthesisUseCase::new(
                &runtime_resolution,
                &auth_resolver,
                &runtime_client,
            );
            let result = synthesis
                .synthesize_dataset(DatasetSynthesizeRequest {
                    layout: runtime_layout_input(LayoutResolveMode::Create),
                    runtime: PythonRuntimeResolutionInput::default(),
                    auth: AuthSecretResolutionRequest::for_secret_use(
                        provider_auth_provider(provider),
                        AuthEnvLoadPolicy::CwdDotenvOverride,
                    ),
                    synth: DatasetSynthRequest {
                        provider,
                        provider_model: model,
                        output_dir,
                        prompt_source,
                        split,
                        counts,
                        max_tokens,
                        temperature,
                        timeout_seconds,
                        retries,
                    },
                })
                .await
                .into_diagnostic()?;
            render_synth_outcome(&result.output.outcome);
        }
        DatasetCommands::Eval {
            input,
            provider,
            model,
            output,
            split,
            max_records,
            criteria,
            max_tokens,
            temperature,
            timeout_seconds,
        } => {
            if is_help_token(&input) {
                print_dataset_subcommand_help("eval")?;
                return Ok(());
            }

            let provider = parse_dataset_provider("eval", "--provider <PROVIDER>", &provider)?;
            let split = parse_dataset_eval_split("eval", "--split <SPLIT>", &split)?;
            let input_selection = dataset_eval_input_selection(&input)?;
            let output_dir = absolutize_cli_path(&output)?;
            ensure_dataset_eval_output_dir_ready(&output_dir)?;
            let auth = dataset
                .validate_dataset_provider_auth(provider, "dataset eval")
                .await?;
            render_dataset_provider_auth_preflight(auth.provider, auth.source, "dataset eval");
            render_dataset_eval_started(provider, &model, &input, &output_dir, split, max_records);
            let runtime_resolution = dataset.runtime_resolution_usecase();
            let auth_resolver = dataset.auth_resolver_usecase();
            let runtime_client = CloudDatasetEvalRuntimeClient::new();
            let evaluation = StdDatasetEvaluationUseCase::new(
                &runtime_resolution,
                &auth_resolver,
                &dataset.catalog,
                &runtime_client,
            );
            let result = evaluation
                .evaluate_dataset(DatasetEvaluateRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                    runtime: PythonRuntimeResolutionInput::default(),
                    auth: AuthSecretResolutionRequest::for_secret_use(
                        provider_auth_provider(provider),
                        AuthEnvLoadPolicy::CwdDotenvOverride,
                    ),
                    provider,
                    provider_model: model,
                    input: input_selection,
                    output_dir,
                    split,
                    max_records,
                    criteria: criteria.clone(),
                    max_tokens,
                    temperature,
                    timeout_seconds,
                })
                .await
                .into_diagnostic()?;
            render_eval_outcome(&result.report);
        }
        DatasetCommands::Ls => {
            let result = dataset
                .catalog_usecase()
                .list_datasets(DatasetListRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                })
                .into_diagnostic()?;
            render_dataset_list(&result.datasets);
        }
        DatasetCommands::Inspect { reference } => {
            if is_help_token(&reference) {
                print_dataset_subcommand_help("inspect")?;
                return Ok(());
            }

            let selector = parse_dataset_selector("inspect", "DATASET_REF", &reference)?;
            let inspection =
                match dataset
                    .catalog_usecase()
                    .inspect_dataset(DatasetInspectRequest {
                        layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                        selector,
                    }) {
                    Ok(result) => result.dataset,
                    Err(err) => return Err(explain_dataset_lookup_error("inspect", err)),
                };
            render_dataset_inspection(&inspection);
        }
        DatasetCommands::Export { reference, path } => {
            if is_help_token(&reference) {
                print_dataset_subcommand_help("export")?;
                return Ok(());
            }

            let Some(path) = path else {
                return Err(miette!(
                    "missing required argument `<PATH>`\n\nUsage: tentgent dataset export <DATASET_REF> <PATH>"
                ));
            };

            let selector = parse_dataset_selector("export", "DATASET_REF", &reference)?;
            let outcome = match dataset
                .export_usecase()
                .export_dataset(DatasetExportRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                    selector,
                    destination_path: path.clone(),
                }) {
                Ok(result) => result.outcome,
                Err(err) => return Err(explain_dataset_export_error(&reference, &path, err)),
            };
            render_export_outcome(&outcome);
        }
        DatasetCommands::Diff { left, right, path } => {
            if is_help_token(&left) {
                print_dataset_subcommand_help("diff")?;
                return Ok(());
            }

            let left = parse_dataset_selector("diff", "LEFT_REF", &left)?;
            let outcome = if let Some(path) = path {
                match dataset.diff_usecase().diff_dataset(DatasetDiffRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                    left,
                    right: DatasetDiffRightSelection::LocalPath(path),
                }) {
                    Ok(result) => result.outcome,
                    Err(err) => return Err(explain_dataset_lookup_error("diff", err)),
                }
            } else if let Some(right) = right {
                let right = parse_dataset_selector("diff", "RIGHT_REF", &right)?;
                match dataset.diff_usecase().diff_dataset(DatasetDiffRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                    left,
                    right: DatasetDiffRightSelection::ManagedDataset(right),
                }) {
                    Ok(result) => result.outcome,
                    Err(err) => return Err(explain_dataset_lookup_error("diff", err)),
                }
            } else {
                return Err(miette!(
                    "missing required argument `<RIGHT_REF>` or `--path <PATH>`\n\nUsage: tentgent dataset diff <LEFT_REF> <RIGHT_REF>\n       tentgent dataset diff <LEFT_REF> --path <PATH>"
                ));
            };
            render_diff_outcome(&outcome);
        }
        DatasetCommands::Rm { reference } => {
            if is_help_token(&reference) {
                print_dataset_subcommand_help("rm")?;
                return Ok(());
            }

            let selector = parse_dataset_selector("rm", "DATASET_REF", &reference)?;
            let outcome =
                match dataset
                    .remove_usecase()
                    .remove_dataset_guarded(DatasetRemoveRequest {
                        layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                        selector,
                    }) {
                    Ok(result) => project_resource_mutation(result)?.outcome,
                    Err(err) => return Err(explain_dataset_lookup_error("rm", err)),
                };
            render_removal_outcome(&outcome);
        }
    }

    Ok(())
}

struct CliDatasetKernel {
    layout_resolver: StdRuntimeLayoutResolver,
    runtime_resolver: StdPythonRuntimeResolver,
    env_probe: StdAuthEnvSecretProbe,
    keychain_store: SystemKeychainAuthSecretStore,
    metadata_store: FileAuthMetadataStore,
    cache: ProcessSessionAuthSecretCache,
    layout_initializer: StdDatasetStoreLayoutInitializer,
    stager: StdDatasetSourceStager,
    manifest_builder: StdDatasetManifestBuilder,
    identity: StdDatasetIdentityGenerator,
    package_detector: StdDatasetPackageDetector,
    catalog: FileDatasetCatalogStore,
    source_indexes: FileDatasetSourceIndexStore,
    content: FileDatasetContentStore,
    validator: StdDatasetValidator,
    differ: StdDatasetDiffer,
    template_renderer: MarkdownDatasetTemplateRenderer,
    reference_guard: FileDatasetReferenceGuard,
}

impl CliDatasetKernel {
    fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            runtime_resolver: StdPythonRuntimeResolver,
            env_probe: StdAuthEnvSecretProbe,
            keychain_store: SystemKeychainAuthSecretStore::new(),
            metadata_store: default_auth_metadata_store(),
            cache: ProcessSessionAuthSecretCache::new(),
            layout_initializer: StdDatasetStoreLayoutInitializer,
            stager: StdDatasetSourceStager,
            manifest_builder: StdDatasetManifestBuilder,
            identity: StdDatasetIdentityGenerator,
            package_detector: StdDatasetPackageDetector,
            catalog: FileDatasetCatalogStore,
            source_indexes: FileDatasetSourceIndexStore,
            content: FileDatasetContentStore,
            validator: StdDatasetValidator,
            differ: StdDatasetDiffer,
            template_renderer: MarkdownDatasetTemplateRenderer,
            reference_guard: FileDatasetReferenceGuard,
        }
    }

    fn catalog_usecase(&self) -> StdDatasetCatalogReadUseCase<'_> {
        StdDatasetCatalogReadUseCase::new(&self.layout_resolver, &self.catalog)
    }

    fn local_import_usecase(&self) -> StdDatasetLocalImportUseCase<'_> {
        StdDatasetLocalImportUseCase::new(
            &self.layout_resolver,
            &self.layout_initializer,
            &self.stager,
            &self.manifest_builder,
            &self.identity,
            &self.package_detector,
            &self.catalog,
            &self.source_indexes,
            &self.content,
        )
    }

    fn validation_usecase(&self) -> StdDatasetValidationUseCase<'_> {
        StdDatasetValidationUseCase::new(&self.layout_resolver, &self.catalog, &self.validator)
    }

    fn template_usecase(&self) -> StdDatasetTemplateUseCase<'_> {
        StdDatasetTemplateUseCase::new(&self.template_renderer)
    }

    fn export_usecase(&self) -> StdDatasetExportUseCase<'_> {
        StdDatasetExportUseCase::new(&self.layout_resolver, &self.catalog, &self.content)
    }

    fn diff_usecase(&self) -> StdDatasetDiffUseCase<'_> {
        StdDatasetDiffUseCase::new(&self.layout_resolver, &self.differ)
    }

    fn remove_usecase(&self) -> StdDatasetRemoveUseCase<'_> {
        StdDatasetRemoveUseCase::new(
            &self.layout_resolver,
            &self.catalog,
            &self.source_indexes,
            &self.content,
            &self.reference_guard,
        )
    }

    fn runtime_resolution_usecase(&self) -> StdRuntimeResolutionUseCase<'_> {
        StdRuntimeResolutionUseCase::new(&self.layout_resolver, &self.runtime_resolver)
    }

    fn auth_resolver_usecase(&self) -> StdAuthSecretResolverUseCase<'_> {
        StdAuthSecretResolverUseCase::new(
            &self.env_probe,
            &self.keychain_store,
            &self.metadata_store,
            &self.cache,
        )
    }

    async fn validate_dataset_provider_auth(
        &self,
        provider: DatasetProvider,
        purpose: &'static str,
    ) -> Result<DatasetProviderAuthPreflight> {
        let auth_provider = provider_auth_provider(provider);
        let layout = self
            .layout_resolver
            .resolve(runtime_layout_input(LayoutResolveMode::Create))
            .into_diagnostic()?;
        let metadata_store = FileAuthMetadataStore::from_layout(&layout);
        let validator = ReqwestAuthSecretValidator::new().into_diagnostic()?;
        let resolver = self.auth_resolver_usecase();
        let validation =
            StdAuthSecretValidationUseCase::new(&resolver, &validator, &metadata_store);
        let result = validation
            .validate_secret(AuthSecretValidationRequest::new(
                AuthSecretResolutionRequest::for_secret_validation(
                    auth_provider,
                    AuthEnvLoadPolicy::CwdDotenvOverride,
                ),
            ))
            .await
            .into_diagnostic()?;

        match &result.validation {
            AuthValidationState::Verified => Ok(DatasetProviderAuthPreflight {
                provider: auth_provider,
                source: result.source.ok_or_else(|| {
                    miette!(
                        "{} key was verified for {purpose}, but the key source was not recorded",
                        auth_provider.display_name()
                    )
                })?,
            }),
            AuthValidationState::Missing => Err(miette!(
                "{} API key is required for {purpose}. Run `tentgent auth set {}` or provide {}.",
                auth_provider.display_name(),
                auth_provider.cli_name(),
                auth_provider.env_var()
            )),
            AuthValidationState::Invalid { reason } => Err(miette!(
                "{} API key from {} is invalid for {purpose}: {reason}",
                auth_provider.display_name(),
                display_optional_source(result.source),
            )),
            AuthValidationState::Unknown { reason } => Err(miette!(
                "{} API key from {} could not be verified for {purpose}: {reason}",
                auth_provider.display_name(),
                display_optional_source(result.source),
            )),
            AuthValidationState::NotChecked => Err(miette!(
                "{} API key validation was not checked for {purpose}",
                auth_provider.display_name()
            )),
        }
    }
}

struct DatasetProviderAuthPreflight {
    provider: Provider,
    source: AuthSecretSource,
}

fn default_auth_metadata_store() -> FileAuthMetadataStore {
    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: None,
            data_root_dir: None,
        })
        .expect("runtime layout should resolve for auth metadata");
    FileAuthMetadataStore::from_layout(&layout)
}

fn runtime_layout_input(mode: LayoutResolveMode) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode,
        home_dir: None,
        data_root_dir: None,
    }
}

mod render;
use render::*;

fn dataset_eval_input_selection(input: &str) -> Result<DatasetEvaluationInputSelection> {
    let candidate = PathBuf::from(input);
    if candidate.exists() {
        return Ok(DatasetEvaluationInputSelection::LocalPath(
            absolutize_cli_path(&candidate)?,
        ));
    }

    Ok(DatasetEvaluationInputSelection::ManagedDataset(
        parse_dataset_selector("eval", "DATASET_REF|PATH", input)?,
    ))
}

fn dataset_prompt_source(
    brief: Option<String>,
    spec: Option<&Path>,
) -> Result<DatasetPromptSource> {
    if let Some(brief) = brief {
        return Ok(DatasetPromptSource::Brief(brief));
    }

    if let Some(spec) = spec {
        return Ok(DatasetPromptSource::SpecPath(absolutize_cli_path(spec)?));
    }

    Err(miette!(
        "missing generation input; provide `--brief <TEXT>` or `--spec <PATH>`"
    ))
}

fn absolutize_cli_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }

    Ok(env::current_dir().into_diagnostic()?.join(path))
}

fn ensure_dataset_eval_output_dir_ready(output_dir: &Path) -> Result<()> {
    if !output_dir.exists() {
        return Ok(());
    }
    if !output_dir.is_dir() {
        return Err(miette!(
            "dataset eval output path exists but is not a directory: {}",
            output_dir.display()
        ));
    }
    if output_dir
        .read_dir()
        .into_diagnostic()?
        .next()
        .transpose()
        .into_diagnostic()?
        .is_some()
    {
        return Err(miette!(
            "dataset eval output directory must be empty: {}. `eval` checks the input dataset and writes a separate report; use a fresh report directory, for example `./generated-dataset-eval`.",
            output_dir.display()
        ));
    }

    Ok(())
}

fn render_dataset_provider_auth_preflight(
    provider: Provider,
    source: AuthSecretSource,
    purpose: &'static str,
) {
    println!(
        "{} {} key verified from {} for {}.",
        style("verified").green().bold(),
        provider.display_name(),
        source,
        purpose
    );
}

fn render_dataset_synth_started(
    provider: DatasetProvider,
    model: &str,
    output_dir: &Path,
    split: DatasetSplitKind,
    counts: &DatasetSynthCounts,
    retries: u32,
) {
    println!(
        "{} dataset with {}:{} into {} ({}, retries={}). This may take a few minutes.",
        style("generating").cyan().bold(),
        provider.as_str(),
        model,
        output_dir.display(),
        synth_job_summary(split, counts),
        retries
    );
}

fn render_dataset_eval_started(
    provider: DatasetProvider,
    model: &str,
    input: &str,
    output_dir: &Path,
    split: DatasetEvalSplit,
    max_records: u32,
) {
    println!(
        "{} dataset with {}:{} from {} into {} (split={}, max_records={}). This may take a few minutes.",
        style("evaluating").cyan().bold(),
        provider.as_str(),
        model,
        input,
        output_dir.display(),
        split.as_str(),
        max_records
    );
}

fn synth_job_summary(split: DatasetSplitKind, counts: &DatasetSynthCounts) -> String {
    let mut parts = Vec::new();
    if let Some(count) = counts.train_count {
        parts.push(format!("train={count}"));
    }
    if let Some(count) = counts.valid_count {
        parts.push(format!("valid={count}"));
    }
    if let Some(count) = counts.test_count {
        parts.push(format!("test={count}"));
    }
    if let Some(count) = counts.eval_count {
        parts.push(format!("eval_cases={count}"));
    }
    if parts.is_empty() {
        match counts.count {
            Some(count) => format!("{}={count}", split.as_str()),
            None => split.as_str().to_string(),
        }
    } else {
        parts.join(", ")
    }
}

fn provider_auth_provider(provider: DatasetProvider) -> Provider {
    match provider {
        DatasetProvider::OpenAI => Provider::OpenAI,
        DatasetProvider::Anthropic => Provider::Anthropic,
        DatasetProvider::Gemini => Provider::Gemini,
    }
}

fn parse_dataset_provider(command: &str, value_name: &str, value: &str) -> Result<DatasetProvider> {
    match value.trim().to_ascii_lowercase().as_str() {
        "openai" => Ok(DatasetProvider::OpenAI),
        "anthropic" | "claude" => Ok(DatasetProvider::Anthropic),
        "gemini" | "google" => Ok(DatasetProvider::Gemini),
        _ => Err(usage_error(
            command,
            value_name,
            "provider must be one of: openai, anthropic, claude, gemini",
        )),
    }
}

fn parse_dataset_split(command: &str, value_name: &str, value: &str) -> Result<DatasetSplitKind> {
    match value.trim().to_ascii_lowercase().as_str() {
        "train" => Ok(DatasetSplitKind::Train),
        "valid" => Ok(DatasetSplitKind::Valid),
        "test" => Ok(DatasetSplitKind::Test),
        "eval_cases" => Ok(DatasetSplitKind::EvalCases),
        _ => Err(usage_error(
            command,
            value_name,
            "split must be one of: train, valid, test, eval_cases",
        )),
    }
}

fn parse_dataset_eval_split(
    command: &str,
    value_name: &str,
    value: &str,
) -> Result<DatasetEvalSplit> {
    match value.trim().to_ascii_lowercase().as_str() {
        "train" => Ok(DatasetEvalSplit::Train),
        "valid" => Ok(DatasetEvalSplit::Valid),
        "test" => Ok(DatasetEvalSplit::Test),
        "eval_cases" => Ok(DatasetEvalSplit::EvalCases),
        "all" => Ok(DatasetEvalSplit::All),
        _ => Err(usage_error(
            command,
            value_name,
            "split must be one of: train, valid, test, eval_cases, all",
        )),
    }
}

fn parse_dataset_selector(
    command: &str,
    value_name: &str,
    value: &str,
) -> Result<DatasetRefSelector> {
    DatasetRefSelector::parse(value).map_err(|err| usage_error(command, value_name, err))
}

fn display_optional_source(source: Option<AuthSecretSource>) -> String {
    source
        .map(|source| source.to_string())
        .unwrap_or_else(|| "unknown source".to_string())
}

fn json_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("-")
        .to_string()
}

fn json_usize_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn json_optional_number_field(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::Number(number)) => number.to_string(),
        _ => "-".to_string(),
    }
}

fn is_help_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|file_name| file_name.to_str())
        .is_some_and(is_help_token)
}

fn is_help_token(value: &str) -> bool {
    matches!(value, "help" | "--help" | "-h")
}

fn print_dataset_subcommand_help(name: &str) -> Result<()> {
    let mut root = Cli::command();
    let dataset = root
        .find_subcommand_mut("dataset")
        .ok_or_else(|| miette!("dataset command metadata is unavailable"))?;
    let subcommand = dataset
        .find_subcommand_mut(name)
        .ok_or_else(|| miette!("dataset subcommand `{name}` is unavailable"))?;
    subcommand.print_help().into_diagnostic()?;
    println!();
    Ok(())
}

fn explain_dataset_lookup_error(command: &str, err: KernelError) -> miette::Report {
    let message = err.to_string();
    if message.contains(" was not found") || message.contains(" is ambiguous") {
        return miette!(
            "{message}\n\nUsage: {}\nHint: use `tentgent dataset {command} --help` for the command template.",
            usage_for_command(command),
        );
    }

    miette!("{message}")
}

fn explain_dataset_export_error(reference: &str, path: &Path, err: KernelError) -> miette::Report {
    let message = err.to_string();
    if message.contains(" was not found") || message.contains(" is ambiguous") {
        return explain_dataset_lookup_error("export", err);
    }
    if message.contains("export destination already exists and is not empty") {
        let suggested_path = export_child_path(path, reference);
        return miette!(
            "{message}\n\nHint: export into a new child directory instead:\n  tentgent dataset export {reference} {}",
            suggested_path.display()
        );
    }

    miette!("{message}")
}

fn export_child_path(path: &Path, reference: &str) -> PathBuf {
    path.join(reference)
}

fn usage_for_command(command: &str) -> &'static str {
    match command {
        "diff" => "tentgent dataset diff <LEFT_REF> <RIGHT_REF>\n       tentgent dataset diff <LEFT_REF> -p <PATH>",
        "eval" => "tentgent dataset eval <DATASET_REF|PATH> -p <openai|anthropic|claude> -m <MODEL> -o <DIR>",
        "export" => "tentgent dataset export <DATASET_REF> <PATH>",
        "rm" => "tentgent dataset rm <DATASET_REF>",
        "synth" => "tentgent dataset synth -p <openai|anthropic|claude> -m <MODEL> -o <DIR> (-b <TEXT> | -s <PATH>)",
        "validate" => "tentgent dataset validate <PATH>",
        _ => "tentgent dataset inspect <DATASET_REF>",
    }
}

fn usage_error(command: &str, value_name: &str, message: impl std::fmt::Display) -> miette::Report {
    miette!(
        "{message}\n\nUsage: {}\nHint: use `tentgent dataset {command} --help` for the command template.\nArgument: <{value_name}>",
        usage_for_command(command),
    )
}
