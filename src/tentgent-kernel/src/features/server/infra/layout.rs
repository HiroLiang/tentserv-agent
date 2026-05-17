use std::fs;

use crate::features::server::domain::ServerStoreLayout;
use crate::features::server::ports::ServerStoreLayoutInitializer;
use crate::foundation::error::KernelResult;

use super::error::path_error;

/// Creates the standard server-store directory layout.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdServerStoreLayoutInitializer;

impl ServerStoreLayoutInitializer for StdServerStoreLayoutInitializer {
    fn ensure_server_store_layout(&self, layout: &ServerStoreLayout) -> KernelResult<()> {
        fs::create_dir_all(&layout.servers_dir)
            .map_err(|err| path_error("create server directory", &layout.servers_dir, err))
    }
}
