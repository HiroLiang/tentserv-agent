pub mod adapter;
pub mod auth;
pub mod daemon;
pub mod dataset;
pub mod dataset_runtime;
pub mod doctor;
pub mod model;
pub mod platform;
pub mod runtime_assets;
pub mod server;
pub mod server_runtime;
pub mod session;
pub mod train;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_ID: &str = "com.tentserv.tentgent";
pub const AUTH_SERVICE: &str = "com.tentserv.tentgent.auth";
