use std::sync::Arc;

use pandar_core::CommandStatus;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, IntoActiveModel};
use serde::Deserialize;
use tokio::sync::Barrier;

use super::*;
use crate::repositories::{
    AuditActor, PrintErrorAction, PrinterOperationKind, PrinterOperationPayload,
    PrinterSnapshotUpsert, WebPrintErrorRecovery, printer_operation_ownership_pause,
};

mod native;
mod ownership;
mod single_flight;

fn native_operation(
    error_action: PrintErrorAction,
    print_error: u32,
    sequence_id: u64,
) -> PrinterOperationKind {
    PrinterOperationKind::HandlePrintError {
        error_action,
        print_error,
        printer_job_id: "job-7".to_owned(),
        sequence_id,
    }
}

fn native_audit_actor() -> AuditActor {
    AuditActor::tenant_token(None, "postgres-native-print-error", vec!["*"])
}

async fn additional_printer(
    database: &crate::db::Database,
    tenant_id: pandar_core::TenantId,
    agent_id: pandar_core::AgentId,
) -> String {
    crate::repositories::test_helpers::insert_printer_fixture_with_model(
        database,
        tenant_id,
        agent_id,
        Some("A1"),
    )
    .await
    .unwrap()
}

fn web_recovery_input(
    action: PrintErrorAction,
    expected_agent_id: pandar_core::AgentId,
    expected_session_id: &str,
) -> WebPrintErrorRecovery {
    WebPrintErrorRecovery {
        action,
        error_generation: 9,
        expected_agent_id,
        expected_session_id: expected_session_id.to_owned(),
    }
}

async fn seed_web_recovery_state(
    database: &crate::db::Database,
    printer_id: &str,
    session_id: &str,
    serial_number: &str,
) {
    let printer = crate::entities::printers::Entity::find_by_id(printer_id)
        .one(&database.sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    let mut active = printer.into_active_model();
    active.serial_number = Set(serial_number.to_owned());
    active.status = Set("RUNNING".to_owned());
    active.print_task_generation = Set(9);
    active.print_error_generation = Set(9);
    active.print_job_attr = Set(Some(0x10));
    active.print_error_task_generation = Set(Some(9));
    active.print_error_session_id = Set(Some(session_id.to_owned()));
    active.print_error_received_at = Set(Some("2026-07-10T00:00:00Z".to_owned()));
    active.print_gcode_state = Set(Some("PAUSE".to_owned()));
    active.print_error = Set(Some(83_918_929));
    active.print_job_id = Set(Some("job-7".to_owned()));
    active.update(&database.sea_orm_connection()).await.unwrap();
}

fn reassigned_snapshot(serial_number: String) -> PrinterSnapshotUpsert {
    PrinterSnapshotUpsert {
        serial_number,
        host: Some("192.0.2.20".to_owned()),
        access_code: None,
        name: "Reassigned Printer".to_owned(),
        model: Some("A1".to_owned()),
        status: Some("IDLE".to_owned()),
        observed_at: "2026-07-10T00:00:00Z".to_owned(),
        nozzle_temperatures: Vec::new(),
        active_nozzle: None,
        bed_temperature_celsius: None,
        bed_target_temperature_celsius: None,
        chamber_temperature_celsius: None,
        chamber_target_temperature_celsius: None,
        chamber_light_on: None,
        nozzle_system: None,
        connection_authoritative: false,
        telemetry_authoritative: true,
    }
}

async fn load(
    commands: &CommandRepository,
    tenant_id: pandar_core::TenantId,
    command_id: pandar_core::CommandId,
) -> pandar_core::CommandRecord {
    commands
        .get_for_tenant(tenant_id, command_id)
        .await
        .unwrap()
        .unwrap()
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TestPrintErrorAuditMetadata {
    agent_id: String,
    serial_number: String,
    action: String,
    error_action: PrintErrorAction,
    print_error: u32,
    printer_job_id: String,
    sequence_id: u64,
    tenant_token_id: String,
    tenant_token_scopes: Vec<String>,
}
