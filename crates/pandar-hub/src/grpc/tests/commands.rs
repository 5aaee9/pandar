use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};

use pandar_core::{AgentId, CommandId, CommandRecord, CommandRecordParts, CommandStatus, TenantId};
use serde::Deserialize;
use tokio_stream::StreamExt;
use tonic::Code;
use tracing_subscriber::fmt::MakeWriter;

use super::*;
use crate::protocol::agent::v1::{
    Axis, CommandAck, CommandResult, DeviceFeature, HubCommand, LinkPrinter, printer_operation,
};
use crate::{
    grpc::commands::{
        CommandConversionOptions, handle_result_and_job, hub_command_from_record,
        hub_command_from_record_with_options,
    },
    repositories::{
        CreatePrintJob, DiagnosePrinterPayload, DiscoverPrintersPayload, LinkPrinterPayload,
        PrintProjectFilePayload, PrinterAxis, PrinterOperationKind, PrinterOperationPayload,
        RefreshPrinterMaterialsPayload, ReloadPrinterConnectionPayload,
    },
};

mod acknowledgements;
mod cancel_race;
mod command_conversion;
mod device_features;
mod gcode_line;
mod link_redaction_primary;
mod link_redaction_stream;
mod print_error;
mod printer_commands;
mod printer_operations;
mod results;

fn command_result_payload(
    success: bool,
    error: impl Into<String>,
    result_json: impl Into<String>,
) -> CommandResult {
    CommandResult {
        command_id: String::new(),
        success,
        error: error.into(),
        result_json: result_json.into(),
        firmware_result: None,
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct RedactedLinkPrinterResult {
    access_code: String,
    status: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct RedactedLinkPrinterFailure {
    #[serde(rename = "type")]
    kind: String,
    error_code: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct RedactedNumericLinkPrinterResult {
    echoed: String,
    status: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct RedactedSecretKeyLinkPrinterResult {
    #[serde(rename = "[redacted]")]
    secret_key: String,
    status: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct RedactedUnknownLinkPrinterResult {
    #[serde(rename = "[redacted_0]")]
    first: String,
    #[serde(rename = "[redacted_1]")]
    second: String,
}

fn redacted_result<T>(result_json: &str) -> T
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_str(result_json).unwrap()
}

fn result_event(
    tenant_id: TenantId,
    agent_id: AgentId,
    command_id: CommandId,
    success: bool,
    error: String,
    result_json: String,
) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "event".to_string(),
        event: Some(agent_event::Event::CommandResult(CommandResult {
            command_id: command_id.to_string(),
            success,
            error,
            result_json,
            firmware_result: None,
        })),
    }
}

fn failed_ack_event(
    tenant_id: TenantId,
    agent_id: AgentId,
    command_id: CommandId,
    error: &str,
) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "event".to_string(),
        event: Some(agent_event::Event::CommandAck(CommandAck {
            command_id: command_id.to_string(),
            accepted: false,
            error: error.to_owned(),
        })),
    }
}

fn link_printer_hub_command(command_id: CommandId, access_code: &str) -> HubCommand {
    HubCommand {
        command_id: command_id.to_string(),
        command: Some(hub_command::Command::LinkPrinter(LinkPrinter {
            host: "192.0.2.10".to_owned(),
            access_code: access_code.to_owned(),
            name: String::new(),
            printer_type: "BambuLab".to_owned(),
        })),
    }
}

#[derive(Clone)]
struct CapturedLogs {
    output: Arc<Mutex<Vec<u8>>>,
}

impl CapturedLogs {
    fn new() -> Self {
        Self {
            output: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn writer(&self) -> TestLogWriter {
        TestLogWriter {
            output: self.output.clone(),
        }
    }
}

impl std::fmt::Display for CapturedLogs {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let output = self.output.lock().unwrap().clone();
        formatter.write_str(&String::from_utf8_lossy(&output))
    }
}

#[derive(Clone)]
struct TestLogWriter {
    output: Arc<Mutex<Vec<u8>>>,
}

impl<'writer> MakeWriter<'writer> for TestLogWriter {
    type Writer = TestLogBuffer;

    fn make_writer(&'writer self) -> Self::Writer {
        TestLogBuffer {
            output: self.output.clone(),
        }
    }
}

struct TestLogBuffer {
    output: Arc<Mutex<Vec<u8>>>,
}

impl Write for TestLogBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.output.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
