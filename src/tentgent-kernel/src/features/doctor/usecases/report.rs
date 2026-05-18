//! Doctor report use case.

use crate::capabilities::usecases::{MachineCapabilitiesInput, MachineCapabilitiesResolver};
use crate::features::doctor::domain::{
    DoctorCheck, DoctorCheckCategory, DoctorCheckStatus, DoctorCommandCheck, DoctorPathCheck,
    DoctorPathExpectation, DoctorReport,
};
use crate::features::doctor::ports::{
    DoctorCapabilityCheckMapper, DoctorCommandProbe, DoctorPathProbe, DoctorRuntimeCheckMapper,
};
use crate::features::runtime::usecases::{RuntimeStateRequest, RuntimeStateUseCase};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput};

use super::port::{
    DoctorCapabilityReadPolicy, DoctorCommandCheckPolicy, DoctorReportUseCase,
    DoctorReportUseCaseRequest, DoctorReportUseCaseResult,
};

/// Standard doctor report orchestration.
pub struct StdDoctorReportUseCase<'a> {
    runtime_state: &'a dyn RuntimeStateUseCase,
    capabilities: &'a dyn MachineCapabilitiesResolver,
    path_probe: &'a dyn DoctorPathProbe,
    command_probe: &'a dyn DoctorCommandProbe,
    runtime_mapper: &'a dyn DoctorRuntimeCheckMapper,
    capability_mapper: &'a dyn DoctorCapabilityCheckMapper,
}

impl<'a> StdDoctorReportUseCase<'a> {
    pub fn new(
        runtime_state: &'a dyn RuntimeStateUseCase,
        capabilities: &'a dyn MachineCapabilitiesResolver,
        path_probe: &'a dyn DoctorPathProbe,
        command_probe: &'a dyn DoctorCommandProbe,
        runtime_mapper: &'a dyn DoctorRuntimeCheckMapper,
        capability_mapper: &'a dyn DoctorCapabilityCheckMapper,
    ) -> Self {
        Self {
            runtime_state,
            capabilities,
            path_probe,
            command_probe,
            runtime_mapper,
            capability_mapper,
        }
    }
}

impl DoctorReportUseCase for StdDoctorReportUseCase<'_> {
    fn doctor_report(
        &self,
        request: DoctorReportUseCaseRequest,
    ) -> KernelResult<DoctorReportUseCaseResult> {
        let layout = layout_input(&request);
        let runtime = self.runtime_state.runtime_state(RuntimeStateRequest {
            layout: layout.clone(),
            runtime: request.runtime.clone(),
        })?;
        let capabilities_input = MachineCapabilitiesInput {
            layout,
            runtime: runtime.runtime.clone(),
        };
        let capability_snapshot = match request.capabilities {
            DoctorCapabilityReadPolicy::Current => self.capabilities.current(capabilities_input)?,
            DoctorCapabilityReadPolicy::Refresh => self.capabilities.refresh(capabilities_input)?,
        };

        let mut checks = Vec::new();
        checks.extend(path_checks(
            self.path_probe,
            &runtime.layout,
            request.doctor.mode,
        )?);
        checks.extend(self.runtime_mapper.runtime_checks(
            &runtime.layout,
            runtime.runtime.as_ref(),
            Some(&runtime.state),
            request.doctor.mode,
        )?);
        checks.extend(command_checks(self.command_probe, request.commands)?);
        checks.extend(self.capability_mapper.capability_checks(
            &capability_snapshot.platform,
            &capability_snapshot.capabilities,
        )?);

        Ok(DoctorReportUseCaseResult {
            report: DoctorReport::from_checks(checks),
        })
    }
}

fn layout_input(request: &DoctorReportUseCaseRequest) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode: LayoutResolveMode::ReadOnly,
        home_dir: request.doctor.runtime_home.clone(),
        data_root_dir: None,
    }
}

fn path_checks(
    path_probe: &dyn DoctorPathProbe,
    layout: &RuntimeLayout,
    mode: crate::features::doctor::domain::DoctorExecutionMode,
) -> KernelResult<Vec<DoctorCheck>> {
    let mut checks = Vec::new();
    for check in standard_path_checks(layout, mode) {
        checks.push(path_probe.check_path(check)?);
    }
    Ok(checks)
}

fn standard_path_checks(
    layout: &RuntimeLayout,
    mode: crate::features::doctor::domain::DoctorExecutionMode,
) -> Vec<DoctorPathCheck> {
    let mut checks = vec![DoctorPathCheck {
        name: "runtime home".to_string(),
        category: DoctorCheckCategory::RuntimeHome,
        path: layout.home_dir.clone(),
        expectation: DoctorPathExpectation::RequiredDirectory,
        mode,
    }];

    for (name, path) in [
        ("models", &layout.models_dir),
        ("servers", &layout.servers_dir),
        ("adapters", &layout.adapters_dir),
        ("datasets", &layout.datasets_dir),
        ("sessions", &layout.sessions_dir),
        ("train", &layout.train_dir),
        ("cache", &layout.cache_dir),
        ("runtime", &layout.runtime_dir),
        ("logs", &layout.logs_dir),
        ("locks", &layout.locks_dir),
    ] {
        checks.push(DoctorPathCheck {
            name: format!("dir {name}"),
            category: DoctorCheckCategory::RuntimeHome,
            path: path.clone(),
            expectation: DoctorPathExpectation::RequiredDirectory,
            mode,
        });
    }

    checks.push(DoctorPathCheck {
        name: "bootstrap cache".to_string(),
        category: DoctorCheckCategory::Bootstrap,
        path: layout.bootstrap_uv_cache_dir.clone(),
        expectation: DoctorPathExpectation::OptionalDirectory,
        mode,
    });
    checks
}

fn command_checks(
    command_probe: &dyn DoctorCommandProbe,
    policy: DoctorCommandCheckPolicy,
) -> KernelResult<Vec<DoctorCheck>> {
    match policy {
        DoctorCommandCheckPolicy::SkipOptional => Ok(Vec::new()),
        DoctorCommandCheckPolicy::IncludeDeveloperTools => {
            let mut uv = command_probe.check_command(DoctorCommandCheck {
                name: "uv dev bootstrap".to_string(),
                category: DoctorCheckCategory::Command,
                command: "uv".to_string(),
                args: vec!["--version".to_string()],
                missing_status: DoctorCheckStatus::Warn,
            })?;
            uv.detail = match uv.status {
                DoctorCheckStatus::Pass => {
                    format!("available for current developer bootstrap: {}", uv.detail)
                }
                DoctorCheckStatus::Warn | DoctorCheckStatus::Fail | DoctorCheckStatus::Skipped => {
                    format!(
                        "needed only by the current developer bootstrap; release installers must bundle or replace this step: {}",
                        uv.detail
                    )
                }
            };
            Ok(vec![uv])
        }
    }
}
