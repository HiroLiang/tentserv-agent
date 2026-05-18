//! Standard daemon status and lifecycle orchestration.

use std::{thread, time::Instant};

use crate::features::daemon::domain::{DaemonBind, DaemonProcessMetadata};
use crate::features::daemon::ports::{
    DaemonBindSafetyChecker, DaemonBindSafetyRequest, DaemonClock, DaemonDetachedCommand,
    DaemonDetachedLauncher, DaemonHttpReadinessProbe, DaemonProcessController, DaemonProcessProbe,
    DaemonStateStore, DaemonStoreLayoutInitializer,
};
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayoutResolver;
use crate::foundation::layout::{LayoutResolveMode, RuntimeLayout};

use super::common::{daemon_store_layout, inspect_daemon};
use super::port::{
    DaemonClearProcessRequest, DaemonDetachedStartRequest, DaemonDetachedStartResult,
    DaemonDetachedStartUseCase, DaemonInspectionMode, DaemonLifecycleUseCase,
    DaemonPrepareRunRequest, DaemonPrepareRunResult, DaemonRecordProcessStartRequest,
    DaemonStatusRequest, DaemonStatusResult, DaemonStatusUseCase, DaemonStopRequest,
    DaemonStopResult, DaemonUseCaseFuture,
};

const DAEMON_READINESS_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

/// Standard orchestration for daemon process metadata and lifecycle.
pub struct StdDaemonUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    layout_initializer: &'a dyn DaemonStoreLayoutInitializer,
    state_store: &'a dyn DaemonStateStore,
    process_probe: &'a dyn DaemonProcessProbe,
    process_controller: &'a dyn DaemonProcessController,
    bind_safety_checker: &'a dyn DaemonBindSafetyChecker,
    detached_launcher: &'a dyn DaemonDetachedLauncher,
    readiness_probe: &'a dyn DaemonHttpReadinessProbe,
    clock: &'a dyn DaemonClock,
}

impl<'a> StdDaemonUseCase<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        layout_initializer: &'a dyn DaemonStoreLayoutInitializer,
        state_store: &'a dyn DaemonStateStore,
        process_probe: &'a dyn DaemonProcessProbe,
        process_controller: &'a dyn DaemonProcessController,
        bind_safety_checker: &'a dyn DaemonBindSafetyChecker,
        detached_launcher: &'a dyn DaemonDetachedLauncher,
        readiness_probe: &'a dyn DaemonHttpReadinessProbe,
        clock: &'a dyn DaemonClock,
    ) -> Self {
        Self {
            layout_resolver,
            layout_initializer,
            state_store,
            process_probe,
            process_controller,
            bind_safety_checker,
            detached_launcher,
            readiness_probe,
            clock,
        }
    }

    fn status_for(
        &self,
        layout: RuntimeLayout,
        mode: DaemonInspectionMode,
    ) -> KernelResult<DaemonStatusResult> {
        let store = daemon_store_layout(&layout);
        let inspection = inspect_daemon(&store, self.state_store, self.process_probe, mode)?;
        Ok(DaemonStatusResult {
            layout,
            store,
            inspection,
        })
    }

    fn prepare_run_for_layout(
        &self,
        layout: RuntimeLayout,
        host: Option<&str>,
        port: Option<u16>,
        token_enabled: bool,
        allow_unsafe_bind: bool,
    ) -> KernelResult<DaemonPrepareRunResult> {
        let store = daemon_store_layout(&layout);
        self.layout_initializer.ensure_daemon_store_layout(&store)?;
        let inspection = inspect_daemon(
            &store,
            self.state_store,
            self.process_probe,
            DaemonInspectionMode::CleanupStale,
        )?;
        if let Some(process) = &inspection.process {
            if inspection.running {
                return Err(KernelError::DaemonRuntimeUnavailable(format!(
                    "daemon is already running as pid {}",
                    process.pid
                )));
            }
        }

        let bind = DaemonBind::from_optional(host, port)
            .map_err(|err| KernelError::DaemonRuntimeUnavailable(err.to_string()))?;
        let bind_safety = self
            .bind_safety_checker
            .check_bind_safety(DaemonBindSafetyRequest {
                bind: bind.clone(),
                token_enabled,
                allow_unsafe_bind,
            })?;

        Ok(DaemonPrepareRunResult {
            layout,
            store,
            bind,
            inspection,
            bind_warnings: bind_safety.warnings,
        })
    }
}

impl DaemonStatusUseCase for StdDaemonUseCase<'_> {
    fn daemon_status(&self, request: DaemonStatusRequest) -> KernelResult<DaemonStatusResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        self.status_for(layout, request.mode)
    }
}

impl DaemonLifecycleUseCase for StdDaemonUseCase<'_> {
    fn prepare_run(
        &self,
        request: DaemonPrepareRunRequest,
    ) -> KernelResult<DaemonPrepareRunResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        self.prepare_run_for_layout(
            layout,
            request.host.as_deref(),
            request.port,
            request.token_enabled,
            request.allow_unsafe_bind,
        )
    }

    fn record_process_start(
        &self,
        request: DaemonRecordProcessStartRequest,
    ) -> KernelResult<DaemonStatusResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = daemon_store_layout(&layout);
        self.layout_initializer.ensure_daemon_store_layout(&store)?;
        let inspection = inspect_daemon(
            &store,
            self.state_store,
            self.process_probe,
            DaemonInspectionMode::CleanupStale,
        )?;
        if let Some(process) = &inspection.process {
            if inspection.running {
                return Err(KernelError::DaemonRuntimeUnavailable(format!(
                    "daemon is already running as pid {}",
                    process.pid
                )));
            }
        }

        self.state_store.record_process_start(
            &store,
            &DaemonProcessMetadata {
                pid: request.pid,
                host: request.bind.host,
                port: request.bind.port,
                started_at: self.clock.now_rfc3339()?,
            },
        )?;

        self.status_for(layout, DaemonInspectionMode::CleanupStale)
    }

    fn clear_process_if_matches(&self, request: DaemonClearProcessRequest) -> KernelResult<()> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = daemon_store_layout(&layout);
        self.state_store
            .clear_process_if_matches(&store, request.expected_pid)
    }

    fn stop_daemon(&self, request: DaemonStopRequest) -> KernelResult<DaemonStopResult> {
        let status = self.daemon_status(DaemonStatusRequest {
            layout: request.layout,
            mode: DaemonInspectionMode::CleanupStale,
        })?;
        let process =
            status.inspection.process.clone().ok_or_else(|| {
                KernelError::DaemonRuntimeUnavailable("daemon is not running".into())
            })?;
        if !status.inspection.running {
            return Err(KernelError::DaemonRuntimeUnavailable(
                "daemon is not running".into(),
            ));
        }

        self.process_controller.terminate_process(process.pid)?;
        self.state_store
            .clear_process_if_matches(&status.store, Some(process.pid))?;
        let next = self.status_for(status.layout, DaemonInspectionMode::CleanupStale)?;

        Ok(DaemonStopResult {
            layout: next.layout,
            store: next.store,
            inspection: next.inspection,
            stopped_pid: process.pid,
        })
    }
}

impl DaemonDetachedStartUseCase for StdDaemonUseCase<'_> {
    fn start_daemon_detached<'a>(
        &'_ self,
        request: DaemonDetachedStartRequest,
    ) -> DaemonUseCaseFuture<'_, DaemonDetachedStartResult> {
        Box::pin(async move {
            let mut layout_input = request.layout;
            layout_input.mode = LayoutResolveMode::Create;
            let layout = self.layout_resolver.resolve(layout_input)?;
            let initial = self.status_for(layout.clone(), DaemonInspectionMode::CleanupStale)?;
            if initial.inspection.running {
                let daemon_url = initial.inspection.daemon_url();
                self.readiness_probe.probe_healthz(&daemon_url).await?;
                return Ok(DaemonDetachedStartResult {
                    stdout_log_path: initial.inspection.stdout_log_path.clone(),
                    stderr_log_path: initial.inspection.stderr_log_path.clone(),
                    layout: initial.layout,
                    store: initial.store,
                    inspection: initial.inspection,
                    daemon_url,
                    launched_pid: None,
                    status_warning: None,
                    bind_warnings: Vec::new(),
                    already_running: true,
                });
            }

            let prepared = self.prepare_run_for_layout(
                layout,
                request.host.as_deref(),
                request.port,
                request.token_enabled,
                request.allow_unsafe_bind,
            )?;
            let command =
                detached_child_command(request.executable, &prepared, request.allow_unsafe_bind);
            let launched_pid = self.detached_launcher.launch_detached(&command)?;
            let expected_url = prepared.bind.daemon_url();
            let readiness = self
                .wait_for_readiness(
                    prepared.layout.clone(),
                    &expected_url,
                    request
                        .status_probe_token
                        .as_ref()
                        .map(|token| token.as_str()),
                    request.startup_timeout,
                )
                .await?;

            Ok(DaemonDetachedStartResult {
                daemon_url: readiness.inspection.daemon_url(),
                layout: readiness.layout,
                store: readiness.store,
                inspection: readiness.inspection,
                launched_pid: Some(launched_pid),
                stdout_log_path: command.stdout_log_path,
                stderr_log_path: command.stderr_log_path,
                status_warning: readiness.status_warning,
                bind_warnings: prepared.bind_warnings,
                already_running: false,
            })
        })
    }
}

impl StdDaemonUseCase<'_> {
    async fn wait_for_readiness(
        &self,
        layout: RuntimeLayout,
        expected_url: &str,
        token: Option<&str>,
        timeout: std::time::Duration,
    ) -> KernelResult<DaemonReadiness> {
        let deadline = Instant::now() + timeout;
        let mut last_error = None;

        loop {
            let status = self.status_for(layout.clone(), DaemonInspectionMode::CleanupStale)?;
            if let Some(process) = status.inspection.process.as_ref() {
                if status.inspection.running {
                    let daemon_url = process.daemon_url();
                    match self.readiness_probe.probe_healthz(&daemon_url).await {
                        Ok(()) => {
                            let status_warning = if let Some(token) = token {
                                Some(
                                    self.readiness_probe
                                        .probe_status(&daemon_url, token)
                                        .await?
                                        .status_warning,
                                )
                                .flatten()
                            } else {
                                None
                            };
                            return Ok(DaemonReadiness {
                                layout: status.layout,
                                store: status.store,
                                inspection: status.inspection,
                                status_warning,
                            });
                        }
                        Err(error) => {
                            last_error = Some(error.to_string());
                        }
                    }
                }
            }

            if Instant::now() >= deadline {
                let detail = last_error
                    .map(|error| format!("; last health probe: {error}"))
                    .unwrap_or_default();
                return Err(KernelError::DaemonRuntimeUnavailable(format!(
                    "timed out waiting for daemon readiness at {expected_url}{detail}"
                )));
            }

            thread::sleep(DAEMON_READINESS_POLL_INTERVAL);
        }
    }
}

struct DaemonReadiness {
    layout: RuntimeLayout,
    store: crate::features::daemon::domain::DaemonStoreLayout,
    inspection: crate::features::daemon::domain::DaemonInspection,
    status_warning: Option<String>,
}

fn detached_child_command(
    executable: std::path::PathBuf,
    prepared: &DaemonPrepareRunResult,
    allow_unsafe_bind: bool,
) -> DaemonDetachedCommand {
    let mut args = vec![
        "daemon".to_string(),
        "run".to_string(),
        "--home".to_string(),
        prepared.layout.home_dir.display().to_string(),
        "--host".to_string(),
        prepared.bind.host.clone(),
        "--port".to_string(),
        prepared.bind.port.to_string(),
    ];
    if allow_unsafe_bind {
        args.push("--allow-unsafe-bind".to_string());
    }

    DaemonDetachedCommand {
        executable,
        args,
        stdout_log_path: prepared.store.stdout_log_path(),
        stderr_log_path: prepared.store.stderr_log_path(),
    }
}
