use std::{path::PathBuf, time::Duration};

use crate::features::daemon::usecases::StdDaemonUseCase;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver};

use super::{
    FileDaemonStateStore, ReqwestDaemonHttpReadinessProbe, StdDaemonBindSafetyChecker,
    StdDaemonDetachedLauncher, StdDaemonProcessController, StdDaemonProcessProbe,
    StdDaemonStoreLayoutInitializer, SystemDaemonClock,
};

/// Owned standard daemon adapter bundle for CLI and HTTP entrypoints.
pub struct StdDaemonKernel {
    layout_resolver: StdRuntimeLayoutResolver,
    layout_initializer: StdDaemonStoreLayoutInitializer,
    state_store: FileDaemonStateStore,
    process_probe: StdDaemonProcessProbe,
    process_controller: StdDaemonProcessController,
    bind_safety_checker: StdDaemonBindSafetyChecker,
    detached_launcher: StdDaemonDetachedLauncher,
    readiness_probe: ReqwestDaemonHttpReadinessProbe,
    clock: SystemDaemonClock,
}

impl StdDaemonKernel {
    pub fn new(probe_timeout: Duration) -> KernelResult<Self> {
        Ok(Self {
            layout_resolver: StdRuntimeLayoutResolver,
            layout_initializer: StdDaemonStoreLayoutInitializer,
            state_store: FileDaemonStateStore,
            process_probe: StdDaemonProcessProbe,
            process_controller: StdDaemonProcessController::default(),
            bind_safety_checker: StdDaemonBindSafetyChecker,
            detached_launcher: StdDaemonDetachedLauncher,
            readiness_probe: ReqwestDaemonHttpReadinessProbe::new(probe_timeout)?,
            clock: SystemDaemonClock,
        })
    }

    pub fn usecase(&self) -> StdDaemonUseCase<'_> {
        StdDaemonUseCase::new(
            &self.layout_resolver,
            &self.layout_initializer,
            &self.state_store,
            &self.process_probe,
            &self.process_controller,
            &self.bind_safety_checker,
            &self.detached_launcher,
            &self.readiness_probe,
            &self.clock,
        )
    }
}

pub fn daemon_runtime_layout_input(
    home: Option<PathBuf>,
    mode: LayoutResolveMode,
) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode,
        home_dir: home,
        data_root_dir: None,
    }
}
