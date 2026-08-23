use super::*;

#[tokio::test]
async fn studio_projection_receives_sibling_change_once_and_ignores_wrong_tenant() {
    let fixture = studio_fixture("studio-replica-acme").await;
    let other = fixture
        .state
        .tenants()
        .create("studio-replica-beta", "Studio Replica Beta")
        .await
        .unwrap();
    let other_agent = fixture
        .state
        .agents()
        .create(other.id, "beta-agent")
        .await
        .unwrap();
    let other_printer_id =
        insert_printer_fixture(fixture.state.database(), other.id, other_agent.id)
            .await
            .unwrap();

    let sibling = sibling_state(&fixture.state);
    let _control_plane = start_control_plane(sibling.clone()).await;
    let http_addr = serve_http(router(sibling)).await;
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

    // A changed authoritative record published by the primary instance reaches
    // the sibling's studio stream exactly once.
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
            "Sibling rename".to_owned(),
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
    let StudioFrame::PrinterUpsert { printer } = next_frame(&mut ws, "sibling change").await else {
        panic!("expected sibling-driven upsert");
    };
    assert_eq!(
        printer["pandar_printer_id"],
        Value::String(fixture.printer_id.clone())
    );
    assert_eq!(printer["name"], "Sibling rename");
    assert_studio_quiet(&mut ws, "no duplicate sibling change").await;

    // Wrong-tenant changes never reach this stream.
    fixture
        .state
        .publish_printer_projection_change(other.id, &other_printer_id, "serial-beta")
        .await;
    assert_studio_quiet(&mut ws, "wrong-tenant change").await;
}

#[tokio::test]
async fn default_printer_event_stream_never_receives_studio_frames() {
    let fixture = studio_fixture("studio-regression-acme").await;
    let _control_plane = start_control_plane(fixture.state.clone()).await;
    let viewer_token = auth_token_for_role(
        &fixture.state,
        &fixture.tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "regression-viewer-token",
    )
    .await;
    let http_addr = serve_http(router(fixture.state.clone())).await;
    let mut ws = connect_printer_events(http_addr, fixture.tenant.id, &viewer_token).await;

    // Drive both transports: a projection change and a regular printer event.
    fixture
        .state
        .publish_printer_projection_change(
            fixture.tenant.id,
            &fixture.printer_id,
            &fixture.serial_number,
        )
        .await;
    let printer = fixture
        .state
        .printers()
        .get_with_live_status_for_tenant(fixture.tenant.id, &fixture.printer_id)
        .await
        .unwrap()
        .unwrap();
    fixture
        .state
        .publish_printer_event(
            fixture.tenant.id,
            crate::printer_events::PrinterEvent::PrinterSnapshot {
                printer: Box::new(crate::printer_events::printer_event_printer(printer, None)),
            },
        )
        .await;

    // The default stream carries the printer snapshot but never a studio frame.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut saw_printer_snapshot = false;
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(200), ws.next()).await {
            Err(_) => break,
            Ok(None) => break,
            Ok(Some(Err(err))) => panic!("websocket error: {err}"),
            Ok(Some(Ok(Message::Text(text)))) => {
                let event = serde_json::from_str::<crate::printer_events::PrinterEvent>(&text)
                    .unwrap_or_else(|err| {
                        panic!("default stream leaked a non-default frame {text}: {err}")
                    });
                if matches!(
                    event,
                    crate::printer_events::PrinterEvent::PrinterSnapshot { .. }
                ) {
                    saw_printer_snapshot = true;
                }
            }
            Ok(Some(Ok(_))) => {}
        }
    }
    assert!(
        saw_printer_snapshot,
        "default stream should have received the regular printer snapshot"
    );
}
