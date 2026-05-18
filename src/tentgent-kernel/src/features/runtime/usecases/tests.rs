use std::path::PathBuf;

use crate::features::runtime::domain::{
    BootstrapProfile, BootstrapRuntimeInput, PythonRuntimeLayout, PythonRuntimeResolutionInput,
    PythonRuntimeSource, PythonRuntimeState, RuntimeBootstrapOutcome, RuntimeBootstrapPlan,
    RuntimeBootstrapStatus, RuntimeEntrypoint, RuntimeInitState, RuntimeProfileState,
    RuntimeReadiness,
};
use crate::features::runtime::ports::{
    PythonRuntimeResolver, RuntimeBootstrapExecutor, RuntimeBootstrapPlanner,
    RuntimeExecutableResolver, RuntimeStateProbe,
};
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::{
    LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput, RuntimeLayoutResolver,
};
use crate::foundation::platform::{
    Architecture, CpuFacts, GpuFacts, LibcFacts, OperatingSystem, PlatformFacts, PlatformProbe,
};

use super::port::{
    RuntimeBootstrapRequest, RuntimeBootstrapUseCase, RuntimeExecutableResolutionRequest,
    RuntimeExecutableResolutionUseCase, RuntimeExecutableTarget, RuntimeResolutionRequest,
    RuntimeResolutionUseCase, RuntimeStateRequest, RuntimeStateUseCase,
};
use super::{
    StdRuntimeBootstrapUseCase, StdRuntimeExecutableResolutionUseCase, StdRuntimeResolutionUseCase,
    StdRuntimeStateUseCase,
};

#[test]
fn resolution_usecase_resolves_layout_then_runtime() {
    let layout_resolver = FakeLayoutResolver;
    let runtime_resolver = FakeRuntimeResolver { fail: false };
    let usecase = StdRuntimeResolutionUseCase::new(&layout_resolver, &runtime_resolver);

    let result = usecase
        .resolve_runtime(RuntimeResolutionRequest {
            layout: layout_input("/tmp/tentgent-resolution"),
            runtime: PythonRuntimeResolutionInput {
                project_dir: Some(PathBuf::from("/opt/tentgent/python")),
                python_env_dir: Some(PathBuf::from("/var/tentgent/python-env")),
            },
        })
        .expect("resolve runtime");

    assert_eq!(
        result.runtime.project_dir,
        PathBuf::from("/opt/tentgent/python")
    );
    assert_eq!(
        result.runtime.env_dir,
        PathBuf::from("/var/tentgent/python-env")
    );
}

#[test]
fn bootstrap_usecase_resolves_plans_and_executes_runtime_bootstrap() {
    let layout_resolver = FakeLayoutResolver;
    let platform_probe = FakePlatformProbe;
    let runtime_resolver = FakeRuntimeResolver { fail: false };
    let bootstrap_planner = FakeBootstrapPlanner;
    let bootstrap_executor = FakeBootstrapExecutor;
    let usecase = StdRuntimeBootstrapUseCase::new(
        &layout_resolver,
        &platform_probe,
        &runtime_resolver,
        &bootstrap_planner,
        &bootstrap_executor,
    );

    let result = usecase
        .bootstrap_runtime(RuntimeBootstrapRequest {
            layout: layout_input("/tmp/tentgent-bootstrap"),
            runtime: PythonRuntimeResolutionInput {
                project_dir: Some(PathBuf::from("/opt/tentgent/python")),
                python_env_dir: Some(PathBuf::from("/var/tentgent/python-env")),
            },
            bootstrap: BootstrapRuntimeInput {
                project_dir: None,
                python_env_dir: None,
                uv_path: Some(PathBuf::from("/tmp/uv")),
                profile: BootstrapProfile::Training,
                dry_run: true,
                print_plan: true,
            },
        })
        .expect("bootstrap runtime");

    assert_eq!(result.platform.os, OperatingSystem::Macos);
    assert_eq!(result.plan.profile, BootstrapProfile::Training);
    assert_eq!(result.plan.uv_path, Some(PathBuf::from("/tmp/uv")));
    assert_eq!(result.outcome.status, RuntimeBootstrapStatus::Succeeded);
    assert_eq!(result.outcome.exit_code, Some(0));
}

#[test]
fn state_usecase_probes_with_resolved_runtime() {
    let layout_resolver = FakeLayoutResolver;
    let runtime_resolver = FakeRuntimeResolver { fail: false };
    let state_probe = FakeStateProbe;
    let usecase = StdRuntimeStateUseCase::new(&layout_resolver, &runtime_resolver, &state_probe);

    let result = usecase
        .runtime_state(RuntimeStateRequest {
            layout: layout_input("/tmp/tentgent-state"),
            runtime: PythonRuntimeResolutionInput {
                project_dir: Some(PathBuf::from("/opt/tentgent/python")),
                python_env_dir: Some(PathBuf::from("/var/tentgent/python-env")),
            },
        })
        .expect("probe state");

    assert!(result.runtime.is_some());
    assert_eq!(
        result.state.python.binary_path,
        PathBuf::from("/var/tentgent/python-env/bin/python")
    );
    assert_eq!(result.state.profiles[0].readiness, RuntimeReadiness::Ready);
}

#[test]
fn state_usecase_allows_missing_runtime_when_no_runtime_override_was_requested() {
    let layout_resolver = FakeLayoutResolver;
    let runtime_resolver = FakeRuntimeResolver { fail: true };
    let state_probe = FakeStateProbe;
    let usecase = StdRuntimeStateUseCase::new(&layout_resolver, &runtime_resolver, &state_probe);

    let result = usecase
        .runtime_state(RuntimeStateRequest {
            layout: layout_input("/tmp/tentgent-state-missing"),
            runtime: PythonRuntimeResolutionInput::default(),
        })
        .expect("probe default state");

    assert_eq!(result.runtime, None);
    assert_eq!(
        result.state.python.binary_path,
        PathBuf::from("/tmp/tentgent-state-missing/runtime/python-env/bin/python")
    );
}

#[test]
fn executable_usecase_resolves_selected_entrypoint_path() {
    let layout_resolver = FakeLayoutResolver;
    let runtime_resolver = FakeRuntimeResolver { fail: false };
    let executable_resolver = FakeExecutableResolver;
    let usecase = StdRuntimeExecutableResolutionUseCase::new(
        &layout_resolver,
        &runtime_resolver,
        &executable_resolver,
    );

    let result = usecase
        .resolve_runtime_executable(RuntimeExecutableResolutionRequest {
            layout: layout_input("/tmp/tentgent-executable"),
            runtime: PythonRuntimeResolutionInput {
                project_dir: Some(PathBuf::from("/opt/tentgent/python")),
                python_env_dir: Some(PathBuf::from("/var/tentgent/python-env")),
            },
            target: RuntimeExecutableTarget::Entrypoint(RuntimeEntrypoint::Server),
        })
        .expect("resolve executable");

    assert_eq!(
        result.path,
        PathBuf::from("/var/tentgent/python-env/bin/tentgent-server")
    );
}

struct FakeLayoutResolver;

impl RuntimeLayoutResolver for FakeLayoutResolver {
    fn resolve(&self, input: RuntimeLayoutInput) -> KernelResult<RuntimeLayout> {
        Ok(runtime_layout(input.home_dir.expect("test layout home")))
    }
}

struct FakePlatformProbe;

impl PlatformProbe for FakePlatformProbe {
    fn probe(&self) -> KernelResult<PlatformFacts> {
        Ok(PlatformFacts {
            os: OperatingSystem::Macos,
            arch: Architecture::Aarch64,
            libc: None::<LibcFacts>,
            cpu: CpuFacts {
                vendor: None,
                brand: None,
                features: Vec::new(),
            },
            gpu: GpuFacts {
                devices: Vec::new(),
                cuda: None,
                metal: None,
            },
        })
    }
}

struct FakeRuntimeResolver {
    fail: bool,
}

impl PythonRuntimeResolver for FakeRuntimeResolver {
    fn resolve_python_runtime(
        &self,
        layout: &RuntimeLayout,
        input: PythonRuntimeResolutionInput,
    ) -> KernelResult<PythonRuntimeLayout> {
        if self.fail {
            return Err(KernelError::RuntimeStateUnavailable(
                "runtime unavailable".to_string(),
            ));
        }

        Ok(PythonRuntimeLayout {
            project_dir: input
                .project_dir
                .unwrap_or_else(|| layout.home_dir.join("python")),
            env_dir: input
                .python_env_dir
                .unwrap_or_else(|| layout.python_env_dir.clone()),
            source: PythonRuntimeSource::EnvironmentOverride,
        })
    }
}

struct FakeBootstrapPlanner;

impl RuntimeBootstrapPlanner for FakeBootstrapPlanner {
    fn plan_bootstrap(
        &self,
        _layout: &RuntimeLayout,
        runtime: &PythonRuntimeLayout,
        _platform: &PlatformFacts,
        input: BootstrapRuntimeInput,
    ) -> KernelResult<RuntimeBootstrapPlan> {
        Ok(RuntimeBootstrapPlan {
            project_dir: input
                .project_dir
                .unwrap_or_else(|| runtime.project_dir.clone()),
            python_env_dir: input
                .python_env_dir
                .unwrap_or_else(|| runtime.env_dir.clone()),
            script_path: runtime
                .project_dir
                .join("../scripts/bootstrap-python-env.sh"),
            uv_path: input.uv_path,
            profile: input.profile,
            dry_run: input.dry_run,
            print_plan: input.print_plan,
        })
    }
}

struct FakeBootstrapExecutor;

impl RuntimeBootstrapExecutor for FakeBootstrapExecutor {
    fn execute_bootstrap(
        &self,
        _plan: &RuntimeBootstrapPlan,
    ) -> KernelResult<RuntimeBootstrapOutcome> {
        Ok(RuntimeBootstrapOutcome {
            status: RuntimeBootstrapStatus::Succeeded,
            exit_code: Some(0),
        })
    }
}

struct FakeStateProbe;

impl RuntimeStateProbe for FakeStateProbe {
    fn probe_runtime_state(
        &self,
        layout: &RuntimeLayout,
        runtime: Option<&PythonRuntimeLayout>,
    ) -> KernelResult<RuntimeInitState> {
        let python_env_dir = runtime
            .map(|runtime| runtime.env_dir.clone())
            .unwrap_or_else(|| layout.python_env_dir.clone());

        Ok(RuntimeInitState {
            home_dir: layout.home_dir.clone(),
            python_env_dir: python_env_dir.clone(),
            bootstrap_dir: layout.bootstrap_dir.clone(),
            uv_cache_dir: layout.bootstrap_uv_cache_dir.clone(),
            python: PythonRuntimeState {
                env_exists: true,
                binary_path: python_env_dir.join("bin/python"),
                version: Some("Python 3.13".to_string()),
            },
            profiles: vec![RuntimeProfileState {
                profile: BootstrapProfile::Base,
                readiness: RuntimeReadiness::Ready,
                message: None,
            }],
        })
    }
}

struct FakeExecutableResolver;

impl RuntimeExecutableResolver for FakeExecutableResolver {
    fn python_binary_path(&self, runtime: &PythonRuntimeLayout) -> KernelResult<PathBuf> {
        Ok(runtime.env_dir.join("bin/python"))
    }

    fn entrypoint_path(
        &self,
        runtime: &PythonRuntimeLayout,
        entrypoint: RuntimeEntrypoint,
    ) -> KernelResult<PathBuf> {
        Ok(runtime.env_dir.join("bin").join(entrypoint.script_name()))
    }
}

fn layout_input(root: &str) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode: LayoutResolveMode::ReadOnly,
        home_dir: Some(PathBuf::from(root)),
        data_root_dir: None,
    }
}

fn runtime_layout(root: PathBuf) -> RuntimeLayout {
    RuntimeLayout {
        home_dir: root.clone(),
        data_root_dir: root.clone(),
        config_path: root.join("config.toml"),
        models_dir: root.join("models"),
        adapters_dir: root.join("adapters"),
        datasets_dir: root.join("datasets"),
        sessions_dir: root.join("sessions"),
        servers_dir: root.join("servers"),
        train_dir: root.join("train"),
        cache_dir: root.join("cache"),
        runtime_dir: root.join("runtime"),
        logs_dir: root.join("logs"),
        locks_dir: root.join("locks"),
        python_env_dir: root.join("runtime/python-env"),
        bootstrap_dir: root.join("runtime/bootstrap"),
        bootstrap_uv_dir: root.join("runtime/bootstrap/uv"),
        bootstrap_uv_cache_dir: root.join("runtime/bootstrap/uv-cache"),
        capabilities_path: root.join("runtime/capabilities.toml"),
        auth_metadata_path: root.join("runtime/auth.toml"),
    }
}
