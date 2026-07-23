use super::*;

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
                bed_leveling: false,
                auto_bed_leveling: pandar_core::PrintCalibrationMode::Off,
                flow_cali: false,
                auto_flow_cali: pandar_core::PrintCalibrationMode::Off,
                auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
                timelapse: true,
                ams_mapping_json: None,
                ams_mapping2_json: None,
                ams_mapping_info_json: None,
                studio_submission_id: crate::test_support::studio_submission_id_for_tests(),
                studio_metadata: Some(crate::test_support::studio_metadata_for_tests()),
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
