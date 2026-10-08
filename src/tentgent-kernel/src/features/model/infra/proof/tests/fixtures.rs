use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::features::model::{
    compatibility::{
        Architecture, CompatibilityAdapter, CompatibilityBackend, CompatibilityPlatform,
        CompatibilityProfile, CompatibilityProofV2, CompatibilityTuple, CompatibilityTupleInput,
        DeviceClass, Observation, OperatingSystem, Quantization, RuntimeFamily, RuntimeIdentity,
        RuntimePackage, RuntimeVersion,
    },
    domain::{
        default_model_capability_source, ModelCapability, ModelCapabilityProof,
        ModelCapabilityProofSource, ModelCapabilityProofStatus, ModelFormat, ModelMetadata,
        ModelRef, ModelSourceKind, ModelStoreLayout,
    },
    infra::FileModelCatalogStore,
    ports::ModelCatalogStore,
    proof_context::ModelProofContext,
};
use crate::features::resource_coordination::{
    infra::FileResourceCoordinator, new_operation_id, ResourceCoordinator, ResourceKey,
    ResourceKind, ResourceLockMode, ResourceLockRequest, ResourcePermit,
};
use crate::foundation::layout::{
    LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput, RuntimeLayoutResolver,
    StdRuntimeLayoutResolver,
};

pub(super) struct Fixture {
    pub root: PathBuf,
    pub runtime: RuntimeLayout,
    pub store: ModelStoreLayout,
    pub metadata: ModelMetadata,
    cleanup: bool,
}

impl Fixture {
    pub fn new(label: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("tentgent-proof-{label}-{}", new_operation_id()));
        let mut fixture = Self::at(&root);
        fixture.cleanup = true;
        FileModelCatalogStore
            .save_model_metadata(&fixture.store, &fixture.metadata)
            .unwrap();
        fixture
    }

    pub fn at(root: &Path) -> Self {
        let runtime = StdRuntimeLayoutResolver
            .resolve(RuntimeLayoutInput {
                mode: LayoutResolveMode::ReadOnly,
                home_dir: Some(root.join("home")),
                data_root_dir: Some(root.join("data")),
            })
            .unwrap();
        let store = ModelStoreLayout::from_models_dir(runtime.models_dir.clone());
        let model_ref = ModelRef::parse("a".repeat(64)).unwrap();
        let metadata = ModelMetadata {
            short_ref: model_ref.short_ref().to_string(),
            model_ref,
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
            total_bytes: 1,
            imported_at: "2026-10-08T00:00:00Z".into(),
        };
        Self {
            root: root.to_path_buf(),
            runtime,
            store,
            metadata,
            cleanup: false,
        }
    }

    pub fn context(&self) -> ModelProofContext<'_> {
        ModelProofContext::new(&self.runtime)
    }

    pub fn model_ref(&self) -> &ModelRef {
        &self.metadata.model_ref
    }

    pub fn proof(&self, backend: &str) -> ModelCapabilityProof {
        ModelCapabilityProof {
            model_ref: self.metadata.model_ref.clone(),
            capability: ModelCapability::Chat,
            status: ModelCapabilityProofStatus::Verified,
            source: ModelCapabilityProofSource::ServerStart,
            primary_format: self.metadata.primary_format,
            mlx_runtime_family: None,
            backend: backend.into(),
            runtime_version: None,
            runtime_profile: None,
            runtime_profile_version: None,
            server_ref: None,
            checked_at: "2026-10-08T00:00:00Z".into(),
            error: None,
        }
    }

    pub fn proof_v2(&self, version: &str, iteration: u32) -> CompatibilityProofV2 {
        self.proof_v2_for(ModelCapability::Chat, version, iteration)
    }

    pub fn proof_v2_for(
        &self,
        capability: ModelCapability,
        version: &str,
        iteration: u32,
    ) -> CompatibilityProofV2 {
        CompatibilityProofV2::new(
            CompatibilityTuple::new(CompatibilityTupleInput {
                model_ref: self.model_ref().clone(),
                capability,
                primary_format: ModelFormat::Safetensors,
                quantization: Quantization::Unquantized,
                backend: CompatibilityBackend::Transformers,
                runtime_family: RuntimeFamily::Transformers,
                runtime: RuntimeIdentity {
                    package: RuntimePackage::Transformers,
                    version: RuntimeVersion::parse(version).unwrap(),
                },
                profile: CompatibilityProfile::NoProfile,
                platform: CompatibilityPlatform {
                    os: OperatingSystem::Linux,
                    architecture: Architecture::X86_64,
                },
                device_class: DeviceClass::Cpu,
                adapter: CompatibilityAdapter::Base,
                observation: Observation::Load,
            })
            .unwrap(),
            ModelCapabilityProofStatus::Verified,
            ModelCapabilityProofSource::ServerStart,
            format!("2026-10-08T00:00:{iteration:02}Z"),
            None,
        )
        .unwrap()
    }

    pub fn locks(&self, mode: ResourceLockMode) -> Vec<(ResourceKey, ResourceLockMode)> {
        vec![
            (ResourceKey::maintenance(), ResourceLockMode::Shared),
            (
                ResourceKey::new(ResourceKind::Model, self.model_ref().as_str()),
                ResourceLockMode::Shared,
            ),
            (
                ResourceKey::new(
                    ResourceKind::ModelCapability,
                    format!("{}|chat", self.model_ref()),
                ),
                mode,
            ),
        ]
    }

    pub fn permit(&self, mode: ResourceLockMode) -> ResourcePermit {
        FileResourceCoordinator
            .acquire(
                &self.runtime,
                ResourceLockRequest::new("proof-test", self.locks(mode)),
            )
            .unwrap()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if self.cleanup {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
