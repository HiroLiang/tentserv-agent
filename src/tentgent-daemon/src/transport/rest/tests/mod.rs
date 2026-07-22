use std::{fs, path::PathBuf};

use axum::{
    body::{to_bytes, Body},
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use serde_json::Value;
use tentgent_kernel::{
    features::job::{
        domain::{JobResultFile, JobWorkspaceStreamSummary},
        infra::FileJobWorkspaceStore,
        ports::{JobChunkPort, JobChunkWrite, JobResultPort, JobStreamKind, JobWorkspacePort},
    },
    features::model::{
        domain::{
            ModelCapability, ModelCapabilityProof, ModelCapabilityProofSource,
            ModelCapabilityProofStatus, ModelFormat, ModelRef,
        },
        infra::FileModelCapabilityProofStore,
        ports::ModelCapabilityProofStore,
    },
    foundation::layout::{
        LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
    },
};
use tower::ServiceExt;

use crate::{
    app::{DaemonAppState, DaemonServices},
    bootstrap::{DaemonBootstrapConfig, LoggingConfig, LoggingRuntime, RestConfig},
    runtime::{
        JobArtifact, JobKind, JobOutputLine, JobProgressPatch, JobProgressUpdate, JobStatus,
        JobStream, JobTarget,
    },
    transport::rest::{build_router, security::DaemonSecurityConfig, state::RestState},
};

mod claude_messages_compat;
mod conformance_smoke;
mod gemini_compat;
mod openai_audio_compat;
mod openai_chat_compat;
mod openai_embeddings_compat;
mod openai_image_generation_compat;
mod provider_rerank_compat;

mod auth_and_chat;
mod clusters_and_sessions;
mod datasets_and_servers;
mod fixtures;
mod media_and_jobs;
mod model_capabilities;
mod model_resources;

use fixtures::*;
