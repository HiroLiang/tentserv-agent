//! Runtime bootstrap use case.

use crate::features::runtime::ports::{
    PythonRuntimeResolver, RuntimeBootstrapExecutor, RuntimeBootstrapPlanner,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;
use crate::foundation::platform::PlatformProbe;

use super::port::{RuntimeBootstrapRequest, RuntimeBootstrapResult, RuntimeBootstrapUseCase};

pub struct StdRuntimeBootstrapUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    platform_probe: &'a dyn PlatformProbe,
    runtime_resolver: &'a dyn PythonRuntimeResolver,
    bootstrap_planner: &'a dyn RuntimeBootstrapPlanner,
    bootstrap_executor: &'a dyn RuntimeBootstrapExecutor,
}

impl<'a> StdRuntimeBootstrapUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        platform_probe: &'a dyn PlatformProbe,
        runtime_resolver: &'a dyn PythonRuntimeResolver,
        bootstrap_planner: &'a dyn RuntimeBootstrapPlanner,
        bootstrap_executor: &'a dyn RuntimeBootstrapExecutor,
    ) -> Self {
        Self {
            layout_resolver,
            platform_probe,
            runtime_resolver,
            bootstrap_planner,
            bootstrap_executor,
        }
    }
}

impl RuntimeBootstrapUseCase for StdRuntimeBootstrapUseCase<'_> {
    fn bootstrap_runtime(
        &self,
        request: RuntimeBootstrapRequest,
    ) -> KernelResult<RuntimeBootstrapResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let platform = self.platform_probe.probe()?;
        let runtime = self
            .runtime_resolver
            .resolve_python_runtime(&layout, request.runtime)?;
        let plan = self.bootstrap_planner.plan_bootstrap(
            &layout,
            &runtime,
            &platform,
            request.bootstrap,
        )?;
        let outcome = self.bootstrap_executor.execute_bootstrap(&plan)?;

        Ok(RuntimeBootstrapResult {
            layout,
            platform,
            runtime,
            plan,
            outcome,
        })
    }
}
