//! Runtime feature package ports.

use std::path::PathBuf;

use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;
use crate::foundation::platform::PlatformFacts;

use super::domain::{
    BootstrapRuntimeInput, PythonRuntimeLayout, PythonRuntimeResolutionInput,
    RuntimeBootstrapOutcome, RuntimeBootstrapPlan, RuntimeEntrypoint, RuntimeInitState,
};

/// Resolves the Python runtime layout that Tentgent should use for daemon work.
pub trait PythonRuntimeResolver {
    /// Selects development, packaged, or override paths without spawning Python.
    fn resolve_python_runtime(
        &self,
        layout: &RuntimeLayout,
        input: PythonRuntimeResolutionInput,
    ) -> KernelResult<PythonRuntimeLayout>;
}

/// Resolves concrete executables inside an already selected Python runtime layout.
pub trait RuntimeExecutableResolver {
    /// Returns the Python interpreter path for the runtime layout.
    fn python_binary_path(&self, runtime: &PythonRuntimeLayout) -> KernelResult<PathBuf>;

    /// Returns the executable/script path for a named runtime entrypoint.
    fn entrypoint_path(
        &self,
        runtime: &PythonRuntimeLayout,
        entrypoint: RuntimeEntrypoint,
    ) -> KernelResult<PathBuf>;
}

/// Plans runtime bootstrap work without executing external commands.
pub trait RuntimeBootstrapPlanner {
    /// Builds a bootstrap plan from runtime layout, platform facts, and caller input.
    fn plan_bootstrap(
        &self,
        layout: &RuntimeLayout,
        runtime: &PythonRuntimeLayout,
        platform: &PlatformFacts,
        input: BootstrapRuntimeInput,
    ) -> KernelResult<RuntimeBootstrapPlan>;
}

/// Executes a previously planned runtime bootstrap operation.
pub trait RuntimeBootstrapExecutor {
    /// Runs the bootstrap plan and returns the observed outcome.
    fn execute_bootstrap(
        &self,
        plan: &RuntimeBootstrapPlan,
    ) -> KernelResult<RuntimeBootstrapOutcome>;
}

/// Probes whether the managed Python runtime appears initialized and ready.
pub trait RuntimeStateProbe {
    /// Inspects runtime state without mutating runtime-home or bootstrapping missing pieces.
    fn probe_runtime_state(
        &self,
        layout: &RuntimeLayout,
        runtime: Option<&PythonRuntimeLayout>,
    ) -> KernelResult<RuntimeInitState>;
}
