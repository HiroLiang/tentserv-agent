//! Capability use case implementations.

pub mod gate;
pub mod port;
pub mod resolver;

#[cfg(test)]
mod tests;

pub use gate::StdCapabilityGate;
pub use port::{CapabilityGate, MachineCapabilitiesResolver};
pub use resolver::{
    MachineCapabilitiesInput, MachineCapabilitiesSnapshot, StdMachineCapabilitiesResolver,
};
