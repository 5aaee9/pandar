use super::*;
use pandar_core::AgentId;
use requests::{
    PrinterControlRequest, diagnose_printer_body, diagnose_printer_with_access_code_body,
    discover_printers_body, discover_printers_timeout_string_body, empty_body, move_axis,
    printer_control_body, printer_control_value, printer_discovery_result_json,
    web_print_error_body,
};
use serde::Deserialize;
use tokio::sync::mpsc;

mod agent_commands;
mod command_routes;
mod control_dispatch;
mod control_validation;
use control_validation::assert_no_printer_control_audit;
mod models;
mod print_error;
mod requests;
mod sibling_commands;

#[derive(Debug, Deserialize)]
struct TenantResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct AgentResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct CommandResponse {
    id: String,
    agent_id: String,
    printer_id: Option<String>,
    kind: String,
    status: String,
    payload_json: String,
    result_json: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DiscoverPrintersPayload {
    timeout_seconds: u64,
}

#[derive(Debug, Deserialize)]
struct DiagnosePrinterPayload {
    serial_number: String,
}

#[derive(Debug, Deserialize)]
struct PrinterOperationPayload {
    printer_id: String,
    serial_number: String,
    operation: PrinterOperationPayloadDetails,
}

#[derive(Debug, Deserialize)]
struct PrinterOperationPayloadDetails {
    #[serde(rename = "type")]
    kind: String,
    speed_mode: Option<u8>,
    fan_index: Option<u8>,
    speed_percent: Option<u8>,
    airduct: Option<bool>,
    extruder_id: Option<u32>,
    on: Option<bool>,
    action: Option<u32>,
    id: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct PrinterControlAuditMetadata {
    agent_id: String,
    serial_number: String,
    action: String,
    speed_mode: u8,
}

#[derive(Debug, Deserialize)]
struct TenantTokenAuditMetadata {
    tenant_token_id: String,
    tenant_token_scopes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> T {
    decode_json(value)
}
