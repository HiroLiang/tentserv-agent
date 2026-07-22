use std::path::PathBuf;

use crate::features::cluster::{
    domain::{
        ClusterDefinition, ClusterRef, ClusterRouteKey, ClusterRouteTarget, ClusterStoreLayout,
        CLUSTER_SCHEMA_VERSION,
    },
    infra::FileClusterCatalogStore,
    ports::ClusterCatalogStore,
};
use crate::features::model::domain::{
    default_model_capabilities, default_model_capability_source, MlxRuntimeFamily, ModelCapability,
    ModelCapabilityProof, ModelCapabilityProofSource, ModelCapabilityProofStatus, ModelFormat,
    ModelImportMethod, ModelManifest, ModelManifestEntry, ModelMetadata, ModelRef, ModelSourceKind,
    ModelStoreLayout, ModelVariantMetadata, ModelVariantStatus, SOURCE_DIRNAME,
};
use crate::features::model::infra::{FileModelCapabilityProofStore, FileModelCatalogStore};
use crate::features::model::ports::{ModelCapabilityProofStore, ModelCatalogStore};
use crate::features::server::domain::{
    CloudProvider, LaunchMode, ServerCapability, ServerPrepareTarget, ServerRef, ServerRefSelector,
    ServerRuntimeKind, ServerSpec,
};
use crate::features::server::infra::{
    FileServerCatalogStore, StdServerIdentityGenerator, StdServerStoreLayoutInitializer,
};
use crate::features::server::ports::{
    ServerCatalogStore, ServerClock, ServerProcessController, ServerProcessProbe,
    ServerStoreLayoutInitializer,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
};

use super::{
    ServerInspectRequest, ServerLifecycleUseCase, ServerListRequest, ServerPrepareRequest,
    ServerRecordProcessStartRequest, ServerRemoveRequest, ServerResolveForStartRequest,
    ServerSpecUseCase, ServerStopRequest, StdServerUseCase,
};

mod fixtures;
mod lifecycle;
mod support_gate;

use fixtures::*;
