use super::*;

#[tokio::test]
async fn printer_list_returns_tenant_printers() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<PrinterListResponse>(body);
    let printer = body.printers.first().unwrap();
    assert_eq!(printer.id, printer_id);
    assert_eq!(printer.tenant_id, tenant_id.to_string());
    assert_eq!(printer.agent_id, agent_id.to_string());
    assert_eq!(printer.materials, None);
}

#[tokio::test]
async fn printer_detail_returns_tenant_printer() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<PrinterResponse>(body);
    assert_eq!(body.id, printer_id);
    assert_eq!(body.tenant_id, tenant_id.to_string());
    assert_eq!(body.materials, None);
}

#[tokio::test]
async fn printer_list_and_detail_share_enriched_sanitized_print_shape() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    let applied = state
        .jobs()
        .apply_print_report(crate::repositories::ApplyPrintReport {
            tenant_id,
            agent_id,
            serial: format!("serial-{printer_id}"),
            task_id: Some("task-42".to_owned()),
            job_id: None,
            print_error: Some(83_918_929),
            printer_job_id: Some(String::new()),
            job_attr: Some(0x00b0),
            artifact_id: None,
            subtask_id: None,
            gcode_file: Some("/data/Metadata/plate_1.gcode".to_owned()),
            subtask_name: Some("Cube".to_owned()),
            gcode_state: Some("RUNNING".to_owned()),
            percent: Some(42),
            speed_level: Some(3),
            remaining_time_minutes: Some(11),
            current_layer: Some(2),
            total_layers: Some(128),
            hms: Some(vec![crate::repositories::PrinterHms {
                attr: 83_887_616,
                code: 131_184,
            }]),
            diagnostics: Vec::new(),
            printer_materials_json: String::new(),
            observed_at: "2026-07-10T01:02:03Z".to_owned(),
        })
        .await
        .unwrap();
    let expected_revision = applied.printer.unwrap().state_revision;

    let (list_status, list_body) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers"),
        None,
        &token,
    )
    .await;
    let (detail_status, detail_body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(detail_status, StatusCode::OK);
    let list_json = list_body["printers"][0].clone();
    assert_eq!(list_json, detail_body);
    for private_field in ["host", "access_code"] {
        assert!(list_json.get(private_field).is_none());
    }
    for private_field in [
        "job_attr",
        "error_task_generation",
        "error_session_id",
        "error_received_at",
    ] {
        assert!(list_json["print"].get(private_field).is_none());
    }
    assert_eq!(list_json["print"]["subtask_id"], Value::Null);
    assert!(
        !serde_json::to_string(&list_json)
            .unwrap()
            .contains("SECRET")
    );

    let printer = decode::<PrinterResponse>(detail_body);
    assert_eq!(printer.state_revision, expected_revision);
    assert_eq!(printer.print.task_generation, 1);
    assert_eq!(printer.print.error_generation, 1);
    assert_eq!(printer.print.job_state, Some((0x00b0 >> 4) & 0x0f));
    assert_eq!(printer.print.gcode_state.as_deref(), Some("RUNNING"));
    assert_eq!(printer.print.task_id.as_deref(), Some("task-42"));
    assert_eq!(printer.print.subtask_id, None);
    assert_eq!(printer.print.progress_percent, Some(42));
    assert_eq!(printer.print.speed_level, Some(3));
    assert_eq!(printer.print.remaining_time_minutes, Some(11));
    assert_eq!(printer.print.current_layer, Some(2));
    assert_eq!(printer.print.total_layers, Some(128));
    assert_eq!(
        printer.print.gcode_file.as_deref(),
        Some("/data/Metadata/plate_1.gcode")
    );
    assert_eq!(printer.print.subtask_name.as_deref(), Some("Cube"));
    assert_eq!(printer.print.print_error, Some(83_918_929));
    assert_eq!(printer.print.printer_job_id.as_deref(), Some(""));
    assert_eq!(
        printer.print.hms,
        vec![crate::repositories::PrinterHms {
            attr: 83_887_616,
            code: 131_184,
        }]
    );
}
