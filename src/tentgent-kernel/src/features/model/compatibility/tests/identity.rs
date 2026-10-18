use std::collections::BTreeSet;

use serde_json::json;

use crate::features::adapter::domain::AdapterRef;
use crate::features::model::domain::{
    ModelCapability, ModelCapabilityProofSource as Source, ModelCapabilityProofStatus as Status,
    ModelFormat, ModelRef,
};

use super::super::*;
use super::fixtures::*;

#[test]
fn canonical_identity_golden_vector() {
    let canonical = tuple().canonical_json().unwrap();
    assert_eq!(
        canonical,
        concat!(
            "{\"identity_version\":1,\"components\":{\"model_ref\":\"",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "\",\"capability\":\"chat\",\"primary_format\":\"mlx\",\"quantization\":{\"kind\":\"unquantized\"},",
            "\"backend\":\"mlx\",\"runtime_family\":\"mlx-lm\",\"runtime\":{\"package\":\"mlx-lm\",\"version\":\"0.30.1\"},",
            "\"profile\":{\"kind\":\"no-profile\"},\"platform\":{\"os\":\"macos\",\"architecture\":\"aarch64\"},",
            "\"device_class\":\"metal\",\"adapter\":{\"kind\":\"base\"},\"observation\":{\"kind\":\"load\"}}}"
        )
    );
    assert_eq!(
        tuple().key().unwrap().tuple_sha256(),
        "f4d0b1c26cd3a7d34210796ff12c50d07ff81053a1cb1842a74527addba31121"
    );
}

#[test]
fn canonicalization_is_idempotent_for_json_and_toml() {
    for original in [tuple(), changed(|f| f.observation = execution())] {
        let json = original.canonical_json().unwrap();
        let reread: CompatibilityTuple = serde_json::from_str(&json).unwrap();
        assert_eq!(reread.canonical_json().unwrap(), json);
        let toml = toml::to_string(&original).unwrap();
        let reread: CompatibilityTuple = toml::from_str(&toml).unwrap();
        assert_eq!(reread, original);
        assert_eq!(reread.key().unwrap(), original.key().unwrap());
    }
}

#[test]
fn registered_aliases_and_model_hex_case_share_identity() {
    let mut value = serde_json::to_value(tuple()).unwrap();
    value["components"]["model_ref"] = json!("A".repeat(64));
    value["components"]["backend"] = json!(" MLX ");
    value["components"]["runtime_family"] = json!(" MLX-LM ");
    value["components"]["runtime"]["package"] = json!(" MLX-LM ");
    value["components"]["platform"]["os"] = json!(" Darwin ");
    value["components"]["platform"]["architecture"] = json!(" ARM64 ");
    value["components"]["device_class"] = json!(" MPS ");
    let alias: CompatibilityTuple = serde_json::from_value(value).unwrap();
    assert_eq!(alias, tuple());
    assert_eq!(alias.key().unwrap(), tuple().key().unwrap());
}

#[test]
fn unordered_modalities_are_sorted_and_deduplicated() {
    let original = changed(|f| f.observation = execution());
    let mut first = serde_json::to_value(&original).unwrap();
    first["components"]["observation"]["input"]["modalities"] = json!(["image", "text", "image"]);
    let mut second = first.clone();
    second["components"]["observation"]["input"]["modalities"] = json!(["text", "image"]);
    let first: CompatibilityTuple = serde_json::from_value(first).unwrap();
    let second: CompatibilityTuple = serde_json::from_value(second).unwrap();
    assert_eq!(first.key().unwrap(), second.key().unwrap());
    assert_eq!(
        first.canonical_json().unwrap(),
        second.canonical_json().unwrap()
    );
}

#[test]
fn each_top_level_fact_changes_identity() {
    let changes: Vec<CompatibilityTuple> = vec![
        changed(|f| f.model_ref = ModelRef::parse("b".repeat(64)).unwrap()),
        changed(|f| f.capability = ModelCapability::Embedding),
        changed(|f| f.primary_format = ModelFormat::Safetensors),
        changed(|f| {
            f.quantization = Quantization::Quantized {
                method: QuantizationMethod::Mlx,
                variant: identifier("affine"),
                bits: QuantizationBits::Fixed { bits: 4 },
                group_size: QuantizationGroupSize::Fixed { size: 64 },
            }
        }),
        changed(|f| {
            f.backend = CompatibilityBackend::Transformers;
            f.runtime_family = RuntimeFamily::Transformers;
            f.runtime.package = RuntimePackage::Transformers;
        }),
        changed(|f| {
            f.runtime_family = RuntimeFamily::MlxVlm;
            f.runtime.package = RuntimePackage::MlxVlm;
        }),
        changed(|f| f.runtime.version = version("0.30.2")),
        changed(|f| {
            f.profile = CompatibilityProfile::Selected {
                id: identifier("chat/default"),
                version: 1,
            }
        }),
        changed(|f| f.platform.os = OperatingSystem::Linux),
        changed(|f| f.platform.architecture = Architecture::X86_64),
        changed(|f| f.device_class = DeviceClass::Cpu),
        changed(|f| {
            f.adapter = CompatibilityAdapter::Selected {
                adapter_ref: AdapterRef::parse("c".repeat(64)).unwrap(),
                load_identity: identifier("loader:V1"),
            }
        }),
        changed(|f| f.observation = execution()),
    ];
    let mut hashes = BTreeSet::from([tuple().key().unwrap().tuple_sha256().to_owned()]);
    for changed in changes {
        assert!(hashes.insert(changed.key().unwrap().tuple_sha256().to_owned()));
    }
}

#[test]
fn profile_id_and_version_are_independent_identity_dimensions() {
    let make = |id, version| {
        changed(|f| {
            f.profile = CompatibilityProfile::Selected {
                id: identifier(id),
                version,
            }
        })
    };
    assert_ne!(
        make("profileA", 1).key().unwrap(),
        make("profileB", 1).key().unwrap()
    );
    assert_ne!(
        make("profileA", 1).key().unwrap(),
        make("profileA", 2).key().unwrap()
    );
}

#[test]
fn adapter_ref_and_load_identity_are_independent_identity_dimensions() {
    let make = |reference: char, load| {
        changed(|f| {
            f.adapter = CompatibilityAdapter::Selected {
                adapter_ref: AdapterRef::parse(reference.to_string().repeat(64)).unwrap(),
                load_identity: identifier(load),
            }
        })
    };
    assert_ne!(
        make('b', "loadA").key().unwrap(),
        make('c', "loadA").key().unwrap()
    );
    assert_ne!(
        make('b', "loadA").key().unwrap(),
        make('b', "loadB").key().unwrap()
    );
}

#[test]
fn quantization_method_variant_bits_and_group_size_change_identity() {
    use QuantizationBits::{Fixed as Bits, Mixed};
    use QuantizationGroupSize::{Fixed as Group, NotApplicable, PerChannel};
    let make = |method, variant, bits, group_size| {
        changed(|f| {
            f.quantization = Quantization::Quantized {
                method,
                variant: identifier(variant),
                bits,
                group_size,
            }
        })
    };
    let original = make(
        QuantizationMethod::Mlx,
        "affine",
        Bits { bits: 4 },
        Group { size: 64 },
    )
    .key()
    .unwrap();
    for candidate in [
        make(
            QuantizationMethod::Gptq,
            "affine",
            Bits { bits: 4 },
            Group { size: 64 },
        ),
        make(
            QuantizationMethod::Mlx,
            "symmetric",
            Bits { bits: 4 },
            Group { size: 64 },
        ),
        make(
            QuantizationMethod::Mlx,
            "affine",
            Bits { bits: 8 },
            Group { size: 64 },
        ),
        make(
            QuantizationMethod::Mlx,
            "affine",
            Mixed {},
            Group { size: 64 },
        ),
        make(
            QuantizationMethod::Mlx,
            "affine",
            Bits { bits: 4 },
            Group { size: 128 },
        ),
        make(
            QuantizationMethod::Gptq,
            "affine",
            Bits { bits: 4 },
            PerChannel {},
        ),
        make(
            QuantizationMethod::Gguf,
            "Q4_K_M",
            Mixed {},
            NotApplicable {},
        ),
    ] {
        assert_ne!(candidate.key().unwrap(), original);
    }
}

#[test]
fn input_output_provider_streaming_and_attribute_shapes_change_identity() {
    let base = changed(|f| f.observation = execution());
    let mut variants = Vec::new();
    let mut input_modalities = base.components().clone();
    if let Observation::Execution { input, .. } = &mut input_modalities.observation {
        input.modalities.insert(Modality::Image);
    }
    variants.push(input_modalities);
    let mut output_modalities = base.components().clone();
    if let Observation::Execution { output, .. } = &mut output_modalities.observation {
        output.modalities.insert(Modality::Audio);
    }
    variants.push(output_modalities);
    let mut provider = base.components().clone();
    if let Observation::Execution { input, .. } = &mut provider.observation {
        input.provider = ProviderShape::Openai;
    }
    variants.push(provider);
    let mut streaming = base.components().clone();
    if let Observation::Execution { output, .. } = &mut streaming.observation {
        output.streaming = true;
    }
    variants.push(streaming);
    let mut format = base.components().clone();
    if let Observation::Execution { output, .. } = &mut format.observation {
        output.format = OutputFormat::Json;
    }
    variants.push(format);
    for attribute in ["tool_calls", "structured_output"] {
        let mut value = serde_json::to_value(&base).unwrap();
        value["components"]["observation"]["input"]["attributes"][attribute] = json!(true);
        variants.push(
            serde_json::from_value::<CompatibilityTuple>(value)
                .unwrap()
                .components()
                .clone(),
        );
    }
    let mut hashes = BTreeSet::from([base.key().unwrap().tuple_sha256().to_owned()]);
    for variant in variants {
        assert!(hashes.insert(
            CompatibilityTuple::new(variant)
                .unwrap()
                .key()
                .unwrap()
                .tuple_sha256()
                .to_owned()
        ));
    }
}

#[test]
fn opaque_case_and_old_filename_escape_collisions_remain_distinct() {
    let make = |id| {
        changed(|f| {
            f.profile = CompatibilityProfile::Selected {
                id: identifier(id),
                version: 1,
            }
        })
    };
    for (left, right) in [("a/b", "a_2fb"), ("profileA", "profilea"), ("a:b", "a_3ab")] {
        assert_ne!(make(left).key().unwrap(), make(right).key().unwrap());
    }
    assert!(CompatibilityIdentifier::parse("").is_err());
    assert!(CompatibilityIdentifier::parse("empty").is_ok());
    assert_ne!(
        changed(|f| f.runtime.version = version("1.0+CPU"))
            .key()
            .unwrap(),
        changed(|f| f.runtime.version = version("1.0+cpu"))
            .key()
            .unwrap()
    );
}

#[test]
fn backend_family_and_selected_package_must_agree() {
    let mut wrong_backend = facts();
    wrong_backend.backend = CompatibilityBackend::Transformers;
    assert!(CompatibilityTuple::new(wrong_backend).is_err());
    let mut wrong_package = facts();
    wrong_package.runtime.package = RuntimePackage::Transformers;
    assert!(CompatibilityTuple::new(wrong_package).is_err());
    for family in [
        RuntimeFamily::MlxLm,
        RuntimeFamily::MlxVlm,
        RuntimeFamily::MlxAudio,
        RuntimeFamily::MlxDiffusion,
        RuntimeFamily::Transformers,
        RuntimeFamily::LlamaCpp,
        RuntimeFamily::Diffusers,
    ] {
        let mut input = facts();
        input.backend = family.backend();
        input.runtime_family = family;
        input.runtime.package = family.package();
        assert!(CompatibilityTuple::new(input).is_ok());
    }
}

#[test]
fn selected_profile_and_adapter_require_paired_valid_fields() {
    for partial in [
        json!({"kind":"selected", "id":"profile"}),
        json!({"kind":"selected", "version":1}),
        json!({"kind":"selected", "id":"profile", "version":0}),
    ] {
        let mut value = serde_json::to_value(tuple()).unwrap();
        value["components"]["profile"] = partial;
        assert!(serde_json::from_value::<CompatibilityTuple>(value).is_err());
    }
    for partial in [
        json!({"kind":"selected", "adapter_ref":"b".repeat(64)}),
        json!({"kind":"selected", "load_identity":"load"}),
        json!({"kind":"selected", "adapter_ref":"invalid", "load_identity":"load"}),
        json!({"kind":"base", "adapter_ref":"b".repeat(64)}),
    ] {
        let mut value = serde_json::to_value(tuple()).unwrap();
        value["components"]["adapter"] = partial;
        assert!(serde_json::from_value::<CompatibilityTuple>(value).is_err());
    }
}

#[test]
fn image_workflow_and_embedding_kind_have_typed_identity() {
    for (capability, field, values) in [
        (
            ModelCapability::ImageGeneration,
            "image_workflow",
            ["text-to-image", "image-to-image"],
        ),
        (
            ModelCapability::Embedding,
            "embedding_input",
            ["query", "document"],
        ),
    ] {
        let mut keys = Vec::new();
        for value in values {
            let mut wire = serde_json::to_value(changed(|f| f.observation = execution())).unwrap();
            wire["components"]["capability"] = json!(capability);
            wire["components"]["observation"]["input"]["family"] = json!(capability);
            wire["components"]["observation"]["output"]["family"] = json!(capability);
            wire["components"]["observation"]["input"]["attributes"][field] = json!(value);
            keys.push(
                serde_json::from_value::<CompatibilityTuple>(wire)
                    .unwrap()
                    .key()
                    .unwrap(),
            );
        }
        assert_ne!(keys[0], keys[1]);
    }
}

#[test]
fn load_has_no_shape_and_cannot_authorize_execution() {
    let loaded = tuple();
    let executed = changed(|f| f.observation = execution());
    assert_ne!(loaded.key().unwrap(), executed.key().unwrap());
    assert_eq!(loaded.observation().scope(), ObservationScope::Load);
    let exact = CompatibilityFilter {
        exact_key: Some(executed.key().unwrap()),
        ..Default::default()
    };
    assert!(!exact.matches_tuple(&loaded).unwrap());
    let mut wire = serde_json::to_value(loaded).unwrap();
    wire["components"]["observation"]["input"] = json!({});
    assert!(serde_json::from_value::<CompatibilityTuple>(wire).is_err());
}

#[test]
fn proof_outcome_source_and_time_do_not_change_tuple_identity() {
    let verified = proof();
    let failed = CompatibilityProofV2::new(
        tuple(),
        Status::Failed,
        Source::ServerStart,
        "2026-10-09T00:00:00Z",
        Some(ProofFailureCode::ModelLoadFailed),
    )
    .unwrap();
    assert_eq!(verified.key().unwrap(), failed.key().unwrap());
    let executed = changed(|f| f.observation = execution());
    let smoke = CompatibilityProofV2::new(
        executed.clone(),
        Status::Verified,
        Source::EndpointSmoke,
        "2026-10-08T00:00:00Z",
        None,
    )
    .unwrap();
    let runtime = CompatibilityProofV2::new(
        executed,
        Status::Verified,
        Source::RuntimeExecution,
        "2026-10-09T00:00:00Z",
        None,
    )
    .unwrap();
    assert_eq!(smoke.key().unwrap(), runtime.key().unwrap());
}

#[test]
fn v2_proof_json_and_toml_round_trip_with_safe_failures() {
    for original in [
        proof(),
        CompatibilityProofV2::new(
            tuple(),
            Status::Failed,
            Source::ServerStart,
            "2026-10-08T00:00:00Z",
            Some(ProofFailureCode::ModelLoadFailed),
        )
        .unwrap(),
    ] {
        let serialized = toml::to_string(&original).unwrap();
        assert!(serialized.len() < MAX_PROOF_V2_BYTES);
        assert!(serialized.contains("schema_version = 2"));
        assert!(serialized.contains("record_kind = \"local-proof\""));
        assert_eq!(
            toml::from_str::<CompatibilityProofV2>(&serialized).unwrap(),
            original
        );
        assert_eq!(
            serde_json::from_str::<CompatibilityProofV2>(
                &serde_json::to_string(&original).unwrap()
            )
            .unwrap(),
            original
        );
    }
}

#[test]
fn proof_directory_model_capability_and_filename_must_all_match() {
    let proof = proof();
    let key = proof.key().unwrap();
    assert!(proof.validate_location(&key).is_ok());
    for wrong in [
        CompatibilityProofKey::parse(
            ModelRef::parse("b".repeat(64)).unwrap(),
            key.capability(),
            key.tuple_sha256(),
        )
        .unwrap(),
        CompatibilityProofKey::parse(
            key.model_ref().clone(),
            ModelCapability::Embedding,
            key.tuple_sha256(),
        )
        .unwrap(),
        CompatibilityProofKey::parse(key.model_ref().clone(), key.capability(), "0".repeat(64))
            .unwrap(),
    ] {
        assert_eq!(
            proof.validate_location(&wrong),
            Err(CompatibilityError::LocationMismatch)
        );
    }
    assert_eq!(key.filename(), format!("{}.toml", key.tuple_sha256()));
}
