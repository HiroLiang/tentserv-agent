use std::path::PathBuf;

use super::domain::{
    BootstrapProfile, PythonRuntimeLayout, PythonRuntimeSource, RuntimeEntrypoint,
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
        RuntimeEntrypoint::HfSnapshot.script_name(),
        "tentgent-hf-snapshot"
    );
    assert_eq!(RuntimeEntrypoint::Server.script_name(), "tentgent-server");
    assert_eq!(
        RuntimeEntrypoint::TrainLoraRun.script_name(),
        "tentgent-train-lora-run"
    );
}
