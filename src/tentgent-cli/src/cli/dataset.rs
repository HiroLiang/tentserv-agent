use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use clap::CommandFactory;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use miette::{miette, IntoDiagnostic, Result};
use serde_json::Value;
use tentgent_core::{
    auth::{AuthManager, KeySource, KeyValidationState, Provider},
    dataset::{
        render_dataset_template, validate_dataset_path, write_dataset_template, DatasetDiffOutcome,
        DatasetDiffStatus, DatasetError, DatasetExportOutcome, DatasetImportOutcome,
        DatasetInspection, DatasetManager, DatasetMetadata, DatasetRemovalOutcome, DatasetSummary,
        DatasetTemplateRequest, DatasetValidationOutcome,
    },
};
use tokio::process::Command;

use super::app::Cli;
use super::commands::DatasetCommands;
use super::python_runtime::{require_python_interpreter, resolve_python_runtime};

#[derive(Debug, Clone)]
struct DatasetProviderAuth {
    provider: Provider,
    normalized_provider: &'static str,
    secret: String,
}

pub async fn handle_dataset_command(action: DatasetCommands) -> Result<()> {
    match action {
        DatasetCommands::Add { path } => {
            if is_help_path(&path) {
                print_dataset_subcommand_help("add")?;
                return Ok(());
            }

            let manager = DatasetManager::new().into_diagnostic()?;
            let outcome = manager.add_path(path).into_diagnostic()?;
            render_import_outcome(&outcome);
        }
        DatasetCommands::Validate { path } => {
            if is_help_path(&path) {
                print_dataset_subcommand_help("validate")?;
                return Ok(());
            }

            let outcome = validate_dataset_path(&path).into_diagnostic()?;
            render_validation_outcome(&outcome);
            if !outcome.is_valid() {
                return Err(miette!(
                    "dataset validation failed with {} error(s)",
                    outcome.errors.len()
                ));
            }
        }
        DatasetCommands::Template {
            task,
            language,
            output,
        } => {
            let request = DatasetTemplateRequest::new(task, language);
            let body = render_dataset_template(&request);
            if let Some(path) = output {
                write_dataset_template(&path, &body).into_diagnostic()?;
                render_template_written(&path, &request);
            } else {
                print!("{body}");
            }
        }
        DatasetCommands::Synth {
            provider,
            model,
            output,
            brief,
            spec,
            split,
            max_tokens,
            temperature,
        } => {
            let auth = preflight_dataset_provider_auth(&provider).await?;
            let outcome = run_dataset_synth_runtime(
                &auth,
                &model,
                &output,
                brief.as_deref(),
                spec.as_deref(),
                &split,
                max_tokens,
                temperature,
            )
            .await?;
            render_synth_outcome(&outcome);
        }
        DatasetCommands::Ls => {
            let manager = DatasetManager::new().into_diagnostic()?;
            let datasets = manager.list_datasets().into_diagnostic()?;
            render_dataset_list(&datasets);
        }
        DatasetCommands::Inspect { reference } => {
            if is_help_token(&reference) {
                print_dataset_subcommand_help("inspect")?;
                return Ok(());
            }

            let manager = DatasetManager::new().into_diagnostic()?;
            let inspection = match manager.inspect(&reference) {
                Ok(inspection) => inspection,
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

            let manager = DatasetManager::new().into_diagnostic()?;
            let outcome = match manager.export_to(&reference, &path) {
                Ok(outcome) => outcome,
                Err(err) => return Err(explain_dataset_export_error(&reference, &path, err)),
            };
            render_export_outcome(&outcome);
        }
        DatasetCommands::Diff { left, right, path } => {
            if is_help_token(&left) {
                print_dataset_subcommand_help("diff")?;
                return Ok(());
            }

            let manager = DatasetManager::new().into_diagnostic()?;
            let outcome = if let Some(path) = path {
                match manager.diff_ref_to_path(&left, path) {
                    Ok(outcome) => outcome,
                    Err(err) => return Err(explain_dataset_lookup_error("diff", err)),
                }
            } else if let Some(right) = right {
                match manager.diff_refs(&left, &right) {
                    Ok(outcome) => outcome,
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

            let manager = DatasetManager::new().into_diagnostic()?;
            let outcome = match manager.remove(&reference) {
                Ok(outcome) => outcome,
                Err(err) => return Err(explain_dataset_lookup_error("rm", err)),
            };
            render_removal_outcome(&outcome);
        }
    }

    Ok(())
}

fn render_import_outcome(outcome: &DatasetImportOutcome) {
    let status = if outcome.deduplicated {
        style("reused").yellow().bold()
    } else {
        style("stored").green().bold()
    };

    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Dataset imported").bold()
    );
    println!(
        "{} dataset {} under {}",
        status,
        outcome.metadata.short_ref,
        outcome.store_path.display()
    );

    let mut table = base_table();
    add_dataset_metadata_rows(&mut table, &outcome.metadata);
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

fn render_validation_outcome(outcome: &DatasetValidationOutcome) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Dataset validation").bold()
    );

    let status = if outcome.is_valid() {
        style("valid").green().bold()
    } else {
        style("invalid").red().bold()
    };
    println!(
        "{} {} record(s) across {} split(s)",
        status,
        outcome.record_count(),
        outcome.splits.len()
    );

    let mut table = base_table();
    table.add_row(vec![
        Cell::new("path"),
        Cell::new(outcome.path.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("target"),
        Cell::new(outcome.target_kind.as_str()),
    ]);
    table.add_row(vec![
        Cell::new("tuning_ready"),
        Cell::new(yes_no(outcome.tuning_ready)),
    ]);
    table.add_row(vec![
        Cell::new("records"),
        Cell::new(outcome.record_count()),
    ]);
    table.add_row(vec![Cell::new("errors"), Cell::new(outcome.errors.len())]);
    println!("{table}");

    if !outcome.splits.is_empty() {
        let mut splits = Table::new();
        splits
            .load_preset(UTF8_FULL_CONDENSED)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec!["split", "path", "records", "errors"]);

        for split in &outcome.splits {
            splits.add_row(vec![
                Cell::new(&split.name),
                Cell::new(split.path.display().to_string()),
                Cell::new(split.records),
                Cell::new(split.errors),
            ]);
        }
        println!("{splits}");
    }

    if !outcome.warnings.is_empty() {
        println!("{} Warnings", style("note").yellow().bold());
        for warning in &outcome.warnings {
            println!("- {warning}");
        }
    }

    if !outcome.errors.is_empty() {
        let mut errors = Table::new();
        errors
            .load_preset(UTF8_FULL_CONDENSED)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec!["path", "line", "message"]);

        for error in &outcome.errors {
            errors.add_row(vec![
                Cell::new(error.path.display().to_string()),
                Cell::new(error.line),
                Cell::new(&error.message),
            ]);
        }
        println!("{errors}");
    }

    println!();
}

fn render_template_written(path: &Path, request: &DatasetTemplateRequest) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Dataset template written").bold()
    );

    let mut table = base_table();
    table.add_row(vec![
        Cell::new("path"),
        Cell::new(path.display().to_string()),
    ]);
    table.add_row(vec![Cell::new("task"), Cell::new(&request.task)]);
    table.add_row(vec![Cell::new("language"), Cell::new(&request.language)]);
    table.add_row(vec![
        Cell::new("next step"),
        Cell::new("paste this template into OpenAI, Claude, or another agent"),
    ]);
    println!("{table}");
    println!();
}

fn render_synth_outcome(outcome: &Value) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Dataset synthesized").bold()
    );

    let output_dir = json_field(outcome, "output_dir");
    let mut table = base_table();
    table.add_row(vec![
        Cell::new("provider"),
        Cell::new(json_field(outcome, "provider")),
    ]);
    table.add_row(vec![
        Cell::new("model"),
        Cell::new(json_field(outcome, "model")),
    ]);
    table.add_row(vec![
        Cell::new("split"),
        Cell::new(json_field(outcome, "split")),
    ]);
    table.add_row(vec![
        Cell::new("records"),
        Cell::new(json_usize_field(outcome, "record_count")),
    ]);
    table.add_row(vec![Cell::new("output_dir"), Cell::new(output_dir.clone())]);
    table.add_row(vec![
        Cell::new("split_path"),
        Cell::new(json_field(outcome, "split_path")),
    ]);
    table.add_row(vec![
        Cell::new("manifest_path"),
        Cell::new(json_field(outcome, "manifest_path")),
    ]);
    table.add_row(vec![
        Cell::new("template"),
        Cell::new(json_field(outcome, "template_version")),
    ]);
    table.add_row(vec![
        Cell::new("next"),
        Cell::new(format!("tentgent dataset validate {output_dir}")),
    ]);
    table.add_row(vec![
        Cell::new("import"),
        Cell::new(format!("tentgent dataset add {output_dir}")),
    ]);
    println!("{table}");

    if let Some(warnings) = outcome.get("warnings").and_then(Value::as_array) {
        if !warnings.is_empty() {
            println!("{} Warnings", style("note").yellow().bold());
            for warning in warnings.iter().filter_map(Value::as_str) {
                println!("- {warning}");
            }
        }
    }

    println!();
}

fn render_removal_outcome(outcome: &DatasetRemovalOutcome) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Dataset removed").bold()
    );
    println!(
        "{} dataset {} from {}",
        style("removed").red().bold(),
        outcome.metadata.short_ref,
        outcome.store_path.display()
    );

    if !outcome.removed_index_paths.is_empty() {
        println!(
            "removed {} index file(s)",
            outcome.removed_index_paths.len()
        );
    }
    println!();
}

fn render_export_outcome(outcome: &DatasetExportOutcome) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Dataset exported").bold()
    );
    println!(
        "{} dataset {} to {}",
        style("exported").green().bold(),
        outcome.metadata.short_ref,
        outcome.destination_path.display()
    );

    let mut table = base_table();
    table.add_row(vec![
        Cell::new("dataset_ref"),
        Cell::new(&outcome.metadata.dataset_ref),
    ]);
    table.add_row(vec![
        Cell::new("short_ref"),
        Cell::new(&outcome.metadata.short_ref),
    ]);
    table.add_row(vec![
        Cell::new("managed source"),
        Cell::new(outcome.managed_source_path.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("destination"),
        Cell::new(outcome.destination_path.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("next step"),
        Cell::new("edit the exported copy, then run `tentgent dataset add <PATH>`"),
    ]);

    println!("{table}");
    println!();
}

fn render_diff_outcome(outcome: &DatasetDiffOutcome) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Dataset diff").bold()
    );
    println!(
        "left {}  right {}",
        style(&outcome.left.label).bold(),
        style(&outcome.right.label).bold()
    );

    let summary = &outcome.diff.summary;
    let mut table = base_table();
    table.add_row(vec![Cell::new("added"), Cell::new(summary.added)]);
    table.add_row(vec![Cell::new("removed"), Cell::new(summary.removed)]);
    table.add_row(vec![Cell::new("modified"), Cell::new(summary.modified)]);
    table.add_row(vec![Cell::new("unchanged"), Cell::new(summary.unchanged)]);
    table.add_row(vec![
        Cell::new("bytes"),
        Cell::new(format!(
            "{} -> {}",
            summary.left_total_bytes, summary.right_total_bytes
        )),
    ]);
    table.add_row(vec![
        Cell::new("tuning_ready"),
        Cell::new(format!(
            "{} -> {}",
            yes_no(outcome.left.tuning_ready),
            yes_no(outcome.right.tuning_ready)
        )),
    ]);
    table.add_row(vec![
        Cell::new("splits"),
        Cell::new(format!(
            "{} -> {}",
            outcome.left.splits, outcome.right.splits
        )),
    ]);
    if let Some(path) = &outcome.right.path {
        table.add_row(vec![
            Cell::new("right path"),
            Cell::new(path.display().to_string()),
        ]);
    }
    println!("{table}");

    let changed_files = outcome
        .diff
        .files
        .iter()
        .filter(|file| file.status != DatasetDiffStatus::Unchanged)
        .collect::<Vec<_>>();
    if changed_files.is_empty() {
        println!("{} No file-level changes.\n", style("clean").green().bold());
        return;
    }

    let mut files = Table::new();
    files
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["status", "path", "bytes"]);

    for file in changed_files {
        files.add_row(vec![
            Cell::new(file.status.as_str()),
            Cell::new(&file.relative_path),
            Cell::new(size_transition(file.left_size_bytes, file.right_size_bytes)),
        ]);
    }

    println!("{files}");
    println!();
}

fn render_dataset_list(datasets: &[DatasetSummary]) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Managed datasets").bold()
    );

    if datasets.is_empty() {
        println!(
            "{} No managed datasets are stored yet.\n",
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
            "ready",
            "splits",
            "source",
            "files",
            "bytes",
        ]);

    for dataset in datasets {
        table.add_row(vec![
            Cell::new(&dataset.metadata.short_ref),
            Cell::new(dataset.metadata.dataset_format.as_str()),
            Cell::new(yes_no(dataset.metadata.package.tuning_ready)),
            Cell::new(split_summary(&dataset.metadata)),
            Cell::new(dataset.metadata.source_summary()),
            Cell::new(dataset.metadata.file_count),
            Cell::new(dataset.metadata.total_bytes),
        ]);
    }

    println!("{table}");
    println!();
}

fn render_dataset_inspection(inspection: &DatasetInspection) {
    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style("Dataset inspection").bold(),
        style(&inspection.metadata.short_ref).bold()
    );

    let mut table = base_table();
    add_dataset_metadata_rows(&mut table, &inspection.metadata);
    table.add_row(vec![
        Cell::new("store path"),
        Cell::new(inspection.store_path.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("manifest path"),
        Cell::new(inspection.manifest_path.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("managed source"),
        Cell::new(inspection.source_path.display().to_string()),
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

fn add_dataset_metadata_rows(table: &mut Table, metadata: &DatasetMetadata) {
    table.add_row(vec![
        Cell::new("dataset_ref"),
        Cell::new(&metadata.dataset_ref),
    ]);
    table.add_row(vec![Cell::new("short_ref"), Cell::new(&metadata.short_ref)]);
    table.add_row(vec![
        Cell::new("source_kind"),
        Cell::new(metadata.source_kind.as_str()),
    ]);

    if let Some(path) = &metadata.source_path {
        table.add_row(vec![Cell::new("source_path"), Cell::new(path)]);
    }

    if let Some(repo) = &metadata.source_repo {
        table.add_row(vec![Cell::new("source_repo"), Cell::new(repo)]);
    }

    if let Some(revision) = &metadata.source_revision {
        table.add_row(vec![Cell::new("source_revision"), Cell::new(revision)]);
    }

    table.add_row(vec![
        Cell::new("dataset_format"),
        Cell::new(metadata.dataset_format.as_str()),
    ]);
    table.add_row(vec![
        Cell::new("tuning_ready"),
        Cell::new(yes_no(metadata.package.tuning_ready)),
    ]);
    add_optional_row(table, "train", metadata.package.splits.train.as_deref());
    add_optional_row(
        table,
        "validation",
        metadata.package.splits.validation.as_deref(),
    );
    add_optional_row(table, "test", metadata.package.splits.test.as_deref());
    add_optional_row(
        table,
        "eval_cases",
        metadata.package.splits.eval_cases.as_deref(),
    );
    add_optional_row(
        table,
        "source_manifest",
        metadata.package.splits.source_manifest.as_deref(),
    );
    if !metadata.package.warnings.is_empty() {
        table.add_row(vec![
            Cell::new("warnings"),
            Cell::new(metadata.package.warnings.join("\n")),
        ]);
    }
    table.add_row(vec![
        Cell::new("file_count"),
        Cell::new(metadata.file_count),
    ]);
    table.add_row(vec![
        Cell::new("total_bytes"),
        Cell::new(metadata.total_bytes),
    ]);
    table.add_row(vec![
        Cell::new("imported_at"),
        Cell::new(&metadata.imported_at),
    ]);
}

fn add_optional_row(table: &mut Table, label: &str, value: Option<&str>) {
    if let Some(value) = value {
        table.add_row(vec![Cell::new(label), Cell::new(value)]);
    }
}

fn split_summary(metadata: &DatasetMetadata) -> String {
    let splits = &metadata.package.splits;
    let mut names = Vec::new();
    if splits.train.is_some() {
        names.push("train");
    }
    if splits.validation.is_some() {
        names.push("valid");
    }
    if splits.test.is_some() {
        names.push("test");
    }
    if splits.eval_cases.is_some() {
        names.push("eval");
    }

    if names.is_empty() {
        "-".to_string()
    } else {
        names.join(",")
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn size_transition(left: Option<u64>, right: Option<u64>) -> String {
    match (left, right) {
        (Some(left), Some(right)) => format!("{left} -> {right}"),
        (Some(left), None) => format!("{left} -> -"),
        (None, Some(right)) => format!("- -> {right}"),
        (None, None) => "-".to_string(),
    }
}

async fn preflight_dataset_provider_auth(provider_name: &str) -> Result<DatasetProviderAuth> {
    let (provider, normalized_provider) = auth_provider_for_dataset_synth(provider_name)?;
    let auth = AuthManager::new().into_diagnostic()?;
    let Some((source, secret)) = auth.effective_secret(provider).into_diagnostic()? else {
        return Err(miette!(
            "{} key is missing for dataset synth; run `tentgent auth {} set` or set `{}` before launch",
            provider.display_name(),
            provider.cli_name(),
            provider.env_var()
        ));
    };

    match auth.validate_secret(provider, &secret).await {
        KeyValidationState::Verified => {
            render_dataset_provider_auth_preflight(provider, source);
            Ok(DatasetProviderAuth {
                provider,
                normalized_provider,
                secret,
            })
        }
        KeyValidationState::Invalid { reason } => Err(miette!(
            "{} key from {} is invalid for dataset synth: {}",
            provider.display_name(),
            source,
            reason
        )),
        KeyValidationState::Unknown { reason } => Err(miette!(
            "{} key from {} could not be verified for dataset synth: {}",
            provider.display_name(),
            source,
            reason
        )),
        KeyValidationState::Missing => Err(miette!(
            "{} key is missing for dataset synth; run `tentgent auth {} set` or set `{}` before launch",
            provider.display_name(),
            provider.cli_name(),
            provider.env_var()
        )),
    }
}

async fn run_dataset_synth_runtime(
    auth: &DatasetProviderAuth,
    model: &str,
    output: &Path,
    brief: Option<&str>,
    spec: Option<&Path>,
    split: &str,
    max_tokens: Option<u32>,
    temperature: f32,
) -> Result<Value> {
    let python_runtime = resolve_python_runtime()?;
    let python = require_python_interpreter(&python_runtime, "python dataset synth runtime")?;

    let mut process = Command::new(&python);
    process
        .current_dir(python_runtime.project_dir())
        .env("PYTHONPATH", python_runtime.python_src_dir())
        .env(auth.provider.env_var(), &auth.secret)
        .arg("-m")
        .arg("tentgent_daemon.cli.dataset_synth")
        .arg("--provider")
        .arg(auth.normalized_provider)
        .arg("--model")
        .arg(model)
        .arg("--output")
        .arg(output)
        .arg("--split")
        .arg(split)
        .arg("--temperature")
        .arg(temperature.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if let Some(brief) = brief {
        process.arg("--brief").arg(brief);
    }
    if let Some(spec) = spec {
        process.arg("--spec").arg(spec);
    }
    if let Some(max_tokens) = max_tokens {
        process.arg("--max-tokens").arg(max_tokens.to_string());
    }

    let output = process.output().await.into_diagnostic()?;
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        if stderr.is_empty() {
            return Err(miette!(
                "dataset synth runtime exited with status {}",
                output.status
            ));
        }
        return Err(miette!(
            "dataset synth runtime exited with status {}\n\n{}",
            output.status,
            stderr
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    serde_json::from_str::<Value>(&stdout)
        .map_err(|err| miette!("dataset synth runtime returned invalid JSON: {err}\n\n{stdout}"))
}

fn auth_provider_for_dataset_synth(provider: &str) -> Result<(Provider, &'static str)> {
    match provider {
        "openai" => Ok((Provider::OpenAI, "openai")),
        "anthropic" | "claude" => Ok((Provider::Anthropic, "anthropic")),
        other => Err(miette!("unsupported dataset synth provider `{other}`")),
    }
}

fn render_dataset_provider_auth_preflight(provider: Provider, source: KeySource) {
    println!(
        "{} {} key verified from {} for dataset synth.",
        style("verified").green().bold(),
        provider.display_name(),
        source
    );
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

fn explain_dataset_lookup_error(command: &str, err: DatasetError) -> miette::Report {
    match err {
        DatasetError::NotFound(_) | DatasetError::AmbiguousRef(_) => miette!(
            "{err}\n\nUsage: {}\nHint: use `tentgent dataset {command} --help` for the command template.",
            usage_for_command(command),
        ),
        other => miette!("{other}"),
    }
}

fn explain_dataset_export_error(reference: &str, path: &Path, err: DatasetError) -> miette::Report {
    match err {
        DatasetError::NotFound(_) | DatasetError::AmbiguousRef(_) => {
            explain_dataset_lookup_error("export", err)
        }
        DatasetError::ExportDestinationNotEmpty(_) => {
            let suggested_path = export_child_path(path, reference);
            miette!(
                "{err}\n\nHint: export into a new child directory instead:\n  tentgent dataset export {reference} {}",
                suggested_path.display()
            )
        }
        other => miette!("{other}"),
    }
}

fn export_child_path(path: &Path, reference: &str) -> PathBuf {
    path.join(reference)
}

fn usage_for_command(command: &str) -> &'static str {
    match command {
        "diff" => "tentgent dataset diff <LEFT_REF> <RIGHT_REF>\n       tentgent dataset diff <LEFT_REF> --path <PATH>",
        "export" => "tentgent dataset export <DATASET_REF> <PATH>",
        "rm" => "tentgent dataset rm <DATASET_REF>",
        "synth" => "tentgent dataset synth --provider <openai|anthropic|claude> --model <MODEL> --output <DIR> (--brief <TEXT> | --spec <PATH>)",
        "validate" => "tentgent dataset validate <PATH>",
        _ => "tentgent dataset inspect <DATASET_REF>",
    }
}
