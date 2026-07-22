use std::collections::HashMap;

use clap::CommandFactory;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use miette::{miette, IntoDiagnostic, Result};
use tentgent_kernel::features::auth::domain::{AuthEnvLoadPolicy, Provider};
use tentgent_kernel::features::auth::infra::{
    FileAuthMetadataStore, ProcessSessionAuthSecretCache, StdAuthEnvSecretProbe,
    SystemKeychainAuthSecretStore,
};
use tentgent_kernel::features::auth::usecases::{
    AuthSecretResolutionRequest, StdAuthSecretResolverUseCase,
};
use tentgent_kernel::features::model::domain::{
    HfModelPullProgress, MlxRuntimeFamily, ModelCapability, ModelCapabilityProof, ModelFormat,
    ModelImportOutcome, ModelInspection, ModelMetadata, ModelRef, ModelRefSelector,
    ModelRemovalOutcome, ModelStoreLayout, ModelSummary, MODEL_CAPABILITY_CANONICAL_ORDER,
};
use tentgent_kernel::features::model::file_diagnostics::model_file_diagnostics;
use tentgent_kernel::features::model::infra::{
    FileModelCapabilityProofStore, FileModelCatalogStore, FileModelContentStore,
    FileModelServerReferenceProbe, FileModelSourceIndexStore, StdHfModelSnapshotFetcher,
    StdModelIdentityGenerator, StdModelManifestBuilder, StdModelSourceStager,
    StdModelStoreLayoutInitializer, SystemModelClock,
};
use tentgent_kernel::features::model::ports::ModelCapabilityProofStore;
use tentgent_kernel::features::model::support_catalog::{
    built_in_catalog_entries_for_model, built_in_model_support_catalog, ModelSupportCatalogEntry,
    ModelSupportCatalogLevel,
};
use tentgent_kernel::features::model::usecases::{
    ModelCapabilityMutation, ModelCapabilityProofClearRequest, ModelCapabilityProofClearResult,
    ModelCapabilityProofListRequest, ModelCapabilityProofUseCase, ModelCapabilityUpdateRequest,
    ModelCapabilityUpdateResult, ModelCapabilityVerifyRequest, ModelCatalogReadUseCase,
    ModelHfPullRequest, ModelHfPullUseCase, ModelInspectRequest, ModelListRequest,
    ModelLocalImportRequest, ModelLocalImportUseCase, ModelRemoveRequest,
    StdModelCapabilityProofUseCase, StdModelCapabilityUpdateUseCase, StdModelCatalogReadUseCase,
    StdModelHfPullUseCase, StdModelLocalImportUseCase, StdModelRemoveUseCase,
};
use tentgent_kernel::features::runtime::domain::PythonRuntimeResolutionInput;
use tentgent_kernel::features::runtime::infra::StdPythonRuntimeResolver;
use tentgent_kernel::foundation::error::KernelError;
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
};

use super::app::Cli;
use super::commands::{ModelCapabilityCommands, ModelCapabilityProofCommands, ModelCommands};
use super::display::format_bytes;
use super::model_support::{
    model_support_diagnostic_lines, model_support_list_label, model_support_summaries,
};
use super::resource_mutation::project_resource_mutation;

pub fn handle_model_command(action: ModelCommands) -> Result<()> {
    let model = CliModelKernel::new();

    match action {
        ModelCommands::Add { path, capability } => {
            let result = model
                .local_import_usecase()
                .import_local_model(ModelLocalImportRequest {
                    layout: runtime_layout_input(LayoutResolveMode::Create),
                    source_path: path,
                    capability,
                })
                .into_diagnostic()?;
            render_import_outcome("Model imported", &result.outcome);
            render_capability_warning(&result.outcome.metadata);
        }
        ModelCommands::Pull {
            repo_id,
            revision,
            capability,
        } => {
            let mut progress = PullProgress::new(&repo_id, revision.as_deref());
            let auth_resolver = model.auth_resolver_usecase();
            let outcome = model.hf_pull_usecase(&auth_resolver).pull_hf_model(
                ModelHfPullRequest {
                    layout: runtime_layout_input(LayoutResolveMode::Create),
                    runtime: PythonRuntimeResolutionInput::default(),
                    repo_id: repo_id.clone(),
                    revision,
                    capability,
                    auth: AuthSecretResolutionRequest::for_secret_use(
                        Provider::HuggingFace,
                        AuthEnvLoadPolicy::CwdDotenvOverride,
                    ),
                },
                &mut |event| progress.update(event),
            );
            progress.finish();

            let outcome = outcome.into_diagnostic()?;
            render_import_outcome("Model pulled", &outcome.outcome);
            render_capability_warning(&outcome.outcome.metadata);
        }
        ModelCommands::Catalog {
            capability,
            publisher,
            support_level,
            local,
            query,
        } => {
            let support_level = support_level
                .as_deref()
                .map(parse_catalog_support_level)
                .transpose()?;
            let catalog = built_in_model_support_catalog().into_diagnostic()?;
            let entries = filter_model_catalog_entries(
                catalog.models,
                capability,
                publisher.as_deref(),
                support_level,
                local,
                query.as_deref(),
            );
            render_model_catalog(&entries);
        }
        ModelCommands::Ls => {
            let result = model
                .catalog_usecase()
                .list_models(ModelListRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                })
                .into_diagnostic()?;
            let proofs = model_proofs_for_summaries(&model.proofs, &result.store, &result.models)
                .into_diagnostic()?;
            render_model_list(&result.models, &proofs);
        }
        ModelCommands::Rm { hash } => {
            if is_help_token(&hash) {
                print_model_subcommand_help("rm")?;
                return Ok(());
            }

            let selector = parse_model_selector("rm", "HASH", &hash)?;
            let outcome = match model
                .remove_usecase()
                .remove_model_guarded(ModelRemoveRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                    selector,
                }) {
                Ok(outcome) => project_resource_mutation(outcome)?,
                Err(err) => return Err(explain_model_lookup_error("rm", "HASH", err)),
            };
            render_model_removal(&outcome.outcome);
        }
        ModelCommands::Inspect { reference } => {
            if is_help_token(&reference) {
                print_model_subcommand_help("inspect")?;
                return Ok(());
            }

            let selector = parse_model_selector("inspect", "REF", &reference)?;
            let inspection = match model.catalog_usecase().inspect_model(ModelInspectRequest {
                layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                selector: selector.clone(),
            }) {
                Ok(result) => result.model,
                Err(err) => return Err(explain_model_lookup_error("inspect", "REF", err)),
            };
            let proofs = match model
                .capability_proof_usecase()
                .list_model_capability_proofs(ModelCapabilityProofListRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                    selector,
                }) {
                Ok(result) => result.proofs,
                Err(err) => return Err(explain_model_lookup_error("inspect", "REF", err)),
            };
            render_model_inspection(&inspection, &proofs);
        }
        ModelCommands::Capability { action } => {
            handle_model_capability_command(&model, action)?;
        }
        ModelCommands::SetCapability {
            reference,
            capability,
        } => {
            if is_help_token(&reference) {
                print_model_subcommand_help("set-capability")?;
                return Ok(());
            }

            let selector = parse_model_selector("set-capability", "REF", &reference)?;
            let result = match model
                .capability_update_usecase()
                .update_model_capability_guarded(ModelCapabilityUpdateRequest {
                    layout: runtime_layout_input(LayoutResolveMode::Create),
                    selector,
                    mutation: ModelCapabilityMutation::Set(vec![capability]),
                }) {
                Ok(result) => project_resource_mutation(result)?,
                Err(err) => return Err(explain_model_lookup_error("set-capability", "REF", err)),
            };
            render_model_capability_update(&result);
        }
    }

    Ok(())
}

fn handle_model_capability_command(
    model: &CliModelKernel,
    action: ModelCapabilityCommands,
) -> Result<()> {
    match action {
        ModelCapabilityCommands::Show { reference } => {
            if is_help_token(&reference) {
                print_model_capability_subcommand_help("show")?;
                return Ok(());
            }

            let selector = parse_model_selector("capability show", "REF", &reference)?;
            let inspection = match model.catalog_usecase().inspect_model(ModelInspectRequest {
                layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                selector,
            }) {
                Ok(result) => result.model,
                Err(err) => {
                    return Err(explain_model_lookup_error("capability show", "REF", err));
                }
            };
            render_model_capability_show(&inspection);
        }
        ModelCapabilityCommands::Set {
            reference,
            capabilities,
        } => {
            update_model_capabilities(
                model,
                "capability set",
                reference,
                ModelCapabilityMutation::Set(capabilities),
            )?;
        }
        ModelCapabilityCommands::Add {
            reference,
            capabilities,
        } => {
            update_model_capabilities(
                model,
                "capability add",
                reference,
                ModelCapabilityMutation::AddRemove {
                    add: capabilities,
                    remove: vec![],
                },
            )?;
        }
        ModelCapabilityCommands::Remove {
            reference,
            capabilities,
        } => {
            update_model_capabilities(
                model,
                "capability remove",
                reference,
                ModelCapabilityMutation::AddRemove {
                    add: vec![],
                    remove: capabilities,
                },
            )?;
        }
        ModelCapabilityCommands::Proofs { reference } => {
            if is_help_token(&reference) {
                print_model_capability_subcommand_help("proofs")?;
                return Ok(());
            }

            let selector = parse_model_selector("capability proofs", "REF", &reference)?;
            let result = match model
                .capability_proof_usecase()
                .list_model_capability_proofs(ModelCapabilityProofListRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                    selector,
                }) {
                Ok(result) => result,
                Err(err) => {
                    return Err(explain_model_lookup_error("capability proofs", "REF", err));
                }
            };
            render_model_capability_proofs(&result.model, &result.proofs);
        }
        ModelCapabilityCommands::Proof { action } => {
            handle_model_capability_proof_command(model, action)?;
        }
        ModelCapabilityCommands::Verify {
            reference,
            capability,
        } => {
            if is_help_token(&reference) {
                print_model_capability_subcommand_help("verify")?;
                return Ok(());
            }

            let selector = parse_model_selector("capability verify", "REF", &reference)?;
            let result = match model.capability_proof_usecase().verify_model_capability(
                ModelCapabilityVerifyRequest {
                    layout: runtime_layout_input(LayoutResolveMode::Create),
                    selector,
                    capability,
                },
            ) {
                Ok(result) => result,
                Err(err) => {
                    return Err(explain_model_lookup_error("capability verify", "REF", err));
                }
            };
            render_model_capability_verify(&result.model, &result.proof);
        }
    }

    Ok(())
}

fn handle_model_capability_proof_command(
    model: &CliModelKernel,
    action: ModelCapabilityProofCommands,
) -> Result<()> {
    match action {
        ModelCapabilityProofCommands::Clear {
            reference,
            capability,
        } => {
            if is_help_token(&reference) {
                print_model_capability_proof_subcommand_help("clear")?;
                return Ok(());
            }

            let selector = parse_model_selector("capability proof clear", "REF", &reference)?;
            let result = match model
                .capability_proof_usecase()
                .clear_model_capability_proofs(ModelCapabilityProofClearRequest {
                    layout: runtime_layout_input(LayoutResolveMode::Create),
                    selector,
                    capability,
                }) {
                Ok(result) => result,
                Err(err) => {
                    return Err(explain_model_lookup_error(
                        "capability proof clear",
                        "REF",
                        err,
                    ));
                }
            };
            render_model_capability_proof_clear(&result);
        }
    }

    Ok(())
}

fn update_model_capabilities(
    model: &CliModelKernel,
    command: &str,
    reference: String,
    mutation: ModelCapabilityMutation,
) -> Result<()> {
    if is_help_token(&reference) {
        let name = command
            .strip_prefix("capability ")
            .expect("capability command prefix");
        print_model_capability_subcommand_help(name)?;
        return Ok(());
    }

    let selector = parse_model_selector(command, "REF", &reference)?;
    let result = match model
        .capability_update_usecase()
        .update_model_capability_guarded(ModelCapabilityUpdateRequest {
            layout: runtime_layout_input(LayoutResolveMode::Create),
            selector,
            mutation,
        }) {
        Ok(result) => project_resource_mutation(result)?,
        Err(err) => return Err(explain_model_lookup_error(command, "REF", err)),
    };
    render_model_capability_update(&result);
    Ok(())
}

struct CliModelKernel {
    layout_resolver: StdRuntimeLayoutResolver,
    runtime_resolver: StdPythonRuntimeResolver,
    env_probe: StdAuthEnvSecretProbe,
    keychain_store: SystemKeychainAuthSecretStore,
    metadata_store: FileAuthMetadataStore,
    cache: ProcessSessionAuthSecretCache,
    layout_initializer: StdModelStoreLayoutInitializer,
    stager: StdModelSourceStager,
    snapshot_fetcher: StdHfModelSnapshotFetcher,
    manifest_builder: StdModelManifestBuilder,
    identity: StdModelIdentityGenerator,
    catalog: FileModelCatalogStore,
    source_indexes: FileModelSourceIndexStore,
    content: FileModelContentStore,
    server_refs: FileModelServerReferenceProbe,
    proofs: FileModelCapabilityProofStore,
    clock: SystemModelClock,
}

impl CliModelKernel {
    fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            runtime_resolver: StdPythonRuntimeResolver,
            env_probe: StdAuthEnvSecretProbe,
            keychain_store: SystemKeychainAuthSecretStore::new(),
            metadata_store: default_auth_metadata_store(),
            cache: ProcessSessionAuthSecretCache::new(),
            layout_initializer: StdModelStoreLayoutInitializer,
            stager: StdModelSourceStager,
            snapshot_fetcher: StdHfModelSnapshotFetcher,
            manifest_builder: StdModelManifestBuilder,
            identity: StdModelIdentityGenerator,
            catalog: FileModelCatalogStore,
            source_indexes: FileModelSourceIndexStore,
            content: FileModelContentStore,
            server_refs: FileModelServerReferenceProbe,
            proofs: FileModelCapabilityProofStore,
            clock: SystemModelClock,
        }
    }

    fn catalog_usecase(&self) -> StdModelCatalogReadUseCase<'_> {
        StdModelCatalogReadUseCase::new(&self.layout_resolver, &self.catalog)
    }

    fn local_import_usecase(&self) -> StdModelLocalImportUseCase<'_> {
        StdModelLocalImportUseCase::new(
            &self.layout_resolver,
            &self.layout_initializer,
            &self.stager,
            &self.manifest_builder,
            &self.identity,
            &self.catalog,
            &self.source_indexes,
            &self.content,
        )
    }

    fn hf_pull_usecase<'a>(
        &'a self,
        auth_resolver: &'a StdAuthSecretResolverUseCase<'a>,
    ) -> StdModelHfPullUseCase<'a> {
        StdModelHfPullUseCase::new(
            &self.layout_resolver,
            &self.runtime_resolver,
            auth_resolver,
            &self.layout_initializer,
            &self.stager,
            &self.snapshot_fetcher,
            &self.manifest_builder,
            &self.identity,
            &self.catalog,
            &self.source_indexes,
            &self.content,
        )
    }

    fn remove_usecase(&self) -> StdModelRemoveUseCase<'_> {
        StdModelRemoveUseCase::new(
            &self.layout_resolver,
            &self.catalog,
            &self.source_indexes,
            &self.content,
            &self.server_refs,
        )
    }

    fn capability_update_usecase(&self) -> StdModelCapabilityUpdateUseCase<'_> {
        StdModelCapabilityUpdateUseCase::new(
            &self.layout_resolver,
            &self.catalog,
            &self.server_refs,
        )
    }

    fn capability_proof_usecase(&self) -> StdModelCapabilityProofUseCase<'_> {
        StdModelCapabilityProofUseCase::new(
            &self.layout_resolver,
            &self.catalog,
            &self.proofs,
            &self.clock,
        )
    }

    fn auth_resolver_usecase(&self) -> StdAuthSecretResolverUseCase<'_> {
        StdAuthSecretResolverUseCase::new(
            &self.env_probe,
            &self.keychain_store,
            &self.metadata_store,
            &self.cache,
        )
    }
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

fn model_proofs_for_summaries(
    proof_store: &dyn ModelCapabilityProofStore,
    store: &ModelStoreLayout,
    models: &[ModelSummary],
) -> tentgent_kernel::foundation::error::KernelResult<HashMap<ModelRef, Vec<ModelCapabilityProof>>>
{
    let mut proofs = HashMap::new();
    for model in models {
        let model_proofs = proof_store.list_capability_proofs(store, &model.metadata.model_ref)?;
        proofs.insert(model.metadata.model_ref.clone(), model_proofs);
    }
    Ok(proofs)
}

mod render;
use render::*;

fn is_help_token(value: &str) -> bool {
    matches!(value, "help" | "--help" | "-h")
}

fn parse_model_selector(command: &str, value_name: &str, value: &str) -> Result<ModelRefSelector> {
    ModelRefSelector::parse(value).map_err(|err| usage_error(command, value_name, err))
}

fn parse_catalog_support_level(value: &str) -> Result<ModelSupportCatalogLevel> {
    match value.trim() {
        "fixture-supported" => Ok(ModelSupportCatalogLevel::FixtureSupported),
        "local-runtime-supported" => Ok(ModelSupportCatalogLevel::LocalRuntimeSupported),
        "catalog-known" => Ok(ModelSupportCatalogLevel::CatalogKnown),
        "requires-external-runtime" => Ok(ModelSupportCatalogLevel::RequiresExternalRuntime),
        "known-unsupported" => Ok(ModelSupportCatalogLevel::KnownUnsupported),
        "deprecated" => Ok(ModelSupportCatalogLevel::Deprecated),
        other => Err(miette!(
            "unsupported model catalog support level `{other}`\n\nExpected one of: fixture-supported, local-runtime-supported, catalog-known, requires-external-runtime, known-unsupported, deprecated\nHint: use `tentgent model catalog --help` for filters."
        )),
    }
}

fn print_model_subcommand_help(name: &str) -> miette::Result<()> {
    let mut root = Cli::command();
    let model = root
        .find_subcommand_mut("model")
        .ok_or_else(|| miette!("model command metadata is unavailable"))?;
    let subcommand = model
        .find_subcommand_mut(name)
        .ok_or_else(|| miette!("model subcommand `{name}` is unavailable"))?;
    subcommand.print_help().into_diagnostic()?;
    println!();
    Ok(())
}

fn print_model_capability_subcommand_help(name: &str) -> miette::Result<()> {
    let mut root = Cli::command();
    let model = root
        .find_subcommand_mut("model")
        .ok_or_else(|| miette!("model command metadata is unavailable"))?;
    let capability = model
        .find_subcommand_mut("capability")
        .ok_or_else(|| miette!("model capability command metadata is unavailable"))?;
    let subcommand = capability
        .find_subcommand_mut(name)
        .ok_or_else(|| miette!("model capability subcommand `{name}` is unavailable"))?;
    subcommand.print_help().into_diagnostic()?;
    println!();
    Ok(())
}

fn print_model_capability_proof_subcommand_help(name: &str) -> miette::Result<()> {
    let mut root = Cli::command();
    let model = root
        .find_subcommand_mut("model")
        .ok_or_else(|| miette!("model command metadata is unavailable"))?;
    let capability = model
        .find_subcommand_mut("capability")
        .ok_or_else(|| miette!("model capability command metadata is unavailable"))?;
    let proof = capability
        .find_subcommand_mut("proof")
        .ok_or_else(|| miette!("model capability proof command metadata is unavailable"))?;
    let subcommand = proof
        .find_subcommand_mut(name)
        .ok_or_else(|| miette!("model capability proof subcommand `{name}` is unavailable"))?;
    subcommand.print_help().into_diagnostic()?;
    println!();
    Ok(())
}

fn explain_model_lookup_error(command: &str, value_name: &str, err: KernelError) -> miette::Report {
    let message = err.to_string();
    if message.contains(" was not found") || message.contains(" is ambiguous") {
        return usage_error(command, value_name, message);
    }

    miette!("{message}")
}

fn usage_error(command: &str, value_name: &str, message: impl std::fmt::Display) -> miette::Report {
    miette!(
        "{message}\n\nUsage: tentgent model {command} <{value_name}>\nHint: use `tentgent model {command} --help` for the command template."
    )
}

fn model_format_support_summary(format: ModelFormat) -> String {
    match format {
        ModelFormat::Mlx if cfg!(all(target_os = "macos", target_arch = "aarch64")) => {
            "enabled: MLX is enabled on Apple Silicon macOS".to_string()
        }
        ModelFormat::Mlx => {
            "unsupported: MLX is supported only on Apple Silicon macOS".to_string()
        }
        ModelFormat::Safetensors => {
            "dependency-gated: requires Python packages such as torch, transformers, peft, and safetensors"
                .to_string()
        }
        ModelFormat::Diffusers => {
            "dependency-gated: requires Python packages such as torch, diffusers, accelerate, and pillow"
                .to_string()
        }
        ModelFormat::Gguf => {
            "dependency-gated: requires a working llama-cpp-python installation".to_string()
        }
    }
}

fn model_backend_support_summary(metadata: &ModelMetadata) -> String {
    match (metadata.primary_format, metadata.mlx_runtime_family) {
        (ModelFormat::Mlx, Some(MlxRuntimeFamily::Vlm))
            if cfg!(all(target_os = "macos", target_arch = "aarch64")) =>
        {
            "dependency-gated: requires MLX VLM Python packages such as mlx and mlx-vlm".to_string()
        }
        (ModelFormat::Mlx, Some(MlxRuntimeFamily::Vlm)) => {
            "unsupported: MLX VLM is supported only on Apple Silicon macOS".to_string()
        }
        (ModelFormat::Mlx, Some(MlxRuntimeFamily::Audio))
            if cfg!(all(target_os = "macos", target_arch = "aarch64")) =>
        {
            "dependency-gated: requires MLX audio Python packages such as mlx and mlx-audio"
                .to_string()
        }
        (ModelFormat::Mlx, Some(MlxRuntimeFamily::Audio)) => {
            "unsupported: MLX audio is supported only on Apple Silicon macOS".to_string()
        }
        (ModelFormat::Mlx, Some(MlxRuntimeFamily::Diffusion))
            if cfg!(all(target_os = "macos", target_arch = "aarch64")) =>
        {
            "dependency-gated: requires MLX image generation Python packages such as mlx and mflux"
                .to_string()
        }
        (ModelFormat::Mlx, Some(MlxRuntimeFamily::Diffusion)) => {
            "unsupported: MLX image generation is supported only on Apple Silicon macOS".to_string()
        }
        _ => model_format_support_summary(metadata.primary_format),
    }
}

fn model_capabilities_label(capabilities: &[ModelCapability]) -> String {
    capabilities
        .iter()
        .map(|capability| capability.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests;
