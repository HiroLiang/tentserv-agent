use std::path::Path;

use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use tentgent_core::adapter::{AdapterMetadata, AdapterSourceKind, AdapterSummary};

pub fn render_adapter_list(adapters: &[AdapterSummary]) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Managed adapters").bold()
    );

    if adapters.is_empty() {
        println!(
            "{} No managed adapters are stored yet.\n",
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
            "base",
            "source_kind",
            "source",
            "files",
            "bytes",
        ]);

    for adapter in adapters {
        table.add_row(vec![
            Cell::new(&adapter.metadata.short_ref),
            Cell::new(adapter.metadata.adapter_format.as_str()),
            Cell::new(base_summary(&adapter.metadata)),
            Cell::new(adapter.metadata.source_kind.as_str()),
            Cell::new(source_summary(&adapter.metadata)),
            Cell::new(adapter.metadata.file_count),
            Cell::new(adapter.metadata.total_bytes),
        ]);
    }

    println!("{table}");
    println!();
}

fn base_summary(metadata: &AdapterMetadata) -> String {
    if let Some(base_model_ref) = &metadata.base_model_ref {
        return short_ref(base_model_ref);
    }

    if let Some(repo) = &metadata.base_model_source_repo {
        return format!("{repo} (source)");
    }

    "(not bound)".to_string()
}

fn source_summary(metadata: &AdapterMetadata) -> String {
    match metadata.source_kind {
        AdapterSourceKind::TrainRun => metadata
            .training_run_ref
            .as_deref()
            .map(|run_ref| format!("run:{}", short_ref(run_ref)))
            .unwrap_or_else(|| "run:(unknown)".to_string()),
        AdapterSourceKind::HuggingFace => hf_summary(metadata),
        AdapterSourceKind::Local => metadata
            .source_path
            .as_deref()
            .map(compact_path)
            .unwrap_or_else(|| "local".to_string()),
    }
}

fn hf_summary(metadata: &AdapterMetadata) -> String {
    let Some(repo) = &metadata.source_repo else {
        return "huggingface".to_string();
    };

    metadata
        .source_revision
        .as_deref()
        .map(|revision| format!("{}@{}", compact_repo(repo), short_ref(revision)))
        .unwrap_or_else(|| compact_repo(repo))
}

fn compact_path(value: &str) -> String {
    if value.len() <= 40 {
        return value.to_string();
    }

    Path::new(value)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| format!(".../{name}"))
        .unwrap_or_else(|| format!("{}...", value.chars().take(37).collect::<String>()))
}

fn compact_repo(repo: &str) -> String {
    if repo.len() <= 36 {
        return repo.to_string();
    }

    let Some((owner, name)) = repo.split_once('/') else {
        return format!("{}...", repo.chars().take(33).collect::<String>());
    };

    if name.len() <= 24 {
        return repo.to_string();
    }

    let suffix = name
        .chars()
        .rev()
        .take(21)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("{owner}/...{suffix}")
}

fn short_ref(value: &str) -> String {
    value.chars().take(12).collect()
}
