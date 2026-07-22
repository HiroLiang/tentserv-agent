use super::*;

pub(super) fn render_import_outcome(title: &str, outcome: &ModelImportOutcome) {
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

pub(super) fn render_model_removal(outcome: &ModelRemovalOutcome) {
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

pub(super) fn render_model_list(
    models: &[ModelSummary],
    proofs_by_model_ref: &HashMap<ModelRef, Vec<ModelCapabilityProof>>,
) {
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
            "support",
        ]);

    for model in models {
        let proofs = proofs_by_model_ref
            .get(&model.metadata.model_ref)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        table.add_row(vec![
            Cell::new(&model.metadata.short_ref),
            Cell::new(model.metadata.primary_format.as_str()),
            Cell::new(model.metadata.source_kind.as_str()),
            Cell::new(model_list_source_label(&model.metadata)),
            Cell::new(model.metadata.file_count),
            Cell::new(format_bytes(model.metadata.total_bytes)),
            Cell::new(model_support_list_label(&model.metadata, proofs)),
        ]);
    }

    println!("{table}");
    println!();
    println!("{}", style("Inspect:").bold());
    println!("tentgent model inspect <short_ref>");
    println!();
}

pub(super) fn render_model_catalog(entries: &[ModelSupportCatalogEntry]) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Built-in model support catalog").bold()
    );

    if entries.is_empty() {
        println!(
            "{} No built-in catalog entries matched the filters.\n",
            style("empty").yellow().bold()
        );
        return;
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            "publisher",
            "family",
            "scale",
            "capabilities",
            "support",
            "source",
        ]);

    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            table.add_row(vec![
                Cell::new(""),
                Cell::new(""),
                Cell::new(""),
                Cell::new(""),
                Cell::new(""),
                Cell::new(""),
            ]);
        }
        for row in model_catalog_table_rows(entry) {
            table.add_row(row);
        }
    }

    println!("{table}");
    render_model_catalog_pull_suggestions(entries);
    println!();
}

pub(super) fn model_catalog_table_rows(entry: &ModelSupportCatalogEntry) -> Vec<Vec<Cell>> {
    let capabilities = entry
        .capabilities
        .iter()
        .map(|capability| capability.as_str().to_string())
        .collect::<Vec<_>>();
    let sources = catalog_source_rows(entry);

    let first_capability = capabilities.first().map(String::as_str).unwrap_or("");
    let first_source = sources.first().map(String::as_str).unwrap_or("");
    let mut rows = vec![vec![
        Cell::new(&entry.publisher),
        Cell::new(&entry.family),
        Cell::new(entry.parameter_scale.as_deref().unwrap_or("-")),
        Cell::new(first_capability),
        Cell::new(entry.support_level.as_str()),
        Cell::new(first_source),
    ]];

    for capability in capabilities.iter().skip(1) {
        rows.push(vec![
            Cell::new(""),
            Cell::new(""),
            Cell::new(""),
            Cell::new(capability),
            Cell::new(""),
            Cell::new(""),
        ]);
    }

    for source in sources.iter().skip(1) {
        rows.push(vec![
            Cell::new(""),
            Cell::new(""),
            Cell::new(""),
            Cell::new(""),
            Cell::new(""),
            Cell::new(source),
        ]);
    }

    rows
}

pub(super) fn catalog_source_rows(entry: &ModelSupportCatalogEntry) -> Vec<String> {
    if !entry.source_repos.is_empty() {
        return entry.source_repos.clone();
    }

    if !entry.source_repo_patterns.is_empty() {
        return entry.source_repo_patterns.clone();
    }

    vec!["unknown".to_string()]
}

pub(super) fn render_model_catalog_pull_suggestions(entries: &[ModelSupportCatalogEntry]) {
    println!();
    println!("{}", style("Pull command template").bold());
    println!("tentgent model pull <publisher>/<source> --capability <capability>");

    let capabilities = catalog_result_capabilities(entries);
    if !capabilities.is_empty() {
        println!();
        println!("{}", style("capabilities:").bold());
        for capability in capabilities {
            println!(
                "{}: {}",
                capability.as_str(),
                catalog_capability_description(capability)
            );
        }
    }
}

pub(super) fn catalog_result_capabilities(
    entries: &[ModelSupportCatalogEntry],
) -> Vec<ModelCapability> {
    MODEL_CAPABILITY_CANONICAL_ORDER
        .into_iter()
        .filter(|capability| {
            entries
                .iter()
                .any(|entry| entry.capabilities.contains(capability))
        })
        .collect()
}

pub(super) fn catalog_capability_description(capability: ModelCapability) -> &'static str {
    match capability {
        ModelCapability::Chat => "text chat and instruction-following model endpoints",
        ModelCapability::Embedding => "text embedding endpoints for retrieval and similarity",
        ModelCapability::Rerank => "query/document reranking endpoints",
        ModelCapability::AudioTranscription => "audio-to-text transcription workflows",
        ModelCapability::AudioSpeech => "text-to-speech generation workflows",
        ModelCapability::VisionChat => "image-plus-text chat and visual understanding",
        ModelCapability::VideoUnderstanding => "video understanding workflows",
        ModelCapability::ImageGeneration => "text-to-image and image workflow generation",
    }
}

pub(super) fn model_list_source_label(metadata: &ModelMetadata) -> String {
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

pub(super) fn filter_model_catalog_entries(
    entries: Vec<ModelSupportCatalogEntry>,
    capability: Option<ModelCapability>,
    publisher: Option<&str>,
    support_level: Option<ModelSupportCatalogLevel>,
    local: bool,
    query: Option<&str>,
) -> Vec<ModelSupportCatalogEntry> {
    entries
        .into_iter()
        .filter(|entry| {
            capability.map_or(true, |capability| entry.capabilities.contains(&capability))
        })
        .filter(|entry| {
            publisher.map_or(true, |publisher| {
                contains_case_insensitive(&entry.publisher, publisher)
            })
        })
        .filter(|entry| support_level.map_or(true, |level| entry.support_level == level))
        .filter(|entry| {
            !local
                || matches!(
                    entry.support_level,
                    ModelSupportCatalogLevel::FixtureSupported
                        | ModelSupportCatalogLevel::LocalRuntimeSupported
                )
        })
        .filter(|entry| query.map_or(true, |query| catalog_entry_matches_query(entry, query)))
        .collect()
}

pub(super) fn catalog_entry_matches_query(entry: &ModelSupportCatalogEntry, query: &str) -> bool {
    [
        entry.publisher.as_str(),
        entry.family.as_str(),
        entry.parameter_scale.as_deref().unwrap_or(""),
        entry.support_level.as_str(),
        entry.evidence.as_str(),
        entry.reason.as_deref().unwrap_or(""),
    ]
    .into_iter()
    .any(|value| contains_case_insensitive(value, query))
        || entry
            .source_repos
            .iter()
            .chain(entry.source_repo_patterns.iter())
            .chain(entry.tags.iter())
            .chain(entry.recommended_for.iter())
            .chain(entry.runtime_notes.iter())
            .any(|value| contains_case_insensitive(value, query))
}

pub(super) fn contains_case_insensitive(value: &str, needle: &str) -> bool {
    value
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

pub(super) fn render_model_inspection(
    inspection: &ModelInspection,
    proofs: &[ModelCapabilityProof],
) {
    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style("Model inspection").bold(),
        style(&inspection.metadata.short_ref).bold()
    );

    let mut table = base_table();
    add_model_metadata_rows(&mut table, &inspection.metadata);
    add_model_catalog_rows(&mut table, &inspection.metadata);
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
    add_model_file_diagnostic_rows(&mut table, inspection);
    add_model_support_status_rows(&mut table, &inspection.metadata, proofs);

    println!("{table}");
    println!();
}

pub(super) fn add_model_file_diagnostic_rows(table: &mut Table, inspection: &ModelInspection) {
    let store = ModelStoreLayout::from_models_dir(
        inspection
            .store_path
            .parent()
            .and_then(|path| path.parent())
            .map(|path| path.to_path_buf())
            .unwrap_or_else(|| inspection.store_path.clone()),
    );
    let diagnostics = model_file_diagnostics(&store, &inspection.metadata);
    if diagnostics.is_empty() {
        table.add_row(vec![Cell::new("model files"), Cell::new("ok")]);
        return;
    }

    let lines = diagnostics
        .iter()
        .map(|diagnostic| {
            format!(
                "{}: {}\npath: {}\n{}\nnext_action: {}",
                diagnostic.severity.as_str(),
                diagnostic.code.as_str(),
                diagnostic.path.display(),
                diagnostic.message,
                diagnostic.next_action
            )
        })
        .collect::<Vec<_>>();
    table.add_row(vec![
        Cell::new("model files"),
        Cell::new(lines.join("\n\n")),
    ]);
}

pub(super) fn add_model_support_status_rows(
    table: &mut Table,
    metadata: &ModelMetadata,
    proofs: &[ModelCapabilityProof],
) {
    let summaries = model_support_summaries(metadata, proofs);
    if summaries.is_empty() {
        table.add_row(vec![Cell::new("capability support"), Cell::new("none")]);
        return;
    }

    for (index, summary) in summaries.into_iter().enumerate() {
        let mut lines = model_support_diagnostic_lines(&summary, Some(&metadata.short_ref));
        if index > 0 {
            lines.insert(0, String::new());
        }

        table.add_row(vec![
            Cell::new(if index == 0 { "capability support" } else { "" }),
            Cell::new(lines.join("\n")),
        ]);
    }
}

pub(super) fn add_model_catalog_rows(table: &mut Table, metadata: &ModelMetadata) {
    let entries = built_in_catalog_entries_for_model(metadata);
    if entries.is_empty() {
        table.add_row(vec![Cell::new("catalog"), Cell::new("not found")]);
        return;
    }

    let lines = entries
        .iter()
        .enumerate()
        .flat_map(|(index, entry)| model_catalog_entry_lines(index, entry))
        .collect::<Vec<_>>();

    table.add_row(vec![Cell::new("catalog"), Cell::new(lines.join("\n"))]);
}

pub(super) fn model_catalog_entry_lines(
    index: usize,
    entry: &ModelSupportCatalogEntry,
) -> Vec<String> {
    let mut lines = Vec::new();
    if index > 0 {
        lines.push(String::new());
    }
    lines.extend([
        format!("known: yes ({})", entry.source_label()),
        format!("publisher: {}", entry.publisher),
        format!("family: {}", entry.family),
    ]);
    if let Some(parameter_scale) = entry.parameter_scale.as_deref() {
        lines.push(format!("parameter_scale: {parameter_scale}"));
    }
    lines.push(format!(
        "capabilities: {}",
        model_capabilities_label(&entry.capabilities)
    ));
    if !entry.tags.is_empty() {
        lines.push(format!("tags: {}", entry.tags.join(", ")));
    }
    lines.push(format!("support_level: {}", entry.support_level.as_str()));
    lines.push(format!("evidence: {}", entry.evidence.as_str()));
    if !entry.runtime_notes.is_empty() {
        lines.push(format!("runtime_notes: {}", entry.runtime_notes.join(", ")));
    }
    if let Some(reason) = entry.reason.as_deref() {
        lines.push(format!("reason: {reason}"));
    }
    lines
}

pub(super) fn render_model_capability_show(inspection: &ModelInspection) {
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

pub(super) fn render_model_capability_update(result: &ModelCapabilityUpdateResult) {
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

pub(super) fn render_model_capability_proofs(
    inspection: &ModelInspection,
    proofs: &[ModelCapabilityProof],
) {
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
            "profile",
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
            Cell::new(proof_profile_label(proof)),
            Cell::new(proof.server_ref.as_deref().unwrap_or("-")),
            Cell::new(&proof.checked_at),
            Cell::new(proof.error.as_deref().unwrap_or("-")),
        ]);
    }

    println!("{table}");
    println!();
}

pub(super) fn render_model_capability_verify(
    inspection: &ModelInspection,
    proof: &ModelCapabilityProof,
) {
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

pub(super) fn render_model_capability_proof_clear(result: &ModelCapabilityProofClearResult) {
    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style("Model capability proofs cleared").bold(),
        style(&result.model.metadata.short_ref).bold()
    );

    let mut table = base_table();
    table.add_row(vec![
        Cell::new("model_ref"),
        Cell::new(result.model.metadata.model_ref.as_str()),
    ]);
    table.add_row(vec![
        Cell::new("capability"),
        Cell::new(result.capability.as_str()),
    ]);
    table.add_row(vec![
        Cell::new("removed_proofs"),
        Cell::new(result.removed_proof_count.to_string()),
    ]);

    println!("{table}");
    println!();
}

pub(super) fn add_model_proof_rows(table: &mut Table, proof: &ModelCapabilityProof) {
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
    if let Some(profile) = &proof.runtime_profile {
        table.add_row(vec![Cell::new("runtime_profile"), Cell::new(profile)]);
    }
    if let Some(version) = proof.runtime_profile_version {
        table.add_row(vec![
            Cell::new("runtime_profile_version"),
            Cell::new(version.to_string()),
        ]);
    }
    if let Some(server_ref) = &proof.server_ref {
        table.add_row(vec![Cell::new("server_ref"), Cell::new(server_ref)]);
    }
    table.add_row(vec![Cell::new("checked_at"), Cell::new(&proof.checked_at)]);
    if let Some(error) = &proof.error {
        table.add_row(vec![Cell::new("error"), Cell::new(error)]);
    }
}

pub(super) fn proof_profile_label(proof: &ModelCapabilityProof) -> String {
    match (&proof.runtime_profile, proof.runtime_profile_version) {
        (Some(profile), Some(version)) => format!("{profile}-v{version}"),
        (Some(profile), None) => profile.clone(),
        (None, Some(version)) => format!("v{version}"),
        (None, None) => "-".to_string(),
    }
}

pub(super) fn render_capability_warning(
    metadata: &tentgent_kernel::features::model::domain::ModelMetadata,
) {
    if let Some(warning) = metadata.capability_warning() {
        eprintln!("{} {}", style("warning").yellow().bold(), warning);
    }
}

pub(super) fn base_table() -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["Field", "Value"]);
    table
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PullProgressMode {
    Spinner,
    Files,
    Bytes,
}

pub(super) struct PullProgress {
    bar: ProgressBar,
    repo_id: String,
    mode: PullProgressMode,
}

impl PullProgress {
    pub(super) fn new(repo_id: &str, revision: Option<&str>) -> Self {
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

    pub(super) fn update(&mut self, event: HfModelPullProgress) {
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

    pub(super) fn finish(&self) {
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

pub(super) fn add_model_metadata_rows(
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
