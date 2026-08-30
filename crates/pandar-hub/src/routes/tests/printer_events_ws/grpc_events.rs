use super::*;

#[tokio::test]
async fn printer_events_websocket_receives_snapshot_from_grpc_stream() {
    let state = state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "ws-token",
    )
    .await;
    let agent = state
        .agents()
        .create(tenant.id, "shop-agent")
        .await
        .unwrap();
    state
        .agents()
        .rotate_credential(
            tenant.id,
            agent.id,
            TEST_AGENT_CREDENTIAL,
            test_audit_actor(),
        )
        .await
        .unwrap();
    let http_addr = serve_http(app).await;
    let grpc_addr = serve_grpc(state).await;
    let mut request = format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events",
        tenant.id
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let (sender, receiver) = tokio::sync::mpsc::channel(8);
    sender.send(hello_event(tenant.id, agent.id)).await.unwrap();
    let mut client = AgentControlClient::connect(format!("http://{grpc_addr}"))
        .await
        .unwrap();
    let stream = client
        .reverse_connect(ReceiverStream::new(receiver))
        .await
        .unwrap()
        .into_inner();
    sender
        .send(snapshot_event(tenant.id, agent.id))
        .await
        .unwrap();

    let message = ws.next().await.unwrap().unwrap();
    let body = decode_ws_message::<WebSocketPrinterEvent>(message);
    let WebSocketPrinterEvent::PrinterSnapshot { printer } = body else {
        panic!("expected printer snapshot websocket event");
    };
    assert_eq!(printer.tenant_id, tenant.id.to_string());
    assert_eq!(printer.agent_id, agent.id.to_string());
    assert_eq!(printer.serial_number, "SN-001");
    assert_eq!(
        printer.compatibility.normalized_model.as_deref(),
        Some("X1C")
    );
    assert_eq!(
        printer.compatibility.features.dual_nozzle,
        pandar_core::Capability::Unsupported
    );
    assert_eq!(
        printer
            .compatibility
            .print_options
            .flow_calibration
            .as_ref()
            .map(|option| option.default_mode),
        Some(pandar_core::PrintCalibrationMode::On)
    );
    assert_eq!(
        printer.chamber_target_temperature_celsius.as_deref(),
        Some("45")
    );
    drop(stream);
}

#[tokio::test]
async fn printer_events_websocket_receives_job_progress_from_grpc_stream() {
    let state = state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "job-progress-ws-token",
    )
    .await;
    let agent = state
        .agents()
        .create(tenant.id, "shop-agent")
        .await
        .unwrap();
    state
        .agents()
        .rotate_credential(
            tenant.id,
            agent.id,
            TEST_AGENT_CREDENTIAL,
            test_audit_actor(),
        )
        .await
        .unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();
    let created = state
        .jobs()
        .create_print_job(CreatePrintJob {
            tenant_id: tenant.id,
            printer_id: printer_id.clone(),
            agent_id: agent.id,
            artifact: crate::repositories::PrintArtifactInput {
                id: JOB_PROGRESS_ARTIFACT_ID.to_string(),
                filename: "plate.3mf".to_string(),
                content_type: "model/3mf".to_string(),
                size_bytes: 3,
                storage_path: format!("{}/{JOB_PROGRESS_ARTIFACT_ID}/plate.3mf", tenant.id),
                metadata_json: None,
            },
            options: crate::repositories::PrintExecutionOptions {
                plate_id: 1,
                use_ams: true,
                auto_bed_leveling: pandar_core::PrintCalibrationMode::Off,
                bed_leveling: false,
                flow_cali: false,
                auto_flow_cali: pandar_core::PrintCalibrationMode::Off,
                auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
                timelapse: true,
                ams_mapping_json: None,
                ams_mapping2_json: None,
                ams_mapping_info_json: None,
            },
        })
        .await
        .unwrap();
    let http_addr = serve_http(app).await;
    let grpc_addr = serve_grpc(state).await;
    let mut request = format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events",
        tenant.id
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let (sender, receiver) = tokio::sync::mpsc::channel(8);
    sender.send(hello_event(tenant.id, agent.id)).await.unwrap();
    let mut client = AgentControlClient::connect(format!("http://{grpc_addr}"))
        .await
        .unwrap();
    let stream = client
        .reverse_connect(ReceiverStream::new(receiver))
        .await
        .unwrap()
        .into_inner();
    sender
        .send(print_report_event(
            tenant.id,
            agent.id,
            format!("serial-{printer_id}"),
            created.job.id.to_string(),
            created.artifact.id,
        ))
        .await
        .unwrap();

    let message = ws.next().await.unwrap().unwrap();
    let body = decode_ws_message::<WebSocketPrinterEvent>(message);
    let WebSocketPrinterEvent::JobProgress { job } = body else {
        panic!("expected job progress websocket event");
    };
    assert_eq!(job.id, created.job.id.to_string());
    assert!(
        job.status == "queued" || job.status == "sent",
        "unexpected dispatch status: {}",
        job.status
    );
    assert_eq!(job.print.status, "running");
    assert_eq!(job.print.progress_percent, Some(66));
    drop(stream);
}
