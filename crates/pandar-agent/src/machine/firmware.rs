mod cache;
mod control;
mod gateway;
mod types;

#[cfg(test)]
pub(crate) use cache::event_pause as firmware_event_pause;
pub use cache::{FirmwareGenerationTransition, FirmwareObservationCache};
pub use control::{FirmwareExecutionLease, FirmwarePublishTransition};
pub use gateway::FirmwareMachineGateway;
pub use types::{
    FirmwareCacheSnapshot, FirmwareControlOutcome, FirmwareControlPhase, FirmwareExecuteRequest,
    FirmwareModulesDelivery, FirmwareModulesObservation, FirmwarePrepareRequest,
    FirmwarePreparedObservation, FirmwareRefreshRequest, FirmwareReportContext,
    FirmwareReservationState, FirmwareStatusObservation, FirmwareVersionObservation,
    firmware_modules_event, firmware_status_event,
};
pub(crate) use types::{proto_module, proto_upgrade_state};
