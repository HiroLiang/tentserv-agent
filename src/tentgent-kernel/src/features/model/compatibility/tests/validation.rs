use serde_json::{json, Value};

use crate::features::model::domain::{
    ModelCapability, ModelCapabilityProofSource as Source, ModelCapabilityProofStatus as Status,
};

use super::super::*;
use super::fixtures::*;

#[test]
fn opaque_identifier_bounds_unicode_and_placeholders_are_rejected() {
    for invalid in [
        "",
        "unknown",
        "latest",
        "n/a",
        "not-applicable",
        " HASSPACE",
        "hello\nworld",
        "模型",
        "token=secret",
    ] {
        assert!(
            CompatibilityIdentifier::parse(invalid).is_err(),
            "{invalid}"
        );
    }
    assert!(CompatibilityIdentifier::parse("a".repeat(128)).is_ok());
    assert!(CompatibilityIdentifier::parse("a".repeat(129)).is_err());
}

#[test]
fn runtime_version_cannot_hide_unknown_or_contain_payloads() {
    for invalid in [
        "",
        "latest",
        "unknown",
        "n/a",
        "1.2 secret",
        "1.2/secret",
        "1.2\n",
        "一1",
        " 1.2 ",
    ] {
        assert!(RuntimeVersion::parse(invalid).is_err(), "{invalid}");
    }
    for valid in ["1.2.0", "0.0.0.dev1", "1!2.0+CPU", "1.2-rc.1"] {
        assert!(RuntimeVersion::parse(valid).is_ok());
    }
    assert!(RuntimeVersion::parse("1".repeat(129)).is_err());
}

#[test]
fn unknown_fields_are_rejected_at_every_nested_boundary() {
    let original = changed(|f| f.observation = execution());
    let paths: &[&[&str]] = &[
        &[],
        &["components"],
        &["components", "runtime"],
        &["components", "profile"],
        &["components", "platform"],
        &["components", "adapter"],
        &["components", "quantization"],
        &["components", "observation"],
        &["components", "observation", "input"],
        &["components", "observation", "input", "attributes"],
        &["components", "observation", "output"],
    ];
    for path in paths {
        let mut value = serde_json::to_value(&original).unwrap();
        let mut target = &mut value;
        for field in *path {
            target = &mut target[*field];
        }
        target["prompt"] = json!("synthetic-secret");
        assert!(
            serde_json::from_value::<CompatibilityTuple>(value).is_err(),
            "path {path:?}"
        );
    }
}

#[test]
fn all_tuple_fields_are_required_and_null_is_not_absence() {
    for field in [
        "model_ref",
        "capability",
        "primary_format",
        "quantization",
        "backend",
        "runtime_family",
        "runtime",
        "profile",
        "platform",
        "device_class",
        "adapter",
        "observation",
    ] {
        let mut missing = serde_json::to_value(tuple()).unwrap();
        missing["components"].as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<CompatibilityTuple>(missing).is_err(),
            "missing {field}"
        );
        let mut null = serde_json::to_value(tuple()).unwrap();
        null["components"][field] = Value::Null;
        assert!(
            serde_json::from_value::<CompatibilityTuple>(null).is_err(),
            "null {field}"
        );
    }
}

#[test]
fn unknown_identity_versions_and_registered_labels_fail_closed() {
    for invalid in [0, 2, 65535] {
        let mut value = serde_json::to_value(tuple()).unwrap();
        value["identity_version"] = json!(invalid);
        assert!(serde_json::from_value::<CompatibilityTuple>(value).is_err());
    }
    for field in ["backend", "runtime_family", "device_class"] {
        let mut value = serde_json::to_value(tuple()).unwrap();
        value["components"][field] = json!("invented");
        assert!(serde_json::from_value::<CompatibilityTuple>(value).is_err());
    }
}

#[test]
fn quantization_rejects_invalid_sizes_and_missing_variant() {
    for (bits, group_size) in [
        (
            QuantizationBits::Fixed { bits: 0 },
            QuantizationGroupSize::Fixed { size: 64 },
        ),
        (
            QuantizationBits::Fixed { bits: 65 },
            QuantizationGroupSize::Fixed { size: 64 },
        ),
        (
            QuantizationBits::Fixed { bits: 4 },
            QuantizationGroupSize::Fixed { size: 0 },
        ),
        (
            QuantizationBits::Fixed { bits: 4 },
            QuantizationGroupSize::NotApplicable {},
        ),
        (
            QuantizationBits::Fixed { bits: 4 },
            QuantizationGroupSize::PerChannel {},
        ),
    ] {
        let mut input = facts();
        input.quantization = Quantization::Quantized {
            method: QuantizationMethod::Mlx,
            variant: identifier("affine"),
            bits,
            group_size,
        };
        assert!(CompatibilityTuple::new(input).is_err());
    }
    let mut value = serde_json::to_value(tuple()).unwrap();
    value["components"]["quantization"] = json!({"kind":"quantized", "method":"mlx", "bits":4});
    assert!(serde_json::from_value::<CompatibilityTuple>(value).is_err());
}

#[test]
fn quantization_dimensions_cannot_be_missing_null_or_disguised_unknown() {
    let original = changed(|f| {
        f.quantization = Quantization::Quantized {
            method: QuantizationMethod::Mlx,
            variant: identifier("affine"),
            bits: QuantizationBits::Fixed { bits: 4 },
            group_size: QuantizationGroupSize::Fixed { size: 64 },
        }
    });
    for field in ["bits", "group_size"] {
        for value in [
            None,
            Some(Value::Null),
            Some(json!({"kind":"unknown"})),
            Some(json!({"kind":"mixed", "secret":"payload"})),
        ] {
            let mut wire = serde_json::to_value(&original).unwrap();
            let quantization = wire["components"]["quantization"].as_object_mut().unwrap();
            if let Some(value) = value {
                quantization.insert(field.to_owned(), value);
            } else {
                quantization.remove(field);
            }
            assert!(serde_json::from_value::<CompatibilityTuple>(wire).is_err());
        }
    }
}

#[test]
fn invalid_shapes_and_unrelated_attributes_are_rejected() {
    let base = changed(|f| f.observation = execution());
    for side in ["input", "output"] {
        let mut value = serde_json::to_value(&base).unwrap();
        value["components"]["observation"][side]["family"] = json!("embedding");
        assert!(serde_json::from_value::<CompatibilityTuple>(value).is_err());
        let mut value = serde_json::to_value(&base).unwrap();
        value["components"]["observation"][side]["modalities"] = json!([]);
        assert!(serde_json::from_value::<CompatibilityTuple>(value).is_err());
    }
    for (attribute, invalid) in [
        ("image_workflow", "text-to-image"),
        ("embedding_input", "query"),
    ] {
        let mut value = serde_json::to_value(&base).unwrap();
        value["components"]["observation"]["input"]["attributes"][attribute] = json!(invalid);
        assert!(serde_json::from_value::<CompatibilityTuple>(value).is_err());
    }
}

#[test]
fn source_restrictions_prevent_manual_and_preload_inference_proof() {
    let executed = changed(|f| f.observation = execution());
    for observation in [tuple(), executed.clone()] {
        assert!(CompatibilityProofV2::new(
            observation,
            Status::Verified,
            Source::ManualProbe,
            "2026-10-08T00:00:00Z",
            None
        )
        .is_err());
    }
    assert!(CompatibilityProofV2::new(
        executed.clone(),
        Status::Verified,
        Source::ServerStart,
        "2026-10-08T00:00:00Z",
        None
    )
    .is_err());
    for source in [Source::RuntimeExecution, Source::EndpointSmoke] {
        assert!(CompatibilityProofV2::new(
            tuple(),
            Status::Verified,
            source,
            "2026-10-08T00:00:00Z",
            None
        )
        .is_err());
        assert!(CompatibilityProofV2::new(
            executed.clone(),
            Status::Verified,
            source,
            "2026-10-08T00:00:00Z",
            None
        )
        .is_ok());
    }
}

#[test]
fn proof_status_failure_pair_and_timestamp_validate_on_construction_and_read() {
    for (status, failure) in [
        (Status::Verified, Some(ProofFailureCode::ModelLoadFailed)),
        (Status::Failed, None),
    ] {
        assert!(CompatibilityProofV2::new(
            tuple(),
            status,
            Source::ServerStart,
            "2026-10-08T00:00:00Z",
            failure
        )
        .is_err());
    }
    for invalid in ["", "yesterday", "2026-13-99T00:00:00Z", "2026-10-08"] {
        assert!(CompatibilityProofV2::new(
            tuple(),
            Status::Verified,
            Source::ServerStart,
            invalid,
            None
        )
        .is_err());
    }
    let mut value = serde_json::to_value(proof()).unwrap();
    value["status"] = json!("failed");
    assert!(serde_json::from_value::<CompatibilityProofV2>(value).is_err());
}

#[test]
fn proof_rejects_unknown_schema_kind_fields_and_freeform_errors() {
    for (field, bad) in [
        ("schema_version", json!(3)),
        ("record_kind", json!("support-hint")),
        ("error", json!("synthetic-secret")),
        ("failure_code", json!("raw-secret-error")),
    ] {
        let mut value = serde_json::to_value(proof()).unwrap();
        value[field] = bad;
        assert!(
            serde_json::from_value::<CompatibilityProofV2>(value).is_err(),
            "field {field}"
        );
    }
}

#[test]
fn key_rejects_path_traversal_noncanonical_hashes_and_unknown_fields() {
    for invalid in [
        "../proof",
        "ABCDEF",
        &"A".repeat(64),
        &"a".repeat(63),
        &"g".repeat(64),
    ] {
        assert!(
            CompatibilityProofKey::parse(facts().model_ref, ModelCapability::Chat, invalid)
                .is_err()
        );
    }
    let mut value = serde_json::to_value(tuple().key().unwrap()).unwrap();
    value["path"] = json!("../secret");
    assert!(serde_json::from_value::<CompatibilityProofKey>(value).is_err());
}

#[test]
fn invalid_and_partial_legacy_facts_stay_missing() {
    let mut proof = legacy_proof();
    proof.backend = "invented".to_owned();
    proof.mlx_runtime_family = None;
    proof.runtime_version = Some("latest".to_owned());
    proof.runtime_profile = Some("chat/default".to_owned());
    proof.runtime_profile_version = Some(0);
    let evidence =
        CompatibilityEvidence::from_legacy(proof, EvidenceGeneration::TupleAwareV1).unwrap();
    for dimension in [
        MissingDimension::Backend,
        MissingDimension::RuntimeFamily,
        MissingDimension::RuntimeVersion,
        MissingDimension::RuntimeProfileVersion,
    ] {
        assert!(evidence.missing_dimensions().contains(&dimension));
    }
    assert!(!evidence
        .missing_dimensions()
        .contains(&MissingDimension::RuntimeProfile));
}

#[test]
fn validation_errors_do_not_echo_rejected_values() {
    let error = CompatibilityIdentifier::parse("api_key=synthetic-secret")
        .unwrap_err()
        .to_string();
    assert!(!error.contains("synthetic-secret"));
    let error = RuntimeVersion::parse("1.0 password=synthetic-secret")
        .unwrap_err()
        .to_string();
    assert!(!error.contains("synthetic-secret"));
}

#[test]
fn image_execution_requires_observed_workflow_while_load_has_no_shape() {
    let loaded = changed(|f| f.capability = ModelCapability::ImageGeneration);
    assert_eq!(loaded.observation().scope(), ObservationScope::Load);
    let executed = changed(|f| f.observation = execution());
    for absent in [None, Some(Value::Null)] {
        let mut value = serde_json::to_value(&executed).unwrap();
        value["components"]["capability"] = json!("image-generation");
        value["components"]["observation"]["input"]["family"] = json!("image-generation");
        value["components"]["observation"]["output"]["family"] = json!("image-generation");
        if let Some(absent) = absent {
            value["components"]["observation"]["input"]["attributes"]["image_workflow"] = absent;
        }
        assert!(serde_json::from_value::<CompatibilityTuple>(value).is_err());
    }
}

#[test]
fn embedding_without_query_document_distinction_is_explicit_observed_absence() {
    let mut value = serde_json::to_value(changed(|f| f.observation = execution())).unwrap();
    value["components"]["capability"] = json!("embedding");
    value["components"]["observation"]["input"]["family"] = json!("embedding");
    value["components"]["observation"]["output"]["family"] = json!("embedding");
    value["components"]["observation"]["output"]["format"] = json!("float-vector");
    let generic: CompatibilityTuple = serde_json::from_value(value.clone()).unwrap();
    value["components"]["observation"]["input"]["attributes"]["embedding_input"] = json!("query");
    let query: CompatibilityTuple = serde_json::from_value(value).unwrap();
    assert_ne!(generic.key().unwrap(), query.key().unwrap());
}
