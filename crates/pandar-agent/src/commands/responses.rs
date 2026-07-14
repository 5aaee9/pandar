use crate::{
    AgentConfig,
    machine::{MachineSnapshot, MaterialRefreshResult},
    protocol::agent::v1::{
        AgentEvent, CommandAck, CommandResult, FirmwareCommandResult, PrinterDeviceFeatures,
        PrinterSnapshot, agent_event,
    },
};

use super::events::event;

pub(crate) fn ack_event(config: &AgentConfig, command_id: &str) -> AgentEvent {
    command_ack_event(config, command_id, true, String::new())
}

pub(crate) fn rejected_ack_event(
    config: &AgentConfig,
    command_id: &str,
    error: String,
) -> AgentEvent {
    command_ack_event(config, command_id, false, error)
}

fn command_ack_event(
    config: &AgentConfig,
    command_id: &str,
    accepted: bool,
    error: String,
) -> AgentEvent {
    event(
        config,
        "ack",
        agent_event::Event::CommandAck(CommandAck {
            command_id: command_id.to_owned(),
            accepted,
            error,
        }),
    )
}

pub(crate) fn success_event(config: &AgentConfig, command_id: &str) -> AgentEvent {
    result_event(config, command_id, true, String::new(), String::new())
}

pub(crate) fn failure_event(config: &AgentConfig, command_id: &str, error: String) -> AgentEvent {
    result_event(config, command_id, false, error, String::new())
}

pub(crate) fn failure_event_with_result(
    config: &AgentConfig,
    command_id: &str,
    error: String,
    result_json: String,
) -> AgentEvent {
    result_event(config, command_id, false, error, result_json)
}

pub(crate) fn success_event_with_result(
    config: &AgentConfig,
    command_id: &str,
    result_json: String,
) -> AgentEvent {
    result_event(config, command_id, true, String::new(), result_json)
}

fn result_event(
    config: &AgentConfig,
    command_id: &str,
    success: bool,
    error: String,
    result_json: String,
) -> AgentEvent {
    event(
        config,
        if success { "success" } else { "failure" },
        agent_event::Event::CommandResult(CommandResult {
            command_id: command_id.to_owned(),
            success,
            error,
            result_json,
            firmware_result: None,
        }),
    )
}

pub(crate) fn firmware_result_event(
    config: &AgentConfig,
    command_id: &str,
    result: FirmwareCommandResult,
) -> AgentEvent {
    event(
        config,
        "firmware-result",
        agent_event::Event::CommandResult(CommandResult {
            command_id: command_id.to_owned(),
            success: true,
            error: String::new(),
            result_json: String::new(),
            firmware_result: Some(result),
        }),
    )
}

pub(crate) fn printer_snapshot_event(
    config: &AgentConfig,
    snapshot: MachineSnapshot,
) -> AgentEvent {
    printer_snapshot_event_with_connection_authority(config, snapshot, false)
}

pub(crate) fn authoritative_printer_snapshot_event(
    config: &AgentConfig,
    snapshot: MachineSnapshot,
) -> AgentEvent {
    printer_snapshot_event_with_connection_authority(config, snapshot, true)
}

fn printer_snapshot_event_with_connection_authority(
    config: &AgentConfig,
    snapshot: MachineSnapshot,
    connection_authoritative: bool,
) -> AgentEvent {
    event(
        config,
        "printer-snapshot",
        agent_event::Event::PrinterSnapshot(PrinterSnapshot {
            serial: snapshot.serial,
            host: snapshot.host.unwrap_or_default(),
            access_code: snapshot.access_code.unwrap_or_default(),
            name: snapshot.name,
            state: snapshot.state,
            model: snapshot.model.unwrap_or_default(),
            nozzle_temperatures: snapshot
                .nozzle_temperatures
                .into_iter()
                .map(
                    |temperature| crate::protocol::agent::v1::NozzleTemperature {
                        label: temperature.label.unwrap_or_default(),
                        current_celsius: temperature.current_celsius.unwrap_or_default(),
                        target_celsius: temperature.target_celsius.unwrap_or_default(),
                        diameter_mm: temperature.diameter_mm.unwrap_or_default(),
                        nozzle_type: temperature.nozzle_type.unwrap_or_default(),
                    },
                )
                .collect(),
            bed_temperature_celsius: snapshot.bed_temperature_celsius.unwrap_or_default(),
            bed_target_temperature_celsius: snapshot
                .bed_target_temperature_celsius
                .unwrap_or_default(),
            chamber_temperature_celsius: snapshot.chamber_temperature_celsius.unwrap_or_default(),
            active_nozzle: snapshot.active_nozzle.unwrap_or_default(),
            chamber_light_on: snapshot.chamber_light_on,
            device_features: snapshot
                .device_features
                .map(|features| PrinterDeviceFeatures {
                    bambu_fun_bits: features.bits(),
                }),
            connection_authoritative,
        }),
    )
}

pub(crate) fn printer_materials_snapshot_event(
    config: &AgentConfig,
    materials: MaterialRefreshResult,
) -> AgentEvent {
    crate::machine::mqtt::printer_materials_snapshot_event(config, materials)
}
