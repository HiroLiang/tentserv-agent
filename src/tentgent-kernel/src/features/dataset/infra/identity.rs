use sha2::{Digest, Sha256};

use crate::features::dataset::domain::{DatasetManifest, DatasetRef};
use crate::features::dataset::ports::DatasetIdentityGenerator;
use crate::foundation::error::KernelResult;

use super::error::dataset_store_error;

/// Generates canonical dataset identity from manifest content.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdDatasetIdentityGenerator;

impl DatasetIdentityGenerator for StdDatasetIdentityGenerator {
    fn dataset_ref_for_manifest(&self, manifest: &DatasetManifest) -> KernelResult<DatasetRef> {
        let canonical = manifest.clone().sorted();
        let bytes = serde_json::to_vec(&canonical).map_err(|err| {
            dataset_store_error(format!(
                "serialize canonical dataset manifest failed: {err}"
            ))
        })?;
        let digest = hex::encode(Sha256::digest(bytes));
        DatasetRef::parse(digest).map_err(|err| dataset_store_error(err.to_string()))
    }
}
