//! Capability manifest persistence boundary.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::domain::RuntimeLayout;

use super::manifest::MachineCapabilityManifest;

pub trait CapabilityManifestStore {
    fn load(&self) -> KernelResult<Option<MachineCapabilityManifest>>;
    fn save(&self, manifest: &MachineCapabilityManifest) -> KernelResult<()>;
}

#[derive(Debug, Clone)]
pub struct FileCapabilityManifestStore {
    path: PathBuf,
}

impl FileCapabilityManifestStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn from_layout(layout: &RuntimeLayout) -> Self {
        Self::new(layout.capability_manifest_path.clone())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl CapabilityManifestStore for FileCapabilityManifestStore {
    fn load(&self) -> KernelResult<Option<MachineCapabilityManifest>> {
        let body = match fs::read_to_string(&self.path) {
            Ok(body) => body,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                return Err(KernelError::CapabilityManifestUnavailable(format!(
                    "failed to read {}: {err}",
                    self.path.display()
                )));
            }
        };

        toml::from_str(&body).map(Some).map_err(|err| {
            KernelError::CapabilityManifestUnavailable(format!(
                "failed to parse {}: {err}",
                self.path.display()
            ))
        })
    }

    fn save(&self, manifest: &MachineCapabilityManifest) -> KernelResult<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                KernelError::CapabilityManifestUnavailable(format!(
                    "failed to create {}: {err}",
                    parent.display()
                ))
            })?;
        }

        let body = toml::to_string_pretty(manifest).map_err(|err| {
            KernelError::CapabilityManifestUnavailable(format!(
                "failed to serialize capability manifest: {err}"
            ))
        })?;

        let tmp_path = self.path.with_extension("toml.tmp");
        fs::write(&tmp_path, body).map_err(|err| {
            KernelError::CapabilityManifestUnavailable(format!(
                "failed to write {}: {err}",
                tmp_path.display()
            ))
        })?;

        if cfg!(windows) && self.path.exists() {
            fs::remove_file(&self.path).map_err(|err| {
                KernelError::CapabilityManifestUnavailable(format!(
                    "failed to replace {}: {err}",
                    self.path.display()
                ))
            })?;
        }

        fs::rename(&tmp_path, &self.path).map_err(|err| {
            KernelError::CapabilityManifestUnavailable(format!(
                "failed to move {} to {}: {err}",
                tmp_path.display(),
                self.path.display()
            ))
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::capabilities::domain::{
        BackendCapability, BackendKind, CapabilityState, RuntimeCapabilityState,
    };
    use crate::capabilities::manifest::{CapabilityManifestSchema, MachineCapabilityManifest};
    use crate::capabilities::store::{CapabilityManifestStore, FileCapabilityManifestStore};
    use crate::foundation::error::KernelError;
    use crate::foundation::layout::domain::RuntimeLayout;
    use crate::foundation::platform::domain::{
        Architecture, CpuFacts, GpuFacts, OperatingSystem, PlatformFacts,
    };

    #[test]
    fn missing_manifest_loads_none() {
        let root = temp_root("missing");
        let store = FileCapabilityManifestStore::new(root.join("runtime/capabilities.toml"));

        let output = store.load().expect("load missing manifest");

        assert_eq!(output, None);
        cleanup(root);
    }

    #[test]
    fn save_creates_parent_and_loads_manifest() {
        let root = preview_root();
        cleanup(root.clone());
        let path = root.join("runtime/capabilities.toml");
        let store = FileCapabilityManifestStore::new(path.clone());
        let manifest = manifest();

        store.save(&manifest).expect("save manifest");

        assert!(path.exists());
        assert_eq!(store.load().expect("load manifest"), Some(manifest));
    }

    #[test]
    fn from_layout_uses_capability_manifest_path() {
        let layout = runtime_layout("/tmp/tentgent-capabilities-store-layout");
        let store = FileCapabilityManifestStore::from_layout(&layout);

        assert_eq!(store.path(), layout.capability_manifest_path.as_path());
    }

    #[test]
    fn invalid_manifest_returns_parse_error() {
        let root = temp_root("invalid");
        let path = root.join("runtime/capabilities.toml");
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        fs::write(&path, "not = [valid").expect("write invalid toml");
        let store = FileCapabilityManifestStore::new(path);

        let err = store.load().expect_err("invalid manifest should fail");

        assert!(matches!(
            err,
            KernelError::CapabilityManifestUnavailable(message)
                if message.contains("failed to parse")
        ));
        cleanup(root);
    }

    fn manifest() -> MachineCapabilityManifest {
        MachineCapabilityManifest {
            schema: CapabilityManifestSchema {
                name: "tentgent.capabilities".to_string(),
                version: 1,
            },
            generated_at: Some("2026-05-14T00:00:00Z".to_string()),
            platform: PlatformFacts {
                os: OperatingSystem::Linux,
                arch: Architecture::X86_64,
                libc: None,
                cpu: CpuFacts {
                    vendor: Some("GenuineIntel".to_string()),
                    brand: Some("fixture cpu".to_string()),
                    features: vec!["avx2".to_string()],
                },
                gpu: GpuFacts {
                    devices: Vec::new(),
                    cuda: None,
                    metal: None,
                },
            },
            runtime: RuntimeCapabilityState {
                home_dir: PathBuf::from("/tmp/tentgent"),
                python_env_dir: PathBuf::from("/tmp/tentgent/runtime/python-env"),
                profiles: Vec::new(),
            },
            backends: vec![BackendCapability {
                backend: BackendKind::CpuGguf,
                state: CapabilityState::Unknown,
                message: Some("fixture".to_string()),
                next_step: Some("refresh capabilities".to_string()),
            }],
        }
    }

    fn runtime_layout(root: &str) -> RuntimeLayout {
        let home_dir = PathBuf::from(root);
        RuntimeLayout {
            config_path: home_dir.join("config.toml"),
            models_dir: home_dir.join("models"),
            adapters_dir: home_dir.join("adapters"),
            datasets_dir: home_dir.join("datasets"),
            sessions_dir: home_dir.join("sessions"),
            servers_dir: home_dir.join("servers"),
            train_dir: home_dir.join("train"),
            cache_dir: home_dir.join("cache"),
            runtime_dir: home_dir.join("runtime"),
            logs_dir: home_dir.join("logs"),
            locks_dir: home_dir.join("locks"),
            python_env_dir: home_dir.join("runtime/python-env"),
            bootstrap_dir: home_dir.join("runtime/bootstrap"),
            bootstrap_uv_dir: home_dir.join("runtime/bootstrap/uv"),
            bootstrap_uv_cache_dir: home_dir.join("runtime/bootstrap/uv-cache"),
            capability_manifest_path: home_dir.join("runtime/capabilities.toml"),
            home_dir,
        }
    }

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        workspace_root()
            .join("target/tentgent-kernel/capability-store-tests")
            .join(format!(
                "tentgent-kernel-capability-store-{name}-{nanos}-{}",
                std::process::id()
            ))
    }

    fn preview_root() -> PathBuf {
        workspace_root().join("target/tentgent-kernel/capability-store-preview")
    }

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("kernel crate should live under workspace src/")
            .to_path_buf()
    }

    fn cleanup(path: PathBuf) {
        let _ = fs::remove_dir_all(path);
    }
}
