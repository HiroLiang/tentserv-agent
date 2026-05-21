//! Managed store maintenance use cases.

mod gc;
mod port;

pub use gc::StdStoreGcUseCase;
pub use port::{StoreGcRequest, StoreGcResult, StoreGcUseCase};
