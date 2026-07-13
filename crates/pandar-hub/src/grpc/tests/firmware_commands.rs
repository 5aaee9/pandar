use std::{collections::HashSet, sync::Arc, time::Duration};

use pandar_core::{
    AgentId, CommandId, CommandStatus, FirmwareCommand, FirmwareControlMetadata,
    FirmwareTerminalOutcome, TenantId,
};
use tokio::sync::mpsc;
use tonic::Code;

use super::log_capture::CapturedLogs;
use super::*;
use crate::{
    firmware_control::{FirmwareExecutePhase, FirmwareServiceError},
    protocol::agent::v1::{
        AgentCapability, AgentEvent, AmsFirmwareDescriptor, AmsFirmwareDescriptorList,
        AmsFirmwareSwitchState, CommandAck, CommandResult, FirmwareAcknowledgement,
        FirmwareCommandResult, FirmwarePrepared, FirmwarePublished, FirmwareRefreshedModules,
        PrinterFirmwareInvalidated, PrinterFirmwareModule, PrinterFirmwareModulesSnapshot,
        PrinterFirmwareStatus, PrinterFirmwareStatusSnapshot, PrinterFirmwareVersion,
        PrinterFirmwareVersionList, PrinterUpgradeState, agent_event, firmware_command_result,
        hub_command,
    },
    repositories::AuditActor,
    sessions::{AgentSession, SessionToken, empty_pending_live_commands},
};

const GENERATION: u64 = 7;
const URL_SENTINEL: &str =
    "https://user:secret@firmware.invalid/main.bin?signature=FIRMWARE-URL-SENTINEL";
const TICKET_URL_SENTINEL: &str = "https://user:secret@firmware.invalid/FIRMWARE-PATH-SENTINEL.bin?ticket=FIRMWARE-TICKET-SENTINEL";
const SECOND_URL_SENTINEL: &str = "https://SECOND-USER:SECOND-PASSWORD@firmware.invalid/FIRMWARE-SECOND-PATH.bin?signature=FIRMWARE-SECOND-QUERY";

mod availability_and_timeouts;
mod execute_durability;
mod fixture;
mod lifecycle_cleanup;
mod ownership_fencing;
mod phases_and_metadata;
mod post_publish_lifecycle;
mod prepare_lifecycle;
mod printer_reassignment;
mod redaction_dispatch;
mod redaction_siblings;
mod redaction_surfaces;
mod refresh_persistence;
mod sibling_cleanup;
mod support;
mod terminal_cleanup;
