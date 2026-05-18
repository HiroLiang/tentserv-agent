use sha2::{Digest, Sha256};

use crate::features::train::ports::LoraTrainRunRefGenerator;
use crate::foundation::error::KernelResult;

/// SHA-256 based train run ref generator.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdLoraTrainRunRefGenerator;

impl LoraTrainRunRefGenerator for StdLoraTrainRunRefGenerator {
    fn generate_run_ref(&self, plan_ref: &str, created_at: &str) -> KernelResult<String> {
        let mut hasher = Sha256::new();
        hasher.update(plan_ref.as_bytes());
        hasher.update(b"\0");
        hasher.update(created_at.as_bytes());
        hasher.update(b"\0");
        hasher.update(std::process::id().to_string().as_bytes());
        Ok(hex::encode(hasher.finalize()))
    }
}
