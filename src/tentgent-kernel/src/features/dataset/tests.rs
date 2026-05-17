use std::path::PathBuf;

use super::{
    domain::{
        DatasetFormat, DatasetManifest, DatasetManifestEntry, DatasetMetadata,
        DatasetPackageMetadata, DatasetRef, DatasetRefParseError, DatasetRefSelector,
        DatasetSourceKind, DatasetSplitKind, DatasetSplits, DatasetStoreLayout, DatasetSynthCounts,
        DatasetTemplateRequest, SHORT_DATASET_REF_LENGTH,
    },
    templates::render_dataset_generation_template,
};

#[test]
fn dataset_ref_is_canonical_sha256_hex_and_derives_short_ref() {
    let dataset_ref = DatasetRef::parse("A".repeat(64)).expect("dataset ref");

    assert_eq!(dataset_ref.as_str(), "a".repeat(64));
    assert_eq!(
        dataset_ref.short_ref(),
        "a".repeat(SHORT_DATASET_REF_LENGTH)
    );
    assert_eq!(dataset_ref.to_string(), "a".repeat(64));
}

#[test]
fn dataset_ref_selector_accepts_short_or_full_hex_prefixes() {
    let short = DatasetRefSelector::parse("abc123").expect("short selector");
    let full = DatasetRefSelector::parse("b".repeat(64)).expect("full selector");

    assert_eq!(short.as_str(), "abc123");
    assert!(!short.is_full_ref());
    assert!(full.is_full_ref());
}

#[test]
fn dataset_ref_validation_rejects_empty_wrong_length_and_non_hex_values() {
    assert_eq!(DatasetRef::parse(""), Err(DatasetRefParseError::Empty));
    assert_eq!(
        DatasetRef::parse("abc"),
        Err(DatasetRefParseError::InvalidFullLength { actual: 3 })
    );
    assert_eq!(
        DatasetRef::parse("z".repeat(64)),
        Err(DatasetRefParseError::NonHex)
    );
    assert_eq!(
        DatasetRefSelector::parse("a".repeat(65)),
        Err(DatasetRefParseError::PrefixTooLong { actual: 65 })
    );
}

#[test]
fn dataset_store_layout_matches_contract_paths() {
    let layout = DatasetStoreLayout::from_datasets_dir("/tmp/tentgent/datasets");
    let dataset_ref = DatasetRef::parse("d".repeat(64)).expect("dataset ref");

    assert_eq!(
        layout.store_dir,
        PathBuf::from("/tmp/tentgent/datasets/store")
    );
    assert_eq!(
        layout.local_index_dir,
        PathBuf::from("/tmp/tentgent/datasets/by-source/local")
    );
    assert_eq!(
        layout.dataset_metadata_path(&dataset_ref),
        PathBuf::from(format!(
            "/tmp/tentgent/datasets/store/{}/dataset.toml",
            dataset_ref.as_str()
        ))
    );
    assert_eq!(
        layout.source_dir(&dataset_ref),
        PathBuf::from(format!(
            "/tmp/tentgent/datasets/store/{}/source",
            dataset_ref.as_str()
        ))
    );
}

#[test]
fn dataset_metadata_reports_source_summary_and_short_ref_consistency() {
    let dataset_ref = DatasetRef::parse("c".repeat(64)).expect("dataset ref");
    let metadata = DatasetMetadata {
        short_ref: dataset_ref.short_ref().to_string(),
        dataset_ref: dataset_ref.clone(),
        source_kind: DatasetSourceKind::Local,
        source_path: Some("/tmp/data".to_string()),
        source_repo: None,
        source_revision: None,
        dataset_format: DatasetFormat::Directory,
        file_count: 2,
        total_bytes: 42,
        imported_at: "2026-05-17T00:00:00Z".to_string(),
        package: DatasetPackageMetadata {
            tuning_ready: true,
            splits: DatasetSplits {
                train: Some("train.jsonl".to_string()),
                validation: Some("valid.jsonl".to_string()),
                ..DatasetSplits::default()
            },
            warnings: Vec::new(),
        },
    };

    assert_eq!(metadata.expected_short_ref(), dataset_ref.short_ref());
    assert!(metadata.has_consistent_short_ref());
    assert_eq!(metadata.source_summary(), "/tmp/data");
    assert_eq!(
        metadata.package.splits.split_names(),
        vec!["train", "valid"]
    );
}

#[test]
fn dataset_manifest_sorts_counts_and_sums_files_without_io() {
    let manifest = DatasetManifest {
        files: vec![manifest_entry("b.jsonl", 20), manifest_entry("a.jsonl", 10)],
    }
    .sorted();

    assert_eq!(manifest.files[0].relative_path, "a.jsonl");
    assert_eq!(manifest.file_count(), 2);
    assert_eq!(manifest.total_bytes(), 30);
    assert!(!manifest.is_empty());
}

#[test]
fn dataset_split_kinds_keep_contract_file_names() {
    assert_eq!(DatasetSplitKind::Train.file_name(), "train.jsonl");
    assert_eq!(DatasetSplitKind::Valid.file_name(), "valid.jsonl");
    assert_eq!(DatasetSplitKind::EvalCases.as_str(), "eval_cases");
}

#[test]
fn dataset_synth_counts_reports_expected_jobs() {
    assert_eq!(DatasetSynthCounts::default().expected_jobs(), 1);
    assert_eq!(
        DatasetSynthCounts {
            train_count: Some(20),
            valid_count: Some(5),
            ..DatasetSynthCounts::default()
        }
        .expected_jobs(),
        2
    );
}

#[test]
fn markdown_generation_template_is_rendered_from_dataset_template_folder() {
    let request = DatasetTemplateRequest::new(Some("support".into()), Some("zh-TW".into()));
    let rendered = render_dataset_generation_template(&request);

    assert_eq!(rendered.template_version, "tentgent.dataset.synth.v1");
    assert!(rendered
        .body
        .contains("Canonical schema: `tentgent.chat.v1`"));
    assert!(rendered.body.contains("Task/domain hint: `support`"));
    assert!(rendered.body.contains("Language/content hint: `zh-TW`"));
    assert!(!rendered.body.contains("{{task}}"));
}

fn manifest_entry(relative_path: &str, size_bytes: u64) -> DatasetManifestEntry {
    DatasetManifestEntry {
        relative_path: relative_path.to_string(),
        size_bytes,
        sha256: "0".repeat(64),
    }
}
