use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use tentgent_kernel::features::adapter::domain::{
    AdapterBindOutcome, AdapterImportOutcome, AdapterInspection, AdapterMetadata,
    AdapterRemovalOutcome,
};

use super::display::format_bytes;

pub(super) fn render_import_outcome(title: &str, outcome: &AdapterImportOutcome) {
    let status = if outcome.deduplicated {
        style("reused").yellow().bold()
    } else {
        style("stored").green().bold()
    };

    println!("{} {}", style("==>").cyan().bold(), style(title).bold());
    println!(
        "{} adapter {} under {}",
        status,
        outcome.metadata.short_ref,
        outcome.store_path.display()
    );

    let mut table = base_table();
    add_adapter_metadata_rows(&mut table, &outcome.metadata);
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
    if let Some(path) = &outcome.base_index_path {
        table.add_row(vec![
            Cell::new("base index"),
            Cell::new(path.display().to_string()),
        ]);
    }

    println!("{table}");
    println!();
}

pub(super) fn render_bind_outcome(outcome: &AdapterBindOutcome) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Adapter bound").bold()
    );
    println!(
        "{} adapter {} to model {}",
        style("bound").green().bold(),
        outcome.metadata.short_ref,
        outcome
            .metadata
            .base_model_ref
            .as_ref()
            .map(|model_ref| model_ref.as_str())
            .unwrap_or("(not bound)")
    );

    let mut table = base_table();
    add_adapter_metadata_rows(&mut table, &outcome.metadata);
    table.add_row(vec![
        Cell::new("store path"),
        Cell::new(outcome.store_path.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("base index"),
        Cell::new(outcome.base_index_path.display().to_string()),
    ]);
    if let Some(path) = &outcome.removed_base_index_path {
        table.add_row(vec![
            Cell::new("removed old index"),
            Cell::new(path.display().to_string()),
        ]);
    }

    println!("{table}");
    println!();
}

pub(super) fn render_removal_outcome(outcome: &AdapterRemovalOutcome) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Adapter removed").bold()
    );
    println!(
        "{} adapter {} from {}",
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

pub(super) fn render_adapter_inspection(inspection: &AdapterInspection) {
    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style("Adapter inspection").bold(),
        style(&inspection.metadata.short_ref).bold()
    );

    let mut table = base_table();
    add_adapter_metadata_rows(&mut table, &inspection.metadata);
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

fn add_adapter_metadata_rows(table: &mut Table, metadata: &AdapterMetadata) {
    table.add_row(vec![
        Cell::new("adapter_ref"),
        Cell::new(metadata.adapter_ref.as_str()),
    ]);
    table.add_row(vec![Cell::new("short_ref"), Cell::new(&metadata.short_ref)]);
    table.add_row(vec![
        Cell::new("adapter_format"),
        Cell::new(metadata.adapter_format.as_str()),
    ]);
    table.add_row(vec![
        Cell::new("adapter_type"),
        Cell::new(metadata.adapter_type.as_str()),
    ]);
    if let Some(capability) = metadata.target_capability {
        table.add_row(vec![
            Cell::new("target_capability"),
            Cell::new(capability.as_str()),
        ]);
    }

    if let Some(base_model_ref) = &metadata.base_model_ref {
        table.add_row(vec![
            Cell::new("base_model_ref"),
            Cell::new(base_model_ref.as_str()),
        ]);
    }
    if let Some(repo) = &metadata.base_model_source_repo {
        table.add_row(vec![Cell::new("base_model_source_repo"), Cell::new(repo)]);
    }
    if let Some(revision) = &metadata.base_model_source_revision {
        table.add_row(vec![
            Cell::new("base_model_source_revision"),
            Cell::new(revision),
        ]);
    }
    if let Some(family) = &metadata.model_family {
        table.add_row(vec![Cell::new("model_family"), Cell::new(family)]);
    }

    table.add_row(vec![
        Cell::new("backend_support"),
        Cell::new(
            metadata
                .backend_support
                .iter()
                .map(|backend| backend.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        ),
    ]);
    if let Some(control_kind) = &metadata.control_kind {
        table.add_row(vec![Cell::new("control_kind"), Cell::new(control_kind)]);
    }
    if let Some(weight_file) = &metadata.weight_file {
        table.add_row(vec![Cell::new("weight_file"), Cell::new(weight_file)]);
    }
    if !metadata.trigger_words.is_empty() {
        table.add_row(vec![
            Cell::new("trigger_words"),
            Cell::new(metadata.trigger_words.join(", ")),
        ]);
    }
    if let Some(scale) = metadata.recommended_scale {
        table.add_row(vec![Cell::new("recommended_scale"), Cell::new(scale)]);
    }
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
    if let Some(dataset_ref) = &metadata.training_dataset_ref {
        table.add_row(vec![
            Cell::new("training_dataset_ref"),
            Cell::new(dataset_ref),
        ]);
    }
    if let Some(run_ref) = &metadata.training_run_ref {
        table.add_row(vec![Cell::new("training_run_ref"), Cell::new(run_ref)]);
    }
    if let Some(config_ref) = &metadata.training_config_ref {
        table.add_row(vec![
            Cell::new("training_config_ref"),
            Cell::new(config_ref),
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
