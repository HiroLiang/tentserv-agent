//! Runtime resolution use case.

use crate::features::runtime::ports::PythonRuntimeResolver;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::port::{RuntimeResolutionRequest, RuntimeResolutionResult, RuntimeResolutionUseCase};

pub struct StdRuntimeResolutionUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    runtime_resolver: &'a dyn PythonRuntimeResolver,
}

impl<'a> StdRuntimeResolutionUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        runtime_resolver: &'a dyn PythonRuntimeResolver,
    ) -> Self {
        Self {
            layout_resolver,
            runtime_resolver,
        }
    }
}

impl RuntimeResolutionUseCase for StdRuntimeResolutionUseCase<'_> {
    fn resolve_runtime(
        &self,
        request: RuntimeResolutionRequest,
    ) -> KernelResult<RuntimeResolutionResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let runtime = self
            .runtime_resolver
            .resolve_python_runtime(&layout, request.runtime)?;

        Ok(RuntimeResolutionResult { layout, runtime })
    }
}
