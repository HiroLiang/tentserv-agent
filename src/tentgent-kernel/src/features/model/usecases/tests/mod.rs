use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::features::auth::domain::{AuthEnvLoadPolicy, Provider};
use crate::features::auth::usecases::{
    AuthSecretResolution, AuthSecretResolutionRequest, AuthSecretResolverUseCase,
};
use crate::features::model::domain::{
    default_model_capabilities, default_model_capability_source, HfModelMetadata,
    HfModelPullProgress, MlxRuntimeFamily, ModelCapability, ModelCapabilityProof,
    ModelCapabilityProofSource, ModelCapabilityProofStatus, ModelCapabilitySource, ModelFormat,
    ModelImportOutcome, ModelInspection, ModelMetadata, ModelRef, ModelRefSelector,
    ModelRemovalOutcome, ModelSourceKind, ModelStoreLayout, ModelSummary,
};
use crate::features::model::infra::{
    FileModelCapabilityProofStore, FileModelCatalogStore, FileModelContentStore,
    FileModelServerReferenceProbe, FileModelSourceIndexStore, StdModelIdentityGenerator,
    StdModelManifestBuilder, StdModelSourceStager, StdModelStoreLayoutInitializer,
};
use crate::features::model::ports::{
    HfModelSnapshot, HfModelSnapshotFetcher, HfModelSnapshotRequest, ModelCatalogStore, ModelClock,
};
use crate::features::runtime::domain::{
    PythonRuntimeLayout, PythonRuntimeResolutionInput, PythonRuntimeSource,
};
use crate::features::runtime::ports::PythonRuntimeResolver;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{
    LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput, RuntimeLayoutResolver,
};

use super::port::{
    ModelCapabilityMutation, ModelCapabilityProofClearRequest, ModelCapabilityProofListRequest,
    ModelCapabilityProofRecordRequest, ModelCapabilityProofUseCase, ModelCapabilityUpdateRequest,
    ModelCapabilityUpdateUseCase, ModelCapabilityVerifyRequest, ModelCatalogReadUseCase,
    ModelHfPullRequest, ModelHfPullUseCase, ModelInspectRequest, ModelListRequest,
    ModelLocalImportRequest, ModelLocalImportUseCase, ModelRemoveRequest, ModelRemoveUseCase,
};
use super::{
    StdModelCapabilityProofUseCase, StdModelCapabilityUpdateUseCase, StdModelCatalogReadUseCase,
    StdModelHfPullUseCase, StdModelLocalImportUseCase, StdModelRemoveUseCase,
};

mod fixtures;
mod workflows;

use fixtures::*;
