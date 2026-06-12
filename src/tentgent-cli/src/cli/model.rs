use clap::CommandFactory;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use miette::{miette, IntoDiagnostic, Result};
use tentgent_kernel::features::auth::domain::{AuthEnvLoadPolicy, Provider};
use tentgent_kernel::features::auth::infra::{
    ProcessSessionAuthSecretCache, StdAuthEnvSecretProbe, SystemKeychainAuthSecretStore,
};
use tentgent_kernel::features::auth::usecases::{
    AuthSecretResolutionRequest, StdAuthSecretResolverUseCase,
};
use tentgent_kernel::features::model::domain::{
    HfModelPullProgress, MlxRuntimeFamily, ModelCapability, ModelCapabilityProof, ModelFormat,
    ModelImportOutcome, ModelInspection, ModelMetadata, ModelRefSelector, ModelRemovalOutcome,
    ModelSummary, MODEL_CAPABILITY_CANONICAL_ORDER,
};
use tentgent_kernel::features::model::infra::{
    FileModelCapabilityProofStore, FileModelCatalogStore, FileModelContentStore,
    FileModelServerReferenceProbe, FileModelSourceIndexStore, StdHfModelSnapshotFetcher,
    StdModelIdentityGenerator, StdModelManifestBuilder, StdModelSourceStager,
    StdModelStoreLayoutInitializer, SystemModelClock,
};
use tentgent_kernel::features::model::support_status::{
    ModelSupportQuery, ModelSupportStatusResolver,
};
use tentgent_kernel::features::model::usecases::{
    ModelCapabilityMutation, ModelCapabilityProofListRequest, ModelCapabilityProofUseCase,
    ModelCapabilityUpdateRequest, ModelCapabilityUpdateResult, ModelCapabilityUpdateUseCase,
    ModelCapabilityVerifyRequest, ModelCatalogReadUseCase, ModelHfPullRequest, ModelHfPullUseCase,
    ModelInspectRequest, ModelListRequest, ModelLocalImportRequest, ModelLocalImportUseCase,
    ModelRemoveRequest, ModelRemoveUseCase, StdModelCapabilityProofUseCase,
    StdModelCapabilityUpdateUseCase, StdModelCatalogReadUseCase, StdModelHfPullUseCase,
    StdModelLocalImportUseCase, StdModelRemoveUseCase,
};
use tentgent_kernel::features::runtime::domain::PythonRuntimeResolutionInput;
use tentgent_kernel::features::runtime::infra::StdPythonRuntimeResolver;
use tentgent_kernel::foundation::error::KernelError;
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver,
};

use super::app::Cli;
use super::commands::{ModelCapabilityCommands, ModelCommands};
use super::display::format_bytes;

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
        ModelCommands::Ls => {
            let result = model
                .catalog_usecase()
                .list_models(ModelListRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                })
                .into_diagnostic()?;
            render_model_list(&result.models);
        }
        ModelCommands::Rm { hash } => {
            if is_help_token(&hash) {
                print_model_subcommand_help("rm")?;
                return Ok(());
            }

            let selector = parse_model_selector("rm", "HASH", &hash)?;
            let outcome = match model.remove_usecase().remove_model(ModelRemoveRequest {
                layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                selector,
            }) {
                Ok(outcome) => outcome,
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
            let result = match model.capability_update_usecase().update_model_capability(
                ModelCapabilityUpdateRequest {
                    layout: runtime_layout_input(LayoutResolveMode::Create),
                    selector,
                    mutation: ModelCapabilityMutation::Set(vec![capability]),
                },
            ) {
                Ok(result) => result,
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
    let result = match model.capability_update_usecase().update_model_capability(
        ModelCapabilityUpdateRequest {
            layout: runtime_layout_input(LayoutResolveMode::Create),
            selector,
            mutation,
        },
    ) {
        Ok(result) => result,
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
        StdModelCapabilityUpdateUseCase::new(&self.layout_resolver, &self.catalog)
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

fn render_import_outcome(title: &str, outcome: &ModelImportOutcome) {
    let status = if outcome.deduplicated {
        style("reused").yellow().bold()
    } else {
        style("stored").green().bold()
    };

    println!("{} {}", style("==>").cyan().bold(), style(title).bold());
    println!(
        "{} model {} under {}",
        status,
        outcome.metadata.short_ref,
        outcome.store_path.display()
    );

    let mut table = base_table();
    add_model_metadata_rows(&mut table, &outcome.metadata);
    table.add_row(vec![
        Cell::new("status"),
        Cell::new(if outcome.deduplicated {
            "deduplicated"
        } else {
            "imported"
        }),
    ]);
    table.add_row(vec![
        Cell::new("store path"),
        Cell::new(outcome.store_path.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("source index"),
        Cell::new(outcome.source_index_path.display().to_string()),
    ]);

    println!("{table}");
    println!();
}

fn render_model_removal(outcome: &ModelRemovalOutcome) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Model removed").bold()
    );
    println!(
        "{} model {} from {}",
        style("removed").red().bold(),
        outcome.metadata.short_ref,
        outcome.store_path.display()
    );

    let mut table = base_table();
    add_model_metadata_rows(&mut table, &outcome.metadata);
    table.add_row(vec![Cell::new("status"), Cell::new("removed")]);
    table.add_row(vec![
        Cell::new("store path"),
        Cell::new(outcome.store_path.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("removed indexes"),
        Cell::new(outcome.removed_index_paths.len()),
    ]);
    if !outcome.removed_index_paths.is_empty() {
        table.add_row(vec![
            Cell::new("index paths"),
            Cell::new(
                outcome
                    .removed_index_paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        ]);
    }

    println!("{table}");
    println!();
}

fn render_model_list(models: &[ModelSummary]) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Managed models").bold()
    );

    if models.is_empty() {
        println!(
            "{} No managed models are stored yet.\n",
            style("empty").yellow().bold()
        );
        return;
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            "short_ref",
            "format",
            "source_kind",
            "source",
            "files",
            "size",
            "inspect",
        ]);

    for model in models {
        table.add_row(vec![
            Cell::new(&model.metadata.short_ref),
            Cell::new(model.metadata.primary_format.as_str()),
            Cell::new(model.metadata.source_kind.as_str()),
            Cell::new(model_list_source_label(&model.metadata)),
            Cell::new(model.metadata.file_count),
            Cell::new(format_bytes(model.metadata.total_bytes)),
            Cell::new(format!(
                "tentgent model inspect {}",
                model.metadata.short_ref
            )),
        ]);
    }

    println!("{table}");
    println!();
}

fn model_list_source_label(metadata: &ModelMetadata) -> String {
    match metadata.source_kind {
        tentgent_kernel::features::model::domain::ModelSourceKind::HuggingFace => metadata
            .source_repo
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        tentgent_kernel::features::model::domain::ModelSourceKind::Local => metadata
            .source_path
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
    }
}

fn render_model_inspection(inspection: &ModelInspection, proofs: &[ModelCapabilityProof]) {
    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style("Model inspection").bold(),
        style(&inspection.metadata.short_ref).bold()
    );

    let mut table = base_table();
    add_model_metadata_rows(&mut table, &inspection.metadata);
    table.add_row(vec![
        Cell::new("store path"),
        Cell::new(inspection.store_path.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("manifest path"),
        Cell::new(inspection.manifest_path.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("variant source"),
        Cell::new(inspection.variant_source_path.display().to_string()),
    ]);
    add_model_support_status_rows(&mut table, &inspection.metadata, proofs);

    println!("{table}");
    println!();
}

fn add_model_support_status_rows(
    table: &mut Table,
    metadata: &ModelMetadata,
    proofs: &[ModelCapabilityProof],
) {
    let capabilities = inspect_capabilities(metadata, proofs);
    if capabilities.is_empty() {
        table.add_row(vec![Cell::new("capability support"), Cell::new("none")]);
        return;
    }

    for (index, capability) in capabilities.into_iter().enumerate() {
        let query = ModelSupportQuery::from_metadata(metadata, capability);
        let resolution = ModelSupportStatusResolver.resolve(metadata, &query, proofs, &[]);
        let mut lines = Vec::new();
        if index > 0 {
            lines.push(String::new());
        }
        lines.extend([
            capability.as_str().to_string(),
            format!(
                "declared: {}",
                if metadata.supports_capability(capability) {
                    "yes"
                } else {
                    "no"
                }
            ),
            format!("status: {}", resolution.status.as_str()),
            format!("evidence: {}", resolution.evidence.as_str()),
            format!("backend: {}", query.backend),
        ]);

        if let Some(family) = query.mlx_runtime_family {
            lines.push(format!("mlx_runtime_family: {}", family.as_str()));
        }
        if let Some(version) = query.runtime_version.as_deref() {
            lines.push(format!("runtime_version: {version}"));
        }
        if let Some(stale_reason) = resolution.stale_reason.as_deref() {
            lines.push(format!("stale: {stale_reason}"));
        }
        if let Some(failure_reason) = resolution.failure_reason.as_deref() {
            lines.push(format!("failure: {failure_reason}"));
        }
        if !resolution.reason.is_empty() {
            lines.push(format!("reason: {}", resolution.reason));
        }

        table.add_row(vec![
            Cell::new(if index == 0 { "capability support" } else { "" }),
            Cell::new(lines.join("\n")),
        ]);
    }
}

fn inspect_capabilities(
    metadata: &ModelMetadata,
    proofs: &[ModelCapabilityProof],
) -> Vec<ModelCapability> {
    MODEL_CAPABILITY_CANONICAL_ORDER
        .into_iter()
        .filter(|capability| {
            metadata.supports_capability(*capability)
                || proofs.iter().any(|proof| proof.capability == *capability)
        })
        .collect()
}

fn render_model_capability_show(inspection: &ModelInspection) {
    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style("Model capabilities").bold(),
        style(&inspection.metadata.short_ref).bold()
    );

    let mut table = base_table();
    table.add_row(vec![
        Cell::new("model_ref"),
        Cell::new(inspection.metadata.model_ref.as_str()),
    ]);
    table.add_row(vec![
        Cell::new("short_ref"),
        Cell::new(&inspection.metadata.short_ref),
    ]);
    table.add_row(vec![
        Cell::new("model_capabilities"),
        Cell::new(model_capabilities_label(
            &inspection.metadata.model_capabilities,
        )),
    ]);
    table.add_row(vec![
        Cell::new("model_capability_source"),
        Cell::new(inspection.metadata.model_capability_source.as_str()),
    ]);
    if let Some(family) = inspection.metadata.mlx_runtime_family {
        table.add_row(vec![
            Cell::new("mlx_runtime_family"),
            Cell::new(family.as_str()),
        ]);
    }
    table.add_row(vec![
        Cell::new("store path"),
        Cell::new(inspection.store_path.display().to_string()),
    ]);

    println!("{table}");
    println!();
}

fn render_model_capability_update(result: &ModelCapabilityUpdateResult) {
    let inspection = &result.model;
    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style("Model capability updated").bold(),
        style(&inspection.metadata.short_ref).bold()
    );

    let mut table = base_table();
    add_model_metadata_rows(&mut table, &inspection.metadata);
    table.add_row(vec![
        Cell::new("previous_capabilities"),
        Cell::new(model_capabilities_label(&result.previous_capabilities)),
    ]);
    table.add_row(vec![
        Cell::new("added_capabilities"),
        Cell::new(model_capabilities_label(&result.added_capabilities)),
    ]);
    table.add_row(vec![
        Cell::new("removed_capabilities"),
        Cell::new(model_capabilities_label(&result.removed_capabilities)),
    ]);
    table.add_row(vec![
        Cell::new("store path"),
        Cell::new(inspection.store_path.display().to_string()),
    ]);

    println!("{table}");
    println!();
}

fn render_model_capability_proofs(inspection: &ModelInspection, proofs: &[ModelCapabilityProof]) {
    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style("Model capability proofs").bold(),
        style(&inspection.metadata.short_ref).bold()
    );

    if proofs.is_empty() {
        println!(
            "{} No capability proofs are stored for this model.\n",
            style("empty").yellow().bold()
        );
        return;
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            "capability",
            "status",
            "source",
            "backend",
            "server_ref",
            "checked_at",
            "error",
        ]);

    for proof in proofs {
        table.add_row(vec![
            Cell::new(proof.capability.as_str()),
            Cell::new(proof.status.as_str()),
            Cell::new(proof.source.as_str()),
            Cell::new(&proof.backend),
            Cell::new(proof.server_ref.as_deref().unwrap_or("-")),
            Cell::new(&proof.checked_at),
            Cell::new(proof.error.as_deref().unwrap_or("-")),
        ]);
    }

    println!("{table}");
    println!();
}

fn render_model_capability_verify(inspection: &ModelInspection, proof: &ModelCapabilityProof) {
    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style("Model capability proof recorded").bold(),
        style(&inspection.metadata.short_ref).bold()
    );

    let mut table = base_table();
    add_model_proof_rows(&mut table, proof);

    println!("{table}");
    println!();
}

fn add_model_proof_rows(table: &mut Table, proof: &ModelCapabilityProof) {
    table.add_row(vec![
        Cell::new("model_ref"),
        Cell::new(proof.model_ref.as_str()),
    ]);
    table.add_row(vec![
        Cell::new("capability"),
        Cell::new(proof.capability.as_str()),
    ]);
    table.add_row(vec![Cell::new("status"), Cell::new(proof.status.as_str())]);
    table.add_row(vec![Cell::new("source"), Cell::new(proof.source.as_str())]);
    table.add_row(vec![
        Cell::new("primary_format"),
        Cell::new(proof.primary_format.as_str()),
    ]);
    table.add_row(vec![Cell::new("backend"), Cell::new(&proof.backend)]);
    if let Some(family) = proof.mlx_runtime_family {
        table.add_row(vec![
            Cell::new("mlx_runtime_family"),
            Cell::new(family.as_str()),
        ]);
    }
    if let Some(version) = &proof.runtime_version {
        table.add_row(vec![Cell::new("runtime_version"), Cell::new(version)]);
    }
    if let Some(server_ref) = &proof.server_ref {
        table.add_row(vec![Cell::new("server_ref"), Cell::new(server_ref)]);
    }
    table.add_row(vec![Cell::new("checked_at"), Cell::new(&proof.checked_at)]);
    if let Some(error) = &proof.error {
        table.add_row(vec![Cell::new("error"), Cell::new(error)]);
    }
}

fn render_capability_warning(metadata: &tentgent_kernel::features::model::domain::ModelMetadata) {
    if let Some(warning) = metadata.capability_warning() {
        eprintln!("{} {}", style("warning").yellow().bold(), warning);
    }
}

fn base_table() -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["Field", "Value"]);
    table
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PullProgressMode {
    Spinner,
    Files,
    Bytes,
}

struct PullProgress {
    bar: ProgressBar,
    repo_id: String,
    mode: PullProgressMode,
}

impl PullProgress {
    fn new(repo_id: &str, revision: Option<&str>) -> Self {
        let bar = ProgressBar::new_spinner();
        bar.set_style(
            ProgressStyle::with_template("{spinner} {msg} [{elapsed_precise}]")
                .expect("valid pull spinner template"),
        );
        bar.set_message(match revision {
            Some(revision) => format!("Resolving {repo_id} @ {revision} from Hugging Face"),
            None => format!("Resolving {repo_id} from Hugging Face"),
        });
        bar.enable_steady_tick(std::time::Duration::from_millis(100));

        Self {
            bar,
            repo_id: repo_id.to_string(),
            mode: PullProgressMode::Spinner,
        }
    }

    fn update(&mut self, event: HfModelPullProgress) {
        if event.finished {
            return;
        }

        if event.unit == "B" {
            self.switch_mode(PullProgressMode::Bytes);
            if let Some(total) = event.total {
                self.bar.set_length(total);
            }
            self.bar.set_position(event.position);
            self.bar.set_message(match event.description.as_str() {
                "" | "Downloading (incomplete total...)" => {
                    format!("Downloading {}", self.repo_id)
                }
                description => description.to_string(),
            });
            return;
        }

        self.switch_mode(PullProgressMode::Files);
        if let Some(total) = event.total {
            self.bar.set_length(total);
        }
        self.bar.set_position(event.position);
        self.bar.set_message(if event.description.is_empty() {
            format!("Fetching files for {}", self.repo_id)
        } else {
            event.description
        });
    }

    fn finish(&self) {
        self.bar.finish_and_clear();
    }

    fn switch_mode(&mut self, mode: PullProgressMode) {
        if self.mode == mode {
            return;
        }

        self.mode = mode;
        match mode {
            PullProgressMode::Spinner => {}
            PullProgressMode::Files => {
                self.bar.set_style(
                    ProgressStyle::with_template(
                        "{spinner:.cyan} {msg} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len}",
                    )
                    .expect("valid file progress template")
                    .progress_chars("=> "),
                );
            }
            PullProgressMode::Bytes => {
                self.bar.set_style(
                    ProgressStyle::with_template(
                        "{spinner:.cyan} {msg} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} {bytes_per_sec} ETA {eta_precise}",
                    )
                    .expect("valid byte progress template")
                    .progress_chars("=> "),
                );
            }
        }
    }
}

fn add_model_metadata_rows(
    table: &mut Table,
    metadata: &tentgent_kernel::features::model::domain::ModelMetadata,
) {
    table.add_row(vec![
        Cell::new("model_ref"),
        Cell::new(metadata.model_ref.as_str()),
    ]);
    table.add_row(vec![Cell::new("short_ref"), Cell::new(&metadata.short_ref)]);
    table.add_row(vec![
        Cell::new("source_kind"),
        Cell::new(metadata.source_kind.as_str()),
    ]);

    if let Some(repo) = &metadata.source_repo {
        table.add_row(vec![Cell::new("source_repo"), Cell::new(repo)]);
    }

    if let Some(revision) = &metadata.source_revision {
        table.add_row(vec![Cell::new("source_revision"), Cell::new(revision)]);
    }

    if let Some(path) = &metadata.source_path {
        table.add_row(vec![Cell::new("source_path"), Cell::new(path)]);
    }

    table.add_row(vec![
        Cell::new("primary_format"),
        Cell::new(metadata.primary_format.as_str()),
    ]);
    table.add_row(vec![
        Cell::new("detected_formats"),
        Cell::new(
            metadata
                .detected_formats
                .iter()
                .map(|format| format.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        ),
    ]);
    if let Some(family) = metadata.mlx_runtime_family {
        table.add_row(vec![
            Cell::new("mlx_runtime_family"),
            Cell::new(family.as_str()),
        ]);
    }
    table.add_row(vec![
        Cell::new("model_capabilities"),
        Cell::new(model_capabilities_label(&metadata.model_capabilities)),
    ]);
    table.add_row(vec![
        Cell::new("model_capability_source"),
        Cell::new(metadata.model_capability_source.as_str()),
    ]);
    table.add_row(vec![
        Cell::new("backend_support"),
        Cell::new(model_backend_support_summary(metadata)),
    ]);
    table.add_row(vec![
        Cell::new("file_count"),
        Cell::new(metadata.file_count),
    ]);
    table.add_row(vec![
        Cell::new("size"),
        Cell::new(format_bytes(metadata.total_bytes)),
    ]);
    table.add_row(vec![
        Cell::new("imported_at"),
        Cell::new(&metadata.imported_at),
    ]);
}

fn is_help_token(value: &str) -> bool {
    matches!(value, "help" | "--help" | "-h")
}

fn parse_model_selector(command: &str, value_name: &str, value: &str) -> Result<ModelRefSelector> {
    ModelRefSelector::parse(value).map_err(|err| usage_error(command, value_name, err))
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
mod tests {
    use clap::Parser as _;

    use super::*;
    use crate::cli::commands::Commands;

    #[test]
    fn parses_model_pull_revision_command() {
        let cli = Cli::try_parse_from([
            "tentgent",
            "model",
            "pull",
            "org/model",
            "--revision",
            "main",
        ])
        .expect("parse model pull");

        match cli.command {
            Commands::Model {
                action:
                    ModelCommands::Pull {
                        repo_id,
                        revision,
                        capability,
                    },
            } => {
                assert_eq!(repo_id, "org/model");
                assert_eq!(revision.as_deref(), Some("main"));
                assert_eq!(capability, None);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_model_add_capability_command() {
        let cli = Cli::try_parse_from([
            "tentgent",
            "model",
            "add",
            "/tmp/model",
            "--capability",
            "embedding",
        ])
        .expect("parse model add");

        match cli.command {
            Commands::Model {
                action: ModelCommands::Add { path, capability },
            } => {
                assert_eq!(path, std::path::PathBuf::from("/tmp/model"));
                assert_eq!(capability, Some(ModelCapability::Embedding));
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_model_pull_capability_command() {
        let cli = Cli::try_parse_from([
            "tentgent",
            "model",
            "pull",
            "org/model",
            "--capability",
            "rerank",
            "--revision",
            "main",
        ])
        .expect("parse model pull");

        match cli.command {
            Commands::Model {
                action:
                    ModelCommands::Pull {
                        repo_id,
                        revision,
                        capability,
                    },
            } => {
                assert_eq!(repo_id, "org/model");
                assert_eq!(revision.as_deref(), Some("main"));
                assert_eq!(capability, Some(ModelCapability::Rerank));
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_model_set_capability_command() {
        let cli =
            Cli::try_parse_from(["tentgent", "model", "set-capability", "abc123", "embedding"])
                .expect("parse model set-capability");

        match cli.command {
            Commands::Model {
                action:
                    ModelCommands::SetCapability {
                        reference,
                        capability,
                    },
            } => {
                assert_eq!(reference, "abc123");
                assert_eq!(capability, ModelCapability::Embedding);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_model_capability_show_command() {
        let cli = Cli::try_parse_from(["tentgent", "model", "capability", "show", "abc123"])
            .expect("parse model capability show");

        match cli.command {
            Commands::Model {
                action:
                    ModelCommands::Capability {
                        action: ModelCapabilityCommands::Show { reference },
                    },
            } => {
                assert_eq!(reference, "abc123");
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_model_capability_set_command_with_multiple_values() {
        let cli = Cli::try_parse_from([
            "tentgent",
            "model",
            "capability",
            "set",
            "abc123",
            "chat",
            "vision-chat",
        ])
        .expect("parse model capability set");

        match cli.command {
            Commands::Model {
                action:
                    ModelCommands::Capability {
                        action:
                            ModelCapabilityCommands::Set {
                                reference,
                                capabilities,
                            },
                    },
            } => {
                assert_eq!(reference, "abc123");
                assert_eq!(
                    capabilities,
                    vec![ModelCapability::Chat, ModelCapability::VisionChat]
                );
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_model_capability_add_and_remove_commands() {
        let add = Cli::try_parse_from([
            "tentgent",
            "model",
            "capability",
            "add",
            "abc123",
            "embedding",
        ])
        .expect("parse model capability add");
        match add.command {
            Commands::Model {
                action:
                    ModelCommands::Capability {
                        action:
                            ModelCapabilityCommands::Add {
                                reference,
                                capabilities,
                            },
                    },
            } => {
                assert_eq!(reference, "abc123");
                assert_eq!(capabilities, vec![ModelCapability::Embedding]);
            }
            other => panic!("unexpected command: {other:?}"),
        }

        let remove =
            Cli::try_parse_from(["tentgent", "model", "capability", "rm", "abc123", "chat"])
                .expect("parse model capability remove alias");
        match remove.command {
            Commands::Model {
                action:
                    ModelCommands::Capability {
                        action:
                            ModelCapabilityCommands::Remove {
                                reference,
                                capabilities,
                            },
                    },
            } => {
                assert_eq!(reference, "abc123");
                assert_eq!(capabilities, vec![ModelCapability::Chat]);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_model_capability_proofs_and_verify_commands() {
        let proofs = Cli::try_parse_from(["tentgent", "model", "capability", "proofs", "abc123"])
            .expect("parse model capability proofs");
        match proofs.command {
            Commands::Model {
                action:
                    ModelCommands::Capability {
                        action: ModelCapabilityCommands::Proofs { reference },
                    },
            } => assert_eq!(reference, "abc123"),
            other => panic!("unexpected command: {other:?}"),
        }

        let verify = Cli::try_parse_from([
            "tentgent",
            "model",
            "capability",
            "verify",
            "abc123",
            "vision-chat",
        ])
        .expect("parse model capability verify");
        match verify.command {
            Commands::Model {
                action:
                    ModelCommands::Capability {
                        action:
                            ModelCapabilityCommands::Verify {
                                reference,
                                capability,
                            },
                    },
            } => {
                assert_eq!(reference, "abc123");
                assert_eq!(capability, ModelCapability::VisionChat);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_media_model_capability_values_as_metadata() {
        let cli = Cli::try_parse_from([
            "tentgent",
            "model",
            "pull",
            "org/whisper",
            "--capability",
            "audio-transcription",
        ])
        .expect("parse media model capability");

        match cli.command {
            Commands::Model {
                action:
                    ModelCommands::Pull {
                        repo_id,
                        capability,
                        ..
                    },
            } => {
                assert_eq!(repo_id, "org/whisper");
                assert_eq!(capability, Some(ModelCapability::AudioTranscription));
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn model_set_capability_rejects_unknown_cli_value() {
        let err = Cli::try_parse_from(["tentgent", "model", "set-capability", "abc123", "audio"])
            .expect_err("parse error");

        assert!(err.to_string().contains("unsupported model capability"));
    }

    #[test]
    fn model_capability_set_rejects_unknown_cli_value() {
        let err =
            Cli::try_parse_from(["tentgent", "model", "capability", "set", "abc123", "audio"])
                .expect_err("parse error");

        assert!(err.to_string().contains("unsupported model capability"));
    }

    #[test]
    fn model_capability_rejects_unknown_cli_value() {
        let err = Cli::try_parse_from([
            "tentgent",
            "model",
            "pull",
            "org/model",
            "--capability",
            "audio",
        ])
        .expect_err("parse error");

        assert!(err.to_string().contains("unsupported model capability"));
    }

    #[test]
    fn model_selector_errors_keep_subcommand_usage_hint() {
        let err = parse_model_selector("inspect", "REF", "not-a-ref").expect_err("parse error");
        let message = err.to_string();

        assert!(message.contains("model reference must contain only hexadecimal characters"));
        assert!(message.contains("Usage: tentgent model inspect <REF>"));
    }

    #[test]
    fn backend_support_summary_uses_kernel_model_format() {
        assert!(model_format_support_summary(ModelFormat::Gguf).contains("llama-cpp-python"));
        assert!(model_format_support_summary(ModelFormat::Safetensors).contains("transformers"));
        assert!(model_format_support_summary(ModelFormat::Diffusers).contains("diffusers"));
    }
}
