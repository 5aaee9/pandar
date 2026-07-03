use pandar_core::CommandRecord;
use serde::Serialize;

use crate::printer_events::PrinterEventPrinter;

pub(in crate::routes) type PrinterResponse = PrinterEventPrinter;

#[derive(Debug, Serialize)]
pub(in crate::routes) struct PrinterListResponse {
    pub(in crate::routes) printers: Vec<PrinterResponse>,
}

#[derive(Debug, Serialize)]
pub(in crate::routes) struct CommandResponse {
    id: String,
    tenant_id: String,
    agent_id: String,
    printer_id: Option<String>,
    kind: String,
    status: String,
    payload_json: String,
    error: Option<String>,
    result_json: Option<String>,
    created_at: String,
    updated_at: String,
}

impl From<CommandRecord> for CommandResponse {
    fn from(command: CommandRecord) -> Self {
        Self {
            id: command.id.to_string(),
            tenant_id: command.tenant_id.to_string(),
            agent_id: command.agent_id.to_string(),
            printer_id: command.printer_id,
            kind: command.kind,
            status: command.status.to_string(),
            payload_json: command.payload_json,
            error: command.error,
            result_json: command.result_json,
            created_at: command.created_at,
            updated_at: command.updated_at,
        }
    }
}
