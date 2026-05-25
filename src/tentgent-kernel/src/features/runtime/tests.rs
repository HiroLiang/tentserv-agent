use std::path::PathBuf;

use super::domain::{
    BootstrapProfile, BootstrapRuntimeInput, PythonRuntimeLayout, PythonRuntimeResolutionInput,
    PythonRuntimeSource, PythonRuntimeState, RuntimeBootstrapOutcome, RuntimeBootstrapPlan,
    RuntimeBootstrapStatus, RuntimeEntrypoint, RuntimeInitState, RuntimeProfileState,
    RuntimeReadiness,
};
use super::ports::{
    PythonRuntimeResolver, RuntimeBootstrapExecutor, RuntimeBootstrapPlanner,
    RuntimeExecutableResolver, RuntimeStateProbe,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;
use crate::foundation::platform::{
    Architecture, CpuFacts, GpuFacts, LibcFacts, OperatingSystem, PlatformFacts,
};

#[test]
fn bootstrap_profiles_use_cli_contract_names() {
    assert_eq!(BootstrapProfile::Base.as_str(), "base");
    assert_eq!(BootstrapProfile::LocalModel.as_str(), "local-model");
    assert_eq!(BootstrapProfile::Training.as_str(), "training");
    assert_eq!(BootstrapProfile::Full.as_str(), "full");
    assert_eq!(BootstrapProfile::default(), BootstrapProfile::Base);
}

#[test]
fn python_runtime_layout_derives_project_paths() {
    let layout = PythonRuntimeLayout {
        project_dir: PathBuf::from("/opt/tentgent/python"),
        env_dir: PathBuf::from("/var/tentgent/runtime/python-env"),
        source: PythonRuntimeSource::InstalledPrefix,
    };

    assert_eq!(
        layout.pyproject_path(),
        PathBuf::from("/opt/tentgent/python/pyproject.toml")
    );
    assert_eq!(
        layout.python_src_dir(),
        PathBuf::from("/opt/tentgent/python/src")
    );
}

#[test]
fn runtime_entrypoints_match_python_project_scripts() {
    assert_eq!(
        RuntimeEntrypoint::AudioSpeechOnce.script_name(),
        "tentgent-audio-speech"
    );
    assert_eq!(
        RuntimeEntrypoint::AudioTranscriptionBatch.script_name(),
        "tentgent-audio-transcribe"
    );
    assert_eq!(
        RuntimeEntrypoint::ChatOnce.script_name(),
        "tentgent-chat-once"
    );
    assert_eq!(
        RuntimeEntrypoint::DatasetEval.script_name(),
        "tentgent-dataset-eval"
    );
    assert_eq!(
        RuntimeEntrypoint::DatasetSynth.script_name(),
        "tentgent-dataset-synth"
    );
    assert_eq!(
        RuntimeEntrypoint::EmbeddingOnce.script_name(),
        "tentgent-embed-once"
    );
    assert_eq!(
        RuntimeEntrypoint::ImageGenerateOnce.script_name(),
        "tentgent-image-generate-once"
    );
    assert_eq!(
        RuntimeEntrypoint::ModelRuntimeDaemon.script_name(),
        "tentgent-model-runtime-daemon"
    );
    assert_eq!(
        RuntimeEntrypoint::RerankOnce.script_name(),
        "tentgent-rerank-once"
    );
    assert_eq!(
        RuntimeEntrypoint::HfSnapshot.script_name(),
        "tentgent-hf-snapshot"
    );
    assert_eq!(RuntimeEntrypoint::Server.script_name(), "tentgent-server");
    assert_eq!(
        RuntimeEntrypoint::TrainLoraRun.script_name(),
        "tentgent-train-lora-run"
    );
    assert_eq!(
        RuntimeEntrypoint::VisionChatOnce.script_name(),
        "tentgent-vision-chat-once"
    );
    assert_eq!(
        RuntimeEntrypoint::VideoUnderstandingOnce.script_name(),
        "tentgent-video-understanding"
    );
}

#[test]
fn python_runtime_resolution_input_defaults_to_no_overrides() {
    let input = PythonRuntimeResolutionInput::default();

    assert_eq!(input.project_dir, None);
    assert_eq!(input.python_env_dir, None);
}

#[test]
fn runtime_ports_cover_resolution_bootstrap_executables_and_state() {
    let ports = FakeRuntimePorts;
    let layout = runtime_layout("/tmp/tentgent-home");
    let runtime = ports
        .resolve_python_runtime(
            &layout,
            PythonRuntimeResolutionInput {
                project_dir: Some(PathBuf::from("/opt/tentgent/python")),
                python_env_dir: Some(PathBuf::from("/var/tentgent/python-env")),
            },
        )
        .expect("resolve runtime");

    assert_eq!(runtime.project_dir, PathBuf::from("/opt/tentgent/python"));
    assert_eq!(runtime.env_dir, PathBuf::from("/var/tentgent/python-env"));
    assert_eq!(
        ports
            .python_binary_path(&runtime)
            .expect("resolve python binary"),
        PathBuf::from("/var/tentgent/python-env/bin/python")
    );
    assert_eq!(
        ports
            .entrypoint_path(&runtime, RuntimeEntrypoint::Server)
            .expect("resolve server entrypoint"),
        PathBuf::from("/var/tentgent/python-env/bin/tentgent-server")
    );
    assert_eq!(
        ports
            .entrypoint_path(&runtime, RuntimeEntrypoint::ModelRuntimeDaemon)
            .expect("resolve model runtime daemon entrypoint"),
        PathBuf::from("/var/tentgent/python-env/bin/tentgent-model-runtime-daemon")
    );

    let plan = ports
        .plan_bootstrap(
            &layout,
            &runtime,
            &platform_facts(),
            BootstrapRuntimeInput {
                project_dir: Some(runtime.project_dir.clone()),
                python_env_dir: Some(runtime.env_dir.clone()),
                uv_path: Some(PathBuf::from("/tmp/uv")),
                profile: BootstrapProfile::Training,
                dry_run: true,
                print_plan: true,
            },
        )
        .expect("plan bootstrap");

    assert_eq!(plan.project_dir, runtime.project_dir);
    assert_eq!(plan.python_env_dir, runtime.env_dir);
    assert_eq!(plan.profile, BootstrapProfile::Training);
    assert!(plan.dry_run);
    assert!(plan.print_plan);

    let outcome = ports.execute_bootstrap(&plan).expect("execute bootstrap");

    assert_eq!(outcome.status, RuntimeBootstrapStatus::Succeeded);
    assert_eq!(outcome.exit_code, Some(0));

    let state = ports
        .probe_runtime_state(&layout, Some(&runtime))
        .expect("probe runtime state");

    assert_eq!(state.home_dir, layout.home_dir);
    assert_eq!(state.python_env_dir, runtime.env_dir);
    assert_eq!(state.profiles[0].readiness, RuntimeReadiness::Ready);
}

struct FakeRuntimePorts;

impl PythonRuntimeResolver for FakeRuntimePorts {
    fn resolve_python_runtime(
        &self,
        layout: &RuntimeLayout,
        input: PythonRuntimeResolutionInput,
    ) -> KernelResult<PythonRuntimeLayout> {
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

impl RuntimeExecutableResolver for FakeRuntimePorts {
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

impl RuntimeBootstrapPlanner for FakeRuntimePorts {
    fn plan_bootstrap(
        &self,
        layout: &RuntimeLayout,
        _runtime: &PythonRuntimeLayout,
        _platform: &PlatformFacts,
        input: BootstrapRuntimeInput,
    ) -> KernelResult<RuntimeBootstrapPlan> {
        let project_dir = input
            .project_dir
            .unwrap_or_else(|| layout.home_dir.join("python"));

        Ok(RuntimeBootstrapPlan {
            script_path: project_dir.join("../scripts/bootstrap-python-env.sh"),
            project_dir,
            python_env_dir: input
                .python_env_dir
                .unwrap_or_else(|| layout.python_env_dir.clone()),
            uv_path: input.uv_path,
            profile: input.profile,
            dry_run: input.dry_run,
            print_plan: input.print_plan,
        })
    }
}

impl RuntimeBootstrapExecutor for FakeRuntimePorts {
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

impl RuntimeStateProbe for FakeRuntimePorts {
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
                version: Some("3.13".to_string()),
            },
            profiles: vec![RuntimeProfileState {
                profile: BootstrapProfile::Base,
                readiness: RuntimeReadiness::Ready,
                message: None,
            }],
        })
    }
}

fn runtime_layout(root: &str) -> RuntimeLayout {
    let root = PathBuf::from(root);
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

fn platform_facts() -> PlatformFacts {
    PlatformFacts {
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
    }
}
