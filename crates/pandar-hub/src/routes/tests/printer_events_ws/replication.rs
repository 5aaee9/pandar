use super::*;

#[tokio::test]
async fn printer_events_websocket_receives_event_from_sibling_instance() {
    let state = state().await;
    let sibling = sibling_state(&state);
    let _control_plane = start_control_plane(sibling.clone()).await;
    let app = router(sibling);
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "sibling-event-ws-token",
    )
    .await;
    let agent = state
        .agents()
        .create(tenant.id, "shop-agent")
        .await
        .unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();
    state
        .jobs()
        .apply_print_report(crate::repositories::ApplyPrintReport {
            tenant_id: tenant.id,
            agent_id: agent.id,
            serial: format!("serial-{printer_id}"),
            task_id: Some("external-task".to_owned()),
            job_id: None,
            print_error: Some(83_918_929),
            printer_job_id: Some(String::new()),
            job_attr: Some(0x21),
            artifact_id: None,
            subtask_id: None,
            gcode_file: None,
            subtask_name: None,
            gcode_state: Some("RUNNING".to_owned()),
            percent: Some(66),
            speed_level: Some(4),
            remaining_time_minutes: None,
            current_layer: None,
            total_layers: None,
            hms: Some(Vec::new()),
            diagnostics: Vec::new(),
            printer_materials_json: String::new(),
            observed_at: "2026-07-10T00:00:00Z".to_owned(),
        })
        .await
        .unwrap();
    let printer = state
        .printers()
        .get_with_live_status_for_tenant(tenant.id, &printer_id)
        .await
        .unwrap()
        .unwrap();
    let http_addr = serve_http(app).await;
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

    state
        .publish_printer_event(
            tenant.id,
            crate::printer_events::PrinterEvent::PrinterSnapshot {
                printer: Box::new(crate::printer_events::printer_event_printer(printer, None)),
            },
        )
        .await;

    let message = tokio::time::timeout(std::time::Duration::from_secs(1), ws.next())
        .await
        .expect("sibling websocket should receive event")
        .unwrap()
        .unwrap();
    let body = decode_ws_message::<WebSocketPrinterEvent>(message);
    let WebSocketPrinterEvent::PrinterSnapshot { printer } = body else {
        panic!("expected printer snapshot websocket event");
    };
    assert_eq!(printer.tenant_id, tenant.id.to_string());
    assert!(printer.state_revision > 1);
    assert_eq!(printer.print.task_generation, 1);
    assert_eq!(printer.print.error_generation, 1);
    assert_eq!(printer.print.job_state, Some(2));
    assert_eq!(printer.print.gcode_state.as_deref(), Some("RUNNING"));
    assert_eq!(printer.print.task_id.as_deref(), Some("external-task"));
    assert_eq!(printer.print.subtask_id, None);
    assert_eq!(printer.print.progress_percent, Some(66));
    assert_eq!(printer.print.remaining_time_minutes, None);
    assert_eq!(printer.print.current_layer, None);
    assert_eq!(printer.print.total_layers, None);
    assert_eq!(printer.print.gcode_file, None);
    assert_eq!(printer.print.subtask_name, None);
    assert_eq!(printer.print.print_error, Some(83_918_929));
    assert_eq!(printer.print.printer_job_id.as_deref(), Some(""));
    assert!(printer.print.hms.is_empty());
}

#[tokio::test]
async fn printer_events_websocket_receives_one_event_from_publishing_instance() {
    let state = state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "single-event-ws-token",
    )
    .await;
    let agent = state
        .agents()
        .create(tenant.id, "shop-agent")
        .await
        .unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();
    let printer = state
        .printers()
        .get_with_live_status_for_tenant(tenant.id, &printer_id)
        .await
        .unwrap()
        .unwrap();
    let http_addr = serve_http(app).await;
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

    state
        .publish_printer_event(
            tenant.id,
            crate::printer_events::PrinterEvent::PrinterSnapshot {
                printer: Box::new(crate::printer_events::printer_event_printer(printer, None)),
            },
        )
        .await;

    let message = tokio::time::timeout(std::time::Duration::from_secs(1), ws.next())
        .await
        .expect("websocket should receive event")
        .unwrap()
        .unwrap();
    assert!(matches!(message, Message::Text(_)));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), ws.next())
            .await
            .is_err(),
        "publishing instance should not deliver a duplicate event"
    );
}

#[tokio::test]
async fn printer_events_websocket_ignores_wrong_tenant_event_from_sibling_instance() {
    let state = state().await;
    let sibling = sibling_state(&state);
    let _control_plane = start_control_plane(sibling.clone()).await;
    let app = router(sibling);
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let other = state.tenants().create("beta", "Beta Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "wrong-tenant-event-ws-token",
    )
    .await;
    let agent = state
        .agents()
        .create(other.id, "other-agent")
        .await
        .unwrap();
    let printer_id = insert_printer_fixture(state.database(), other.id, agent.id)
        .await
        .unwrap();
    let printer = state
        .printers()
        .get_with_live_status_for_tenant(other.id, &printer_id)
        .await
        .unwrap()
        .unwrap();
    let http_addr = serve_http(app).await;
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

    state
        .publish_printer_event(
            other.id,
            crate::printer_events::PrinterEvent::PrinterSnapshot {
                printer: Box::new(crate::printer_events::printer_event_printer(printer, None)),
            },
        )
        .await;

    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), ws.next())
            .await
            .is_err(),
        "websocket should ignore wrong-tenant events"
    );
}
