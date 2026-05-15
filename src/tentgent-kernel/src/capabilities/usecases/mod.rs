//! Capability use case implementations.

pub mod gate;
pub mod resolver;

#[cfg(test)]
mod tests;

pub use gate::StdCapabilityGate;
pub use resolver::{
    MachineCapabilitiesInput, MachineCapabilitiesSnapshot, StdMachineCapabilitiesResolver,
};
