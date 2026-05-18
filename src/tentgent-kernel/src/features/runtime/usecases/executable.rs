//! Runtime executable resolution use case.

use crate::features::runtime::ports::{PythonRuntimeResolver, RuntimeExecutableResolver};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::port::{
    RuntimeExecutableResolutionRequest, RuntimeExecutableResolutionResult,
    RuntimeExecutableResolutionUseCase, RuntimeExecutableTarget,
};

pub struct StdRuntimeExecutableResolutionUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    runtime_resolver: &'a dyn PythonRuntimeResolver,
    executable_resolver: &'a dyn RuntimeExecutableResolver,
}

impl<'a> StdRuntimeExecutableResolutionUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        runtime_resolver: &'a dyn PythonRuntimeResolver,
        executable_resolver: &'a dyn RuntimeExecutableResolver,
    ) -> Self {
        Self {
            layout_resolver,
            runtime_resolver,
            executable_resolver,
        }
    }
}

impl RuntimeExecutableResolutionUseCase for StdRuntimeExecutableResolutionUseCase<'_> {
    fn resolve_runtime_executable(
        &self,
        request: RuntimeExecutableResolutionRequest,
    ) -> KernelResult<RuntimeExecutableResolutionResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let runtime = self
            .runtime_resolver
            .resolve_python_runtime(&layout, request.runtime)?;
        let path = match request.target {
            RuntimeExecutableTarget::Python => {
                self.executable_resolver.python_binary_path(&runtime)?
            }
            RuntimeExecutableTarget::Entrypoint(entrypoint) => self
                .executable_resolver
                .entrypoint_path(&runtime, entrypoint)?,
        };

        Ok(RuntimeExecutableResolutionResult {
            layout,
            runtime,
            target: request.target,
            path,
        })
    }
}
