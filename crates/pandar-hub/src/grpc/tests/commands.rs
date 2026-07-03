use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};

use pandar_core::{AgentId, CommandId, CommandRecord, CommandRecordParts, CommandStatus, TenantId};
use tokio_stream::StreamExt;
use tonic::Code;
use tracing_subscriber::fmt::MakeWriter;

use super::*;
use crate::protocol::agent::v1::{
    Axis, CommandAck, CommandResult, HubCommand, LinkPrinter, printer_operation,
};
use crate::{
    grpc::commands::{
        CommandConversionOptions, handle_result_and_job, hub_command_from_record,
        hub_command_from_record_with_options,
    },
    repositories::{
        DiagnosePrinterPayload, DiscoverPrintersPayload, LinkPrinterPayload,
        PrintProjectFilePayload, PrinterAxis, PrinterOperationKind, PrinterOperationPayload,
        RefreshPrinterMaterialsPayload,
    },
};

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
    }
}

#[tokio::test]
async fn grpc_wrong_agent_ack_is_permission_denied() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let other = state.agents().create(tenant_id, "other").await.unwrap();
    let command_id = sent_command(&state, tenant_id, agent_id).await;

    let err = handle_ack(
        &state,
        tenant_id,
        other.id,
        CommandAck {
            command_id: command_id.to_string(),
            accepted: true,
            error: String::new(),
        },
    )
    .await
    .unwrap_err();

    assert_eq!(err.code(), Code::PermissionDenied);
}

#[tokio::test]
async fn grpc_wrong_agent_ack_streams_permission_denied() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let other = state.agents().create(tenant_id, "other").await.unwrap();
    let command_id = sent_command(&state, tenant_id, other.id).await;
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    sender
        .send(Ok(ack_event(tenant_id, agent_id, command_id)))
        .await
        .unwrap();
    let err = stream.next().await.unwrap().unwrap_err();

    assert_eq!(err.code(), Code::PermissionDenied);
}

#[tokio::test]
async fn grpc_unknown_command_ack_is_not_found() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;

    let err = handle_ack(
        &state,
        tenant_id,
        agent_id,
        CommandAck {
            command_id: CommandId::new().to_string(),
            accepted: true,
            error: String::new(),
        },
    )
    .await
    .unwrap_err();

    assert_eq!(err.code(), Code::NotFound);
}

#[tokio::test]
async fn grpc_live_stream_ack_and_result_update_command_ledger() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let command = state
        .commands()
        .enqueue_refresh_printers(tenant_id, agent_id)
        .await
        .unwrap();
    state.sessions().wake_local_agent(tenant_id, agent_id).await;
    let _ = stream.next().await.unwrap().unwrap();

    sender
        .send(Ok(ack_event(tenant_id, agent_id, command.id)))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let err = state
        .commands()
        .mark_sent(command.id, tenant_id, agent_id)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        crate::repositories::RepositoryError::InvalidCommandTransition { from, action }
            if from == CommandStatus::Acknowledged.as_str() && action == "send"
    ));

    sender
        .send(Ok(success_event(tenant_id, agent_id, command.id)))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let err = state
        .commands()
        .mark_acknowledged(command.id, tenant_id, agent_id)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        crate::repositories::RepositoryError::InvalidCommandTransition { from, action }
            if from == CommandStatus::Succeeded.as_str() && action == "acknowledge"
    ));
}

#[tokio::test]
async fn grpc_hub_command_from_record_maps_discovery_and_diagnostics() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let discovery = state
        .commands()
        .enqueue_discover_printers(
            tenant_id,
            agent_id,
            DiscoverPrintersPayload { timeout_seconds: 7 },
        )
        .await
        .unwrap();
    let diagnostic = state
        .commands()
        .enqueue_diagnose_printer(
            tenant_id,
            agent_id,
            DiagnosePrinterPayload {
                serial_number: "SERIAL123".to_owned(),
            },
        )
        .await
        .unwrap();

    let discovery_command = hub_command_from_record(discovery).unwrap();
    assert!(matches!(
        discovery_command.command,
        Some(hub_command::Command::DiscoverPrinters(command)) if command.timeout_seconds == 7
    ));
    let diagnostic_command = hub_command_from_record(diagnostic).unwrap();
    assert!(matches!(
        diagnostic_command.command,
        Some(hub_command::Command::DiagnosePrinter(command)) if command.serial_number == "SERIAL123"
    ));
}

#[tokio::test]
async fn grpc_hub_command_from_record_maps_printer_operation() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = PrinterOperationPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
        operation: PrinterOperationKind::SetPrintSpeed { speed_mode: 4 },
    };
    let command = CommandRecord::from_parts(CommandRecordParts {
        id: CommandId::new(),
        tenant_id,
        agent_id,
        printer_id: Some(printer_id),
        kind: "printer_operation".to_string(),
        status: "queued".to_string(),
        payload_json: serde_json::to_string(&payload).unwrap(),
        result_json: None,
        error: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    })
    .unwrap();

    let hub_command = hub_command_from_record(command).unwrap();

    assert!(matches!(
        hub_command.command,
        Some(hub_command::Command::PrinterOperation(command))
            if command.serial_number == "SERIAL123"
                && matches!(
                    command.operation,
                    Some(printer_operation::Operation::SetPrintSpeed(speed))
                        if speed.speed_mode == 4
                )
    ));
}

#[tokio::test]
async fn grpc_hub_command_from_record_maps_select_extruder_operation() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = PrinterOperationPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
        operation: PrinterOperationKind::SelectExtruder { extruder_id: 1 },
    };
    let command = CommandRecord::from_parts(CommandRecordParts {
        id: CommandId::new(),
        tenant_id,
        agent_id,
        printer_id: Some(printer_id),
        kind: "printer_operation".to_string(),
        status: "queued".to_string(),
        payload_json: serde_json::to_string(&payload).unwrap(),
        result_json: None,
        error: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    })
    .unwrap();

    let hub_command = hub_command_from_record(command).unwrap();

    assert!(matches!(
        hub_command.command,
        Some(hub_command::Command::PrinterOperation(command))
            if command.serial_number == "SERIAL123"
                && matches!(
                    command.operation,
                    Some(printer_operation::Operation::SelectExtruder(operation))
                        if operation.extruder_id == 1
                )
    ));
}

#[tokio::test]
async fn grpc_hub_command_from_record_maps_targeted_hotend_operation() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = PrinterOperationPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
        operation: PrinterOperationKind::SetHotendTemperature {
            temperature_celsius: 220,
            wait: false,
            extruder_id: Some(1),
        },
    };
    let command = CommandRecord::from_parts(CommandRecordParts {
        id: CommandId::new(),
        tenant_id,
        agent_id,
        printer_id: Some(printer_id),
        kind: "printer_operation".to_string(),
        status: "queued".to_string(),
        payload_json: serde_json::to_string(&payload).unwrap(),
        result_json: None,
        error: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    })
    .unwrap();

    let hub_command = hub_command_from_record(command).unwrap();

    assert!(matches!(
        hub_command.command,
        Some(hub_command::Command::PrinterOperation(command))
            if command.serial_number == "SERIAL123"
                && matches!(
                    command.operation,
                    Some(printer_operation::Operation::SetHotendTemperature(operation))
                        if operation.temperature_celsius == 220 && operation.extruder_id == Some(1)
                )
    ));
}

#[tokio::test]
async fn grpc_hub_command_from_record_maps_bed_temperature_operation() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = PrinterOperationPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
        operation: PrinterOperationKind::SetBedTemperature {
            temperature_celsius: 75,
            wait: false,
        },
    };
    let command = CommandRecord::from_parts(CommandRecordParts {
        id: CommandId::new(),
        tenant_id,
        agent_id,
        printer_id: Some(printer_id),
        kind: "printer_operation".to_string(),
        status: "queued".to_string(),
        payload_json: serde_json::to_string(&payload).unwrap(),
        result_json: None,
        error: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    })
    .unwrap();

    let hub_command = hub_command_from_record(command).unwrap();

    assert!(matches!(
        hub_command.command,
        Some(hub_command::Command::PrinterOperation(command))
            if command.serial_number == "SERIAL123"
                && matches!(
                    command.operation,
                    Some(printer_operation::Operation::SetBedTemperature(operation))
                        if operation.temperature_celsius == 75
                )
    ));
}

#[tokio::test]
async fn grpc_hub_command_from_record_maps_chamber_temperature_operation() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = PrinterOperationPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
        operation: PrinterOperationKind::SetChamberTemperature {
            temperature_celsius: 45,
            wait: false,
        },
    };
    let command = CommandRecord::from_parts(CommandRecordParts {
        id: CommandId::new(),
        tenant_id,
        agent_id,
        printer_id: Some(printer_id),
        kind: "printer_operation".to_string(),
        status: "queued".to_string(),
        payload_json: serde_json::to_string(&payload).unwrap(),
        result_json: None,
        error: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    })
    .unwrap();

    let hub_command = hub_command_from_record(command).unwrap();

    assert!(matches!(
        hub_command.command,
        Some(hub_command::Command::PrinterOperation(command))
            if command.serial_number == "SERIAL123"
                && matches!(
                    command.operation,
                    Some(printer_operation::Operation::SetChamberTemperature(operation))
                        if operation.temperature_celsius == 45
                )
    ));
}

#[tokio::test]
async fn grpc_hub_command_from_record_maps_ams_slot_operation() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = PrinterOperationPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
        operation: PrinterOperationKind::AmsLoadFilament {
            ams_id: 0,
            slot_id: 1,
            global_tray_id: Some(1),
            external_id: None,
            extruder_id: Some(0),
        },
    };
    let command = CommandRecord::from_parts(CommandRecordParts {
        id: CommandId::new(),
        tenant_id,
        agent_id,
        printer_id: Some(printer_id),
        kind: "printer_operation".to_string(),
        status: "queued".to_string(),
        payload_json: serde_json::to_string(&payload).unwrap(),
        result_json: None,
        error: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    })
    .unwrap();

    let hub_command = hub_command_from_record(command).unwrap();

    match hub_command.command {
        Some(hub_command::Command::PrinterOperation(command)) => {
            assert_eq!(command.serial_number, "SERIAL123");
            match command.operation {
                Some(printer_operation::Operation::AmsLoadFilament(operation)) => {
                    assert_eq!(operation.ams_id, 0);
                    assert_eq!(operation.slot_id, 1);
                    assert_eq!(operation.global_tray_id, 1);
                    assert_eq!(operation.external_id, "");
                    assert_eq!(operation.extruder_id, Some(0));
                }
                other => panic!("expected AMS load operation, got {other:?}"),
            }
        }
        other => panic!("expected printer operation command, got {other:?}"),
    }
}

#[tokio::test]
async fn converts_refresh_printer_materials_command_to_proto() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = RefreshPrinterMaterialsPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
    };
    let command = CommandRecord::from_parts(CommandRecordParts {
        id: CommandId::new(),
        tenant_id,
        agent_id,
        printer_id: Some(printer_id),
        kind: "refresh_printer_materials".to_string(),
        status: "queued".to_string(),
        payload_json: serde_json::to_string(&payload).unwrap(),
        result_json: None,
        error: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    })
    .unwrap();

    let hub_command = hub_command_from_record(command).unwrap();

    match hub_command.command.unwrap() {
        hub_command::Command::RefreshPrinterMaterials(command) => {
            assert_eq!(command.printer_id, "printer-1");
            assert_eq!(command.serial_number, "SERIAL123");
        }
        other => panic!("expected refresh materials command, got {other:?}"),
    }
}

#[tokio::test]
async fn grpc_hub_command_from_record_maps_printer_operation_home_axes() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = PrinterOperationPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
        operation: PrinterOperationKind::Home {
            axes: vec![PrinterAxis::X, PrinterAxis::Z],
        },
    };
    let command = CommandRecord::from_parts(CommandRecordParts {
        id: CommandId::new(),
        tenant_id,
        agent_id,
        printer_id: Some(printer_id),
        kind: "printer_operation".to_string(),
        status: "queued".to_string(),
        payload_json: serde_json::to_string(&payload).unwrap(),
        result_json: None,
        error: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    })
    .unwrap();

    let hub_command = hub_command_from_record(command).unwrap();

    match hub_command.command {
        Some(hub_command::Command::PrinterOperation(command)) => {
            assert_eq!(command.serial_number, "SERIAL123");
            match command.operation {
                Some(printer_operation::Operation::Home(home)) => {
                    assert_eq!(home.axes, vec![Axis::X as i32, Axis::Z as i32]);
                }
                other => panic!("expected home operation, got {other:?}"),
            }
        }
        other => panic!("expected printer operation command, got {other:?}"),
    }
}

#[tokio::test]
async fn grpc_hub_command_from_record_rejects_invalid_printer_operation_payload() {
    let command = CommandRecord::from_parts(CommandRecordParts {
        id: CommandId::new(),
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        printer_id: Some("printer-1".to_string()),
        kind: "printer_operation".to_string(),
        status: "queued".to_string(),
        payload_json: r#"{"printer_id":"printer-1","serial_number":"SERIAL123","operation":{"type":"unknown"}}"#.to_string(),
        result_json: None,
        error: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    })
    .unwrap();

    let err = hub_command_from_record(command).unwrap_err();

    assert_eq!(err.code(), Code::Internal);
    assert_eq!(err.message(), "invalid printer operation command payload");
}

#[tokio::test]
async fn grpc_hub_command_from_record_requires_artifact_download_path_when_configured() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture(
        state.database(),
        tenant_id,
        agent_id,
    )
    .await
    .unwrap();
    let command = state
        .commands()
        .enqueue_print_project_file(
            tenant_id,
            agent_id,
            &printer_id,
            PrintProjectFilePayload {
                job_id: "job-1".to_string(),
                artifact_id: "artifact-1".to_string(),
                printer_id: printer_id.clone(),
                serial_number: "serial-explicit".to_string(),
                filename: "plate.3mf".to_string(),
                storage_path: "tenant/artifact/plate.3mf".to_string(),
                artifact_download_path: String::new(),
                size_bytes: 3,
                plate_id: 1,
                use_ams: true,
                flow_cali: false,
                timelapse: true,
                ams_mapping_json: None,
                ams_mapping2_json: None,
            },
        )
        .await
        .unwrap();

    hub_command_from_record(command.clone()).unwrap();
    let err = hub_command_from_record_with_options(
        command,
        CommandConversionOptions {
            require_artifact_download_path: true,
        },
    )
    .unwrap_err();

    assert_eq!(err.code(), Code::Internal);
    assert_eq!(err.message(), "missing artifact download path");
}

#[tokio::test]
async fn grpc_command_result_persists_result_json() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let command = state
        .commands()
        .enqueue_diagnose_printer(
            tenant_id,
            agent_id,
            DiagnosePrinterPayload {
                serial_number: "SERIAL123".to_owned(),
            },
        )
        .await
        .unwrap();
    state
        .commands()
        .mark_sent(command.id, tenant_id, agent_id)
        .await
        .unwrap();
    state
        .commands()
        .mark_acknowledged(command.id, tenant_id, agent_id)
        .await
        .unwrap();
    let result_json = r#"{"type":"printer_diagnostic","overall":"problem"}"#;

    handle_result(
        &state,
        tenant_id,
        agent_id,
        CommandResult {
            command_id: command.id.to_string(),
            success: true,
            error: String::new(),
            result_json: result_json.to_owned(),
        },
    )
    .await
    .unwrap();

    let persisted = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.status, CommandStatus::Succeeded);
    assert_eq!(persisted.result_json.as_deref(), Some(result_json));
}

#[tokio::test]
async fn grpc_printer_operation_result_publishes_command_event() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("A1"),
    )
    .await
    .unwrap();
    let command = state
        .commands()
        .enqueue_printer_operation_with_audit(
            tenant_id,
            &printer_id,
            PrinterOperationKind::SetPrintSpeed { speed_mode: 3 },
            test_audit_actor(),
        )
        .await
        .unwrap();
    state
        .commands()
        .mark_sent(command.id, tenant_id, agent_id)
        .await
        .unwrap();
    state
        .commands()
        .mark_acknowledged(command.id, tenant_id, agent_id)
        .await
        .unwrap();
    let mut receiver = state.printer_events().subscribe(tenant_id).await;
    let result_json = r#"{"type":"printer_operation","sequence_id":"20000"}"#;

    handle_result(
        &state,
        tenant_id,
        agent_id,
        CommandResult {
            command_id: command.id.to_string(),
            success: true,
            error: String::new(),
            result_json: result_json.to_owned(),
        },
    )
    .await
    .unwrap();

    let event = receiver.recv().await.unwrap();
    match event {
        crate::printer_events::PrinterEvent::CommandResult {
            command: event_command,
        } => {
            assert_eq!(event_command.id, command.id.to_string());
            assert_eq!(event_command.kind, "printer_operation");
            assert_eq!(event_command.status, "succeeded");
            assert_eq!(event_command.result_json.as_deref(), Some(result_json));
        }
        other => panic!("expected command result event, got {other:?}"),
    }
}

#[test]
fn grpc_hub_command_from_record_rejects_persisted_link_printer_replay() {
    let command = CommandRecord::from_parts(CommandRecordParts {
        id: CommandId::new(),
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        printer_id: None,
        kind: "link_printer".to_string(),
        status: "sent".to_string(),
        payload_json:
            r#"{"printer_type":"BambuLab","host":"192.0.2.10","access_code":"[redacted]"}"#
                .to_string(),
        result_json: None,
        error: None,
        created_at: "2026-07-01T00:00:00Z".to_string(),
        updated_at: "2026-07-01T00:00:00Z".to_string(),
    })
    .unwrap();

    let err = hub_command_from_record(command).unwrap_err();

    assert_eq!(err.code(), Code::FailedPrecondition);
    assert_eq!(
        err.message(),
        "link printer command requires live secret dispatch"
    );
}

#[tokio::test]
async fn grpc_link_printer_failed_result_redacts_access_code() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "SECRET-LINK-CODE";
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: access_code.to_owned(),
                name: None,
            },
            test_audit_actor(),
        )
        .await
        .unwrap();

    handle_result_and_job(
        &state,
        tenant_id,
        agent_id,
        command.id,
        command_result_payload(
            false,
            format!("validation failed for access_code={access_code}"),
            String::new(),
        ),
        None,
    )
    .await
    .unwrap();

    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, CommandStatus::Failed);
    assert!(!stored.error.unwrap().contains(access_code));
}

#[tokio::test]
async fn grpc_link_printer_result_json_redacts_access_code() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "SECRET-LINK-CODE";
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: access_code.to_owned(),
                name: None,
            },
            test_audit_actor(),
        )
        .await
        .unwrap();
    let result_json = format!(r#"{{"access_code":"{access_code}","status":"rejected"}}"#);

    handle_result_and_job(
        &state,
        tenant_id,
        agent_id,
        command.id,
        command_result_payload(false, String::new(), result_json),
        Some(access_code),
    )
    .await
    .unwrap();

    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .unwrap()
        .unwrap();
    let stored_result = stored.result_json.unwrap();
    assert!(!stored_result.contains(access_code));
    let parsed: serde_json::Value = serde_json::from_str(&stored_result).unwrap();
    assert_eq!(parsed["access_code"], "[redacted]");
    assert_eq!(parsed["status"], "rejected");
}

#[tokio::test]
async fn grpc_link_printer_numeric_result_json_redacts_digit_access_code() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "12345678";
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: access_code.to_owned(),
                name: None,
            },
            test_audit_actor(),
        )
        .await
        .unwrap();

    handle_result_and_job(
        &state,
        tenant_id,
        agent_id,
        command.id,
        command_result_payload(
            false,
            String::new(),
            r#"{"echoed":12345678,"status":"rejected"}"#.to_owned(),
        ),
        Some(access_code),
    )
    .await
    .unwrap();

    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .unwrap()
        .unwrap();
    let stored_result = stored.result_json.unwrap();
    assert!(!stored_result.contains(access_code));
    let parsed: serde_json::Value = serde_json::from_str(&stored_result).unwrap();
    assert_eq!(parsed["echoed"], "[redacted]");
    assert_eq!(parsed["status"], "rejected");
}

#[tokio::test]
async fn grpc_link_printer_result_json_redacts_access_code_object_key() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "SECRET-LINK-CODE";
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: access_code.to_owned(),
                name: None,
            },
            test_audit_actor(),
        )
        .await
        .unwrap();

    handle_result_and_job(
        &state,
        tenant_id,
        agent_id,
        command.id,
        command_result_payload(
            false,
            String::new(),
            format!(r#"{{"{access_code}":"rejected","status":"failed"}}"#),
        ),
        Some(access_code),
    )
    .await
    .unwrap();

    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .unwrap()
        .unwrap();
    let stored_result = stored.result_json.unwrap();
    assert!(!stored_result.contains(access_code));
    let parsed: serde_json::Value = serde_json::from_str(&stored_result).unwrap();
    let object = parsed.as_object().unwrap();
    assert!(object.keys().all(|key| !key.contains(access_code)));
}

#[tokio::test]
async fn grpc_late_link_printer_result_logs_without_access_code() {
    let logs = CapturedLogs::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.writer())
        .with_ansi(false)
        .finish();
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "SECRET-LINK-CODE";
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: access_code.to_owned(),
                name: None,
            },
            test_audit_actor(),
        )
        .await
        .unwrap();
    state
        .commands()
        .mark_succeeded(command.id, tenant_id, agent_id)
        .await
        .unwrap();

    let _guard = tracing::subscriber::set_default(subscriber);
    let err = handle_result_and_job(
        &state,
        tenant_id,
        agent_id,
        command.id,
        command_result_payload(
            false,
            format!("validation failed for access_code={access_code}"),
            String::new(),
        ),
        None,
    )
    .await
    .unwrap_err();
    tracing::error!(error = %crate::redaction::redact_secrets(&format!("{err:#}")), "failed to process late link printer result");
    drop(_guard);

    let captured = logs.to_string();
    assert!(captured.contains("failed to process late link printer result"));
    assert!(!captured.contains(access_code));
}

#[tokio::test]
async fn grpc_late_link_printer_result_stream_keeps_session_and_pending_redacted() {
    let logs = CapturedLogs::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.writer())
        .with_ansi(false)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "SECRET-LINK-CODE";
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let token = state.sessions().get(agent_id).await.unwrap().token;
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: access_code.to_owned(),
                name: None,
            },
            test_audit_actor(),
        )
        .await
        .unwrap();
    state
        .sessions()
        .try_dispatch_live_command(
            tenant_id,
            agent_id,
            token,
            command.id,
            link_printer_hub_command(command.id, access_code),
        )
        .await
        .unwrap();
    let _ = stream.next().await.unwrap().unwrap();
    state
        .commands()
        .mark_failed(
            command.id,
            tenant_id,
            agent_id,
            "stale cleanup failed first",
        )
        .await
        .unwrap();

    sender
        .send(Ok(result_event(
            tenant_id,
            agent_id,
            command.id,
            false,
            format!("printer rejected {access_code}"),
            format!(r#"{{"message":"{access_code}"}}"#),
        )))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    assert_eq!(state.sessions().get(agent_id).await.unwrap().token, token);
    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, CommandStatus::Failed);
    assert_eq!(stored.error.as_deref(), Some("stale cleanup failed first"));
    assert!(
        state
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&command.id)
    );
    drop(_guard);

    let captured = logs.to_string();
    assert!(captured.contains("ignored late live printer link command result"));
    assert!(!captured.contains(access_code));
}

#[tokio::test]
async fn grpc_link_printer_stream_result_redacts_standalone_access_code() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "SECRET-LINK-CODE";
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let token = state.sessions().get(agent_id).await.unwrap().token;
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: access_code.to_owned(),
                name: None,
            },
            test_audit_actor(),
        )
        .await
        .unwrap();
    state
        .sessions()
        .try_dispatch_live_command(
            tenant_id,
            agent_id,
            token,
            command.id,
            link_printer_hub_command(command.id, access_code),
        )
        .await
        .unwrap();
    let _ = stream.next().await.unwrap().unwrap();

    sender
        .send(Ok(result_event(
            tenant_id,
            agent_id,
            command.id,
            false,
            format!("printer rejected {access_code}"),
            format!(r#"{{"message":"{access_code}"}}"#),
        )))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, CommandStatus::Failed);
    assert!(!stored.error.unwrap().contains(access_code));
    assert!(!stored.result_json.unwrap().contains(access_code));
    assert!(
        !state
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&command.id)
    );
}

#[tokio::test]
async fn grpc_link_printer_rejected_ack_redacts_pending_secret_from_error() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "SECRET-LINK-CODE";
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let token = state.sessions().get(agent_id).await.unwrap().token;
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: access_code.to_owned(),
                name: None,
            },
            test_audit_actor(),
        )
        .await
        .unwrap();
    state
        .sessions()
        .try_dispatch_live_command(
            tenant_id,
            agent_id,
            token,
            command.id,
            link_printer_hub_command(command.id, access_code),
        )
        .await
        .unwrap();
    let _ = stream.next().await.unwrap().unwrap();

    sender
        .send(Ok(failed_ack_event(
            tenant_id,
            agent_id,
            command.id,
            access_code,
        )))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, CommandStatus::Failed);
    assert!(!stored.error.unwrap().contains(access_code));
}

#[tokio::test]
async fn grpc_link_printer_result_without_pending_secret_redacts_untrusted_strings() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "SECRET-LINK-CODE";
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: access_code.to_owned(),
                name: None,
            },
            test_audit_actor(),
        )
        .await
        .unwrap();

    handle_result_and_job(
        &state,
        tenant_id,
        agent_id,
        command.id,
        command_result_payload(
            false,
            format!("printer rejected {access_code}"),
            format!(r#"{{"message":"{access_code}","status":"rejected"}}"#),
        ),
        None,
    )
    .await
    .unwrap();

    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .unwrap()
        .unwrap();
    assert!(!stored.error.unwrap().contains(access_code));
    let result_json = stored.result_json.unwrap();
    assert!(!result_json.contains(access_code));
    let parsed: serde_json::Value = serde_json::from_str(&result_json).unwrap();
    let object = parsed.as_object().unwrap();
    assert!(object.keys().all(|key| !key.contains(access_code)));
    assert!(object.values().all(|value| value == "[redacted]"));
}

#[tokio::test]
async fn grpc_link_printer_result_without_pending_secret_redacts_numeric_values() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "12345678";
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: access_code.to_owned(),
                name: None,
            },
            test_audit_actor(),
        )
        .await
        .unwrap();

    handle_result_and_job(
        &state,
        tenant_id,
        agent_id,
        command.id,
        command_result_payload(
            false,
            String::new(),
            r#"{"echoed":12345678,"status":"rejected"}"#.to_owned(),
        ),
        None,
    )
    .await
    .unwrap();

    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .unwrap()
        .unwrap();
    let result_json = stored.result_json.unwrap();
    assert!(!result_json.contains(access_code));
    let parsed: serde_json::Value = serde_json::from_str(&result_json).unwrap();
    let object = parsed.as_object().unwrap();
    assert!(object.keys().all(|key| !key.contains(access_code)));
    assert!(object.values().all(|value| value == "[redacted]"));
}

#[tokio::test]
async fn grpc_link_printer_result_without_pending_secret_redacts_numeric_object_key() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "12345678";
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: access_code.to_owned(),
                name: None,
            },
            test_audit_actor(),
        )
        .await
        .unwrap();

    handle_result_and_job(
        &state,
        tenant_id,
        agent_id,
        command.id,
        command_result_payload(
            false,
            String::new(),
            r#"{"12345678":"rejected","status":"failed"}"#.to_owned(),
        ),
        None,
    )
    .await
    .unwrap();

    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .unwrap()
        .unwrap();
    let result_json = stored.result_json.unwrap();
    assert!(!result_json.contains(access_code));
    let parsed: serde_json::Value = serde_json::from_str(&result_json).unwrap();
    let object = parsed.as_object().unwrap();
    assert!(object.keys().all(|key| !key.contains(access_code)));
}

#[tokio::test]
async fn grpc_unknown_command_ack_streams_not_found() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    sender
        .send(Ok(ack_event(tenant_id, agent_id, CommandId::new())))
        .await
        .unwrap();
    let err = stream.next().await.unwrap().unwrap_err();

    assert_eq!(err.code(), Code::NotFound);
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

#[tokio::test]
async fn grpc_stale_ack_is_failed_precondition() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let command = state
        .commands()
        .enqueue_refresh_printers(tenant_id, agent_id)
        .await
        .unwrap();

    let err = handle_ack(
        &state,
        tenant_id,
        agent_id,
        CommandAck {
            command_id: command.id.to_string(),
            accepted: true,
            error: String::new(),
        },
    )
    .await
    .unwrap_err();

    assert_eq!(err.code(), Code::FailedPrecondition);
}

#[tokio::test]
async fn grpc_stale_ack_streams_failed_precondition() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let command_id = sent_command(&state, tenant_id, agent_id).await;
    state
        .commands()
        .mark_failed(command_id, tenant_id, agent_id, "first")
        .await
        .unwrap();
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    sender
        .send(Ok(ack_event(tenant_id, agent_id, command_id)))
        .await
        .unwrap();
    let err = stream.next().await.unwrap().unwrap_err();

    assert_eq!(err.code(), Code::FailedPrecondition);
}
