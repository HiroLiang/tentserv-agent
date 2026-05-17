use sha2::{Digest, Sha256};

use crate::features::adapter::domain::{AdapterManifest, AdapterRef};
use crate::features::adapter::ports::AdapterIdentityGenerator;
use crate::foundation::error::KernelResult;

use super::error::adapter_store_error;

/// Generates canonical adapter identity from manifest content.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdAdapterIdentityGenerator;

impl AdapterIdentityGenerator for StdAdapterIdentityGenerator {
    fn adapter_ref_for_manifest(&self, manifest: &AdapterManifest) -> KernelResult<AdapterRef> {
        let canonical = manifest.clone().sorted();
        let bytes = serde_json::to_vec(&canonical).map_err(|err| {
            adapter_store_error(format!(
                "serialize canonical adapter manifest failed: {err}"
            ))
        })?;
        let digest = hex::encode(Sha256::digest(bytes));
        AdapterRef::parse(digest).map_err(|err| adapter_store_error(err.to_string()))
    }
}
