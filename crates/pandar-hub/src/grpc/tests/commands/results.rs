use super::*;

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
            firmware_result: None,
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
async fn grpc_print_command_result_persists_dispatch_result_json() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture(
        state.database(),
        tenant_id,
        agent_id,
    )
    .await
    .unwrap();
    let created = state
        .jobs()
        .create_print_job(CreatePrintJob {
            tenant_id,
            printer_id,
            agent_id,
            artifact_id: "artifact-1".to_string(),
            artifact_filename: "plate.3mf".to_string(),
            artifact_content_type: "model/3mf".to_string(),
            artifact_size_bytes: 3,
            artifact_storage_path: "tenant/artifact/plate.3mf".to_string(),
            artifact_metadata_json: None,
            plate_id: 1,
            use_ams: true,
            bed_leveling: false,
            auto_bed_leveling: pandar_core::PrintCalibrationMode::Off,
            flow_cali: false,
            auto_flow_cali: pandar_core::PrintCalibrationMode::Off,
            auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
            timelapse: true,
            ams_mapping_json: None,
            ams_mapping2_json: None,
            ams_mapping_info_json: None,
        })
        .await
        .unwrap();
    let command = state
        .commands()
        .get_for_tenant(tenant_id, created.job.command_id)
        .await
        .unwrap()
        .unwrap();
    state
        .jobs()
        .mark_print_sent(command.id, tenant_id, agent_id)
        .await
        .unwrap();
    state
        .jobs()
        .mark_print_acknowledged(command.id, tenant_id, agent_id)
        .await
        .unwrap();
    let result_json = r#"{"type":"print_project_file","mqtt":{"qos":0}}"#;

    handle_result(
        &state,
        tenant_id,
        agent_id,
        CommandResult {
            command_id: command.id.to_string(),
            success: true,
            error: String::new(),
            result_json: result_json.to_owned(),
            firmware_result: None,
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
            firmware_result: None,
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
