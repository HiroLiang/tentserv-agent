use super::*;

pub(super) fn render_import_outcome(outcome: &DatasetImportOutcome) {
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

pub(super) fn render_validation_outcome(outcome: &DatasetValidationOutcome) {
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

pub(super) fn render_template_written(path: &Path, request: &DatasetTemplateRequest) {
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

pub(super) fn render_synth_outcome(outcome: &Value) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Dataset synthesized").bold()
    );

    let output_dir = json_field(outcome, "output_dir");
    let splits = outcome
        .get("splits")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
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
        Cell::new(if splits.len() > 1 {
            splits
                .iter()
                .filter_map(|split| split.get("split").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            json_field(outcome, "split")
        }),
    ]);
    table.add_row(vec![
        Cell::new("records"),
        Cell::new(json_usize_field(outcome, "record_count")),
    ]);
    table.add_row(vec![Cell::new("output_dir"), Cell::new(output_dir.clone())]);
    if outcome.get("split_path").is_some() {
        table.add_row(vec![
            Cell::new("split_path"),
            Cell::new(json_field(outcome, "split_path")),
        ]);
    }
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

    if splits.len() > 1 {
        let mut split_table = Table::new();
        split_table
            .load_preset(UTF8_FULL_CONDENSED)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec!["split", "records", "path"]);
        for split in &splits {
            split_table.add_row(vec![
                Cell::new(json_field(split, "split")),
                Cell::new(json_usize_field(split, "record_count")),
                Cell::new(json_field(split, "split_path")),
            ]);
        }
        println!("{split_table}");
    }

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

pub(super) fn render_eval_outcome(outcome: &Value) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Dataset evaluated").bold()
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
        Cell::new("reviewed"),
        Cell::new(format!(
            "{} / {}",
            json_usize_field(outcome, "reviewed_records"),
            json_usize_field(outcome, "total_records")
        )),
    ]);
    table.add_row(vec![
        Cell::new("local_issues"),
        Cell::new(json_usize_field(outcome, "local_issue_count")),
    ]);
    table.add_row(vec![
        Cell::new("findings"),
        Cell::new(json_usize_field(outcome, "finding_count")),
    ]);
    table.add_row(vec![
        Cell::new("overall_score"),
        Cell::new(json_optional_number_field(outcome, "overall_score")),
    ]);
    table.add_row(vec![Cell::new("output_dir"), Cell::new(output_dir)]);
    table.add_row(vec![
        Cell::new("report_json"),
        Cell::new(json_field(outcome, "report_json_path")),
    ]);
    table.add_row(vec![
        Cell::new("report_md"),
        Cell::new(json_field(outcome, "report_md_path")),
    ]);
    table.add_row(vec![
        Cell::new("prompt"),
        Cell::new(json_field(outcome, "prompt_path")),
    ]);
    table.add_row(vec![
        Cell::new("raw_output"),
        Cell::new(json_field(outcome, "raw_output_path")),
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

pub(super) fn render_removal_outcome(outcome: &DatasetRemovalOutcome) {
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

pub(super) fn render_export_outcome(outcome: &DatasetExportOutcome) {
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

pub(super) fn render_diff_outcome(outcome: &DatasetDiffOutcome) {
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
        Cell::new("size"),
        Cell::new(format!(
            "{} -> {}",
            format_bytes(summary.left_total_bytes),
            format_bytes(summary.right_total_bytes)
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
        .set_header(vec!["status", "path", "size"]);

    for file in changed_files {
        files.add_row(vec![
            Cell::new(file.status.as_str()),
            Cell::new(&file.relative_path),
            Cell::new(format_size_transition(
                file.left_size_bytes,
                file.right_size_bytes,
            )),
        ]);
    }

    println!("{files}");
    println!();
}

pub(super) fn render_dataset_list(datasets: &[DatasetSummary]) {
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
            "size",
        ]);

    for dataset in datasets {
        table.add_row(vec![
            Cell::new(&dataset.metadata.short_ref),
            Cell::new(dataset.metadata.dataset_format.as_str()),
            Cell::new(yes_no(dataset.metadata.package.tuning_ready)),
            Cell::new(split_summary(&dataset.metadata)),
            Cell::new(dataset.metadata.source_summary()),
            Cell::new(dataset.metadata.file_count),
            Cell::new(format_bytes(dataset.metadata.total_bytes)),
        ]);
    }

    println!("{table}");
    println!();
}

pub(super) fn render_dataset_inspection(inspection: &DatasetInspection) {
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

pub(super) fn base_table() -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["Field", "Value"]);
    table
}

pub(super) fn add_dataset_metadata_rows(table: &mut Table, metadata: &DatasetMetadata) {
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
        Cell::new("size"),
        Cell::new(format_bytes(metadata.total_bytes)),
    ]);
    table.add_row(vec![
        Cell::new("imported_at"),
        Cell::new(&metadata.imported_at),
    ]);
}

pub(super) fn add_optional_row(table: &mut Table, label: &str, value: Option<&str>) {
    if let Some(value) = value {
        table.add_row(vec![Cell::new(label), Cell::new(value)]);
    }
}

pub(super) fn split_summary(metadata: &DatasetMetadata) -> String {
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

pub(super) fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}
