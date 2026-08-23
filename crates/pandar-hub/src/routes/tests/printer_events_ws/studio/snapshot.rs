use super::*;

#[tokio::test]
async fn studio_projection_snapshot_matches_plugin_printer_list() {
    let fixture = studio_fixture("studio-snapshot-acme").await;
    let http_addr = serve_http(router(fixture.state.clone())).await;
    let mut ws = connect_studio(
        http_addr,
        fixture.tenant.id,
        "?projection=studio&version=1",
        Some(&fixture.token),
    )
    .await;

    let StudioFrame::SnapshotBegin { version } = next_frame(&mut ws, "begin").await else {
        panic!("expected snapshot_begin");
    };
    assert_eq!(version, 1);
    let StudioFrame::PrinterUpsert { printer } = next_frame(&mut ws, "upsert").await else {
        panic!("expected printer_upsert");
    };
    let StudioFrame::SnapshotEnd = next_frame(&mut ws, "end").await else {
        panic!("expected snapshot_end");
    };
    assert_studio_quiet(&mut ws, "after snapshot_end").await;

    let (status, body) = request_as(
        router(fixture.state.clone()),
        Method::GET,
        "/api/v1/plugin/printers",
        None,
        &fixture.token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let devices = body["devices"].as_array().unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(
        devices[0], printer,
        "streamed record must match plugin list record"
    );
    assert_eq!(
        printer["dev_id"],
        Value::String(fixture.serial_number.clone())
    );
    assert_eq!(
        printer["pandar_printer_id"],
        Value::String(fixture.printer_id.clone())
    );
}

#[tokio::test]
async fn studio_projection_empty_tenant_sends_only_begin_and_end() {
    let state = state().await;
    let tenant = state
        .tenants()
        .create("studio-empty", "Studio Empty")
        .await
        .unwrap();
    let token =
        plugin_studio_tenant_token(&state, &tenant.id.to_string(), "studio-empty-token").await;
    let http_addr = serve_http(router(state.clone())).await;
    let mut ws = connect_studio(
        http_addr,
        tenant.id,
        "?projection=studio&version=1",
        Some(&token),
    )
    .await;

    let StudioFrame::SnapshotBegin { version } = next_frame(&mut ws, "begin").await else {
        panic!("expected snapshot_begin");
    };
    assert_eq!(version, 1);
    let StudioFrame::SnapshotEnd = next_frame(&mut ws, "end").await else {
        panic!("expected snapshot_end");
    };
    assert_studio_quiet(&mut ws, "empty tenant after end").await;
}

#[tokio::test]
async fn studio_projection_rejects_unauthorized_and_unsupported_requests_before_upgrade() {
    let fixture = studio_fixture("studio-auth-acme").await;
    let other = fixture
        .state
        .tenants()
        .create("studio-auth-beta", "Studio Auth Beta")
        .await
        .unwrap();
    let other_plugin_token =
        plugin_studio_tenant_token(&fixture.state, &other.id.to_string(), "beta-plugin-token")
            .await;
    let viewer_token = auth_token_for_role(
        &fixture.state,
        &fixture.tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "studio-viewer-token",
    )
    .await;
    let app = router(fixture.state.clone());
    let studio_path = format!(
        "/api/v1/tenants/{}/printer-events?projection=studio&version=1",
        fixture.tenant.id
    );

    let (status, body) = request(app.clone(), Method::GET, &studio_path, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(decode::<ErrorResponse>(body).error, "missing_auth_token");

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        &studio_path,
        None,
        "test_tenant_garbage_token",
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_auth_token");

    for token in [&other_plugin_token, &viewer_token] {
        let (status, body) = request_as(app.clone(), Method::GET, &studio_path, None, token).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");
    }

    // PluginStudio tokens must not grant the default (ticket/Viewer) stream.
    let default_path = format!("/api/v1/tenants/{}/printer-events", fixture.tenant.id);
    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        &default_path,
        None,
        &fixture.token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");

    for query in [
        "?projection=studio&version=2",
        "?projection=studio&version=abc",
        "?projection=studio&version=999999999999999999999999999999",
        "?projection=studio",
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::GET,
            &format!("{default_path}{query}"),
            None,
            &fixture.token,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            decode::<ErrorResponse>(body).error,
            "unsupported_printer_event_version"
        );
    }
}

#[tokio::test]
async fn studio_projection_change_published_during_snapshot_arrives_after_snapshot_end_once() {
    let fixture = studio_fixture("studio-sync-acme").await;
    let _control_plane = start_control_plane(fixture.state.clone()).await;
    let http_addr = serve_http(router(fixture.state.clone())).await;
    let mut pause = crate::routes::printer_events::send_pause::install_during_flush();
    let mut ws = connect_studio(
        http_addr,
        fixture.tenant.id,
        "?projection=studio&version=1",
        Some(&fixture.token),
    )
    .await;

    // The first flushed frame is snapshot_begin; mutate the authoritative
    // record while it is in flight so the buffered post-snapshot upsert differs.
    pause.wait_until_reached().await;
    let previous = fixture
        .state
        .printers()
        .get_for_tenant(fixture.tenant.id, &fixture.printer_id)
        .await
        .unwrap()
        .unwrap();
    fixture
        .state
        .printers()
        .update_details_with_audit(
            fixture.tenant.id,
            &fixture.printer_id,
            "Renamed during snapshot".to_owned(),
            previous.host.unwrap_or_default(),
            previous.access_code.unwrap_or_default(),
            test_audit_actor(),
        )
        .await
        .unwrap();
    fixture
        .state
        .publish_printer_projection_change(
            fixture.tenant.id,
            &fixture.printer_id,
            &fixture.serial_number,
        )
        .await;
    pause.resume();

    let StudioFrame::SnapshotBegin { version } = next_frame(&mut ws, "begin").await else {
        panic!("expected snapshot_begin");
    };
    assert_eq!(version, 1);
    let StudioFrame::PrinterUpsert { printer } = next_frame(&mut ws, "snapshot upsert").await
    else {
        panic!("expected snapshot upsert");
    };
    assert_eq!(
        printer["pandar_printer_id"],
        Value::String(fixture.printer_id.clone())
    );
    let StudioFrame::SnapshotEnd = next_frame(&mut ws, "end").await else {
        panic!("expected snapshot_end");
    };

    // The buffered change resolves to exactly one upsert after the snapshot.
    let StudioFrame::PrinterUpsert { printer } = next_frame(&mut ws, "live change").await else {
        panic!("expected live change upsert after snapshot_end");
    };
    assert_eq!(
        printer["pandar_printer_id"],
        Value::String(fixture.printer_id.clone())
    );
    assert_eq!(printer["name"], "Renamed during snapshot");
    assert_studio_quiet(&mut ws, "no duplicate change").await;
}
