use std::path::{Path, PathBuf};

use clap::CommandFactory;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use miette::{miette, IntoDiagnostic, Result};
use tentgent_core::dataset::{
    DatasetDiffOutcome, DatasetDiffStatus, DatasetError, DatasetExportOutcome,
    DatasetImportOutcome, DatasetInspection, DatasetManager, DatasetMetadata,
    DatasetRemovalOutcome, DatasetSummary,
};

use super::app::Cli;
use super::commands::DatasetCommands;

pub fn handle_dataset_command(action: DatasetCommands) -> Result<()> {
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
        _ => "tentgent dataset inspect <DATASET_REF>",
    }
}
