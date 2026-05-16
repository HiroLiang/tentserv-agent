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
    HfModelPullProgress, ModelFormat, ModelImportOutcome, ModelInspection, ModelRefSelector,
    ModelRemovalOutcome, ModelSummary,
};
use tentgent_kernel::features::model::infra::{
    FileModelCatalogStore, FileModelContentStore, FileModelServerReferenceProbe,
    FileModelSourceIndexStore, StdHfModelSnapshotFetcher, StdModelIdentityGenerator,
    StdModelManifestBuilder, StdModelSourceStager, StdModelStoreLayoutInitializer,
};
use tentgent_kernel::features::model::usecases::{
    ModelCatalogReadUseCase, ModelHfPullRequest, ModelHfPullUseCase, ModelInspectRequest,
    ModelListRequest, ModelLocalImportRequest, ModelLocalImportUseCase, ModelRemoveRequest,
    ModelRemoveUseCase, StdModelCatalogReadUseCase, StdModelHfPullUseCase,
    StdModelLocalImportUseCase, StdModelRemoveUseCase,
};
use tentgent_kernel::features::runtime::domain::PythonRuntimeResolutionInput;
use tentgent_kernel::features::runtime::infra::StdPythonRuntimeResolver;
use tentgent_kernel::foundation::error::KernelError;
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver,
};

use super::app::Cli;
use super::commands::ModelCommands;
use super::display::format_bytes;

pub fn handle_model_command(action: ModelCommands) -> Result<()> {
    let model = CliModelKernel::new();

    match action {
        ModelCommands::Add { path } => {
            let result = model
                .local_import_usecase()
                .import_local_model(ModelLocalImportRequest {
                    layout: runtime_layout_input(LayoutResolveMode::Create),
                    source_path: path,
                })
                .into_diagnostic()?;
            render_import_outcome("Model imported", &result.outcome);
        }
        ModelCommands::Pull { repo_id, revision } => {
            let mut progress = PullProgress::new(&repo_id, revision.as_deref());
            let auth_resolver = model.auth_resolver_usecase();
            let outcome = model.hf_pull_usecase(&auth_resolver).pull_hf_model(
                ModelHfPullRequest {
                    layout: runtime_layout_input(LayoutResolveMode::Create),
                    runtime: PythonRuntimeResolutionInput::default(),
                    repo_id: repo_id.clone(),
                    revision,
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
                selector,
            }) {
                Ok(result) => result.model,
                Err(err) => return Err(explain_model_lookup_error("inspect", "REF", err)),
            };
            render_model_inspection(&inspection);
        }
    }

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
        ]);

    for model in models {
        table.add_row(vec![
            Cell::new(&model.metadata.short_ref),
            Cell::new(model.metadata.primary_format.as_str()),
            Cell::new(model.metadata.source_kind.as_str()),
            Cell::new(model.metadata.source_summary()),
            Cell::new(model.metadata.file_count),
            Cell::new(format_bytes(model.metadata.total_bytes)),
        ]);
    }

    println!("{table}");
    println!();
}

fn render_model_inspection(inspection: &ModelInspection) {
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

    println!("{table}");
    println!();
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
    table.add_row(vec![
        Cell::new("backend_support"),
        Cell::new(model_format_support_summary(metadata.primary_format)),
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
        ModelFormat::Gguf => {
            "dependency-gated: requires a working llama-cpp-python installation".to_string()
        }
    }
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
                action: ModelCommands::Pull { repo_id, revision },
            } => {
                assert_eq!(repo_id, "org/model");
                assert_eq!(revision.as_deref(), Some("main"));
            }
            other => panic!("unexpected command: {other:?}"),
        }
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
    }
}
