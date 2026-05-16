use std::path::PathBuf;

use crate::features::runtime::domain::{PythonRuntimeLayout, RuntimeEntrypoint};
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::foundation::error::KernelResult;

use super::path::{python_bin_dir, python_binary_path, python_script_name};

/// Resolves executable paths inside a selected Python runtime environment.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdRuntimeExecutableResolver;

impl RuntimeExecutableResolver for StdRuntimeExecutableResolver {
    fn python_binary_path(&self, runtime: &PythonRuntimeLayout) -> KernelResult<PathBuf> {
        Ok(python_binary_path(&runtime.env_dir))
    }

    fn entrypoint_path(
        &self,
        runtime: &PythonRuntimeLayout,
        entrypoint: RuntimeEntrypoint,
    ) -> KernelResult<PathBuf> {
        Ok(python_bin_dir(&runtime.env_dir).join(python_script_name(entrypoint.script_name())))
    }
}
