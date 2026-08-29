use super::*;

#[tokio::test]
async fn printer_control_enqueues_ams_slot_operation() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("Bambu Lab X2D"),
    )
    .await
    .unwrap();

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
        printer_ams_load_body(0, 1, 1, 0),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.kind, "printer_operation");
    let payload: PrinterOperationPayload = serde_json::from_str(&body.payload_json).unwrap();
    match payload.operation {
        PrinterOperation::AmsLoadFilament {
            ams_id,
            slot_id,
            global_tray_id,
            extruder_id,
        } => {
            assert_eq!(ams_id, 0);
            assert_eq!(slot_id, 1);
            assert_eq!(global_tray_id, 1);
            assert_eq!(extruder_id, 0);
        }
        other => panic!("expected ams_load_filament operation, got {other:?}"),
    }
}

#[tokio::test]
async fn printer_control_enqueues_ams_drying_operations() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("Bambu Lab P2S"),
    )
    .await
    .unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
        printer_ams_start_drying_body(65, 8),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.kind, "printer_operation");
    let payload: PrinterOperationPayload = serde_json::from_str(&body.payload_json).unwrap();
    match payload.operation {
        PrinterOperation::AmsStartDrying {
            ams_id,
            temperature_celsius,
            duration_hours,
            filament,
            rotate_tray,
        } => {
            assert_eq!(ams_id, 0);
            assert_eq!(temperature_celsius, 65);
            assert_eq!(duration_hours, 8);
            assert_eq!(filament, "PETG");
            assert!(rotate_tray);
        }
        other => panic!("expected ams_start_drying operation, got {other:?}"),
    }

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
        printer_ams_stop_drying_body(),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    let payload: PrinterOperationPayload = serde_json::from_str(&body.payload_json).unwrap();
    match payload.operation {
        PrinterOperation::AmsStopDrying { ams_id } => assert_eq!(ams_id, 0),
        other => panic!("expected ams_stop_drying operation, got {other:?}"),
    }
}

#[tokio::test]
async fn printer_control_rejects_invalid_ams_drying_params() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("Bambu Lab P2S"),
    )
    .await
    .unwrap();

    for body in [
        printer_ams_start_drying_body(30, 8),
        printer_ams_start_drying_body(90, 8),
        printer_ams_start_drying_body(65, 0),
        printer_ams_start_drying_body(65, 48),
    ] {
        let (status, _) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
            body,
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn tenant_printer_control_rejects_gcode_line_without_insert() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("A1"),
    )
    .await
    .unwrap();

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
        Some(serde_json::json!({
            "action": "gcode_line",
            "param": "M620 C1 \n",
        })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        decode::<ErrorResponse>(body).error,
        "invalid_printer_control"
    );
    assert_eq!(state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn printer_control_enqueues_select_extruder_operation() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("Bambu Lab X2D"),
    )
    .await
    .unwrap();

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
        printer_select_extruder_body(1),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.kind, "printer_operation");
    let payload: PrinterOperationPayload = serde_json::from_str(&body.payload_json).unwrap();
    match payload.operation {
        PrinterOperation::SelectExtruder { extruder_id } => assert_eq!(extruder_id, 1),
        other => panic!("expected select_extruder operation, got {other:?}"),
    }
}
