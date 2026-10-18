use std::collections::BTreeSet;

use crate::features::adapter::domain::AdapterRef;
use crate::features::model::domain::{
    ModelCapability, ModelCapabilityProofSource as Source, ModelCapabilityProofStatus as Status,
    ModelFormat, ModelRef,
};
use crate::features::model::support_status::{
    ModelSupportHintStatus as HintStatus, ModelSupportStatus as Support,
};

use super::super::*;
use super::fixtures::*;

fn constraints() -> CompatibilityConstraints {
    CompatibilityConstraints {
        model_ref: facts().model_ref,
        declared_capabilities: vec![ModelCapability::Chat],
        hard_incompatibilities: BTreeSet::new(),
    }
}
fn observed(tuple: CompatibilityTuple, status: Status, checked_at: &str) -> CompatibilityEvidence {
    let (source, failure_code) = match tuple.observation().scope() {
        ObservationScope::Load => (Source::ServerStart, ProofFailureCode::ModelLoadFailed),
        ObservationScope::Execution => (
            Source::RuntimeExecution,
            ProofFailureCode::RuntimeExecutionFailed,
        ),
    };
    CompatibilityEvidence::from_v2(
        CompatibilityProofV2::new(
            tuple,
            status,
            source,
            checked_at,
            (status == Status::Failed).then_some(failure_code),
        )
        .unwrap(),
    )
}
fn verified(tuple: CompatibilityTuple) -> CompatibilityEvidence {
    observed(tuple, Status::Verified, "2026-10-08T00:00:00Z")
}
fn hint(tuple: CompatibilityTuple, status: HintStatus) -> CompatibilityHint {
    CompatibilityHint { tuple, status }
}
fn resolve(
    query: &CompatibilityTuple,
    evidence: &[CompatibilityEvidence],
    hints: &[CompatibilityHint],
) -> CompatibilityResolution {
    CompatibilityResolver
        .resolve(query, &constraints(), evidence, hints)
        .unwrap()
}

#[test]
fn hard_incompatibility_precedes_exact_verified_failed_and_hints() {
    for code in [
        HardIncompatibility::BackendUnavailable,
        HardIncompatibility::AdapterIncompatible,
        HardIncompatibility::ExecutionShapeUnsupported,
        HardIncompatibility::ProfileIncompatible,
    ] {
        let mut constraints = constraints();
        constraints.hard_incompatibilities.insert(code);
        for status in [Status::Verified, Status::Failed] {
            let resolution = CompatibilityResolver
                .resolve(
                    &tuple(),
                    &constraints,
                    &[observed(tuple(), status, "2026-10-08T00:00:00Z")],
                    &[hint(tuple(), HintStatus::Supported)],
                )
                .unwrap();
            assert_eq!(resolution.status, Support::Unsupported);
            assert_eq!(
                resolution.evidence,
                CompatibilityResolutionEvidence::HardIncompatibility
            );
            assert_eq!(resolution.hard_incompatibilities, BTreeSet::from([code]));
            assert_eq!(resolution.exact_key, None);
        }
    }
}

#[test]
fn undeclared_capability_is_hard_incompatibility_not_an_unknown() {
    let mut constraints = constraints();
    constraints.declared_capabilities.clear();
    let resolution = CompatibilityResolver
        .resolve(&tuple(), &constraints, &[verified(tuple())], &[])
        .unwrap();
    assert_eq!(resolution.status, Support::Unsupported);
    assert!(resolution
        .hard_incompatibilities
        .contains(&HardIncompatibility::CapabilityNotDeclared));
}

#[test]
fn constraints_must_describe_the_queried_model() {
    let mut constraints = constraints();
    constraints.model_ref = ModelRef::parse("b".repeat(64)).unwrap();
    assert_eq!(
        CompatibilityResolver.resolve(&tuple(), &constraints, &[], &[]),
        Err(CompatibilityError::InvalidField("constraint_model_ref"))
    );
}

#[test]
fn current_exact_proof_beats_newer_unrelated_and_legacy_evidence() {
    let exact = observed(tuple(), Status::Verified, "2020-01-01T00:00:00Z");
    let other = observed(
        changed(|f| f.runtime.version = version("99.0")),
        Status::Failed,
        "2099-01-01T00:00:00Z",
    );
    let mut legacy = legacy_proof();
    legacy.status = Status::Failed;
    let legacy =
        CompatibilityEvidence::from_legacy(legacy, EvidenceGeneration::TupleAwareV1).unwrap();
    for evidence in [
        vec![exact.clone(), other.clone(), legacy.clone()],
        vec![legacy, other, exact],
    ] {
        let resolution = resolve(&tuple(), &evidence, &[]);
        assert_eq!(resolution.status, Support::Verified);
        assert_eq!(
            resolution.evidence,
            CompatibilityResolutionEvidence::ExactProof
        );
        assert_eq!(resolution.exact_key, Some(tuple().key().unwrap()));
        assert!(resolution.stale_reasons.is_empty());
    }
}

#[test]
fn exact_current_failure_cannot_be_overridden_by_positive_hint() {
    let failure = observed(tuple(), Status::Failed, "2026-10-08T00:00:00Z");
    let unrelated = verified(changed(|f| f.runtime.version = version("99.0")));
    let resolution = resolve(
        &tuple(),
        &[failure, unrelated],
        &[hint(tuple(), HintStatus::Supported)],
    );
    assert_eq!(resolution.status, Support::Failed);
    assert_eq!(resolution.reason, CompatibilityReason::ExactProofFailed);
    assert_eq!(
        resolution.failure_code,
        Some(ProofFailureCode::ModelLoadFailed)
    );
    assert_eq!(
        resolution.failure_summary(),
        Some(ProofFailureCode::ModelLoadFailed.summary())
    );
}

#[test]
fn exact_verified_is_not_overridden_by_advisory_negative_hint() {
    let resolution = resolve(
        &tuple(),
        &[verified(tuple())],
        &[hint(tuple(), HintStatus::Unsupported)],
    );
    assert_eq!(resolution.status, Support::Verified);
    assert_eq!(resolution.failure_code, None);
}

#[test]
fn duplicate_current_exact_proofs_fail_closed_instead_of_sorting_timestamps() {
    let first = observed(tuple(), Status::Verified, "2099-01-01T00:00:00Z");
    let second = observed(tuple(), Status::Failed, "2020-01-01T00:00:00Z");
    for evidence in [vec![first.clone(), second.clone()], vec![second, first]] {
        assert_eq!(
            CompatibilityResolver.resolve(&tuple(), &constraints(), &evidence, &[]),
            Err(CompatibilityError::AmbiguousExactProof)
        );
    }
}

#[test]
fn load_and_execution_success_or_failure_never_authorize_each_other() {
    let load = tuple();
    let execution = changed(|f| f.observation = execution());
    for (query, other) in [(load.clone(), execution.clone()), (execution, load)] {
        for status in [Status::Verified, Status::Failed] {
            let resolution = resolve(
                &query,
                &[observed(other.clone(), status, "2026-10-08T00:00:00Z")],
                &[],
            );
            assert_eq!(resolution.status, Support::Stale);
            assert_eq!(resolution.exact_key, None);
            assert_eq!(resolution.failure_code, None);
            assert!(resolution.stale_reasons.contains(
                &CompatibilityStaleReason::ChangedDimension(MissingDimension::Observation)
            ));
        }
    }
}

#[test]
fn base_adapter_and_changed_adapter_load_identity_remain_isolated() {
    let adapter = changed(|f| {
        f.adapter = CompatibilityAdapter::Selected {
            adapter_ref: AdapterRef::parse("b".repeat(64)).unwrap(),
            load_identity: identifier("loadA"),
        }
    });
    let changed_adapter = changed(|f| {
        f.adapter = CompatibilityAdapter::Selected {
            adapter_ref: AdapterRef::parse("b".repeat(64)).unwrap(),
            load_identity: identifier("loadB"),
        }
    });
    for (query, other) in [
        (tuple(), adapter.clone()),
        (adapter.clone(), tuple()),
        (adapter, changed_adapter),
    ] {
        let resolution = resolve(&query, &[verified(other)], &[]);
        assert_eq!(resolution.status, Support::Stale);
        assert_eq!(
            resolution.stale_reasons,
            BTreeSet::from([CompatibilityStaleReason::ChangedDimension(
                MissingDimension::Adapter
            )])
        );
    }
}

#[test]
fn all_changed_known_dimensions_have_stable_typed_reasons() {
    let candidates = [
        (
            MissingDimension::PrimaryFormat,
            changed(|f| f.primary_format = ModelFormat::Safetensors),
        ),
        (
            MissingDimension::Quantization,
            changed(|f| {
                f.quantization = Quantization::Quantized {
                    method: QuantizationMethod::Mlx,
                    variant: identifier("affine"),
                    bits: QuantizationBits::Fixed { bits: 4 },
                    group_size: QuantizationGroupSize::Fixed { size: 64 },
                }
            }),
        ),
        (
            MissingDimension::Backend,
            changed(|f| {
                f.backend = CompatibilityBackend::Transformers;
                f.runtime_family = RuntimeFamily::Transformers;
                f.runtime.package = RuntimePackage::Transformers;
            }),
        ),
        (
            MissingDimension::RuntimeFamily,
            changed(|f| {
                f.runtime_family = RuntimeFamily::MlxVlm;
                f.runtime.package = RuntimePackage::MlxVlm;
            }),
        ),
        (
            MissingDimension::RuntimePackage,
            changed(|f| {
                f.runtime_family = RuntimeFamily::MlxVlm;
                f.runtime.package = RuntimePackage::MlxVlm;
            }),
        ),
        (
            MissingDimension::RuntimeVersion,
            changed(|f| f.runtime.version = version("99.0")),
        ),
        (
            MissingDimension::RuntimeProfile,
            changed(|f| {
                f.profile = CompatibilityProfile::Selected {
                    id: identifier("profile"),
                    version: 1,
                }
            }),
        ),
        (
            MissingDimension::OperatingSystem,
            changed(|f| f.platform.os = OperatingSystem::Windows),
        ),
        (
            MissingDimension::Architecture,
            changed(|f| f.platform.architecture = Architecture::X86_64),
        ),
        (
            MissingDimension::DeviceClass,
            changed(|f| f.device_class = DeviceClass::Cpu),
        ),
    ];
    for (dimension, other) in candidates {
        let resolution = resolve(&tuple(), &[verified(other)], &[]);
        assert_eq!(resolution.status, Support::Stale);
        assert!(
            resolution
                .stale_reasons
                .contains(&CompatibilityStaleReason::ChangedDimension(dimension)),
            "{dimension:?}"
        );
        assert_eq!(resolution.failure_code, None);
    }
}

#[test]
fn profile_version_and_execution_shapes_report_their_specific_dimensions() {
    let selected = changed(|f| {
        f.profile = CompatibilityProfile::Selected {
            id: identifier("profile"),
            version: 1,
        }
    });
    let changed_version = changed(|f| {
        f.profile = CompatibilityProfile::Selected {
            id: identifier("profile"),
            version: 2,
        }
    });
    let resolution = resolve(&selected, &[verified(changed_version)], &[]);
    assert_eq!(
        resolution.stale_reasons,
        BTreeSet::from([CompatibilityStaleReason::ChangedDimension(
            MissingDimension::RuntimeProfileVersion
        )])
    );

    let query = changed(|f| f.observation = execution());
    let mut input = query.components().clone();
    if let Observation::Execution { input, .. } = &mut input.observation {
        input.provider = ProviderShape::Openai;
    }
    let mut output = query.components().clone();
    if let Observation::Execution { output, .. } = &mut output.observation {
        output.streaming = true;
    }
    for (dimension, other) in [
        (MissingDimension::InputShape, input),
        (MissingDimension::OutputShape, output),
    ] {
        let resolution = resolve(
            &query,
            &[verified(CompatibilityTuple::new(other).unwrap())],
            &[],
        );
        assert_eq!(
            resolution.stale_reasons,
            BTreeSet::from([CompatibilityStaleReason::ChangedDimension(dimension)])
        );
    }
}

#[test]
fn incomplete_legacy_verified_or_failed_is_stale_not_exact() {
    for generation in [
        EvidenceGeneration::TupleAwareV1,
        EvidenceGeneration::LegacyLatest,
    ] {
        for status in [Status::Verified, Status::Failed] {
            let mut old = legacy_proof();
            old.status = status;
            let evidence = CompatibilityEvidence::from_legacy(old, generation).unwrap();
            let resolution = resolve(
                &tuple(),
                &[evidence],
                &[hint(tuple(), HintStatus::Supported)],
            );
            assert_eq!(resolution.status, Support::Stale);
            assert_eq!(resolution.exact_key, None);
            assert_eq!(resolution.failure_code, None);
            for dimension in [
                MissingDimension::RuntimeVersion,
                MissingDimension::RuntimePackage,
                MissingDimension::Adapter,
                MissingDimension::Observation,
                MissingDimension::InputShape,
            ] {
                assert!(resolution
                    .stale_reasons
                    .contains(&CompatibilityStaleReason::MissingDimension(dimension)));
            }
        }
    }
}

#[test]
fn known_changed_legacy_facts_are_distinct_from_missing_facts() {
    let mut old = legacy_proof();
    old.primary_format = ModelFormat::Gguf;
    old.runtime_version = Some("0.1.0".to_owned());
    old.runtime_profile = Some("profile".to_owned());
    old.runtime_profile_version = Some(1);
    let resolution = resolve(
        &tuple(),
        &[CompatibilityEvidence::from_legacy(old, EvidenceGeneration::TupleAwareV1).unwrap()],
        &[],
    );
    for dimension in [
        MissingDimension::PrimaryFormat,
        MissingDimension::RuntimeVersion,
        MissingDimension::RuntimeProfile,
        MissingDimension::RuntimeProfileVersion,
    ] {
        assert!(resolution
            .stale_reasons
            .contains(&CompatibilityStaleReason::ChangedDimension(dimension)));
        assert!(!resolution
            .stale_reasons
            .contains(&CompatibilityStaleReason::MissingDimension(dimension)));
    }
    assert!(resolution
        .stale_reasons
        .contains(&CompatibilityStaleReason::MissingDimension(
            MissingDimension::Adapter
        )));
}

#[test]
fn old_proofs_from_other_model_or_capability_do_not_make_query_stale() {
    let mut other_model = legacy_proof();
    other_model.model_ref = ModelRef::parse("b".repeat(64)).unwrap();
    let mut other_capability = legacy_proof();
    other_capability.capability = ModelCapability::Embedding;
    let evidence = [other_model, other_capability]
        .into_iter()
        .map(|proof| {
            CompatibilityEvidence::from_legacy(proof, EvidenceGeneration::TupleAwareV1).unwrap()
        })
        .collect::<Vec<_>>();
    let resolution = resolve(&tuple(), &evidence, &[]);
    assert_eq!(resolution.status, Support::Unknown);
    assert!(resolution.stale_reasons.is_empty());
}

#[test]
fn v2_proofs_from_other_model_or_capability_are_unrelated() {
    let evidence = [
        verified(changed(|f| {
            f.model_ref = ModelRef::parse("b".repeat(64)).unwrap()
        })),
        verified(changed(|f| f.capability = ModelCapability::Embedding)),
    ];
    assert_eq!(resolve(&tuple(), &evidence, &[]).status, Support::Unknown);
}

#[test]
fn stale_explanations_are_independent_of_evidence_order() {
    let first = verified(changed(|f| f.runtime.version = version("99.0")));
    let second = verified(changed(|f| f.device_class = DeviceClass::Cpu));
    assert_eq!(
        resolve(&tuple(), &[first.clone(), second.clone()], &[]),
        resolve(&tuple(), &[second, first], &[])
    );
}

#[test]
fn exact_removal_cannot_resurrect_verified_from_remaining_legacy() {
    let legacy =
        CompatibilityEvidence::from_legacy(legacy_proof(), EvidenceGeneration::TupleAwareV1)
            .unwrap();
    assert_eq!(
        resolve(&tuple(), &[verified(tuple()), legacy.clone()], &[]).status,
        Support::Verified
    );
    assert_eq!(resolve(&tuple(), &[legacy], &[]).status, Support::Stale);
}

#[test]
fn exact_hint_is_advisory_and_never_verified() {
    let resolution = resolve(&tuple(), &[], &[hint(tuple(), HintStatus::Supported)]);
    assert_eq!(resolution.status, Support::Supported);
    assert_eq!(resolution.reason, CompatibilityReason::ExactHintSupported);
    assert_eq!(resolution.exact_key, None);
    assert!(resolution.summary().contains("not verified"));
}

#[test]
fn hint_with_different_adapter_runtime_platform_or_scope_is_not_applicable() {
    for other in [
        changed(|f| f.observation = execution()),
        changed(|f| f.runtime.version = version("99.0")),
        changed(|f| f.platform.os = OperatingSystem::Linux),
        changed(|f| {
            f.adapter = CompatibilityAdapter::Selected {
                adapter_ref: AdapterRef::parse("b".repeat(64)).unwrap(),
                load_identity: identifier("load"),
            }
        }),
    ] {
        assert_eq!(
            resolve(&tuple(), &[], &[hint(other, HintStatus::Supported)]).status,
            Support::Unknown
        );
    }
}

#[test]
fn opposing_exact_hints_choose_unsupported_in_any_order() {
    let supported = hint(tuple(), HintStatus::Supported);
    let unsupported = hint(tuple(), HintStatus::Unsupported);
    for hints in [
        vec![supported.clone(), unsupported.clone()],
        vec![unsupported, supported],
    ] {
        let resolution = resolve(&tuple(), &[], &hints);
        assert_eq!(resolution.status, Support::Unsupported);
        assert_eq!(resolution.reason, CompatibilityReason::ExactHintUnsupported);
    }
}

#[test]
fn broad_filter_selection_does_not_grant_exact_authority() {
    let other = verified(changed(|f| f.runtime.version = version("99.0")));
    let broad = CompatibilityFilter {
        capability: Some(ModelCapability::Chat),
        ..Default::default()
    };
    assert!(broad.matches(other.v2().unwrap()).unwrap());
    // The resolver takes the complete query, not that broad filter result.
    assert_eq!(resolve(&tuple(), &[other], &[]).status, Support::Stale);
}

#[test]
fn no_proof_no_hint_is_unknown_even_when_capability_is_declared() {
    let resolution = resolve(&tuple(), &[], &[]);
    assert_eq!(resolution.status, Support::Unknown);
    assert_eq!(resolution.reason, CompatibilityReason::NoEvidence);
    assert_eq!(resolution.exact_key, None);
}

#[test]
fn resolution_output_contains_only_safe_codes_not_legacy_exceptions() {
    let mut old = legacy_proof();
    old.status = Status::Failed;
    old.error = Some("password=synthetic-secret private-prompt".to_owned());
    let resolution = resolve(
        &tuple(),
        &[CompatibilityEvidence::from_legacy(old, EvidenceGeneration::LegacyLatest).unwrap()],
        &[],
    );
    let output = serde_json::to_string(&resolution).unwrap();
    assert!(!output.contains("synthetic-secret"));
    assert!(!output.contains("private-prompt"));
    assert!(output.contains("missing-dimension"));
    assert!(output.contains("runtime-version"));
}
