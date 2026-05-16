use sha2::{Digest, Sha256};

use crate::features::model::domain::{ModelManifest, ModelRef};
use crate::features::model::ports::ModelIdentityGenerator;
use crate::foundation::error::KernelResult;

use super::error::model_store_error;

/// Generates canonical model identity from manifest content.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdModelIdentityGenerator;

impl ModelIdentityGenerator for StdModelIdentityGenerator {
    fn model_ref_for_manifest(&self, manifest: &ModelManifest) -> KernelResult<ModelRef> {
        let canonical = manifest.clone().sorted();
        let bytes = serde_json::to_vec(&canonical).map_err(|err| {
            model_store_error(format!("serialize canonical model manifest failed: {err}"))
        })?;
        let digest = hex::encode(Sha256::digest(bytes));
        ModelRef::parse(digest).map_err(|err| model_store_error(err.to_string()))
    }
}
