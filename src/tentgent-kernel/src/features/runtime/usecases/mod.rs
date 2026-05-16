//! Runtime use case boundaries and implementations.

pub mod bootstrap;
pub mod executable;
pub mod port;
pub mod resolution;
pub mod state;

#[cfg(test)]
mod tests;

pub use bootstrap::StdRuntimeBootstrapUseCase;
pub use executable::StdRuntimeExecutableResolutionUseCase;
pub use port::{
    RuntimeBootstrapRequest, RuntimeBootstrapResult, RuntimeBootstrapUseCase,
    RuntimeExecutableResolutionRequest, RuntimeExecutableResolutionResult,
    RuntimeExecutableResolutionUseCase, RuntimeExecutableTarget, RuntimeResolutionRequest,
    RuntimeResolutionResult, RuntimeResolutionUseCase, RuntimeStateRequest, RuntimeStateResult,
    RuntimeStateUseCase,
};
pub use resolution::StdRuntimeResolutionUseCase;
pub use state::StdRuntimeStateUseCase;
