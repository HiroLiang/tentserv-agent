use crate::model::ModelFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeBackend {
    Mlx,
    TransformersPeft,
    LlamaCpp,
}

impl RuntimeBackend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mlx => "mlx",
            Self::TransformersPeft => "transformers-peft",
            Self::LlamaCpp => "llama-cpp",
        }
    }

    pub const fn from_model_format(format: ModelFormat) -> Self {
        match format {
            ModelFormat::Mlx => Self::Mlx,
            ModelFormat::Safetensors => Self::TransformersPeft,
            ModelFormat::Gguf => Self::LlamaCpp,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendCapabilityState {
    Enabled,
    DependencyGated,
    Unsupported,
}

impl BackendCapabilityState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::DependencyGated => "dependency-gated",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BackendCapability {
    pub backend: RuntimeBackend,
    pub state: BackendCapabilityState,
    pub reason: String,
}

impl BackendCapability {
    pub fn is_blocked(&self) -> bool {
        self.state == BackendCapabilityState::Unsupported
    }

    pub fn summary(&self) -> String {
        format!("{}: {}", self.state.as_str(), self.reason)
    }
}

#[derive(Debug, Clone)]
pub struct PlatformInfo {
    pub os: &'static str,
    pub arch: &'static str,
}

impl PlatformInfo {
    pub const fn current() -> Self {
        Self {
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
        }
    }

    pub fn label(&self) -> String {
        format!("{}-{}", self.os, self.arch)
    }

    fn is_apple_silicon_macos(&self) -> bool {
        self.os == "macos" && self.arch == "aarch64"
    }
}

pub fn current_backend_capabilities() -> Vec<BackendCapability> {
    let platform = PlatformInfo::current();
    [
        RuntimeBackend::Mlx,
        RuntimeBackend::TransformersPeft,
        RuntimeBackend::LlamaCpp,
    ]
    .into_iter()
    .map(|backend| backend_capability_on(backend, &platform))
    .collect()
}

pub fn backend_capability(backend: RuntimeBackend) -> BackendCapability {
    backend_capability_on(backend, &PlatformInfo::current())
}

pub fn model_format_capability(format: ModelFormat) -> BackendCapability {
    backend_capability(RuntimeBackend::from_model_format(format))
}

pub fn ensure_model_format_supported(format: ModelFormat) -> Result<(), BackendCapabilityError> {
    let capability = model_format_capability(format);
    if capability.is_blocked() {
        return Err(BackendCapabilityError {
            backend: capability.backend,
            platform: PlatformInfo::current().label(),
            reason: capability.reason,
        });
    }
    Ok(())
}

fn backend_capability_on(backend: RuntimeBackend, platform: &PlatformInfo) -> BackendCapability {
    match backend {
        RuntimeBackend::Mlx if platform.is_apple_silicon_macos() => BackendCapability {
            backend,
            state: BackendCapabilityState::Enabled,
            reason: "MLX is enabled on Apple Silicon macOS".to_string(),
        },
        RuntimeBackend::Mlx => BackendCapability {
            backend,
            state: BackendCapabilityState::Unsupported,
            reason: "MLX is supported only on Apple Silicon macOS".to_string(),
        },
        RuntimeBackend::TransformersPeft => BackendCapability {
            backend,
            state: BackendCapabilityState::DependencyGated,
            reason: "requires Python packages such as torch, transformers, peft, and safetensors"
                .to_string(),
        },
        RuntimeBackend::LlamaCpp => BackendCapability {
            backend,
            state: BackendCapabilityState::DependencyGated,
            reason: "requires a working llama-cpp-python installation".to_string(),
        },
    }
}

#[derive(Debug, thiserror::Error)]
#[error("backend `{}` is not supported on {platform}: {reason}", backend.as_str())]
pub struct BackendCapabilityError {
    pub backend: RuntimeBackend,
    pub platform: String,
    pub reason: String,
}
