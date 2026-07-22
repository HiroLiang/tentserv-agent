use super::*;
use std::path::PathBuf;
use tentgent_kernel::features::auth::domain::{
    AuthProviderPreference, AuthSecretSource, KeychainPresence,
};
use tentgent_kernel::features::model::{
    domain::{ModelCapability, ModelFileDiagnosticCode, ModelFileDiagnosticSeverity},
    support_status::{ModelSupportEvidenceKind, ModelSupportStatus},
};

#[test]
fn model_support_warning_check_skips_healthy_statuses() {
    let summary = support_summary(ModelSupportStatus::Verified, None);

    assert!(model_support_warning_check("abc123abc123", &summary).is_none());
}

#[test]
fn runtime_ownership_summary_reports_stale_recovery_action() {
    let check = runtime_ownership_summary_check(&RuntimeOwnershipSummary {
        route_claim_count: 2,
        active_generation_count: 1,
        active_operation_count: 0,
        stale_record_count: 1,
        malformed_record_count: 0,
        status: RuntimeOwnershipStatus::Attention,
    });

    assert_eq!(check.category, DoctorCheckCategory::RuntimeOwnership);
    assert_eq!(check.status, DoctorCheckStatus::Warn);
    assert!(check.detail.contains("1 stale record(s)"));
    assert_eq!(
        check.next_actions[0].command.as_deref(),
        Some("tentgent runtime reconcile")
    );
}

#[test]
fn model_file_diagnostic_warning_check_reports_next_action() {
    let check = model_file_diagnostic_warning_check(
        "abc123abc123",
        &[ModelFileDiagnostic {
            severity: ModelFileDiagnosticSeverity::Blocking,
            code: ModelFileDiagnosticCode::MissingTokenizerAssets,
            path: PathBuf::from("/tmp/model/tokenizer.json"),
            message: "tokenizer assets are missing".to_string(),
            next_action: "remove the corrupted model, then pull or import it again".to_string(),
        }],
    )
    .expect("missing files should warn");

    assert_eq!(check.status, DoctorCheckStatus::Warn);
    assert_eq!(check.category, DoctorCheckCategory::Capability);
    assert_eq!(check.name, "model files: abc123abc123");
    assert!(check.detail.contains("missing-tokenizer-assets"));
    assert!(check.detail.contains("next action"));
    assert_eq!(check.next_actions.len(), 1);
    assert_eq!(
        check.next_actions[0].command.as_deref(),
        Some("tentgent model inspect abc123abc123")
    );
}

#[test]
fn model_support_warning_check_reports_failed_status_with_reason() {
    let summary = support_summary(
        ModelSupportStatus::Failed,
        Some("runtime failed".to_string()),
    );
    let check = model_support_warning_check("abc123abc123", &summary)
        .expect("failed support status should warn");

    assert_eq!(check.status, DoctorCheckStatus::Warn);
    assert_eq!(check.category, DoctorCheckCategory::Capability);
    assert_eq!(check.name, "model support: abc123abc123 chat");
    assert_eq!(
            check.detail,
            "failed via local-proof: runtime failed; execution_backend: mlx-lm; recovery: fix the runtime/backend issue, clear the failed proof, then retry the route or rerun verification"
        );
    assert_eq!(check.next_actions.len(), 1);
    assert_eq!(
        check.next_actions[0].command.as_deref(),
        Some("tentgent model capability proof clear abc123abc123 chat")
    );
}

#[test]
fn model_support_warning_check_reports_stale_status_with_reason() {
    let mut summary = support_summary(ModelSupportStatus::Stale, None);
    summary.stale_reason = Some("backend changed from old-backend to mlx-lm".to_string());
    let check = model_support_warning_check("abc123abc123", &summary)
        .expect("stale support status should warn");

    assert_eq!(check.status, DoctorCheckStatus::Warn);
    assert_eq!(
            check.detail,
            "stale via local-proof: backend changed from old-backend to mlx-lm; execution_backend: mlx-lm; recovery: refresh evidence for the current runtime tuple, or clear stale proof before retrying"
        );
    assert_eq!(
        check.next_actions[0].command.as_deref(),
        Some("tentgent model capability proof clear abc123abc123 chat")
    );
}

#[test]
fn model_support_warning_check_reports_missing_capability_next_action() {
    let mut summary = support_summary(ModelSupportStatus::Unsupported, None);
    summary.declared = false;
    summary.evidence = ModelSupportEvidenceKind::CapabilityMetadata;
    summary.reason = "model does not declare chat capability".to_string();
    let check = model_support_warning_check("abc123abc123", &summary)
        .expect("unsupported support status should warn");

    assert_eq!(check.status, DoctorCheckStatus::Warn);
    assert_eq!(
            check.detail,
            "unsupported via capability-metadata: model does not declare chat capability; execution_backend: mlx-lm; recovery: add capability metadata only if the model is intended to support this capability"
        );
    assert_eq!(
        check.next_actions[0].command.as_deref(),
        Some("tentgent model capability add abc123abc123 chat")
    );
}

#[test]
fn model_support_warning_check_reports_declared_unsupported_tuple_next_action() {
    let mut summary = support_summary(ModelSupportStatus::Unsupported, None);
    summary.evidence = ModelSupportEvidenceKind::SupportHint;
    summary.reason = "known unsupported runtime tuple".to_string();
    let check = model_support_warning_check("abc123abc123", &summary)
        .expect("unsupported support status should warn");

    assert_eq!(check.status, DoctorCheckStatus::Warn);
    assert_eq!(
            check.detail,
            "unsupported via support-hint: known unsupported runtime tuple; execution_backend: mlx-lm; recovery: choose a different model, capability, or backend tuple"
        );
    assert_eq!(
        check.next_actions[0].command.as_deref(),
        Some("tentgent model inspect abc123abc123")
    );
}

#[test]
fn provider_auth_check_reports_missing_provider_next_actions() {
    let check = provider_auth_check(&[
        auth_status(
            Provider::OpenAI,
            AuthSourceMode::Auto,
            None,
            AuthValidationState::Missing,
        ),
        auth_status(
            Provider::Gemini,
            AuthSourceMode::Auto,
            Some(AuthSecretSource::Env),
            AuthValidationState::NotChecked,
        ),
    ]);

    assert_eq!(check.status, DoctorCheckStatus::Warn);
    assert_eq!(check.category, DoctorCheckCategory::Auth);
    assert_eq!(check.name, "provider auth");
    assert!(check.detail.contains("missing: OpenAI"));
    assert_eq!(check.next_actions.len(), 1);
    assert_eq!(
        check.next_actions[0].command.as_deref(),
        Some("tentgent auth openai set")
    );
    assert!(check.next_actions[0]
        .detail
        .as_deref()
        .expect("detail")
        .contains("OPENAI_API_KEY"));
}

#[test]
fn details_are_notable_when_next_actions_exist() {
    let check = DoctorCheck::warn(DoctorCheckCategory::Runtime, "custom", "short")
        .with_next_action(DoctorNextAction::command("Fix it", "tentgent doctor"));

    assert!(should_show_detail(&check));
}

#[test]
fn disabled_progress_is_noop() {
    DoctorProgress::disabled().step("checking test progress");
}

fn support_summary(
    status: ModelSupportStatus,
    failure_reason: Option<String>,
) -> ModelSupportSummary {
    ModelSupportSummary {
        capability: ModelCapability::Chat,
        declared: true,
        status,
        evidence: ModelSupportEvidenceKind::LocalProof,
        backend: "mlx-lm".to_string(),
        mlx_runtime_family: None,
        runtime_version: None,
        runtime_profile: None,
        runtime_profile_version: None,
        reason: "latest local proof failed chat".to_string(),
        stale_reason: None,
        failure_reason,
    }
}

fn auth_status(
    provider: Provider,
    source_mode: AuthSourceMode,
    effective_source: Option<AuthSecretSource>,
    validation: AuthValidationState,
) -> AuthKeyStatus {
    AuthKeyStatus {
        provider,
        preference: AuthProviderPreference {
            provider,
            source_mode,
            env_file: None,
        },
        env_present: effective_source == Some(AuthSecretSource::Env),
        keychain_presence: match effective_source {
            Some(AuthSecretSource::Keychain) => KeychainPresence::Present,
            _ => KeychainPresence::Unknown,
        },
        effective_source,
        validation,
    }
}
