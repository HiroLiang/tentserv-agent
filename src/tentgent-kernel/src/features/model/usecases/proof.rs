//! Model capability proof read and write use cases.

use crate::features::model::domain::{
    MlxRuntimeFamily, ModelCapability, ModelCapabilityProof, ModelCapabilityProofSource,
    ModelCapabilityProofStatus, ModelFormat, ModelMetadata,
};
use crate::features::model::ports::{ModelCapabilityProofStore, ModelCatalogStore, ModelClock};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::model_store_layout;
use super::port::{
    ModelCapabilityProofClearRequest, ModelCapabilityProofClearResult,
    ModelCapabilityProofListRequest, ModelCapabilityProofListResult,
    ModelCapabilityProofRecordRequest, ModelCapabilityProofRecordResult,
    ModelCapabilityProofUseCase, ModelCapabilityVerifyRequest,
};

/// Standard model capability proof orchestration.
pub struct StdModelCapabilityProofUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    catalog: &'a dyn ModelCatalogStore,
    proofs: &'a dyn ModelCapabilityProofStore,
    clock: &'a dyn ModelClock,
}

impl<'a> StdModelCapabilityProofUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        catalog: &'a dyn ModelCatalogStore,
        proofs: &'a dyn ModelCapabilityProofStore,
        clock: &'a dyn ModelClock,
    ) -> Self {
        Self {
            layout_resolver,
            catalog,
            proofs,
            clock,
        }
    }
}

impl ModelCapabilityProofUseCase for StdModelCapabilityProofUseCase<'_> {
    fn list_model_capability_proofs(
        &self,
        request: ModelCapabilityProofListRequest,
    ) -> KernelResult<ModelCapabilityProofListResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = model_store_layout(&layout);
        let model = self.catalog.inspect_model(&store, &request.selector)?;
        let proofs = self
            .proofs
            .list_capability_proofs(&store, &model.metadata.model_ref)?;

        Ok(ModelCapabilityProofListResult {
            layout,
            store,
            model,
            proofs,
        })
    }

    fn verify_model_capability(
        &self,
        request: ModelCapabilityVerifyRequest,
    ) -> KernelResult<ModelCapabilityProofRecordResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = model_store_layout(&layout);
        let model = self.catalog.inspect_model(&store, &request.selector)?;
        let error = if model.metadata.supports_capability(request.capability) {
            None
        } else {
            Some(format!(
                "model `{}` does not advertise capability `{}`",
                model.metadata.model_ref, request.capability
            ))
        };
        let status = if error.is_some() {
            ModelCapabilityProofStatus::Failed
        } else {
            ModelCapabilityProofStatus::Verified
        };
        let proof = build_proof(
            &model.metadata,
            request.capability,
            status,
            ModelCapabilityProofSource::ManualProbe,
            None,
            None,
            None,
            error,
            self.clock.now_rfc3339()?,
        );
        self.proofs.save_capability_proof(&store, &proof)?;

        Ok(ModelCapabilityProofRecordResult {
            layout,
            store,
            model,
            proof,
        })
    }

    fn record_model_capability_proof(
        &self,
        request: ModelCapabilityProofRecordRequest,
    ) -> KernelResult<ModelCapabilityProofRecordResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = model_store_layout(&layout);
        let model = self.catalog.inspect_model(&store, &request.selector)?;
        let proof = build_proof(
            &model.metadata,
            request.capability,
            request.status,
            request.source,
            request.server_ref,
            request.runtime_profile,
            request.runtime_profile_version,
            request.error,
            self.clock.now_rfc3339()?,
        );
        self.proofs.save_capability_proof(&store, &proof)?;

        Ok(ModelCapabilityProofRecordResult {
            layout,
            store,
            model,
            proof,
        })
    }

    fn clear_model_capability_proofs(
        &self,
        request: ModelCapabilityProofClearRequest,
    ) -> KernelResult<ModelCapabilityProofClearResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = model_store_layout(&layout);
        let model = self.catalog.inspect_model(&store, &request.selector)?;
        let removed_proof_count = self
            .proofs
            .list_capability_proofs(&store, &model.metadata.model_ref)?
            .into_iter()
            .filter(|proof| proof.capability == request.capability)
            .count();

        self.proofs.remove_capability_proof(
            &store,
            &model.metadata.model_ref,
            request.capability,
        )?;

        Ok(ModelCapabilityProofClearResult {
            layout,
            store,
            model,
            capability: request.capability,
            removed_proof_count,
        })
    }
}

fn build_proof(
    metadata: &ModelMetadata,
    capability: ModelCapability,
    status: ModelCapabilityProofStatus,
    source: ModelCapabilityProofSource,
    server_ref: Option<String>,
    runtime_profile: Option<String>,
    runtime_profile_version: Option<u32>,
    error: Option<String>,
    checked_at: String,
) -> ModelCapabilityProof {
    ModelCapabilityProof {
        model_ref: metadata.model_ref.clone(),
        capability,
        status,
        source,
        primary_format: metadata.primary_format,
        mlx_runtime_family: metadata.mlx_runtime_family,
        backend: backend_label(metadata.mlx_runtime_family, metadata.primary_format),
        runtime_version: None,
        runtime_profile,
        runtime_profile_version,
        server_ref,
        checked_at,
        error: error.map(sanitize_proof_error),
    }
}

fn sanitize_proof_error(error: String) -> String {
    const MAX_PROOF_ERROR_CHARS: usize = 500;
    let mut sanitized = error
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    for marker in [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
        "HF_TOKEN",
        "HUGGING_FACE_HUB_TOKEN",
    ] {
        sanitized = sanitized.replace(marker, "[redacted-env]");
    }

    if sanitized.chars().count() > MAX_PROOF_ERROR_CHARS {
        let mut truncated = sanitized
            .chars()
            .take(MAX_PROOF_ERROR_CHARS)
            .collect::<String>();
        truncated.push_str("...");
        truncated
    } else {
        sanitized
    }
}

fn backend_label(
    mlx_runtime_family: Option<MlxRuntimeFamily>,
    primary_format: ModelFormat,
) -> String {
    mlx_runtime_family
        .map(|family| family.as_str().to_string())
        .unwrap_or_else(|| primary_format.as_str().to_string())
}
