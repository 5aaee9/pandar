use pandar_core::CommandId;

use pandar_protocol::agent::v1::{
    CommandResult, FirmwareCommandResult, PrinterFirmwareStatus as ProtoPrinterFirmwareStatus,
    agent_event, firmware_command_result,
};

pub(super) fn control_result(
    command_id: CommandId,
    serial: &str,
    generation: u64,
    outcome: firmware_command_result::Outcome,
) -> agent_event::Event {
    control_result_with_status(command_id, serial, generation, None, outcome)
}

pub(super) fn control_result_with_status(
    command_id: CommandId,
    serial: &str,
    generation: u64,
    transient_status: Option<ProtoPrinterFirmwareStatus>,
    outcome: firmware_command_result::Outcome,
) -> agent_event::Event {
    agent_event::Event::CommandResult(CommandResult {
        command_id: command_id.to_string(),
        success: true,
        error: String::new(),
        result_json: String::new(),
        firmware_result: Some(FirmwareCommandResult {
            command_id: command_id.to_string(),
            serial: serial.to_owned(),
            generation,
            transient_status,
            outcome: Some(outcome),
        }),
    })
}
