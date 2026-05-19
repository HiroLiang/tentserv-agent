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
            let mut checks = Vec::new();
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
            checks.push(uv);

            let mut ffmpeg = command_probe.check_command(DoctorCommandCheck {
                name: "media decoder ffmpeg".to_string(),
                category: DoctorCheckCategory::Command,
                command: "ffmpeg".to_string(),
                args: vec!["-version".to_string()],
                missing_status: DoctorCheckStatus::Warn,
            })?;
            ffmpeg.detail = match ffmpeg.status {
                DoctorCheckStatus::Pass => format!(
                    "available for audio/video decoding; needed for MP3, M4A, AAC, Ogg, WebM, MP4, and most compressed media inputs: {}",
                    first_detail_line(&ffmpeg.detail)
                ),
                DoctorCheckStatus::Warn | DoctorCheckStatus::Fail | DoctorCheckStatus::Skipped => {
                    format!(
                        "needed for audio/video decoding before local audio-transcription jobs can read MP3, M4A, AAC, Ogg, WebM, MP4, and many other containers; {}; ensure ffmpeg is on PATH; probe: {}",
                        ffmpeg_install_hint(),
                        ffmpeg.detail
                    )
                }
            };
            checks.push(ffmpeg);

            Ok(checks)
        }
    }
}

fn first_detail_line(detail: &str) -> &str {
    detail.lines().next().unwrap_or(detail)
}

fn ffmpeg_install_hint() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macOS: brew install ffmpeg",
        "linux" => {
            "Linux: install ffmpeg with your distro package manager; Debian/Ubuntu: sudo apt install ffmpeg; Fedora: sudo dnf install ffmpeg; Arch: sudo pacman -S ffmpeg"
        }
        "windows" => {
            "Windows: winget install Gyan.FFmpeg or choco install ffmpeg, then restart the terminal"
        }
        _ => "install ffmpeg with your system package manager",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::doctor::ports::DoctorCommandProbe;

    struct FakeCommandProbe;

    impl DoctorCommandProbe for FakeCommandProbe {
        fn check_command(&self, request: DoctorCommandCheck) -> KernelResult<DoctorCheck> {
            let status = match request.command.as_str() {
                "uv" => DoctorCheckStatus::Pass,
                "ffmpeg" => DoctorCheckStatus::Warn,
                _ => request.missing_status,
            };
            Ok(DoctorCheck::with_status(
                request.category,
                request.name,
                status,
                format!("{} probe detail", request.command),
            ))
        }
    }

    #[test]
    fn command_checks_explain_media_decoder_dependency() {
        let checks = command_checks(
            &FakeCommandProbe,
            DoctorCommandCheckPolicy::IncludeDeveloperTools,
        )
        .expect("command checks");

        let ffmpeg = checks
            .iter()
            .find(|check| check.name == "media decoder ffmpeg")
            .expect("ffmpeg check");
        assert_eq!(ffmpeg.status, DoctorCheckStatus::Warn);
        assert!(ffmpeg.detail.contains("audio/video decoding"));
        assert!(ffmpeg.detail.contains(ffmpeg_install_hint()));
        assert!(ffmpeg.detail.contains("PATH"));
    }
}
