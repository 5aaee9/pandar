use super::*;

#[tokio::test]
async fn printer_control_rejects_invalid_action_and_speed_payloads() {
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

    for payload in [
        printer_control_value(PrinterControlRequest::action("dance")),
        printer_control_value(PrinterControlRequest::action("set_print_speed")),
        printer_control_value(PrinterControlRequest::set_print_speed(0)),
        printer_control_value(PrinterControlRequest::set_print_speed(5)),
        printer_control_value(PrinterControlRequest::set_fan_speed(0, 50, false)),
        printer_control_value(PrinterControlRequest::set_fan_speed(4, 50, true)),
        printer_control_value(PrinterControlRequest::set_fan_speed(1, 101, false)),
        printer_control_value(PrinterControlRequest::action("select_extruder")),
        printer_control_value(PrinterControlRequest::select_extruder(2)),
        printer_control_value(PrinterControlRequest::action("pause").with_speed_mode(2)),
        printer_control_value(PrinterControlRequest::action("pause").with_raw_command("M400")),
        printer_control_value(PrinterControlRequest::move_axes(Vec::new(), None)),
        printer_control_value(PrinterControlRequest::move_axes(
            vec![move_axis("x", 0.0)],
            None,
        )),
        printer_control_value(PrinterControlRequest::move_axes(
            vec![move_axis("a", 5.0)],
            None,
        )),
        printer_control_value(PrinterControlRequest::move_axes(
            vec![move_axis("x", 5.0), move_axis("x", 6.0)],
            None,
        )),
        printer_control_value(PrinterControlRequest::set_hotend_temperature(
            301, None, None,
        )),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
            Some(payload),
            &token,
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            decode::<ErrorResponse>(body).error,
            "invalid_printer_control"
        );
        assert_eq!(state.commands().count().await.unwrap(), 0);
        assert_no_printer_control_audit(&state, tenant_id).await;
    }
}

#[tokio::test]
async fn printer_control_accepts_semantic_home_move_and_hotend_operations() {
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

    for (payload, expected_type) in [
        (
            printer_control_value(PrinterControlRequest::home(vec!["x", "z"])),
            "home",
        ),
        (
            printer_control_value(PrinterControlRequest::move_axes(
                vec![move_axis("x", 10.0), move_axis("z", -1.0)],
                Some(1200),
            )),
            "move_axes",
        ),
        (
            printer_control_value(PrinterControlRequest::set_hotend_temperature(
                215,
                Some(true),
                Some(1),
            )),
            "set_hotend_temperature",
        ),
        (
            printer_control_value(PrinterControlRequest::set_temperature(
                "set_bed_temperature",
                75,
            )),
            "set_bed_temperature",
        ),
        (
            printer_control_value(PrinterControlRequest::set_temperature(
                "set_chamber_temperature",
                45,
            )),
            "set_chamber_temperature",
        ),
        (
            printer_control_value(PrinterControlRequest::set_chamber_light(true)),
            "set_chamber_light",
        ),
        (
            printer_control_value(PrinterControlRequest::set_fan_speed(2, 50, true)),
            "set_fan_speed",
        ),
        (
            printer_control_value(PrinterControlRequest::action("toggle_light")),
            "toggle_light",
        ),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
            Some(payload),
            &token,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let body = decode::<CommandResponse>(body);
        assert_eq!(body.kind, "printer_operation");
        let payload: PrinterOperationPayload = serde_json::from_str(&body.payload_json).unwrap();
        assert_eq!(payload.operation.kind, expected_type);
        if expected_type == "set_hotend_temperature" {
            assert_eq!(payload.operation.extruder_id, Some(1));
        }
        if expected_type == "set_chamber_light" {
            assert_eq!(payload.operation.on, Some(true));
        }
        if expected_type == "set_fan_speed" {
            assert_eq!(payload.operation.fan_index, Some(2));
            assert_eq!(payload.operation.speed_percent, Some(50));
            assert_eq!(payload.operation.airduct, Some(true));
        }
    }
}

#[tokio::test]
async fn printer_control_accepts_h2c_rack_operations() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("H2C"),
    )
    .await
    .unwrap();

    for (payload, expected_type, expected_holder_action, expected_nozzle_id) in [
        (
            printer_control_value(PrinterControlRequest::nozzle_holder_ctrl(2)),
            "nozzle_holder_ctrl",
            Some(2),
            None,
        ),
        (
            printer_control_value(PrinterControlRequest::rack_nozzle_operation(
                "nozzle_info_confirm",
                0xff,
            )),
            "nozzle_info_confirm",
            None,
            Some(0xff),
        ),
        (
            printer_control_value(PrinterControlRequest::rack_nozzle_operation(
                "holder_nozzle_refresh",
                17,
            )),
            "holder_nozzle_refresh",
            None,
            Some(17),
        ),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
            Some(payload),
            &token,
        )
        .await;

        assert_eq!(status, StatusCode::OK, "{expected_type}: {body}");
        let body = decode::<CommandResponse>(body);
        assert_eq!(body.kind, "printer_operation");
        let payload: PrinterOperationPayload = serde_json::from_str(&body.payload_json).unwrap();
        assert_eq!(payload.operation.kind, expected_type);
        assert_eq!(payload.operation.action, expected_holder_action);
        assert_eq!(payload.operation.id, expected_nozzle_id);
    }

    let events = state
        .audit_events()
        .list_for_tenant(tenant_id)
        .await
        .unwrap();
    let rack_audit = events
        .iter()
        .filter(|event| event.action == "printer.dispatch_control")
        .count();
    assert_eq!(rack_audit, 3);
}

#[tokio::test]
async fn printer_control_rejects_invalid_h2c_rack_payloads() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("H2C"),
    )
    .await
    .unwrap();

    for payload in [
        printer_control_value(PrinterControlRequest::action("nozzle_holder_ctrl")),
        printer_control_value(PrinterControlRequest::nozzle_holder_ctrl(3)),
        printer_control_value(PrinterControlRequest::nozzle_holder_ctrl(1).with_nozzle_id(16)),
        printer_control_value(PrinterControlRequest::action("nozzle_info_confirm")),
        printer_control_value(PrinterControlRequest::rack_nozzle_operation(
            "nozzle_info_confirm",
            15,
        )),
        printer_control_value(PrinterControlRequest::rack_nozzle_operation(
            "holder_nozzle_refresh",
            22,
        )),
        printer_control_value(
            PrinterControlRequest::rack_nozzle_operation("holder_nozzle_refresh", 16)
                .with_speed_mode(2),
        ),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
            Some(payload),
            &token,
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(
            decode::<ErrorResponse>(body).error,
            "invalid_printer_control"
        );
        assert_eq!(state.commands().count().await.unwrap(), 0);
        assert_no_printer_control_audit(&state, tenant_id).await;
    }
}

#[tokio::test]
async fn printer_control_rejects_rack_operations_for_non_h2c_printer() {
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
        printer_control_body(PrinterControlRequest::nozzle_holder_ctrl(0)),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        decode::<ErrorResponse>(body).error,
        "printer_control_unavailable"
    );
    assert_eq!(state.commands().count().await.unwrap(), 0);
    assert_no_printer_control_audit(&state, tenant_id).await;
}

pub(super) async fn assert_no_printer_control_audit(state: &AppState, tenant_id: TenantId) {
    assert!(
        state
            .audit_events()
            .list_for_tenant(tenant_id)
            .await
            .unwrap()
            .iter()
            .all(|event| event.action != "printer.dispatch_control")
    );
}
