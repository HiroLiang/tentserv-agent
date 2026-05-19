use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::features::server::domain::{
    ServerCapability, ServerRef, ServerRuntimeKind, ServerRuntimeTarget,
};
use crate::features::server::ports::ServerIdentityGenerator;
use crate::foundation::error::KernelResult;

use super::error::server_store_error;

/// SHA-256 based server spec identity generator.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdServerIdentityGenerator;

impl ServerIdentityGenerator for StdServerIdentityGenerator {
    fn server_ref_for_target(
        &self,
        target: &ServerRuntimeTarget,
        host: &str,
        port: u16,
        lazy_load: bool,
        idle_seconds: Option<u64>,
    ) -> KernelResult<ServerRef> {
        let server_ref = match target {
            ServerRuntimeTarget::LocalModel {
                model_ref,
                capability,
                ..
            } if *capability == ServerCapability::Chat => {
                compute_server_ref(LocalServerIdentity {
                    model_ref: model_ref.as_str(),
                    host,
                    port,
                    lazy_load,
                    idle_seconds,
                })?
            }
            ServerRuntimeTarget::LocalModel {
                model_ref,
                capability,
                ..
            } => compute_server_ref(LocalCapabilityServerIdentity {
                model_ref: model_ref.as_str(),
                capability: capability.as_str(),
                host,
                port,
                lazy_load,
                idle_seconds,
            })?,
            ServerRuntimeTarget::CloudProvider {
                provider,
                provider_model,
            } => compute_server_ref(CloudServerIdentity {
                runtime_kind: ServerRuntimeKind::Cloud,
                provider: provider.as_str(),
                provider_model,
                host,
                port,
                lazy_load,
                idle_seconds,
            })?,
        };

        ServerRef::parse(server_ref).map_err(|err| server_store_error(err.to_string()))
    }
}

#[derive(Debug, Serialize)]
struct LocalServerIdentity<'a> {
    model_ref: &'a str,
    host: &'a str,
    port: u16,
    lazy_load: bool,
    idle_seconds: Option<u64>,
}

#[derive(Debug, Serialize)]
struct LocalCapabilityServerIdentity<'a> {
    model_ref: &'a str,
    capability: &'a str,
    host: &'a str,
    port: u16,
    lazy_load: bool,
    idle_seconds: Option<u64>,
}

#[derive(Debug, Serialize)]
struct CloudServerIdentity<'a> {
    runtime_kind: ServerRuntimeKind,
    provider: &'a str,
    provider_model: &'a str,
    host: &'a str,
    port: u16,
    lazy_load: bool,
    idle_seconds: Option<u64>,
}

fn compute_server_ref(identity: impl Serialize) -> KernelResult<String> {
    let bytes = serde_json::to_vec(&identity)
        .map_err(|err| server_store_error(format!("serialize server identity failed: {err}")))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

#[cfg(test)]
pub(crate) fn local_identity_json_for_test(
    model_ref: &str,
    host: &str,
    port: u16,
    lazy_load: bool,
    idle_seconds: Option<u64>,
) -> String {
    serde_json::to_string(&LocalServerIdentity {
        model_ref,
        host,
        port,
        lazy_load,
        idle_seconds,
    })
    .expect("serialize local identity")
}

#[cfg(test)]
pub(crate) fn local_capability_identity_json_for_test(
    model_ref: &str,
    capability: &str,
    host: &str,
    port: u16,
    lazy_load: bool,
    idle_seconds: Option<u64>,
) -> String {
    serde_json::to_string(&LocalCapabilityServerIdentity {
        model_ref,
        capability,
        host,
        port,
        lazy_load,
        idle_seconds,
    })
    .expect("serialize local capability identity")
}
