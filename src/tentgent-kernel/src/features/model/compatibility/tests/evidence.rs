use crate::features::adapter::domain::AdapterRef;
use crate::features::model::domain::{
    ModelCapability, ModelCapabilityProofSource as Source, ModelCapabilityProofStatus as Status,
};

use super::super::*;
use super::fixtures::*;

#[test]
fn legacy_evidence_preserves_generation_and_missing_facts_without_backfill() {
    for generation in [
        EvidenceGeneration::TupleAwareV1,
        EvidenceGeneration::LegacyLatest,
    ] {
        let proof = legacy_proof();
        let evidence = CompatibilityEvidence::from_legacy(proof.clone(), generation).unwrap();
        assert_eq!(evidence.generation(), generation);
        assert_eq!(evidence.legacy(), Some(&proof));
        assert_eq!(evidence.key().unwrap(), None);
        assert_eq!(evidence.v2(), None);
        for dimension in [
            MissingDimension::RuntimeVersion,
            MissingDimension::RuntimePackage,
            MissingDimension::Adapter,
            MissingDimension::InputShape,
            MissingDimension::OutputShape,
            MissingDimension::Observation,
            MissingDimension::RuntimeProfile,
            MissingDimension::RuntimeProfileVersion,
        ] {
            assert!(evidence.missing_dimensions().contains(&dimension));
        }
        assert!(!evidence
            .missing_dimensions()
            .contains(&MissingDimension::RuntimeFamily));
        assert_eq!(evidence.legacy().unwrap().runtime_profile, None);
    }
    assert!(CompatibilityEvidence::from_legacy(legacy_proof(), EvidenceGeneration::V2).is_err());
}

#[test]
fn legacy_error_projection_never_returns_historical_payload() {
    let mut old = legacy_proof();
    old.status = Status::Failed;
    old.error = Some("api_key=synthetic-secret prompt=private".to_owned());
    let evidence =
        CompatibilityEvidence::from_legacy(old, EvidenceGeneration::LegacyLatest).unwrap();
    let json = serde_json::to_string(&evidence).unwrap();
    assert!(!json.contains("synthetic-secret"));
    assert!(!json.contains("private"));
    assert_eq!(evidence.status(), Status::Failed);
    assert!(evidence.failure_summary().unwrap().contains("Legacy"));
}

#[test]
fn v2_evidence_has_complete_facts_and_no_legacy_projection() {
    let proof = proof();
    let evidence = CompatibilityEvidence::from_v2(proof.clone());
    assert_eq!(evidence.generation(), EvidenceGeneration::V2);
    assert_eq!(evidence.v2(), Some(&proof));
    assert_eq!(evidence.legacy(), None);
    assert!(evidence.missing_dimensions().is_empty());
    assert_eq!(evidence.key().unwrap(), Some(proof.key().unwrap()));
    assert_eq!(evidence.model_ref(), proof.tuple().model_ref());
    assert_eq!(evidence.capability(), ModelCapability::Chat);
    assert_eq!(evidence.source(), Source::ServerStart);
    assert_eq!(evidence.checked_at(), proof.checked_at());
}

#[test]
fn filters_are_partial_enumeration_but_exact_key_and_shape_remain_isolated() {
    let original = proof();
    let broad = CompatibilityFilter {
        capability: Some(ModelCapability::Chat),
        ..Default::default()
    };
    assert!(broad.matches(&original).unwrap());
    let execution = changed(|f| f.observation = execution());
    assert!(broad.matches_tuple(&execution).unwrap());
    let scope = CompatibilityFilter {
        observation_scope: Some(ObservationScope::Execution),
        ..Default::default()
    };
    assert!(!scope.matches(&original).unwrap());
    assert!(scope.matches_tuple(&execution).unwrap());
    let exact = CompatibilityFilter {
        exact_key: Some(original.key().unwrap()),
        ..Default::default()
    };
    assert!(!exact.matches_tuple(&execution).unwrap());
    if let Observation::Execution { input, output } = execution.observation() {
        let shape = CompatibilityFilter {
            input_shape: Some(input.clone()),
            output_shape: Some(output.clone()),
            ..Default::default()
        };
        assert!(shape.matches_tuple(&execution).unwrap());
        assert!(!shape.matches(&original).unwrap());
    }
}

#[test]
fn filter_checks_runtime_profile_adapter_and_platform_facts() {
    let original = tuple();
    for filter in [
        CompatibilityFilter {
            runtime_version: Some(version("99.0")),
            ..Default::default()
        },
        CompatibilityFilter {
            runtime_package: Some(RuntimePackage::Diffusers),
            ..Default::default()
        },
        CompatibilityFilter {
            runtime_family: Some(RuntimeFamily::MlxAudio),
            ..Default::default()
        },
        CompatibilityFilter {
            backend: Some(CompatibilityBackend::Diffusers),
            ..Default::default()
        },
        CompatibilityFilter {
            platform: Some(CompatibilityPlatform {
                os: OperatingSystem::Windows,
                architecture: Architecture::X86_64,
            }),
            ..Default::default()
        },
        CompatibilityFilter {
            profile: Some(CompatibilityProfile::Selected {
                id: identifier("selected"),
                version: 1,
            }),
            ..Default::default()
        },
        CompatibilityFilter {
            adapter: Some(CompatibilityAdapter::Selected {
                adapter_ref: AdapterRef::parse("b".repeat(64)).unwrap(),
                load_identity: identifier("load"),
            }),
            ..Default::default()
        },
        CompatibilityFilter {
            device_class: Some(DeviceClass::Cpu),
            ..Default::default()
        },
    ] {
        assert!(!filter.matches_tuple(&original).unwrap());
    }
    assert!(CompatibilityFilter::default()
        .matches_tuple(&original)
        .unwrap());
}
