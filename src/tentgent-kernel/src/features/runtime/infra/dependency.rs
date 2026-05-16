use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use crate::features::runtime::domain::BootstrapProfile;

use super::path::python_binary_path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PythonModuleProbe {
    Ready,
    Missing { modules: Vec<String> },
    Failed { detail: String },
}

pub(crate) fn python_binary_for_env(env_dir: &Path) -> PathBuf {
    python_binary_path(env_dir)
}

pub(crate) fn probe_python_modules(python_binary: &Path, modules: &[&str]) -> PythonModuleProbe {
    if modules.is_empty() {
        return PythonModuleProbe::Ready;
    }

    let report = cached_probe_report(python_binary, modules);
    report.for_modules(modules)
}

fn cached_probe_report(python_binary: &Path, requested: &[&str]) -> PythonModuleProbeReport {
    let key = python_binary
        .canonicalize()
        .unwrap_or_else(|_| python_binary.to_path_buf());
    let cache = MODULE_PROBE_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));

    if let Some(report) = cache.lock().expect("module probe cache poisoned").get(&key) {
        if report.covers(requested) {
            return report.clone();
        }
    }

    let modules = complete_probe_modules(requested);
    let report = run_python_module_probe(python_binary, &modules);
    cache
        .lock()
        .expect("module probe cache poisoned")
        .insert(key, report.clone());
    report
}

fn run_python_module_probe(python_binary: &Path, modules: &[String]) -> PythonModuleProbeReport {
    let output = Command::new(python_binary)
        .arg("-c")
        .arg(PYTHON_IMPORT_PROBE)
        .args(modules)
        .output();

    match output {
        Ok(output) if output.status.success() => PythonModuleProbeReport::ready(modules),
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let missing: BTreeMap<String, String> =
                stdout.lines().filter_map(parse_missing_module).collect();

            if missing.is_empty() {
                PythonModuleProbeReport::failed(modules, stderr.trim().to_string())
            } else {
                PythonModuleProbeReport {
                    probed: probed_modules(modules),
                    missing,
                    failed: None,
                }
            }
        }
        Err(err) => PythonModuleProbeReport::failed(modules, err.to_string()),
    }
}

fn complete_probe_modules(requested: &[&str]) -> Vec<String> {
    let mut modules = Vec::new();
    for module in local_model_modules()
        .into_iter()
        .chain(training_modules())
        .chain(requested.iter().copied())
    {
        if !modules.iter().any(|existing| existing == module) {
            modules.push(module.to_string());
        }
    }
    modules
}

fn parse_missing_module(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let module = line
        .split_once('\t')
        .map(|(module, _)| module)
        .or_else(|| line.split_once(' ').map(|(module, _)| module))
        .unwrap_or(line)
        .trim();
    (!module.is_empty()).then(|| (module.to_string(), line.to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PythonModuleProbeReport {
    probed: BTreeSet<String>,
    missing: BTreeMap<String, String>,
    failed: Option<String>,
}

impl PythonModuleProbeReport {
    fn ready(modules: &[String]) -> Self {
        Self {
            probed: probed_modules(modules),
            missing: BTreeMap::new(),
            failed: None,
        }
    }

    fn failed(modules: &[String], detail: String) -> Self {
        Self {
            probed: probed_modules(modules),
            missing: BTreeMap::new(),
            failed: Some(detail),
        }
    }

    fn covers(&self, modules: &[&str]) -> bool {
        modules.iter().all(|module| self.probed.contains(*module))
    }

    fn for_modules(&self, modules: &[&str]) -> PythonModuleProbe {
        let missing = modules
            .iter()
            .filter_map(|module| self.missing.get(*module).cloned())
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return PythonModuleProbe::Missing { modules: missing };
        }

        match &self.failed {
            Some(detail) => PythonModuleProbe::Failed {
                detail: detail.clone(),
            },
            None => PythonModuleProbe::Ready,
        }
    }
}

fn probed_modules(modules: &[String]) -> BTreeSet<String> {
    modules.iter().cloned().collect()
}

static MODULE_PROBE_CACHE: OnceLock<Mutex<BTreeMap<PathBuf, PythonModuleProbeReport>>> =
    OnceLock::new();

pub(crate) fn runtime_profile_modules(profile: BootstrapProfile) -> Vec<&'static str> {
    match profile {
        BootstrapProfile::Base => Vec::new(),
        BootstrapProfile::LocalModel => local_model_modules(),
        BootstrapProfile::Training => training_modules(),
        BootstrapProfile::Full => unique_modules(
            local_model_modules()
                .into_iter()
                .chain(training_modules())
                .collect(),
        ),
    }
}

fn local_model_modules() -> Vec<&'static str> {
    let mut modules = vec!["llama_cpp", "peft", "safetensors", "torch", "transformers"];
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        modules.push("mlx");
        modules.push("mlx_lm");
    }
    modules
}

pub(crate) fn training_modules() -> Vec<&'static str> {
    vec!["peft", "safetensors", "torch", "transformers"]
}

fn unique_modules(modules: Vec<&'static str>) -> Vec<&'static str> {
    let mut unique = Vec::new();
    for module in modules {
        if !unique.contains(&module) {
            unique.push(module);
        }
    }
    unique
}

const PYTHON_IMPORT_PROBE: &str = r#"
import importlib
import sys

missing = []
for name in sys.argv[1:]:
    try:
        importlib.import_module(name)
    except Exception as exc:
        missing.append(f"{name} ({exc.__class__.__name__}: {exc})")

if missing:
    print("\n".join(missing))
    raise SystemExit(1)
"#;
