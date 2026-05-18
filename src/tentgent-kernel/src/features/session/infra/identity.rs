use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::features::session::domain::{SessionRef, SessionStoreConfig};
use crate::features::session::ports::SessionIdentityGenerator;
use crate::foundation::error::KernelResult;

use super::error::session_store_error;

/// Generates SHA-256 session refs compatible with the legacy file store.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdSessionIdentityGenerator;

impl SessionIdentityGenerator for StdSessionIdentityGenerator {
    fn generate_session_ref(&self, store: &SessionStoreConfig) -> KernelResult<SessionRef> {
        let layout = store.file_layout().ok_or_else(|| {
            session_store_error("file session identity generator requires a file session store")
        })?;
        for attempt in 0..128_u32 {
            let now = OffsetDateTime::now_utc().unix_timestamp_nanos();
            let mut hasher = Sha256::new();
            hasher.update(layout.home_dir.to_string_lossy().as_bytes());
            hasher.update(b"\0");
            hasher.update(now.to_string().as_bytes());
            hasher.update(b"\0");
            hasher.update(std::process::id().to_string().as_bytes());
            hasher.update(b"\0");
            hasher.update(attempt.to_string().as_bytes());
            let session_ref = SessionRef::parse(hex::encode(hasher.finalize()))
                .map_err(|err| session_store_error(err.to_string()))?;
            if !layout.session_dir(&session_ref).exists() {
                return Ok(session_ref);
            }
        }

        Err(session_store_error(
            "failed to generate a unique session ref after 128 attempts",
        ))
    }
}
