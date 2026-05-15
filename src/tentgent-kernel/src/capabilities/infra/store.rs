//! File-backed capability state store.

use std::fs;
use std::io::ErrorKind;

use crate::capabilities::domain::MachineCapabilities;
use crate::capabilities::ports::CapabilityStateStore;
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayout;

#[derive(Debug, Clone, Copy, Default)]
pub struct FileCapabilityStateStore;

impl CapabilityStateStore for FileCapabilityStateStore {
    fn load(&self, layout: &RuntimeLayout) -> KernelResult<Option<MachineCapabilities>> {
        let path = &layout.capabilities_path;
        let raw = match fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                return Err(KernelError::CapabilityStateUnavailable(format!(
                    "failed to read `{}`: {err}",
                    path.display()
                )));
            }
        };

        toml::from_str(&raw).map(Some).map_err(|err| {
            KernelError::CapabilityStateUnavailable(format!(
                "failed to parse `{}`: {err}",
                path.display()
            ))
        })
    }

    fn save(&self, layout: &RuntimeLayout, capabilities: &MachineCapabilities) -> KernelResult<()> {
        let path = &layout.capabilities_path;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                KernelError::CapabilityStateUnavailable(format!(
                    "failed to create `{}`: {err}",
                    parent.display()
                ))
            })?;
        }

        let raw = toml::to_string_pretty(capabilities).map_err(|err| {
            KernelError::CapabilityStateUnavailable(format!(
                "failed to serialize capability state: {err}"
            ))
        })?;

        fs::write(path, raw).map_err(|err| {
            KernelError::CapabilityStateUnavailable(format!(
                "failed to write `{}`: {err}",
                path.display()
            ))
        })
    }
}
