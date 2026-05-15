//! Platform fact structures, ports, and infrastructure.

pub mod domain;
pub mod infra;
pub mod ports;

#[cfg(test)]
mod tests;

pub use domain::{
    Architecture, CpuFacts, CudaFacts, GpuDeviceFacts, GpuFacts, LibcFacts, LibcFamily, MetalFacts,
    OperatingSystem, PlatformFacts,
};
pub use infra::StdPlatformProbe;
pub use ports::PlatformProbe;
