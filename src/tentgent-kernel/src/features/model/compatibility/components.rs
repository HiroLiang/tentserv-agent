use serde::{Deserialize, Serialize};

use crate::features::adapter::domain::AdapterRef;

use super::CompatibilityError;

// Only registered labels are case-insensitive. Opaque identifiers below are not.
macro_rules! registered_labels {
    ($name:ident, $field:literal, {$($variant:ident => $label:literal $(| $alias:literal)*),+ $(,)?}) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $name { $($variant),+ }
        impl $name {
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $label),+ }
            }
        }
        impl std::str::FromStr for $name {
            type Err = CompatibilityError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value.trim().to_ascii_lowercase().as_str() {
                    $($label $(| $alias)* => Ok(Self::$variant),)+
                    _ => Err(CompatibilityError::InvalidField($field)),
                }
            }
        }
        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                value.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

registered_labels!(CompatibilityBackend, "backend", {
    Mlx => "mlx", Transformers => "transformers", LlamaCpp => "llama-cpp" | "llama_cpp",
    Diffusers => "diffusers",
});
registered_labels!(RuntimeFamily, "runtime_family", {
    MlxLm => "mlx-lm", MlxVlm => "mlx-vlm", MlxAudio => "mlx-audio",
    MlxDiffusion => "mlx-diffusion", Transformers => "transformers",
    LlamaCpp => "llama-cpp" | "llama_cpp", Diffusers => "diffusers",
});
registered_labels!(RuntimePackage, "runtime_package", {
    MlxLm => "mlx-lm", MlxVlm => "mlx-vlm", MlxAudio => "mlx-audio",
    Mflux => "mflux", Transformers => "transformers",
    LlamaCppPython => "llama-cpp-python" | "llama_cpp_python", Diffusers => "diffusers",
});
registered_labels!(OperatingSystem, "operating_system", {
    Macos => "macos" | "darwin", Linux => "linux", Windows => "windows" | "win32",
});
registered_labels!(Architecture, "architecture", {
    Aarch64 => "aarch64" | "arm64", X86_64 => "x86_64" | "amd64",
});
registered_labels!(DeviceClass, "device_class", {
    Cpu => "cpu", Cuda => "cuda", Metal => "metal" | "mps",
});
registered_labels!(QuantizationMethod, "quantization_method", {
    Mlx => "mlx", Gguf => "gguf", BitsAndBytes => "bitsandbytes",
    Gptq => "gptq", Awq => "awq",
});

impl RuntimeFamily {
    pub const fn backend(self) -> CompatibilityBackend {
        match self {
            Self::MlxLm | Self::MlxVlm | Self::MlxAudio | Self::MlxDiffusion => {
                CompatibilityBackend::Mlx
            }
            Self::Transformers => CompatibilityBackend::Transformers,
            Self::LlamaCpp => CompatibilityBackend::LlamaCpp,
            Self::Diffusers => CompatibilityBackend::Diffusers,
        }
    }

    pub const fn package(self) -> RuntimePackage {
        match self {
            Self::MlxLm => RuntimePackage::MlxLm,
            Self::MlxVlm => RuntimePackage::MlxVlm,
            Self::MlxAudio => RuntimePackage::MlxAudio,
            Self::MlxDiffusion => RuntimePackage::Mflux,
            Self::Transformers => RuntimePackage::Transformers,
            Self::LlamaCpp => RuntimePackage::LlamaCppPython,
            Self::Diffusers => RuntimePackage::Diffusers,
        }
    }
}

/// A bounded case-sensitive identifier, not a path or a request-payload bucket.
/// Only its producer knows whether an adapter load identity is authoritative.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CompatibilityIdentifier(String);

impl CompatibilityIdentifier {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, CompatibilityError> {
        let value = value.as_ref();
        if value.is_empty()
            || value.len() > 128
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-:/+".contains(&byte))
            || matches!(
                value.to_ascii_lowercase().as_str(),
                "unknown" | "latest" | "n/a" | "not-applicable"
            )
        {
            return Err(CompatibilityError::InvalidField("identifier"));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for CompatibilityIdentifier {
    type Error = CompatibilityError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<CompatibilityIdentifier> for String {
    fn from(value: CompatibilityIdentifier) -> Self {
        value.0
    }
}

/// Selected backend distribution version, not Tentgent's CLI/project version.
/// Callers must obtain this fact from the runtime actually selected or reused.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RuntimeVersion(String);

impl RuntimeVersion {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, CompatibilityError> {
        let value = value.as_ref();
        if value.is_empty()
            || value.len() > 128
            || !value.bytes().any(|byte| byte.is_ascii_digit())
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._+-!".contains(&byte))
        {
            return Err(CompatibilityError::InvalidField("runtime_version"));
        }
        Ok(Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for RuntimeVersion {
    type Error = CompatibilityError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<RuntimeVersion> for String {
    fn from(value: RuntimeVersion) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeIdentity {
    pub package: RuntimePackage,
    pub version: RuntimeVersion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompatibilityPlatform {
    pub os: OperatingSystem,
    pub architecture: Architecture,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", from = "ProfileWire")]
pub enum CompatibilityProfile {
    NoProfile,
    Selected {
        id: CompatibilityIdentifier,
        version: u32,
    },
}

// Internally tagged unit variants otherwise ignore unknown fields in serde.
// Empty struct variants in the input wire enforce the same strict boundary.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum ProfileWire {
    NoProfile {},
    Selected {
        id: CompatibilityIdentifier,
        version: u32,
    },
}

impl From<ProfileWire> for CompatibilityProfile {
    fn from(value: ProfileWire) -> Self {
        match value {
            ProfileWire::NoProfile {} => Self::NoProfile,
            ProfileWire::Selected { id, version } => Self::Selected { id, version },
        }
    }
}

impl CompatibilityProfile {
    pub(super) fn validate(&self) -> Result<(), CompatibilityError> {
        if matches!(self, Self::Selected { version: 0, .. }) {
            return Err(CompatibilityError::InvalidField("runtime_profile_version"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", from = "AdapterWire")]
pub enum CompatibilityAdapter {
    Base,
    Selected {
        adapter_ref: AdapterRef,
        load_identity: CompatibilityIdentifier,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum AdapterWire {
    Base {},
    Selected {
        adapter_ref: AdapterRef,
        load_identity: CompatibilityIdentifier,
    },
}

impl From<AdapterWire> for CompatibilityAdapter {
    fn from(value: AdapterWire) -> Self {
        match value {
            AdapterWire::Base {} => Self::Base,
            AdapterWire::Selected {
                adapter_ref,
                load_identity,
            } => Self::Selected {
                adapter_ref,
                load_identity,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", from = "QuantizationWire")]
pub enum Quantization {
    Unquantized,
    Quantized {
        method: QuantizationMethod,
        variant: CompatibilityIdentifier,
        bits: QuantizationBits,
        group_size: QuantizationGroupSize,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum QuantizationWire {
    Unquantized {},
    Quantized {
        method: QuantizationMethod,
        variant: CompatibilityIdentifier,
        bits: QuantizationBits,
        group_size: QuantizationGroupSize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum QuantizationBits {
    Fixed { bits: u8 },
    Mixed {},
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum QuantizationGroupSize {
    Fixed { size: u32 },
    PerChannel {},
    NotApplicable {},
}

impl From<QuantizationWire> for Quantization {
    fn from(value: QuantizationWire) -> Self {
        match value {
            QuantizationWire::Unquantized {} => Self::Unquantized,
            QuantizationWire::Quantized {
                method,
                variant,
                bits,
                group_size,
            } => Self::Quantized {
                method,
                variant,
                bits,
                group_size,
            },
        }
    }
}

impl Quantization {
    pub(super) fn validate(&self) -> Result<(), CompatibilityError> {
        if let Self::Quantized {
            method,
            bits,
            group_size,
            ..
        } = self
        {
            if matches!(bits, QuantizationBits::Fixed { bits } if !(1..=64).contains(bits))
                || matches!(group_size, QuantizationGroupSize::Fixed { size: 0 })
                || matches!(group_size, QuantizationGroupSize::NotApplicable {})
                    && !matches!(
                        method,
                        QuantizationMethod::Gguf | QuantizationMethod::BitsAndBytes
                    )
                || *method == QuantizationMethod::Mlx
                    && !matches!(group_size, QuantizationGroupSize::Fixed { .. })
            {
                return Err(CompatibilityError::InvalidField("quantization"));
            }
        }
        Ok(())
    }
}
