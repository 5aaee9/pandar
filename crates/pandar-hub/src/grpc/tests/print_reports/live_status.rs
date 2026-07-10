use super::*;

#[tokio::test]
async fn grpc_print_job_report_preserves_raw_task_id_and_hms_presence() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    handle_snapshot(
        &state,
        tenant_id,
        agent_id,
        crate::grpc::tests::printer_snapshots::snapshot("serial", "Printer", "A1", "IDLE"),
    )
    .await
    .unwrap();

    handle_print_report(
        &state,
        tenant_id,
        agent_id,
        PrintJobReport {
            serial: "serial".to_string(),
            job_id: "external-mqtt-task".to_string(),
            gcode_state: "RUNNING".to_string(),
            percent: 42,
            has_percent: true,
            hms: vec![PrinterHmsItem {
                attr: 0x0102_0304,
                code: 0x0506_0708,
            }],
            has_hms: true,
            observed_at: "2026-07-09T10:00:00Z".to_string(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let current = state
        .printers()
        .list_with_live_status_for_tenant(tenant_id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        current.live_status.task_id.as_deref(),
        Some("external-mqtt-task")
    );
    assert_eq!(current.live_status.progress_percent, Some(42));
    assert_eq!(
        current.live_status.hms,
        vec![crate::repositories::PrinterHms {
            attr: 0x0102_0304,
            code: 0x0506_0708,
        }]
    );

    handle_print_report(
        &state,
        tenant_id,
        agent_id,
        PrintJobReport {
            serial: "serial".to_string(),
            observed_at: "2026-07-09T10:01:00Z".to_string(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let preserved = state
        .printers()
        .list_with_live_status_for_tenant(tenant_id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(preserved.live_status, current.live_status);

    handle_print_report(
        &state,
        tenant_id,
        agent_id,
        PrintJobReport {
            serial: "serial".to_string(),
            has_hms: true,
            observed_at: "2026-07-09T10:02:00Z".to_string(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let cleared = state
        .printers()
        .list_with_live_status_for_tenant(tenant_id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(cleared.live_status.progress_percent, Some(42));
    assert!(cleared.live_status.hms.is_empty());
}
