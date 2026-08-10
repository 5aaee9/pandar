use super::*;

#[tokio::test]
async fn grpc_print_job_report_preserves_raw_task_id_and_hms_presence() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let token = register_test_session(&state, tenant_id, agent_id).await;
    handle_snapshot(
        &state,
        tenant_id,
        agent_id,
        token,
        crate::grpc::tests::printer_snapshots::snapshot("serial", "Printer", "A1", "IDLE"),
    )
    .await
    .unwrap();

    handle_print_report(
        &state,
        tenant_id,
        agent_id,
        token,
        PrintJobReport {
            serial: "serial".to_string(),
            job_id: "external-mqtt-task".to_string(),
            gcode_state: "RUNNING".to_string(),
            percent: 42,
            has_percent: true,
            speed_level: 3,
            has_speed_level: true,
            print_error: 0,
            has_print_error: true,
            printer_job_id: String::new(),
            has_printer_job_id: true,
            job_attr: 0x21,
            has_job_attr: true,
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
    assert_eq!(current.live_status.speed_level, Some(3));
    assert_eq!(current.live_status.print_error, Some(0));
    assert_eq!(current.live_status.printer_job_id.as_deref(), Some(""));
    assert_eq!(current.live_status.job_attr, Some(0x21));
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
        token,
        PrintJobReport {
            serial: "serial".to_string(),
            print_error: 83_918_929,
            has_print_error: true,
            printer_job_id: "studio-job-1".to_string(),
            has_printer_job_id: true,
            job_attr: 0x21,
            has_job_attr: true,
            observed_at: "2026-07-09T10:01:00Z".to_string(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let seeded = state
        .printers()
        .list_with_live_status_for_tenant(tenant_id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(seeded.live_status.print_error, Some(83_918_929));
    assert_eq!(
        seeded.live_status.printer_job_id.as_deref(),
        Some("studio-job-1")
    );
    assert_eq!(seeded.live_status.progress_percent, Some(42));
    assert_eq!(seeded.live_status.error_generation, 1);
    assert_eq!(seeded.live_status.error_task_generation, Some(1));
    assert_eq!(
        seeded.live_status.error_session_id.as_deref(),
        Some(token.persisted_id().as_str())
    );
    assert_ne!(
        seeded.live_status.error_received_at.as_deref(),
        Some("2026-07-09T10:01:00Z")
    );

    handle_print_report(
        &state,
        tenant_id,
        agent_id,
        token,
        PrintJobReport {
            serial: "serial".to_string(),
            print_error: 17,
            has_print_error: false,
            printer_job_id: "conflicting-job".to_string(),
            has_printer_job_id: false,
            percent: 64,
            has_percent: true,
            observed_at: "2026-07-09T10:02:00Z".to_string(),
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
    assert_eq!(preserved.live_status.print_error, Some(83_918_929));
    assert_eq!(
        preserved.live_status.printer_job_id.as_deref(),
        Some("studio-job-1")
    );
    assert_eq!(preserved.live_status.progress_percent, Some(64));
    assert_eq!(preserved.live_status.job_attr, Some(0x21));

    handle_print_report(
        &state,
        tenant_id,
        agent_id,
        token,
        PrintJobReport {
            serial: "serial".to_owned(),
            print_error: u32::MAX,
            has_print_error: true,
            percent: 73,
            has_percent: true,
            job_attr: 0,
            has_job_attr: true,
            has_hms: true,
            observed_at: "2026-07-09T10:03:00Z".to_owned(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let boundary = state
        .printers()
        .list_with_live_status_for_tenant(tenant_id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(boundary.live_status.print_error, Some(83_918_929));
    assert_eq!(boundary.live_status.progress_percent, Some(73));
    assert_eq!(
        boundary.live_status.printer_job_id.as_deref(),
        Some("studio-job-1")
    );
    assert_eq!(boundary.live_status.job_attr, Some(0));
    assert!(boundary.live_status.hms.is_empty());
}
