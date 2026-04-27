use clap::CommandFactory;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use miette::{miette, IntoDiagnostic};
use tentgent_core::model::{
    HfPullProgress, ImportOutcome, ModelError, ModelInspection, ModelManager, ModelSummary,
    RemovalOutcome,
};
use tentgent_core::platform::model_format_capability;

use super::app::Cli;
use super::commands::ModelCommands;

pub fn handle_model_command(action: ModelCommands) -> miette::Result<()> {
    let manager = ModelManager::new().into_diagnostic()?;

    match action {
        ModelCommands::Add { path } => {
            let outcome = manager.add_path(path).into_diagnostic()?;
            render_import_outcome("Model imported", &outcome);
        }
        ModelCommands::Pull { repo_id, revision } => {
            let mut progress = PullProgress::new(&repo_id, revision.as_deref());
            let outcome = manager.pull_hf_with_progress(&repo_id, revision.as_deref(), |event| {
                progress.update(event);
            });
            progress.finish();

            let outcome = outcome.into_diagnostic()?;
            render_import_outcome("Model pulled", &outcome);
        }
        ModelCommands::Ls => {
            let models = manager.list_models().into_diagnostic()?;
            render_model_list(&models);
        }
        ModelCommands::Rm { hash } => {
            if is_help_token(&hash) {
                print_model_subcommand_help("rm")?;
                return Ok(());
            }

            let outcome = match manager.remove(&hash) {
                Ok(outcome) => outcome,
                Err(err) => return Err(explain_hash_lookup_error("rm", "HASH", err)),
            };
            render_model_removal(&outcome);
        }
        ModelCommands::Inspect { reference } => {
            if is_help_token(&reference) {
                print_model_subcommand_help("inspect")?;
                return Ok(());
            }

            let inspection = match manager.inspect(&reference) {
                Ok(inspection) => inspection,
                Err(err) => return Err(explain_hash_lookup_error("inspect", "REF", err)),
            };
            render_model_inspection(&inspection);
        }
    }

    Ok(())
}

fn render_import_outcome(title: &str, outcome: &ImportOutcome) {
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

fn render_model_removal(outcome: &RemovalOutcome) {
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
            "bytes",
        ]);

    for model in models {
        table.add_row(vec![
            Cell::new(&model.metadata.short_ref),
            Cell::new(model.metadata.primary_format.as_str()),
            Cell::new(model.metadata.source_kind.as_str()),
            Cell::new(model.metadata.source_summary()),
            Cell::new(model.metadata.file_count),
            Cell::new(model.metadata.total_bytes),
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

    fn update(&mut self, event: HfPullProgress) {
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

fn add_model_metadata_rows(table: &mut Table, metadata: &tentgent_core::model::ModelMetadata) {
    table.add_row(vec![Cell::new("model_ref"), Cell::new(&metadata.model_ref)]);
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
        Cell::new(model_format_capability(metadata.primary_format).summary()),
    ]);
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

fn is_help_token(value: &str) -> bool {
    matches!(value, "help" | "--help" | "-h")
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

fn explain_hash_lookup_error(command: &str, value_name: &str, err: ModelError) -> miette::Report {
    match err {
        ModelError::NotFound(_) | ModelError::AmbiguousRef(_) => miette!(
            "{err}\n\nUsage: tentgent model {command} <{value_name}>\nHint: use `tentgent model {command} --help` for the command template."
        ),
        other => miette!("{other}"),
    }
}
