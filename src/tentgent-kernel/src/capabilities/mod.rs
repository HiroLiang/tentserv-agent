//! Machine-local capability state package.

pub mod domain;
pub mod infra;
pub mod ports;
pub mod usecases;

#[cfg(test)]
mod tests;

pub use domain::{
    BackendCapability, BackendKind, CapabilityCheck, CapabilityState, MachineCapabilities,
    RuntimeCapabilityState, RuntimeProfileCapability,
};
pub use infra::{FileCapabilityStateStore, StdCapabilityChecker, StdMachineCapabilitiesProbe};
pub use ports::{
    CapabilityChecker, CapabilityGate, CapabilityStateStore, MachineCapabilitiesProbe,
    MachineCapabilitiesResolver,
};
pub use usecases::{
    MachineCapabilitiesInput, MachineCapabilitiesSnapshot, StdCapabilityGate,
    StdMachineCapabilitiesResolver,
};
