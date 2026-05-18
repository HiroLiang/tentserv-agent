use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformFacts {
    pub os: OperatingSystem,
    pub arch: Architecture,
    pub libc: Option<LibcFacts>,
    pub cpu: CpuFacts,
    pub gpu: GpuFacts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperatingSystem {
    Macos,
    Linux,
    Windows,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Architecture {
    Aarch64,
    X86_64,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibcFacts {
    pub family: LibcFamily,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LibcFamily {
    Glibc,
    Musl,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuFacts {
    pub vendor: Option<String>,
    pub brand: Option<String>,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuFacts {
    pub devices: Vec<GpuDeviceFacts>,
    pub cuda: Option<CudaFacts>,
    pub metal: Option<MetalFacts>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuDeviceFacts {
    pub name: Option<String>,
    pub vendor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CudaFacts {
    pub visible: bool,
    pub driver_version: Option<String>,
    pub runtime_version: Option<String>,
    pub device_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetalFacts {
    pub visible: bool,
}
