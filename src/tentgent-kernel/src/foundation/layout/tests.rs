use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use super::domain::{LayoutResolveMode, RuntimeLayoutInput};
use super::infra::{StdRuntimeLayoutResolver, DATA_ROOT_ENV, HOME_ENV};
use super::ports::RuntimeLayoutResolver;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn read_only_explicit_roots_do_not_create_dirs() {
    let home = temp_path("read-only-home");
    let data = temp_path("read-only-data");

    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::ReadOnly,
            home_dir: Some(home.clone()),
            data_root_dir: Some(data.clone()),
        })
        .expect("resolve layout");

    assert_eq!(layout.home_dir, home);
    assert_eq!(layout.data_root_dir, data);
    assert!(!layout.home_dir.exists());
    assert!(!layout.data_root_dir.exists());
}

#[test]
fn explicit_data_root_separates_control_and_data_paths() {
    let home = temp_path("split-home");
    let data = temp_path("split-data");

    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::ReadOnly,
            home_dir: Some(home.clone()),
            data_root_dir: Some(data.clone()),
        })
        .expect("resolve layout");

    assert_eq!(layout.config_path, home.join("config.toml"));
    assert_eq!(layout.sessions_dir, home.join("sessions"));
    assert_eq!(layout.servers_dir, home.join("servers"));
    assert_eq!(layout.runtime_dir, home.join("runtime"));
    assert_eq!(layout.auth_metadata_path, home.join("runtime/auth.toml"));
    assert_eq!(
        layout.capabilities_path,
        home.join("runtime/capabilities.toml")
    );
    assert_eq!(layout.logs_dir, home.join("logs"));
    assert_eq!(layout.locks_dir, home.join("locks"));

    assert_eq!(layout.models_dir, data.join("models"));
    assert_eq!(layout.adapters_dir, data.join("adapters"));
    assert_eq!(layout.datasets_dir, data.join("datasets"));
    assert_eq!(layout.train_dir, data.join("train"));
    assert_eq!(layout.cache_dir, data.join("cache"));
    assert_eq!(
        layout.bootstrap_uv_cache_dir,
        home.join("runtime/bootstrap/uv-cache")
    );
}

#[test]
fn data_root_defaults_to_home() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let _data_root_env = EnvVarGuard::clear(DATA_ROOT_ENV);
    let home = temp_path("default-data-root-home");

    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::ReadOnly,
            home_dir: Some(home.clone()),
            data_root_dir: None,
        })
        .expect("resolve layout");

    assert_eq!(layout.data_root_dir, home);
}

#[test]
fn env_roots_are_used_when_input_is_empty() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let home = temp_path("env-home");
    let data = temp_path("env-data");
    let _home_env = EnvVarGuard::set(HOME_ENV, &home);
    let _data_root_env = EnvVarGuard::set(DATA_ROOT_ENV, &data);

    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::ReadOnly,
            home_dir: None,
            data_root_dir: None,
        })
        .expect("resolve layout");

    assert_eq!(layout.home_dir, home);
    assert_eq!(layout.data_root_dir, data);
}

#[test]
fn create_mode_creates_standard_dirs_without_creating_python_env() {
    let home = temp_path("create-home");
    let data = temp_path("create-data");

    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: Some(home),
            data_root_dir: Some(data),
        })
        .expect("resolve layout");

    for dir in [
        &layout.home_dir,
        &layout.data_root_dir,
        &layout.models_dir,
        &layout.adapters_dir,
        &layout.datasets_dir,
        &layout.sessions_dir,
        &layout.servers_dir,
        &layout.train_dir,
        &layout.cache_dir,
        &layout.runtime_dir,
        &layout.logs_dir,
        &layout.locks_dir,
        &layout.bootstrap_dir,
        &layout.bootstrap_uv_dir,
        &layout.bootstrap_uv_cache_dir,
    ] {
        assert!(dir.is_dir(), "expected directory: {}", dir.display());
    }

    assert!(!layout.config_path.exists());
    assert!(!layout.capabilities_path.exists());
    assert!(!layout.auth_metadata_path.exists());
    assert!(!layout.python_env_dir.exists());
}

fn temp_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "tentgent-kernel-layout-{label}-{}-{nanos}",
        std::process::id()
    ))
}

struct EnvVarGuard {
    name: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn set(name: &'static str, value: &Path) -> Self {
        let guard = Self {
            name,
            previous: std::env::var_os(name),
        };
        std::env::set_var(name, value);
        guard
    }

    fn clear(name: &'static str) -> Self {
        let guard = Self {
            name,
            previous: std::env::var_os(name),
        };
        std::env::remove_var(name);
        guard
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var(self.name, value),
            None => std::env::remove_var(self.name),
        }
    }
}
