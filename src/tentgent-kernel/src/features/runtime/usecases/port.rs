//! Runtime use case ports.

use std::path::PathBuf;

use crate::features::runtime::domain::{
    BootstrapRuntimeInput, PythonRuntimeLayout, PythonRuntimeResolutionInput,
    RuntimeBootstrapOutcome, RuntimeBootstrapPlan, RuntimeEntrypoint, RuntimeInitState,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};
use crate::foundation::platform::PlatformFacts;

/// Request for resolving the effective Python runtime layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeResolutionRequest {
    pub layout: RuntimeLayoutInput,
    pub runtime: PythonRuntimeResolutionInput,
}

/// Result of resolving the effective Python runtime layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeResolutionResult {
    pub layout: RuntimeLayout,
    pub runtime: PythonRuntimeLayout,
}

/// Request for planning and executing managed runtime bootstrap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeBootstrapRequest {
    pub layout: RuntimeLayoutInput,
    pub runtime: PythonRuntimeResolutionInput,
    pub bootstrap: BootstrapRuntimeInput,
}

/// Result of managed runtime bootstrap orchestration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeBootstrapResult {
    pub layout: RuntimeLayout,
    pub platform: PlatformFacts,
    pub runtime: PythonRuntimeLayout,
    pub plan: RuntimeBootstrapPlan,
    pub outcome: RuntimeBootstrapOutcome,
}

/// Request for probing managed runtime state without mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStateRequest {
    pub layout: RuntimeLayoutInput,
    pub runtime: PythonRuntimeResolutionInput,
}

/// Result of probing managed runtime state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStateResult {
    pub layout: RuntimeLayout,
    pub runtime: Option<PythonRuntimeLayout>,
    pub state: RuntimeInitState,
}

/// Executable target requested from a selected runtime layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeExecutableTarget {
    Python,
    Entrypoint(RuntimeEntrypoint),
}

/// Request for resolving one managed runtime executable path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeExecutableResolutionRequest {
    pub layout: RuntimeLayoutInput,
    pub runtime: PythonRuntimeResolutionInput,
    pub target: RuntimeExecutableTarget,
}

/// Result of resolving one managed runtime executable path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeExecutableResolutionResult {
    pub layout: RuntimeLayout,
    pub runtime: PythonRuntimeLayout,
    pub target: RuntimeExecutableTarget,
    pub path: PathBuf,
}

/// Use-case boundary for resolving the effective Python runtime layout.
pub trait RuntimeResolutionUseCase {
    /// Resolves runtime-home layout and the selected Python runtime layout.
    fn resolve_runtime(
        &self,
        request: RuntimeResolutionRequest,
    ) -> KernelResult<RuntimeResolutionResult>;
}

/// Use-case boundary for managed runtime bootstrap orchestration.
pub trait RuntimeBootstrapUseCase {
    /// Resolves layout/runtime/platform, plans bootstrap, executes it, and returns the outcome.
    fn bootstrap_runtime(
        &self,
        request: RuntimeBootstrapRequest,
    ) -> KernelResult<RuntimeBootstrapResult>;
}

/// Use-case boundary for read-only managed runtime state probing.
pub trait RuntimeStateUseCase {
    /// Resolves layout/runtime when possible and probes runtime state without bootstrapping.
    fn runtime_state(&self, request: RuntimeStateRequest) -> KernelResult<RuntimeStateResult>;
}

/// Use-case boundary for resolving managed runtime executable paths.
pub trait RuntimeExecutableResolutionUseCase {
    /// Resolves the selected executable path without checking that the executable works.
    fn resolve_runtime_executable(
        &self,
        request: RuntimeExecutableResolutionRequest,
    ) -> KernelResult<RuntimeExecutableResolutionResult>;
}
