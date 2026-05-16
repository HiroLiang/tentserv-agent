//! Runtime state use case.

use crate::features::runtime::ports::{PythonRuntimeResolver, RuntimeStateProbe};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::port::{RuntimeStateRequest, RuntimeStateResult, RuntimeStateUseCase};

pub struct StdRuntimeStateUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    runtime_resolver: &'a dyn PythonRuntimeResolver,
    state_probe: &'a dyn RuntimeStateProbe,
}

impl<'a> StdRuntimeStateUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        runtime_resolver: &'a dyn PythonRuntimeResolver,
        state_probe: &'a dyn RuntimeStateProbe,
    ) -> Self {
        Self {
            layout_resolver,
            runtime_resolver,
            state_probe,
        }
    }
}

impl RuntimeStateUseCase for StdRuntimeStateUseCase<'_> {
    fn runtime_state(&self, request: RuntimeStateRequest) -> KernelResult<RuntimeStateResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let runtime_input = request.runtime;
        let allow_missing_runtime =
            runtime_input.project_dir.is_none() && runtime_input.python_env_dir.is_none();
        let runtime = match self
            .runtime_resolver
            .resolve_python_runtime(&layout, runtime_input)
        {
            Ok(runtime) => Some(runtime),
            Err(_) if allow_missing_runtime => None,
            Err(err) => return Err(err),
        };
        let state = self
            .state_probe
            .probe_runtime_state(&layout, runtime.as_ref())?;

        Ok(RuntimeStateResult {
            layout,
            runtime,
            state,
        })
    }
}
