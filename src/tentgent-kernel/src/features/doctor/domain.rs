//! Doctor diagnostic report domain types.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DoctorExecutionMode {
    Observational,
    LocalCli,
}

impl DoctorExecutionMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Observational => "observational",
            Self::LocalCli => "local-cli",
        }
    }

    pub const fn allows_write_probes(self) -> bool {
        matches!(self, Self::LocalCli)
    }
}

impl Default for DoctorExecutionMode {
    fn default() -> Self {
        Self::Observational
    }
}

impl std::fmt::Display for DoctorExecutionMode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DoctorRepairIntent {
    ReportOnly,
    DeveloperPythonEnv,
}

impl DoctorRepairIntent {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReportOnly => "report-only",
            Self::DeveloperPythonEnv => "developer-python-env",
        }
    }

    pub const fn mutates_local_state(self) -> bool {
        matches!(self, Self::DeveloperPythonEnv)
    }
}

impl Default for DoctorRepairIntent {
    fn default() -> Self {
        Self::ReportOnly
    }
}

impl std::fmt::Display for DoctorRepairIntent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DoctorReportRequest {
    pub mode: DoctorExecutionMode,
    pub runtime_home: Option<PathBuf>,
    pub repair: DoctorRepairIntent,
}

impl DoctorReportRequest {
    pub fn observational() -> Self {
        Self::default()
    }

    pub fn local_cli() -> Self {
        Self {
            mode: DoctorExecutionMode::LocalCli,
            ..Self::default()
        }
    }

    pub fn with_runtime_home(mut self, runtime_home: PathBuf) -> Self {
        self.runtime_home = Some(runtime_home);
        self
    }

    pub fn with_repair(mut self, repair: DoctorRepairIntent) -> Self {
        self.repair = repair;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DoctorCheckStatus {
    Pass,
    Warn,
    Fail,
    Skipped,
}

impl DoctorCheckStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Fail => "fail",
            Self::Skipped => "skipped",
        }
    }
}

impl std::fmt::Display for DoctorCheckStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DoctorCheckCategory {
    Cli,
    Platform,
    RuntimeHome,
    Runtime,
    Bootstrap,
    Capability,
    Auth,
    Daemon,
    Command,
}

impl DoctorCheckCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Platform => "platform",
            Self::RuntimeHome => "runtime-home",
            Self::Runtime => "runtime",
            Self::Bootstrap => "bootstrap",
            Self::Capability => "capability",
            Self::Auth => "auth",
            Self::Daemon => "daemon",
            Self::Command => "command",
        }
    }
}

impl std::fmt::Display for DoctorCheckCategory {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub name: String,
    pub category: DoctorCheckCategory,
    pub status: DoctorCheckStatus,
    pub detail: String,
}

impl DoctorCheck {
    pub fn with_status(
        category: DoctorCheckCategory,
        name: impl Into<String>,
        status: DoctorCheckStatus,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            category,
            status,
            detail: detail.into(),
        }
    }

    pub fn pass(
        category: DoctorCheckCategory,
        name: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::with_status(category, name, DoctorCheckStatus::Pass, detail)
    }

    pub fn warn(
        category: DoctorCheckCategory,
        name: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::with_status(category, name, DoctorCheckStatus::Warn, detail)
    }

    pub fn fail(
        category: DoctorCheckCategory,
        name: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::with_status(category, name, DoctorCheckStatus::Fail, detail)
    }

    pub fn skipped(
        category: DoctorCheckCategory,
        name: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::with_status(category, name, DoctorCheckStatus::Skipped, detail)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DoctorSummary {
    pub pass: usize,
    pub warn: usize,
    pub fail: usize,
    pub skipped: usize,
}

impl DoctorSummary {
    pub fn from_checks(checks: &[DoctorCheck]) -> Self {
        let mut summary = Self::default();
        for check in checks {
            match check.status {
                DoctorCheckStatus::Pass => summary.pass += 1,
                DoctorCheckStatus::Warn => summary.warn += 1,
                DoctorCheckStatus::Fail => summary.fail += 1,
                DoctorCheckStatus::Skipped => summary.skipped += 1,
            }
        }
        summary
    }

    pub const fn overall_status(self) -> DoctorCheckStatus {
        if self.fail > 0 {
            DoctorCheckStatus::Fail
        } else if self.warn > 0 {
            DoctorCheckStatus::Warn
        } else {
            DoctorCheckStatus::Pass
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorReport {
    pub status: DoctorCheckStatus,
    pub summary: DoctorSummary,
    pub checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    pub fn from_checks(checks: Vec<DoctorCheck>) -> Self {
        let summary = DoctorSummary::from_checks(&checks);
        Self {
            status: summary.overall_status(),
            summary,
            checks,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorPathExpectation {
    RequiredDirectory,
    OptionalDirectory,
    RequiredFile,
    ExecutableFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorPathCheck {
    pub name: String,
    pub category: DoctorCheckCategory,
    pub path: PathBuf,
    pub expectation: DoctorPathExpectation,
    pub mode: DoctorExecutionMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorCommandCheck {
    pub name: String,
    pub category: DoctorCheckCategory,
    pub command: String,
    pub args: Vec<String>,
    pub missing_status: DoctorCheckStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorRepairPlan {
    pub intent: DoctorRepairIntent,
    pub mutates_local_state: bool,
    pub steps: Vec<DoctorRepairStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorRepairStep {
    pub label: String,
    pub command: Option<String>,
    pub detail: String,
}
