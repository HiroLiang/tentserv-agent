//! Server filesystem and process infrastructure.

mod error;
mod identity;
mod layout;
mod process;
mod process_identity;
mod runtime;
mod store;
mod time;

#[cfg(test)]
mod tests;

pub use identity::StdServerIdentityGenerator;
pub use layout::StdServerStoreLayoutInitializer;
pub use process::{StdServerProcessController, StdServerProcessProbe};
pub use process_identity::StdServerProcessIdentityProbe;
pub use runtime::{
    server_process_token_from_env, ServerRuntimeLaunchRequest, ServerRuntimeLauncher,
    SpawnedForegroundServer, SERVER_PROCESS_TOKEN_ENV_VAR,
};
pub use store::FileServerCatalogStore;
pub use time::SystemServerClock;
