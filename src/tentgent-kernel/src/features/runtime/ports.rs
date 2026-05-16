//! Runtime feature package ports.

use std::path::PathBuf;

use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;

use super::domain::{
    BootstrapRuntimeInput, PythonRuntimeLayout, PythonRuntimeResolutionInput,
    RuntimeBootstrapOutcome, RuntimeBootstrapPlan, RuntimeEntrypoint, RuntimeInitState,
};

pub trait PythonRuntimeResolver {
    fn resolve_python_runtime(
        &self,
        layout: &RuntimeLayout,
        input: PythonRuntimeResolutionInput,
    ) -> KernelResult<PythonRuntimeLayout>;
}

pub trait RuntimeExecutableResolver {
    fn python_binary_path(&self, runtime: &PythonRuntimeLayout) -> KernelResult<PathBuf>;

    fn entrypoint_path(
        &self,
        runtime: &PythonRuntimeLayout,
        entrypoint: RuntimeEntrypoint,
    ) -> KernelResult<PathBuf>;
}

pub trait RuntimeBootstrapPlanner {
    fn plan_bootstrap(
        &self,
        layout: &RuntimeLayout,
        input: BootstrapRuntimeInput,
    ) -> KernelResult<RuntimeBootstrapPlan>;
}

pub trait RuntimeBootstrapExecutor {
    fn execute_bootstrap(
        &self,
        plan: &RuntimeBootstrapPlan,
    ) -> KernelResult<RuntimeBootstrapOutcome>;
}

pub trait RuntimeStateProbe {
    fn probe_runtime_state(
        &self,
        layout: &RuntimeLayout,
        runtime: Option<&PythonRuntimeLayout>,
    ) -> KernelResult<RuntimeInitState>;
}
