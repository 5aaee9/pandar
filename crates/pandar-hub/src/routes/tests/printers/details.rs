use super::*;

#[tokio::test]
async fn update_printer_updates_details_without_agent_session() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    let access_code = "UPDATED-LINK-CODE";

    let (status, body) = request_as(
        app,
        Method::PATCH,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        update_printer_body("192.168.2.11", access_code, "Office A1 Updated"),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body_text = body.to_string();
    let body = decode::<PrinterResponse>(body);
    assert_eq!(body.name, "Office A1 Updated");
    assert!(!body_text.contains(access_code));

    let printer = state
        .printers()
        .get_for_tenant(tenant_id, &printer_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(printer.name, "Office A1 Updated");
    assert_eq!(printer.host.as_deref(), Some("192.168.2.11"));
    assert_eq!(printer.access_code.as_deref(), Some(access_code));

    let command = state
        .commands()
        .next_queued_for_agent(tenant_id, agent_id)
        .await
        .unwrap()
        .expect("printer update should enqueue a connection reload");
    assert_eq!(command.kind, "reload_printer_connection");
    assert_eq!(command.printer_id.as_deref(), Some(printer_id.as_str()));
    let payload: crate::repositories::ReloadPrinterConnectionPayload =
        serde_json::from_str(&command.payload_json).unwrap();
    assert_eq!(payload.printer_id, printer_id);
    assert_eq!(payload.serial_number, printer.serial_number);
    assert!(!command.payload_json.contains(access_code));
}

#[tokio::test]
async fn update_printer_keeps_existing_connection_when_fields_are_blank_without_agent_session() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    seed_printer_connection(
        state.database(),
        &printer_id,
        "192.168.2.10",
        "EXISTING-LINK-CODE",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::PATCH,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        update_printer_body(" ", "", "Office A1 Updated"),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body_text = body.to_string();
    let body = decode::<PrinterResponse>(body);
    assert_eq!(body.name, "Office A1 Updated");
    assert!(!body_text.contains("EXISTING-LINK-CODE"));
    let printer = state
        .printers()
        .get_for_tenant(tenant_id, &printer_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(printer.name, "Office A1 Updated");
    assert_eq!(printer.host.as_deref(), Some("192.168.2.10"));
    assert_eq!(printer.access_code.as_deref(), Some("EXISTING-LINK-CODE"));
}

#[tokio::test]
async fn update_printer_rejects_host_change_without_access_code() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    seed_printer_connection(
        state.database(),
        &printer_id,
        "192.168.2.10",
        "EXISTING-LINK-CODE",
    )
    .await;

    let (status, _) = request_as(
        app,
        Method::PATCH,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        update_printer_body("192.168.2.11", "", "Office A1 Updated"),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let printer = state
        .printers()
        .get_for_tenant(tenant_id, &printer_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(printer.host.as_deref(), Some("192.168.2.10"));
    assert_eq!(printer.access_code.as_deref(), Some("EXISTING-LINK-CODE"));
}

#[tokio::test]
async fn printer_routes_return_material_snapshots_without_credentials() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    state
        .materials()
        .upsert_from_patch(crate::repositories::MaterialPatchInput {
            tenant_id,
            agent_id,
            printer_id: printer_id.clone(),
            serial_number: "serial".to_string(),
            printer_materials_json: serde_json::to_string(&PrinterMaterialPatchFixture {
                kind: "printer_material_patch",
                observed_at: "2026-06-23T01:02:03Z",
                filament_switch_installed: true,
                ams_units: [PrinterMaterialPatchAmsUnit {
                    unit_id: "0",
                    trays: [PrinterMaterialPatchTray {
                        tray_id: "0",
                        filament_id: "GFL00",
                        material_type: "PLA",
                        color: "FF0000",
                        access_token: "secret-token",
                        auth: "secret-auth",
                        passwd: "secret-passwd",
                        access_code: "secret-access-code",
                    }],
                }],
                external_spools: [PrinterMaterialPatchExternalSpool {
                    external_id: "254",
                    tray_id: "0",
                    material_type: "PETG",
                }],
                active_tray: PrinterMaterialPatchActiveTray {
                    kind: "ams",
                    global_tray_id: 0,
                    ams_id: "0",
                    tray_id: "0",
                },
            })
            .unwrap(),
        })
        .await
        .unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.to_string().contains("secret-token"));
    assert!(!body.to_string().contains("secret-auth"));
    assert!(!body.to_string().contains("secret-passwd"));
    assert!(!body.to_string().contains("secret-access-code"));
    assert!(!body.to_string().contains("access_token"));
    assert!(!body.to_string().contains("auth"));
    assert!(!body.to_string().contains("passwd"));
    assert!(!body.to_string().contains("access_code"));
    let body = decode::<PrinterListResponse>(body);
    let materials = body.printers[0].materials.as_ref().unwrap();
    assert_eq!(materials.observed_at, "2026-06-23T01:02:03Z");
    assert_eq!(materials.filament_switch_installed, Some(true));
    assert_eq!(materials.ams_units[0].unit_id, "0");
    assert_eq!(materials.external_spools[0].external_id, "254");
    assert_eq!(materials.active_tray.as_ref().unwrap().kind, "ams");

    let (status, detail) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let detail = decode::<PrinterResponse>(detail);
    assert_eq!(detail.materials.as_ref(), Some(materials));
}
