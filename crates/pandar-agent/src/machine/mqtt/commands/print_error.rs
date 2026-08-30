use super::{
    BambuMqttCommandPayload,
    payload::{PrintErrorCommand, PrintPayload, json_payload},
};

pub use pandar_core::PrintErrorAction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlePrintErrorCommand {
    pub error_action: PrintErrorAction,
    pub print_error: u32,
    pub printer_job_id: String,
    pub sequence_id: u64,
}

pub(super) fn print_error_payload(command: &HandlePrintErrorCommand) -> BambuMqttCommandPayload {
    let sequence_id = command.sequence_id.to_string();
    BambuMqttCommandPayload::with_sequence(
        json_payload(PrintPayload {
            print: PrintErrorCommand {
                command: command_name(command.error_action),
                err: command.print_error.to_string(),
                job_id: &command.printer_job_id,
                param: "reserve",
                sequence_id: sequence_id.clone(),
            },
        }),
        sequence_id,
    )
}

fn command_name(action: PrintErrorAction) -> &'static str {
    match action {
        PrintErrorAction::Resume => "resume",
        PrintErrorAction::Ignore => "ignore",
        PrintErrorAction::Stop => "stop",
    }
}
