//! Platform facts, probes, and use cases.

pub mod domain;
pub mod probe;
pub mod usecases;

pub use domain::{
    Architecture, CpuFacts, CudaFacts, GpuDeviceFacts, GpuFacts, LibcFacts, LibcFamily, MetalFacts,
    OperatingSystem, PlatformFacts,
};
pub use probe::{PlatformProbe, StdPlatformProbe};
