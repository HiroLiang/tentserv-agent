//! Platform fact structures.

pub mod domain;

pub use domain::{
    Architecture, CpuFacts, CudaFacts, GpuDeviceFacts, GpuFacts, LibcFacts, LibcFamily, MetalFacts,
    OperatingSystem, PlatformFacts,
};
