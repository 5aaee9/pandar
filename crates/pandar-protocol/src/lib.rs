//! Shared Pandar agent↔hub protocol: the tonic-prost generated wire types and
//! the domain conversions both sides use, so adding a wire field changes one
//! crate instead of mirrored copies in the hub and the agent.

mod firmware;
mod printers;

pub use firmware::{core_module, core_upgrade_state, proto_module, proto_upgrade_state};
pub use printers::{
    core_device_features, proto_cooling_system, proto_device_features, proto_nozzle_system,
};

pub mod agent {
    pub mod v1 {
        tonic::include_proto!("pandar.agent.v1");
    }
}

#[cfg(test)]
mod tests;
