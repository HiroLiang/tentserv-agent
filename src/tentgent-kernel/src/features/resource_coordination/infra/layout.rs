use std::path::PathBuf;

use sha2::{Digest, Sha256};

use crate::foundation::layout::RuntimeLayout;

use super::super::ResourceKey;

pub const RESOURCE_COORDINATION_DIRNAME: &str = "resource-coordination";

pub fn coordination_root(layout: &RuntimeLayout) -> PathBuf {
    layout.locks_dir.join(RESOURCE_COORDINATION_DIRNAME)
}

pub fn coordination_lock_path(layout: &RuntimeLayout, key: &ResourceKey) -> PathBuf {
    let digest = Sha256::digest(key.label().as_bytes());
    coordination_root(layout)
        .join("locks")
        .join(format!("{}.lock", hex::encode(digest)))
}

pub(super) fn holder_dir(layout: &RuntimeLayout, key: &ResourceKey) -> PathBuf {
    let digest = Sha256::digest(key.label().as_bytes());
    coordination_root(layout)
        .join("holders")
        .join(hex::encode(digest))
}
