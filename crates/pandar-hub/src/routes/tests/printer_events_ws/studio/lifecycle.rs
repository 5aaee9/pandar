use super::*;

#[tokio::test]
async fn studio_projection_delete_emits_removed_with_serial_identity() {
    let fixture = studio_fixture("studio-delete-acme").await;
    let admin_token = all_scope_tenant_token(
        &fixture.state,
        &fixture.tenant.id.to_string(),
        "delete-admin-token",
    )
    .await;
    let _control_plane = start_control_plane(fixture.state.clone()).await;
    let http_addr = serve_http(router(fixture.state.clone())).await;
    let mut ws = connect_studio(
        http_addr,
        fixture.tenant.id,
        "?projection=studio&version=1",
        Some(&fixture.token),
    )
    .await;
    let StudioFrame::SnapshotBegin { .. } = next_frame(&mut ws, "begin").await else {
        panic!("expected snapshot_begin");
    };
    let StudioFrame::PrinterUpsert { .. } = next_frame(&mut ws, "upsert").await else {
        panic!("expected printer_upsert");
    };
    let StudioFrame::SnapshotEnd = next_frame(&mut ws, "end").await else {
        panic!("expected snapshot_end");
    };

    let app = router(fixture.state.clone());
    let (status, _) = request_as(
        app,
        Method::DELETE,
        &format!(
            "/api/v1/tenants/{}/printers/{}",
            fixture.tenant.id, fixture.printer_id
        ),
        None,
        &admin_token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);

    assert!(
        fixture
            .state
            .printers()
            .get_for_tenant(fixture.tenant.id, &fixture.printer_id)
            .await
            .unwrap()
            .is_none(),
        "printer row should be gone"
    );
    let StudioFrame::PrinterRemoved {
        dev_id,
        pandar_printer_id,
    } = next_frame(&mut ws, "removed").await
    else {
        panic!("expected printer_removed");
    };
    assert_eq!(dev_id, fixture.serial_number);
    assert_eq!(pandar_printer_id, fixture.printer_id);
    assert_studio_quiet(&mut ws, "after removed").await;
}

fn authoritative_snapshot_event(
    tenant_id: TenantId,
    agent_id: AgentId,
    serial: String,
) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "event".to_string(),
        event: Some(agent_event::Event::PrinterSnapshot(PrinterSnapshot {
            serial,
            host: "192.0.2.10".to_string(),
            access_code: "12345678".to_string(),
            name: "X1 Carbon".to_string(),
            model: "X1C".to_string(),
            state: "PRINTING".to_string(),
            nozzle_temperatures: Vec::new(),
            active_nozzle: String::new(),
            bed_temperature_celsius: String::new(),
            bed_target_temperature_celsius: String::new(),
            chamber_temperature_celsius: String::new(),
            chamber_target_temperature_celsius: "45".to_owned(),
            chamber_light_on: None,
            cooling_system: None,
            device_features: None,
            connection_authoritative: true,
            telemetry_authoritative: true,
            nozzle_system: None,
        })),
    }
}

async fn open_grpc_session(
    grpc_addr: std::net::SocketAddr,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> (
    tokio::sync::mpsc::Sender<AgentEvent>,
    impl tokio_stream::Stream<Item = Result<pandar_protocol::agent::v1::HubCommand, tonic::Status>>
    + Unpin,
) {
    let mut client = AgentControlClient::connect(format!("http://{grpc_addr}"))
        .await
        .unwrap();
    let (sender, receiver) = tokio::sync::mpsc::channel(8);
    sender.send(hello_event(tenant_id, agent_id)).await.unwrap();
    let stream = client
        .reverse_connect(ReceiverStream::new(receiver))
        .await
        .unwrap()
        .into_inner();
    (sender, stream)
}

#[tokio::test]
async fn studio_projection_reflects_grpc_lifecycle_online_fencing() {
    let fixture = studio_fixture("studio-grpc-acme").await;
    let _control_plane = start_control_plane(fixture.state.clone()).await;
    fixture
        .state
        .agents()
        .rotate_credential(
            fixture.tenant.id,
            fixture.agent_id,
            TEST_AGENT_CREDENTIAL,
            test_audit_actor(),
        )
        .await
        .unwrap();

    let http_addr = serve_http(router(fixture.state.clone())).await;
    let grpc_addr = serve_grpc(fixture.state.clone()).await;
    let mut ws = connect_studio(
        http_addr,
        fixture.tenant.id,
        "?projection=studio&version=1",
        Some(&fixture.token),
    )
    .await;
    let StudioFrame::SnapshotBegin { .. } = next_frame(&mut ws, "begin").await else {
        panic!("expected snapshot_begin");
    };
    let StudioFrame::PrinterUpsert { .. } = next_frame(&mut ws, "upsert").await else {
        panic!("expected printer_upsert");
    };
    let StudioFrame::SnapshotEnd = next_frame(&mut ws, "end").await else {
        panic!("expected snapshot_end");
    };

    // Establishing a session that leaves the Studio record unchanged emits no
    // false upsert; the first authoritative presence snapshot changes it.
    let (sender_a, _stream_a) =
        open_grpc_session(grpc_addr, fixture.tenant.id, fixture.agent_id).await;
    assert_studio_quiet(&mut ws, "unchanged establishment").await;

    // An authoritative telemetry snapshot reports the machine online.
    sender_a
        .send(authoritative_snapshot_event(
            fixture.tenant.id,
            fixture.agent_id,
            fixture.serial_number.clone(),
        ))
        .await
        .unwrap();
    let StudioFrame::PrinterUpsert { printer } = next_frame(&mut ws, "online upsert").await else {
        panic!("expected online upsert from snapshot report");
    };
    assert_eq!(printer["online"], Value::Bool(true));
    assert_eq!(printer["dev_online"], Value::Bool(true));
    assert_eq!(printer["state"], Value::String("PRINTING".to_owned()));

    // Session replacement: the new session's establishment fences the stale
    // presence session offline...
    let (sender_b, stream_b) =
        open_grpc_session(grpc_addr, fixture.tenant.id, fixture.agent_id).await;
    let StudioFrame::PrinterUpsert { printer } = next_frame(&mut ws, "replacement").await else {
        panic!("expected replacement establishment upsert");
    };
    assert_eq!(printer["online"], Value::Bool(false));

    // ...and a snapshot reported over the replaced (stale) session emits nothing.
    sender_a
        .send(snapshot_event(fixture.tenant.id, fixture.agent_id))
        .await
        .unwrap();
    assert_studio_quiet(&mut ws, "stale session report").await;

    // Current-session telemetry restores online before current-session loss.
    sender_b
        .send(authoritative_snapshot_event(
            fixture.tenant.id,
            fixture.agent_id,
            fixture.serial_number.clone(),
        ))
        .await
        .unwrap();
    let StudioFrame::PrinterUpsert { printer } = next_frame(&mut ws, "replacement online").await
    else {
        panic!("expected replacement online upsert");
    };
    assert_eq!(printer["online"], Value::Bool(true));

    // Dropping the current session disconnects the agent and fences offline.
    drop(sender_b);
    drop(stream_b);
    let StudioFrame::PrinterUpsert { printer } = next_frame(&mut ws, "disconnect").await else {
        panic!("expected disconnect upsert");
    };
    assert_eq!(printer["online"], Value::Bool(false));
    assert_eq!(printer["dev_online"], Value::Bool(false));
}

#[tokio::test]
async fn studio_projection_streams_print_and_material_changes_once() {
    let fixture = studio_fixture("studio-reports-acme").await;
    let _control_plane = start_control_plane(fixture.state.clone()).await;
    fixture
        .state
        .agents()
        .rotate_credential(
            fixture.tenant.id,
            fixture.agent_id,
            TEST_AGENT_CREDENTIAL,
            test_audit_actor(),
        )
        .await
        .unwrap();
    let created = fixture
        .state
        .jobs()
        .create_print_job(CreatePrintJob {
            tenant_id: fixture.tenant.id,
            printer_id: fixture.printer_id.clone(),
            agent_id: fixture.agent_id,
            artifact_id: JOB_PROGRESS_ARTIFACT_ID.to_owned(),
            artifact_filename: "plate.3mf".to_owned(),
            artifact_content_type: "model/3mf".to_owned(),
            artifact_size_bytes: 3,
            artifact_storage_path: format!(
                "{}/{JOB_PROGRESS_ARTIFACT_ID}/plate.3mf",
                fixture.tenant.id
            ),
            artifact_metadata_json: None,
            plate_id: 1,
            use_ams: true,
            bed_leveling: false,
            auto_bed_leveling: pandar_core::PrintCalibrationMode::Off,
            flow_cali: false,
            auto_flow_cali: pandar_core::PrintCalibrationMode::Off,
            auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
            timelapse: false,
            ams_mapping_json: None,
            ams_mapping2_json: None,
            ams_mapping_info_json: None,
        })
        .await
        .unwrap();
    let http_addr = serve_http(router(fixture.state.clone())).await;
    let grpc_addr = serve_grpc(fixture.state.clone()).await;
    let mut ws = connect_studio(
        http_addr,
        fixture.tenant.id,
        "?projection=studio&version=1",
        Some(&fixture.token),
    )
    .await;
    let StudioFrame::SnapshotBegin { .. } = next_frame(&mut ws, "begin").await else {
        panic!("expected snapshot_begin");
    };
    let StudioFrame::PrinterUpsert { .. } = next_frame(&mut ws, "upsert").await else {
        panic!("expected printer_upsert");
    };
    let StudioFrame::SnapshotEnd = next_frame(&mut ws, "end").await else {
        panic!("expected snapshot_end");
    };

    let (sender, stream) = open_grpc_session(grpc_addr, fixture.tenant.id, fixture.agent_id).await;
    assert_studio_quiet(&mut ws, "unchanged report session establishment").await;
    sender
        .send(print_report_event(
            fixture.tenant.id,
            fixture.agent_id,
            fixture.serial_number.clone(),
            created.job.id.to_string(),
            created.artifact.id,
        ))
        .await
        .unwrap();
    let StudioFrame::PrinterUpsert { printer } = next_frame(&mut ws, "print report").await else {
        panic!("expected print report upsert");
    };
    assert_eq!(printer["mc_percent"], 66);
    assert_eq!(printer["gcode_state"], "RUNNING");
    assert_studio_quiet(&mut ws, "single print report projection").await;

    let materials_json = serde_json::json!({
        "type": "printer_material_patch",
        "observed_at": "2026-08-23T00:00:00Z",
        "filament_switch_installed": true,
        "cfg": "8000000000000001",
        "aux": "A4003001",
        "stat": "1000000001",
        "ams_units": [{"unit_id":"0","info":"00000E00","trays":[{"tray_id":"0","type":"PLA"}]}],
        "external_spools": []
    })
    .to_string();
    let material_event = || AgentEvent {
        tenant_id: fixture.tenant.id.to_string(),
        agent_id: fixture.agent_id.to_string(),
        event_id: "materials".to_owned(),
        event: Some(agent_event::Event::PrinterMaterialsSnapshot(
            pandar_protocol::agent::v1::PrinterMaterialsSnapshot {
                serial: fixture.serial_number.clone(),
                printer_id: fixture.printer_id.clone(),
                printer_materials_json: materials_json.clone(),
            },
        )),
    };
    sender.send(material_event()).await.unwrap();
    let StudioFrame::PrinterUpsert { printer } = next_frame(&mut ws, "materials").await else {
        panic!("expected material upsert");
    };
    assert_eq!(printer["materials"]["cfg"], "8000000000000001");
    sender.send(material_event()).await.unwrap();
    assert_studio_quiet(&mut ws, "unchanged materials report").await;
    drop(sender);
    drop(stream);
}
