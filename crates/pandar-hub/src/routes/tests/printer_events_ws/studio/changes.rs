use super::*;

#[tokio::test]
async fn studio_projection_suppresses_noop_invalidations() {
    let fixture = studio_fixture("studio-noop-acme").await;
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

    fixture
        .state
        .publish_printer_projection_change(
            fixture.tenant.id,
            &fixture.printer_id,
            &fixture.serial_number,
        )
        .await;
    assert_studio_quiet(&mut ws, "noop invalidation").await;
}

#[tokio::test]
async fn studio_projection_publishes_renames_but_not_connection_only_metadata() {
    let fixture = studio_fixture("studio-metadata-acme").await;
    let token = all_scope_tenant_token(
        &fixture.state,
        &fixture.tenant.id.to_string(),
        "metadata-admin-token",
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

    let path = format!(
        "/api/v1/tenants/{}/printers/{}",
        fixture.tenant.id, fixture.printer_id
    );
    let (status, _) = request_as(
        router(fixture.state.clone()),
        Method::PATCH,
        &path,
        Some(serde_json::json!({
            "host": "192.168.2.55",
            "access_code": "87654321",
            "name": "Renamed printer"
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let StudioFrame::PrinterUpsert { printer } = next_frame(&mut ws, "rename").await else {
        panic!("expected rename upsert");
    };
    assert_eq!(printer["name"], "Renamed printer");

    let (status, _) = request_as(
        router(fixture.state.clone()),
        Method::PATCH,
        &path,
        Some(serde_json::json!({
            "host": "192.168.2.99",
            "access_code": "11223344",
            "name": "Renamed printer"
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_studio_quiet(&mut ws, "connection-only metadata update").await;
}

#[tokio::test]
async fn studio_projection_lag_before_snapshot_end_closes_without_committing() {
    let mut fixture = studio_fixture("studio-initial-lag-acme").await;
    let event_hub = crate::printer_events::PrinterEventHub::with_capacity_for_tests(1);
    fixture.state = fixture
        .state
        .with_printer_events_for_tests(event_hub.clone());
    let http_addr = serve_http(router(fixture.state.clone())).await;
    let mut pause = crate::routes::printer_events::send_pause::install_during_flush();
    let mut ws = connect_studio(
        http_addr,
        fixture.tenant.id,
        "?projection=studio&version=1",
        Some(&fixture.token),
    )
    .await;

    pause.wait_until_reached().await;
    for _ in 0..2 {
        event_hub
            .publish_local_projection_change(
                fixture.tenant.id,
                crate::printer_events::PrinterProjectionChange {
                    printer_id: fixture.printer_id.clone(),
                    serial_number: fixture.serial_number.clone(),
                },
            )
            .await;
    }
    pause.resume();

    let StudioFrame::SnapshotBegin { .. } = next_frame(&mut ws, "begin before lag").await else {
        panic!("expected snapshot_begin");
    };
    let StudioFrame::PrinterUpsert { .. } = next_frame(&mut ws, "upsert before lag").await else {
        panic!("expected snapshot upsert");
    };
    let closed = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Text(text))) => {
                    assert!(
                        !text.contains("snapshot_end"),
                        "stale snapshot committed: {text}"
                    );
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(other)) => panic!("unexpected frame before lag close: {other:?}"),
            }
        }
    })
    .await;
    assert!(closed.is_ok(), "lagged initial stream stayed open");
}

#[tokio::test]
async fn studio_projection_epoch_change_drops_a_serialized_live_upsert() {
    let fixture = studio_fixture("studio-live-epoch-acme").await;
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

    let mut pause = crate::routes::printer_events::send_pause::install_after_serialization();
    rename_studio_printer(&fixture, "Epoch rename").await;
    fixture
        .state
        .printer_events()
        .publish_local_projection_change(
            fixture.tenant.id,
            crate::printer_events::PrinterProjectionChange {
                printer_id: fixture.printer_id.clone(),
                serial_number: fixture.serial_number.clone(),
            },
        )
        .await;
    pause.wait_until_reached().await;
    fixture
        .state
        .printer_events()
        .invalidate_epoch(fixture.tenant.id);
    pause.resume();

    assert_socket_closed_without_text(&mut ws, "studio epoch change after serialization").await;
}

#[tokio::test]
async fn studio_projection_lag_closes_without_forwarding_a_newest_only_suffix() {
    let mut fixture = studio_fixture("studio-live-lag-acme").await;
    let event_hub = crate::printer_events::PrinterEventHub::with_capacity_for_tests(1);
    fixture.state = fixture
        .state
        .with_printer_events_for_tests(event_hub.clone());
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

    let mut pause = crate::routes::printer_events::send_pause::install_during_flush();
    rename_studio_printer(&fixture, "First rename").await;
    event_hub
        .publish_local_projection_change(
            fixture.tenant.id,
            crate::printer_events::PrinterProjectionChange {
                printer_id: fixture.printer_id.clone(),
                serial_number: fixture.serial_number.clone(),
            },
        )
        .await;
    pause.wait_until_reached().await;
    for name in ["Second rename", "Newest rename"] {
        rename_studio_printer(&fixture, name).await;
        event_hub
            .publish_local_projection_change(
                fixture.tenant.id,
                crate::printer_events::PrinterProjectionChange {
                    printer_id: fixture.printer_id.clone(),
                    serial_number: fixture.serial_number.clone(),
                },
            )
            .await;
    }
    pause.resume();

    let StudioFrame::PrinterUpsert { printer } = next_frame(&mut ws, "first live upsert").await
    else {
        panic!("expected first live upsert");
    };
    assert_eq!(printer["name"], "First rename");
    assert_socket_closed_without_text(&mut ws, "studio projection receiver lag").await;
}

async fn rename_studio_printer(fixture: &StudioFixture, name: &str) {
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
            name.to_owned(),
            previous.host.unwrap_or_default(),
            previous.access_code.unwrap_or_default(),
            test_audit_actor(),
        )
        .await
        .unwrap();
}
