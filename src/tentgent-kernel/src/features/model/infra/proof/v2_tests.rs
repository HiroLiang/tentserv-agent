//! Public store contract tests. No production helper or test-only export is used.

use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::features::adapter::domain::AdapterRef;
use crate::features::model::{
    compatibility::*,
    domain::{
        default_model_capability_source, ModelCapability, ModelCapabilityProof,
        ModelCapabilityProofSource as Source, ModelCapabilityProofStatus as Status, ModelFormat,
        ModelMetadata, ModelRef, ModelSourceKind, ModelStoreLayout,
    },
    infra::{FileModelCapabilityProofStore, FileModelCatalogStore},
    ports::{ModelCapabilityProofStore, ModelCatalogStore, ModelCompatibilityProofStore},
    proof_context::ModelProofContext,
};
use crate::foundation::layout::{
    LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput, RuntimeLayoutResolver,
    StdRuntimeLayoutResolver,
};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);
const OBSERVED_AT: &str = "2026-10-08T00:00:00Z";

struct Fixture {
    root: PathBuf,
    runtime: RuntimeLayout,
    model: ModelRef,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "tentgent-v2-{label}-{}-{stamp}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        let runtime = StdRuntimeLayoutResolver
            .resolve(RuntimeLayoutInput {
                mode: LayoutResolveMode::Create,
                home_dir: Some(root.clone()),
                data_root_dir: Some(root.clone()),
            })
            .unwrap();
        let fixture = Self {
            root,
            runtime,
            model: reference('a'),
        };
        fixture.add_model(&fixture.model);
        fixture
    }
    fn context(&self) -> ModelProofContext<'_> {
        ModelProofContext::new(&self.runtime)
    }
    fn layout(&self) -> ModelStoreLayout {
        ModelStoreLayout::from_models_dir(self.runtime.models_dir.clone())
    }
    fn add_model(&self, model_ref: &ModelRef) {
        FileModelCatalogStore
            .save_model_metadata(
                &self.layout(),
                &ModelMetadata {
                    model_ref: model_ref.clone(),
                    short_ref: model_ref.short_ref().to_owned(),
                    source_kind: ModelSourceKind::Local,
                    source_repo: None,
                    source_revision: None,
                    source_path: None,
                    primary_format: ModelFormat::Safetensors,
                    detected_formats: vec![ModelFormat::Safetensors],
                    mlx_runtime_family: None,
                    model_capabilities: vec![ModelCapability::Chat, ModelCapability::Embedding],
                    model_capability_source: default_model_capability_source(),
                    file_count: 1,
                    total_bytes: 7,
                    imported_at: OBSERVED_AT.into(),
                },
            )
            .unwrap();
    }
    fn save(&self, proof: &CompatibilityProofV2) -> CompatibilityProofKey {
        FileModelCapabilityProofStore
            .save_exact(&self.context(), proof)
            .unwrap()
    }
    fn list(&self, filter: &CompatibilityFilter) -> Vec<CompatibilityProofV2> {
        FileModelCapabilityProofStore
            .list_exact(&self.context(), &self.model, filter)
            .unwrap()
    }
    fn raw_v2(&self, key: &CompatibilityProofKey, body: &str) {
        let path = self.layout().compatibility_proof_path(key);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }
    fn assert_invalid_mutations_preserve(&self, replacement: &CompatibilityProofV2, body: &str) {
        let key = replacement.key().unwrap();
        let path = self.layout().compatibility_proof_path(&key);
        let save_error = FileModelCapabilityProofStore
            .save_exact(&self.context(), replacement)
            .expect_err("save must not repair corrupt existing evidence")
            .to_string();
        assert!(!save_error.contains("synthetic-secret"));
        assert_eq!(fs::read(&path).unwrap(), body.as_bytes());
        let remove_error = FileModelCapabilityProofStore
            .remove_exact(&self.context(), &key)
            .expect_err("remove must not discard corrupt existing evidence")
            .to_string();
        assert!(!remove_error.contains("synthetic-secret"));
        assert_eq!(fs::read(&path).unwrap(), body.as_bytes());
        assert!(FileModelCapabilityProofStore
            .remove_capability_proof(&self.context(), key.model_ref(), key.capability())
            .is_err());
        assert_eq!(fs::read(&path).unwrap(), body.as_bytes());
    }
    fn old_evidence(&self) -> Vec<ModelCapabilityProof> {
        let support = legacy(&self.model, "4.40.0");
        FileModelCapabilityProofStore
            .save_capability_proof(&self.context(), &support)
            .unwrap();
        // Simulate a latest-only record imported from a pre-tuple-aware store.
        let latest = legacy(&self.model, "4.39.0");
        let path = self
            .layout()
            .capability_proof_path(&self.model, ModelCapability::Chat);
        fs::write(path, toml::to_string(&latest).unwrap()).unwrap();
        vec![support, latest]
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn reference(letter: char) -> ModelRef {
    ModelRef::parse(letter.to_string().repeat(64)).unwrap()
}
fn identifier(value: &str) -> CompatibilityIdentifier {
    CompatibilityIdentifier::parse(value).unwrap()
}
fn version(value: &str) -> RuntimeVersion {
    RuntimeVersion::parse(value).unwrap()
}

fn facts() -> CompatibilityTupleInput {
    CompatibilityTupleInput {
        model_ref: reference('a'),
        capability: ModelCapability::Chat,
        primary_format: ModelFormat::Safetensors,
        quantization: Quantization::Unquantized,
        backend: CompatibilityBackend::Transformers,
        runtime_family: RuntimeFamily::Transformers,
        runtime: RuntimeIdentity {
            package: RuntimePackage::Transformers,
            version: version("4.50.0"),
        },
        profile: CompatibilityProfile::NoProfile,
        platform: CompatibilityPlatform {
            os: OperatingSystem::Linux,
            architecture: Architecture::X86_64,
        },
        device_class: DeviceClass::Cpu,
        adapter: CompatibilityAdapter::Base,
        observation: Observation::Execution {
            input: InputShape {
                family: ModelCapability::Chat,
                modalities: BTreeSet::from([Modality::Text]),
                provider: ProviderShape::Native,
                attributes: ShapeAttributes::default(),
            },
            output: OutputShape {
                family: ModelCapability::Chat,
                modalities: BTreeSet::from([Modality::Text]),
                streaming: false,
                format: OutputFormat::Text,
            },
        },
    }
}

fn proof(input: CompatibilityTupleInput) -> CompatibilityProofV2 {
    let source = match input.observation {
        Observation::Load => Source::ServerStart,
        Observation::Execution { .. } => Source::RuntimeExecution,
    };
    CompatibilityProofV2::new(
        CompatibilityTuple::new(input).unwrap(),
        Status::Verified,
        source,
        OBSERVED_AT,
        None,
    )
    .unwrap()
}

fn changed(change: impl FnOnce(&mut CompatibilityTupleInput)) -> CompatibilityProofV2 {
    let mut input = facts();
    change(&mut input);
    proof(input)
}

fn legacy(model: &ModelRef, runtime_version: &str) -> ModelCapabilityProof {
    ModelCapabilityProof {
        model_ref: model.clone(),
        capability: ModelCapability::Chat,
        status: Status::Verified,
        source: Source::ManualProbe,
        primary_format: ModelFormat::Safetensors,
        mlx_runtime_family: None,
        backend: "safetensors".into(),
        runtime_version: Some(runtime_version.into()),
        runtime_profile: None,
        runtime_profile_version: None,
        server_ref: None,
        checked_at: OBSERVED_AT.into(),
        error: None,
    }
}

#[test]
fn v2_store_three_generations_coexist_without_legacy_projection() {
    let fixture = Fixture::new("generations");
    let old = fixture.old_evidence();
    let current = proof(facts());
    let key = fixture.save(&current);
    assert_eq!(
        FileModelCapabilityProofStore
            .get_exact(&fixture.context(), &key)
            .unwrap(),
        Some(current)
    );
    let old_visible = FileModelCapabilityProofStore
        .list_capability_proofs(&fixture.context(), &fixture.model)
        .unwrap();
    assert_eq!(old_visible.len(), 2);
    assert!(old.iter().all(|expected| old_visible.contains(expected)));
    let evidence = FileModelCapabilityProofStore
        .list_evidence(&fixture.context(), &fixture.model, None)
        .unwrap();
    assert_eq!(evidence.len(), 3);
    for generation in [
        EvidenceGeneration::V2,
        EvidenceGeneration::TupleAwareV1,
        EvidenceGeneration::LegacyLatest,
    ] {
        assert_eq!(
            evidence
                .iter()
                .filter(|entry| entry.generation() == generation)
                .count(),
            1
        );
    }
    assert!(evidence
        .iter()
        .filter(|entry| entry.v2().is_none())
        .all(|entry| !entry.missing_dimensions().is_empty()));
}

#[test]
fn v2_store_alone_is_invisible_to_legacy_gate_readers() {
    let fixture = Fixture::new("legacy-gate");
    fixture.save(&proof(facts()));
    assert!(FileModelCapabilityProofStore
        .list_capability_proofs(&fixture.context(), &fixture.model)
        .unwrap()
        .is_empty());
    assert!(FileModelCapabilityProofStore
        .list_capability_proofs_for(&fixture.context(), &fixture.model, ModelCapability::Chat)
        .unwrap()
        .is_empty());
    assert!(FileModelCapabilityProofStore
        .list_support_proofs(&fixture.context(), &fixture.model)
        .unwrap()
        .is_empty());
    assert!(!fixture
        .layout()
        .capability_proof_path(&fixture.model, ModelCapability::Chat)
        .exists());
    assert!(!fixture
        .layout()
        .support_proofs_capability_dir(&fixture.model, ModelCapability::Chat)
        .exists());
}

#[test]
fn v2_store_replaces_same_key_without_timestamp_ordering_or_other_tuple_loss() {
    let fixture = Fixture::new("replace");
    let first = proof(facts());
    let key = fixture.save(&first);
    let other = changed(|input| input.runtime.version = version("4.51.0"));
    let other_key = fixture.save(&other);
    let failed = CompatibilityProofV2::new(
        first.tuple().clone(),
        Status::Failed,
        Source::RuntimeExecution,
        "2026-01-01T00:00:00Z",
        Some(ProofFailureCode::RuntimeExecutionFailed),
    )
    .unwrap();
    assert_eq!(fixture.save(&failed), key);
    assert_eq!(
        FileModelCapabilityProofStore
            .get_exact(&fixture.context(), &key)
            .unwrap(),
        Some(failed)
    );
    assert_eq!(
        FileModelCapabilityProofStore
            .get_exact(&fixture.context(), &other_key)
            .unwrap(),
        Some(other)
    );
    assert_eq!(fixture.list(&CompatibilityFilter::default()).len(), 2);
}

#[test]
fn v2_store_exact_remove_does_not_resurrect_older_evidence() {
    let fixture = Fixture::new("exact-remove");
    fixture.old_evidence();
    let key = fixture.save(&proof(facts()));
    assert!(FileModelCapabilityProofStore
        .remove_exact(&fixture.context(), &key)
        .unwrap());
    assert!(!FileModelCapabilityProofStore
        .remove_exact(&fixture.context(), &key)
        .unwrap());
    assert!(FileModelCapabilityProofStore
        .get_exact(&fixture.context(), &key)
        .unwrap()
        .is_none());
    assert!(fixture
        .list(&CompatibilityFilter {
            exact_key: Some(key),
            ..Default::default()
        })
        .is_empty());
    let remaining = FileModelCapabilityProofStore
        .list_evidence(
            &fixture.context(),
            &fixture.model,
            Some(ModelCapability::Chat),
        )
        .unwrap();
    assert_eq!(remaining.len(), 2);
    assert!(remaining
        .iter()
        .all(|entry| entry.v2().is_none() && entry.key().unwrap().is_none()));
}

#[test]
fn v2_store_capability_clear_counts_logical_records_and_preserves_other_assets() {
    let fixture = Fixture::new("bulk-clear");
    fixture.old_evidence();
    fixture.save(&proof(facts()));
    fixture.save(&changed(|input| input.runtime.version = version("4.51.0")));
    let embedding = changed(|input| {
        input.capability = ModelCapability::Embedding;
        input.observation = Observation::Load;
    });
    let embedding_key = fixture.save(&embedding);
    let other_model = reference('b');
    fixture.add_model(&other_model);
    let other_key = fixture.save(&changed(|input| input.model_ref = other_model.clone()));
    let asset = fixture
        .layout()
        .model_dir(&fixture.model)
        .join("retained-model-asset.fixture");
    fs::write(&asset, b"weights").unwrap();
    assert_eq!(
        FileModelCapabilityProofStore
            .remove_capability_proof(&fixture.context(), &fixture.model, ModelCapability::Chat)
            .unwrap(),
        4
    );
    assert_eq!(
        FileModelCapabilityProofStore
            .remove_capability_proof(&fixture.context(), &fixture.model, ModelCapability::Chat)
            .unwrap(),
        0
    );
    assert!(FileModelCapabilityProofStore
        .list_evidence(
            &fixture.context(),
            &fixture.model,
            Some(ModelCapability::Chat)
        )
        .unwrap()
        .is_empty());
    assert_eq!(
        FileModelCapabilityProofStore
            .get_exact(&fixture.context(), &embedding_key)
            .unwrap(),
        Some(embedding)
    );
    assert!(FileModelCapabilityProofStore
        .get_exact(&fixture.context(), &other_key)
        .unwrap()
        .is_some());
    assert_eq!(fs::read(asset).unwrap(), b"weights");
    assert!(fixture
        .layout()
        .model_metadata_path(&fixture.model)
        .exists());
}

#[test]
fn v2_store_bulk_clear_deduplicates_a_legacy_mirror() {
    let fixture = Fixture::new("mirror-count");
    FileModelCapabilityProofStore
        .save_capability_proof(&fixture.context(), &legacy(&fixture.model, "4.40.0"))
        .unwrap();
    fixture.save(&proof(facts()));
    assert_eq!(
        FileModelCapabilityProofStore
            .remove_capability_proof(&fixture.context(), &fixture.model, ModelCapability::Chat)
            .unwrap(),
        2
    );
}

#[test]
fn v2_store_filters_each_core_dimension_and_intersects_exact_key() {
    let fixture = Fixture::new("filters");
    let record = proof(facts());
    let key = fixture.save(&record);
    let facts = record.tuple().components();
    let Observation::Execution { input, output } = &facts.observation else {
        unreachable!()
    };
    let mut other_input = input.clone();
    other_input.provider = ProviderShape::Openai;
    let mut other_output = output.clone();
    other_output.streaming = true;
    let quantized: Quantization = serde_json::from_value(serde_json::json!({"kind":"quantized", "method":"gptq", "variant":"q4", "bits":{"kind":"fixed", "bits":4}, "group_size":{"kind":"fixed", "size":128}})).unwrap();
    let mut cases: Vec<(&str, CompatibilityFilter, CompatibilityFilter)> = Vec::new();
    macro_rules! dimension {
        ($field:ident, $yes:expr, $no:expr) => {
            cases.push((
                stringify!($field),
                CompatibilityFilter {
                    $field: Some($yes),
                    ..Default::default()
                },
                CompatibilityFilter {
                    $field: Some($no),
                    ..Default::default()
                },
            ));
        };
    }
    dimension!(model_ref, fixture.model.clone(), reference('b'));
    dimension!(
        capability,
        ModelCapability::Chat,
        ModelCapability::Embedding
    );
    dimension!(primary_format, ModelFormat::Safetensors, ModelFormat::Gguf);
    dimension!(quantization, Quantization::Unquantized, quantized);
    dimension!(
        backend,
        CompatibilityBackend::Transformers,
        CompatibilityBackend::Mlx
    );
    dimension!(
        runtime_family,
        RuntimeFamily::Transformers,
        RuntimeFamily::MlxLm
    );
    dimension!(
        runtime_package,
        RuntimePackage::Transformers,
        RuntimePackage::MlxLm
    );
    dimension!(runtime_version, version("4.50.0"), version("4.51.0"));
    dimension!(
        profile,
        CompatibilityProfile::NoProfile,
        CompatibilityProfile::Selected {
            id: identifier("chat/default"),
            version: 1
        }
    );
    dimension!(
        platform,
        facts.platform,
        CompatibilityPlatform {
            os: OperatingSystem::Macos,
            architecture: Architecture::Aarch64
        }
    );
    dimension!(device_class, DeviceClass::Cpu, DeviceClass::Cuda);
    dimension!(
        adapter,
        CompatibilityAdapter::Base,
        CompatibilityAdapter::Selected {
            adapter_ref: AdapterRef::parse("c".repeat(64)).unwrap(),
            load_identity: identifier("load:V1")
        }
    );
    dimension!(
        observation_scope,
        ObservationScope::Execution,
        ObservationScope::Load
    );
    dimension!(input_shape, input.clone(), other_input);
    dimension!(output_shape, output.clone(), other_output);
    dimension!(
        exact_key,
        key.clone(),
        changed(|input| input.runtime.version = version("9.0.0"))
            .key()
            .unwrap()
    );
    for (label, positive, mut negative) in cases {
        assert_eq!(
            fixture.list(&positive),
            vec![record.clone()],
            "positive {label}"
        );
        assert!(fixture.list(&negative).is_empty(), "negative {label}");
        if label != "exact_key" {
            negative.exact_key = Some(key.clone());
            assert!(
                fixture.list(&negative).is_empty(),
                "exact-key intersection {label}"
            );
        }
    }
}

#[test]
fn v2_store_roundtrips_selected_profile_adapter_quantization_and_execution_shape() {
    let fixture = Fixture::new("complete-fields");
    fixture.save(&proof(facts()));
    let mut complete = facts();
    complete.quantization = Quantization::Quantized {
        method: QuantizationMethod::Gptq,
        variant: identifier("Q4-CaseSensitive"),
        bits: QuantizationBits::Fixed { bits: 4 },
        group_size: QuantizationGroupSize::Fixed { size: 128 },
    };
    complete.profile = CompatibilityProfile::Selected {
        id: identifier("chat/ProfileA"),
        version: 2,
    };
    complete.adapter = CompatibilityAdapter::Selected {
        adapter_ref: AdapterRef::parse("c".repeat(64)).unwrap(),
        load_identity: identifier("sha256:SelectedWeightA"),
    };
    let Observation::Execution { input, output } = &mut complete.observation else {
        unreachable!()
    };
    input.modalities.insert(Modality::File);
    input.provider = ProviderShape::Openai;
    input.attributes.tool_calls = true;
    input.attributes.structured_output = true;
    output.streaming = true;
    output.format = OutputFormat::Json;
    let input_filter = input.clone();
    let output_filter = output.clone();
    let record = proof(complete.clone());
    let key = fixture.save(&record);
    let selected = CompatibilityFilter {
        exact_key: Some(key.clone()),
        quantization: Some(complete.quantization),
        profile: Some(complete.profile),
        adapter: Some(complete.adapter),
        input_shape: Some(input_filter),
        output_shape: Some(output_filter),
        ..Default::default()
    };
    assert_eq!(fixture.list(&selected), vec![record.clone()]);
    assert_eq!(
        FileModelCapabilityProofStore
            .get_exact(&fixture.context(), &key)
            .unwrap(),
        Some(record)
    );
    assert_eq!(fixture.list(&CompatibilityFilter::default()).len(), 2);
}

#[test]
fn v2_store_stable_order_is_capability_then_key_not_write_or_timestamp_order() {
    let fixture = Fixture::new("order");
    let records = vec![
        changed(|input| input.runtime.version = version("9.0.0")),
        changed(|input| {
            input.capability = ModelCapability::Embedding;
            input.observation = Observation::Load;
        }),
        proof(facts()),
    ];
    for record in &records {
        fixture.save(record);
    }
    let mut expected = records;
    expected.sort_by_key(|record| {
        (
            record.tuple().capability().as_str(),
            record.key().unwrap().tuple_sha256().to_owned(),
        )
    });
    for _ in 0..3 {
        assert_eq!(fixture.list(&CompatibilityFilter::default()), expected);
    }
    for record in expected.iter().rev() {
        fixture.save(record);
    }
    assert_eq!(fixture.list(&CompatibilityFilter::default()), expected);
}

#[test]
fn v2_store_rejects_wrong_key_model_and_capability_bodies() {
    let fixture = Fixture::new("mismatch");
    let key = proof(facts()).key().unwrap();
    for record in [
        changed(|input| input.runtime.version = version("9.0.0")),
        changed(|input| input.model_ref = reference('b')),
        changed(|input| {
            input.capability = ModelCapability::Embedding;
            input.observation = Observation::Load;
        }),
    ] {
        let body = toml::to_string(&record).unwrap();
        fixture.raw_v2(&key, &body);
        assert!(FileModelCapabilityProofStore
            .get_exact(&fixture.context(), &key)
            .is_err());
        assert!(FileModelCapabilityProofStore
            .list_exact(
                &fixture.context(),
                &fixture.model,
                &CompatibilityFilter::default()
            )
            .is_err());
        assert_eq!(
            fs::read_to_string(fixture.layout().compatibility_proof_path(&key)).unwrap(),
            body
        );
        fixture.assert_invalid_mutations_preserve(&proof(facts()), &body);
    }
}

#[test]
fn v2_store_rejects_unknown_schema_identity_fields_and_malformed_toml() {
    let fixture = Fixture::new("invalid");
    let record = proof(facts());
    let key = record.key().unwrap();
    let valid = toml::to_string(&record).unwrap();
    for body in [
        valid.replace("schema_version = 2", "schema_version = 999"),
        valid.replace("identity_version = 1", "identity_version = 999"),
        format!("unrecognized = 'synthetic-secret'\n{valid}"),
        "error = 'synthetic-secret\n".into(),
    ] {
        assert_ne!(body, valid, "invalid fixture must differ");
        fixture.raw_v2(&key, &body);
        let error = FileModelCapabilityProofStore
            .get_exact(&fixture.context(), &key)
            .expect_err("reject invalid v2")
            .to_string();
        assert!(!error.contains("synthetic-secret"));
        assert!(FileModelCapabilityProofStore
            .list_exact(
                &fixture.context(),
                &fixture.model,
                &CompatibilityFilter::default()
            )
            .is_err());
        fixture.assert_invalid_mutations_preserve(&record, &body);
    }
}

#[test]
fn v2_store_rejects_record_above_16_kib_before_toml_acceptance() {
    let fixture = Fixture::new("bounded");
    let record = proof(facts());
    let key = record.key().unwrap();
    let oversized = format!(
        "{}\n#{}\n",
        toml::to_string(&record).unwrap(),
        "x".repeat(MAX_PROOF_V2_BYTES)
    );
    assert!(oversized.len() > MAX_PROOF_V2_BYTES);
    assert!(
        toml::from_str::<CompatibilityProofV2>(&oversized).is_ok(),
        "valid TOML must be rejected by store size boundary"
    );
    fixture.raw_v2(&key, &oversized);
    let error = FileModelCapabilityProofStore
        .get_exact(&fixture.context(), &key)
        .expect_err("oversized record")
        .to_string();
    assert!(error.contains("16 KiB"), "{error}");
    assert!(FileModelCapabilityProofStore
        .list_exact(
            &fixture.context(),
            &fixture.model,
            &CompatibilityFilter::default()
        )
        .is_err());
    fixture.assert_invalid_mutations_preserve(&record, &oversized);
}

#[test]
fn v2_store_unknown_capability_only_blocks_model_wide_v2_snapshots() {
    let fixture = Fixture::new("unknown-capability");
    let old = fixture.old_evidence();
    let record = proof(facts());
    let key = fixture.save(&record);
    let unknown = fixture
        .layout()
        .compatibility_proofs_dir(&fixture.model)
        .join("future-capability")
        .join("future.toml");
    fs::create_dir_all(unknown.parent().unwrap()).unwrap();
    let unknown_body = b"schema_version = 999\nsynthetic-secret = true\n";
    fs::write(&unknown, unknown_body).unwrap();

    for error in [
        FileModelCapabilityProofStore
            .list_exact(
                &fixture.context(),
                &fixture.model,
                &CompatibilityFilter::default(),
            )
            .unwrap_err(),
        FileModelCapabilityProofStore
            .list_evidence(&fixture.context(), &fixture.model, None)
            .unwrap_err(),
    ] {
        let message = error.to_string();
        assert!(message.contains("unsupported compatibility proof capability directory"));
        assert!(!message.contains("synthetic-secret"));
    }
    let legacy = FileModelCapabilityProofStore
        .list_capability_proofs(&fixture.context(), &fixture.model)
        .unwrap();
    assert_eq!(legacy.len(), old.len());
    assert!(old.iter().all(|proof| legacy.contains(proof)));
    assert_eq!(
        fixture.list(&CompatibilityFilter {
            capability: Some(ModelCapability::Chat),
            ..Default::default()
        }),
        vec![record.clone()]
    );
    assert_eq!(
        fixture.list(&CompatibilityFilter {
            exact_key: Some(key.clone()),
            ..Default::default()
        }),
        vec![record.clone()]
    );
    assert_eq!(
        FileModelCapabilityProofStore
            .list_evidence(
                &fixture.context(),
                &fixture.model,
                Some(ModelCapability::Chat)
            )
            .unwrap()
            .len(),
        3
    );

    // Exact mutations and the existing capability-only clear do not inspect,
    // modify, or claim to clear a future capability's namespace.
    fixture.save(&record);
    assert!(FileModelCapabilityProofStore
        .remove_exact(&fixture.context(), &key)
        .unwrap());
    fixture.save(&record);
    assert_eq!(
        FileModelCapabilityProofStore
            .remove_capability_proof(&fixture.context(), &fixture.model, ModelCapability::Chat)
            .unwrap(),
        3
    );
    assert_eq!(fs::read(&unknown).unwrap(), unknown_body);
    assert!(fixture
        .list(&CompatibilityFilter {
            capability: Some(ModelCapability::Chat),
            ..Default::default()
        })
        .is_empty());
}

#[test]
fn v2_store_invalid_known_record_does_not_enter_legacy_gate_readers() {
    let fixture = Fixture::new("invalid-legacy-isolation");
    let old = fixture.old_evidence();
    fixture.raw_v2(&proof(facts()).key().unwrap(), "schema_version = 999\n");
    for legacy in [
        FileModelCapabilityProofStore
            .list_capability_proofs(&fixture.context(), &fixture.model)
            .unwrap(),
        FileModelCapabilityProofStore
            .list_capability_proofs_for(&fixture.context(), &fixture.model, ModelCapability::Chat)
            .unwrap(),
    ] {
        assert_eq!(legacy.len(), old.len());
        assert!(old.iter().all(|proof| legacy.contains(proof)));
    }
    assert_eq!(
        FileModelCapabilityProofStore
            .list_support_proofs(&fixture.context(), &fixture.model)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn v2_store_ignores_temporary_files_but_rejects_invalid_final_filenames() {
    let fixture = Fixture::new("temporary");
    let record = proof(facts());
    fixture.save(&record);
    let directory = fixture
        .layout()
        .compatibility_proofs_capability_dir(&fixture.model, ModelCapability::Chat);
    for name in [".proof.toml.123.tmp", "abandoned.tmp", "readme.txt"] {
        fs::write(directory.join(name), "not TOML").unwrap();
    }
    assert_eq!(fixture.list(&CompatibilityFilter::default()), vec![record]);
    fs::write(directory.join("not-a-canonical-key.toml"), "not TOML").unwrap();
    assert!(FileModelCapabilityProofStore
        .list_exact(
            &fixture.context(),
            &fixture.model,
            &CompatibilityFilter::default()
        )
        .is_err());
}

#[test]
fn v2_store_simulated_old_binary_clear_leaves_v2_visible_after_reupgrade() {
    let fixture = Fixture::new("downgrade");
    fixture.old_evidence();
    let record = proof(facts());
    let key = fixture.save(&record);
    // Simulation of the old clear's path scope, not a claim that an old binary ran.
    fs::remove_file(
        fixture
            .layout()
            .capability_proof_path(&fixture.model, ModelCapability::Chat),
    )
    .unwrap();
    fs::remove_dir_all(
        fixture
            .layout()
            .support_proofs_capability_dir(&fixture.model, ModelCapability::Chat),
    )
    .unwrap();
    assert!(FileModelCapabilityProofStore
        .list_capability_proofs(&fixture.context(), &fixture.model)
        .unwrap()
        .is_empty());
    assert_eq!(
        FileModelCapabilityProofStore
            .get_exact(&fixture.context(), &key)
            .unwrap(),
        Some(record)
    );
    assert_eq!(
        FileModelCapabilityProofStore
            .list_evidence(&fixture.context(), &fixture.model, None)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn v2_store_missing_exact_read_and_remove_are_noncreating() {
    let fixture = Fixture::new("missing");
    let key = proof(facts()).key().unwrap();
    assert!(FileModelCapabilityProofStore
        .get_exact(&fixture.context(), &key)
        .unwrap()
        .is_none());
    assert!(!FileModelCapabilityProofStore
        .remove_exact(&fixture.context(), &key)
        .unwrap());
    assert!(!fixture
        .layout()
        .compatibility_proofs_dir(&fixture.model)
        .exists());
}

#[test]
fn v2_contract_toml_example_deserializes_and_matches_constructed_key() {
    let contract = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/contracts/compatibility-tuple-v2.md"
    ));
    let expected = changed(|input| input.observation = Observation::Load);
    let lf = contract.replace("\r\n", "\n");
    for checkout in [lf.clone(), lf.replace('\n', "\r\n")] {
        let example: CompatibilityProofV2 =
            toml::from_str(&document_toml(&checkout)).expect("document proof schema");
        assert_eq!(example, expected);
        assert_eq!(example.key().unwrap(), expected.key().unwrap());
    }
    let example: CompatibilityProofV2 =
        toml::from_str(&document_toml(contract)).expect("native checkout proof schema");
    assert_eq!(example, expected);
    assert_eq!(example.key().unwrap(), expected.key().unwrap());
    let fixture = Fixture::new("contract");
    let key = fixture.save(&example);
    assert_eq!(
        FileModelCapabilityProofStore
            .get_exact(&fixture.context(), &key)
            .unwrap(),
        Some(expected)
    );
}

fn document_toml(contract: &str) -> String {
    // str::lines accepts both LF and CRLF without changing the TOML content.
    let mut lines = contract.lines().skip_while(|line| *line != "```toml");
    assert_eq!(lines.next(), Some("```toml"), "document TOML fence");
    let mut body = Vec::new();
    for line in lines {
        if line == "```" {
            return body.join("\n");
        }
        body.push(line);
    }
    panic!("closing fence");
}
