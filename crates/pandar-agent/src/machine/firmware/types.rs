use pandar_core::{
    FirmwareCommand, FirmwareTerminalOutcome, PrinterFirmwareModule, PrinterFirmwareStatus,
};
use tokio::sync::OwnedMutexGuard;

use crate::{
    AgentConfig,
    machine::BambuPrinterEndpoint,
    protocol::agent::v1::{
        AgentEvent, AmsFirmwareDescriptor, AmsFirmwareDescriptorList, AmsFirmwareSwitchState,
        PrinterFirmwareModule as ProtoFirmwareModule, PrinterFirmwareModulesSnapshot,
        PrinterFirmwareStatusSnapshot, PrinterFirmwareVersion, PrinterFirmwareVersionList,
        PrinterUpgradeState, agent_event,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwareVersionObservation {
    pub model: String,
    pub modules: Vec<PrinterFirmwareModule>,
}

#[derive(Clone, Debug)]
pub struct FirmwareReportContext {
    pub cache: crate::machine::FirmwareObservationCache,
    pub generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwareModulesObservation {
    pub serial: String,
    pub generation: u64,
    pub revision: u64,
    pub modules: Vec<PrinterFirmwareModule>,
}

#[derive(Debug)]
pub struct FirmwareModulesDelivery {
    observation: Option<FirmwareModulesObservation>,
    _version_observation_lease: Option<OwnedMutexGuard<()>>,
}

impl FirmwareModulesDelivery {
    pub fn immediate(observation: FirmwareModulesObservation) -> Self {
        Self {
            observation: Some(observation),
            _version_observation_lease: None,
        }
    }

    pub(crate) fn with_version_observation_lease(
        observation: FirmwareModulesObservation,
        lease: OwnedMutexGuard<()>,
    ) -> Self {
        Self {
            observation: Some(observation),
            _version_observation_lease: Some(lease),
        }
    }

    pub fn take_observation(&mut self) -> FirmwareModulesObservation {
        self.observation
            .take()
            .expect("firmware modules delivery is consumed once")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwareStatusObservation {
    pub serial: String,
    pub generation: u64,
    pub revision: u64,
    pub status: PrinterFirmwareStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwareReservationState {
    pub command_id: String,
    pub session_epoch: u64,
    pub generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwareRefreshRequest {
    pub serial: String,
    pub sequence_id: String,
    pub expected_generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwarePrepareRequest {
    pub command_id: String,
    pub serial: String,
    pub expected_generation: u64,
    pub session_epoch: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwareExecuteRequest {
    pub command_id: String,
    pub serial: String,
    pub expected_generation: u64,
    pub session_epoch: u64,
    pub command: FirmwareCommand,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwarePreparedObservation {
    pub command_id: String,
    pub serial: String,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FirmwareControlPhase {
    Published,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwareControlOutcome {
    pub terminal: FirmwareTerminalOutcome,
    pub transient_status: Option<PrinterFirmwareStatus>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwareCacheSnapshot {
    pub endpoint: BambuPrinterEndpoint,
    pub generation: u64,
    pub module_revision: u64,
    pub status_revision: u64,
    pub modules: Option<Vec<PrinterFirmwareModule>>,
    pub status: Option<PrinterFirmwareStatus>,
    pub reservation: Option<FirmwareReservationState>,
}

pub fn firmware_modules_event(
    config: &AgentConfig,
    observation: FirmwareModulesObservation,
) -> AgentEvent {
    AgentEvent {
        agent_id: config.agent_id.clone(),
        tenant_id: config.tenant_id.clone(),
        event_id: format!(
            "printer-firmware-modules-{}-{}-{}",
            observation.serial, observation.generation, observation.revision
        ),
        event: Some(agent_event::Event::PrinterFirmwareModulesSnapshot(
            PrinterFirmwareModulesSnapshot {
                serial: observation.serial,
                generation: observation.generation,
                module_revision: observation.revision,
                modules: observation.modules.into_iter().map(proto_module).collect(),
            },
        )),
    }
}

pub fn firmware_status_event(
    config: &AgentConfig,
    observation: FirmwareStatusObservation,
) -> AgentEvent {
    let PrinterFirmwareStatus { upgrade_state, cfg } = observation.status;
    AgentEvent {
        agent_id: config.agent_id.clone(),
        tenant_id: config.tenant_id.clone(),
        event_id: format!(
            "printer-firmware-status-{}-{}-{}",
            observation.serial, observation.generation, observation.revision
        ),
        event: Some(agent_event::Event::PrinterFirmwareStatusSnapshot(
            PrinterFirmwareStatusSnapshot {
                serial: observation.serial,
                generation: observation.generation,
                status_revision: observation.revision,
                upgrade_state: upgrade_state.map(proto_upgrade_state),
                cfg,
            },
        )),
    }
}

pub(super) fn firmware_invalidated_event(
    config: &AgentConfig,
    serial: String,
    generation: u64,
) -> AgentEvent {
    AgentEvent {
        agent_id: config.agent_id.clone(),
        tenant_id: config.tenant_id.clone(),
        event_id: format!("printer-firmware-invalidated-{serial}-{generation}"),
        event: Some(agent_event::Event::PrinterFirmwareInvalidated(
            crate::protocol::agent::v1::PrinterFirmwareInvalidated { serial, generation },
        )),
    }
}

pub(crate) fn proto_module(module: PrinterFirmwareModule) -> ProtoFirmwareModule {
    ProtoFirmwareModule {
        name: module.name,
        software_version: module.software_version,
        software_new_version: module.software_new_version,
        new_version: module.new_version,
        visible: module.visible,
        product_name: module.product_name,
        serial_number: module.serial_number,
        hardware_version: module.hardware_version,
        firmware_flag: module.firmware_flag,
    }
}

pub(crate) fn proto_upgrade_state(state: pandar_core::PrinterUpgradeState) -> PrinterUpgradeState {
    PrinterUpgradeState {
        status: state.status,
        progress: state.progress,
        message: state.message,
        module: state.module,
        error_code: state.error_code,
        new_version_state: state.new_version_state,
        consistency_request: state.consistency_request,
        force_upgrade: state.force_upgrade,
        display_state: state.display_state,
        ota_new_version_number: state.ota_new_version_number,
        ams_new_version_number: state.ams_new_version_number,
        ahb_new_version_number: state.ahb_new_version_number,
        new_versions: state
            .new_versions
            .map(|versions| PrinterFirmwareVersionList {
                versions: versions
                    .into_iter()
                    .map(|version| PrinterFirmwareVersion {
                        name: version.name,
                        current_version: version.current_version,
                        new_version: version.new_version,
                    })
                    .collect(),
            }),
        ams_firmware: state.ams_firmware.map(|ams| AmsFirmwareSwitchState {
            firmware: ams.firmware.map(|firmware| AmsFirmwareDescriptorList {
                firmware: firmware
                    .into_iter()
                    .map(|entry| AmsFirmwareDescriptor {
                        id: entry.id,
                        name: entry.name,
                        version: entry.version,
                    })
                    .collect(),
            }),
            current_firmware_id: ams.current_firmware_id,
            current_run_firmware_id: ams.current_run_firmware_id,
            status: ams.status,
        }),
    }
}
