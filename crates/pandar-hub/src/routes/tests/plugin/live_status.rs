use super::*;

#[tokio::test]
async fn plugin_printer_list_returns_current_external_print_and_hms_snapshot() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-live-status", "Plugin Live Status")
        .await
        .unwrap();
    let token = plugin_studio_tenant_token(&state, &tenant.id.to_string(), "live-status").await;
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    state
        .printers()
        .upsert_snapshot(
            tenant.id,
            agent.id,
            crate::repositories::PrinterSnapshotUpsert {
                serial_number: "studio-live-printer".to_string(),
                host: None,
                access_code: None,
                name: "Live Printer".to_string(),
                model: Some("A1".to_string()),
                status: "IDLE".to_string(),
                observed_at: "2026-07-09T09:59:00Z".to_string(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: None,
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_light_on: None,
            },
        )
        .await
        .unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        "/api/v1/plugin/printers",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["devices"][0].get("print_error").is_none());
    assert!(body["devices"][0].get("job_id").is_none());
    assert!(body["devices"][0].get("state_revision").is_none());
    assert!(body["devices"][0].get("print").is_none());

    state
        .jobs()
        .apply_print_report(crate::repositories::ApplyPrintReport {
            tenant_id: tenant.id,
            agent_id: agent.id,
            serial: "studio-live-printer".to_string(),
            task_id: Some("mqtt-task-9001".to_string()),
            job_id: None,
            print_error: Some(83_918_929),
            printer_job_id: Some("studio-job".to_string()),
            job_attr: None,
            artifact_id: None,
            subtask_id: Some("subtask-12".to_string()),
            gcode_file: Some("external.gcode.3mf".to_string()),
            subtask_name: Some("External plate".to_string()),
            gcode_state: Some("RUNNING".to_string()),
            percent: Some(42),
            remaining_time_minutes: Some(87),
            current_layer: Some(12),
            total_layers: Some(120),
            hms: Some(vec![crate::repositories::PrinterHms {
                attr: 0x0102_0304,
                code: 0x0506_0708,
            }]),
            diagnostics: Vec::new(),
            printer_materials_json: String::new(),
            observed_at: "2026-07-09T10:00:00Z".to_string(),
        })
        .await
        .unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        "/api/v1/plugin/printers",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["devices"][0]["print_error"], 83_918_929);
    assert_eq!(body["devices"][0]["job_id"], "studio-job");
    assert!(body["devices"][0].get("state_revision").is_none());
    assert!(body["devices"][0].get("print").is_none());
    let body = decode::<PluginPrinterListResponse>(body);
    let device = &body.devices[0];
    assert_eq!(device.gcode_state.as_deref(), Some("RUNNING"));
    assert_eq!(device.mc_percent, Some(42));
    assert_eq!(device.mc_remaining_time, Some(87));
    assert_eq!(device.layer_num, Some(12));
    assert_eq!(device.total_layer_num, Some(120));
    assert_eq!(device.task_id.as_deref(), Some("mqtt-task-9001"));
    assert_eq!(device.subtask_id.as_deref(), Some("subtask-12"));
    assert_eq!(device.gcode_file.as_deref(), Some("external.gcode.3mf"));
    assert_eq!(device.subtask_name.as_deref(), Some("External plate"));
    assert_eq!(
        device.hms,
        vec![PluginPrinterHmsResponse {
            attr: 0x0102_0304,
            code: 0x0506_0708,
        }]
    );

    state
        .jobs()
        .apply_print_report(crate::repositories::ApplyPrintReport {
            tenant_id: tenant.id,
            agent_id: agent.id,
            serial: "studio-live-printer".to_string(),
            task_id: None,
            job_id: None,
            print_error: None,
            printer_job_id: None,
            job_attr: None,
            artifact_id: None,
            subtask_id: None,
            gcode_file: None,
            subtask_name: None,
            gcode_state: None,
            percent: None,
            remaining_time_minutes: None,
            current_layer: None,
            total_layers: None,
            hms: Some(Vec::new()),
            diagnostics: Vec::new(),
            printer_materials_json: String::new(),
            observed_at: "2026-07-09T10:01:00Z".to_string(),
        })
        .await
        .unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        "/api/v1/plugin/printers",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["devices"][0]["print_error"], 83_918_929);
    assert_eq!(body["devices"][0]["job_id"], "studio-job");
    let device = decode::<PluginPrinterListResponse>(body)
        .devices
        .pop()
        .unwrap();
    assert_eq!(device.mc_percent, Some(42));
    assert_eq!(device.task_id.as_deref(), Some("mqtt-task-9001"));
    assert!(device.hms.is_empty());

    let session_token = crate::grpc::register_test_session(&state, tenant.id, agent.id).await;
    crate::grpc::printer_snapshots::handle_snapshot(
        &state,
        tenant.id,
        agent.id,
        session_token,
        crate::protocol::agent::v1::PrinterSnapshot {
            serial: "studio-live-printer".to_string(),
            name: "Live Printer".to_string(),
            state: "unknown".to_string(),
            model: "A1".to_string(),
            nozzle_temperatures: vec![crate::protocol::agent::v1::NozzleTemperature {
                current_celsius: "42".to_string(),
                ..Default::default()
            }],
            device_features: None,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        "/api/v1/plugin/printers",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["devices"][0]["print_error"], 83_918_929);
    assert_eq!(body["devices"][0]["job_id"], "studio-job");
    let device = decode::<PluginPrinterListResponse>(body)
        .devices
        .pop()
        .unwrap();
    assert_eq!(device.gcode_state.as_deref(), Some("RUNNING"));
    assert_eq!(device.task_status, "unknown");
    assert_eq!(device.state, "unknown");
    assert!(!device.online);

    state
        .jobs()
        .apply_print_report(crate::repositories::ApplyPrintReport {
            tenant_id: tenant.id,
            agent_id: agent.id,
            serial: "studio-live-printer".to_string(),
            task_id: None,
            job_id: None,
            print_error: None,
            printer_job_id: Some(" \t ".to_string()),
            job_attr: None,
            artifact_id: None,
            subtask_id: None,
            gcode_file: None,
            subtask_name: None,
            gcode_state: None,
            percent: None,
            remaining_time_minutes: None,
            current_layer: None,
            total_layers: None,
            hms: None,
            diagnostics: Vec::new(),
            printer_materials_json: String::new(),
            observed_at: "2026-07-09T10:02:00Z".to_string(),
        })
        .await
        .unwrap();

    let (status, body) =
        request_as(app, Method::GET, "/api/v1/plugin/printers", None, &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["devices"][0]["print_error"], 83_918_929);
    assert_eq!(
        body["devices"][0]["job_id"].as_str().unwrap().as_bytes(),
        b" \t "
    );
}
