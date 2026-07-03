use crate::{
    AgentConfig,
    machine::{MachineSnapshot, MaterialRefreshResult},
    protocol::agent::v1::{AgentEvent, CommandAck, CommandResult, PrinterSnapshot, agent_event},
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
        }),
    )
}

pub(crate) fn printer_snapshot_event(
    config: &AgentConfig,
    snapshot: MachineSnapshot,
) -> AgentEvent {
    event(
        config,
        "printer-snapshot",
        agent_event::Event::PrinterSnapshot(PrinterSnapshot {
            serial: snapshot.serial,
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
                    },
                )
                .collect(),
            bed_temperature_celsius: snapshot.bed_temperature_celsius.unwrap_or_default(),
            bed_target_temperature_celsius: snapshot
                .bed_target_temperature_celsius
                .unwrap_or_default(),
            chamber_temperature_celsius: snapshot.chamber_temperature_celsius.unwrap_or_default(),
        }),
    )
}

pub(crate) fn printer_materials_snapshot_event(
    config: &AgentConfig,
    materials: MaterialRefreshResult,
) -> AgentEvent {
    crate::machine::mqtt::printer_materials_snapshot_event(config, materials)
}
