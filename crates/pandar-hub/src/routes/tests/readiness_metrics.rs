use super::*;

#[tokio::test]
async fn readyz_reports_disabled_external_auth_as_ready() {
    let (status, body) = request(router(raw_state().await), Method::GET, "/readyz", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ready");
    assert_eq!(body["checks"]["database"]["ready"], true);
    assert_eq!(body["checks"]["spool"]["ready"], true);
    assert_eq!(body["checks"]["external_auth"]["ready"], true);
    assert_eq!(body["checks"]["external_auth"]["detail"], "disabled");
}

#[tokio::test]
async fn readyz_reports_spool_failure() {
    let database = crate::db::Database::connect(
        &crate::db::DatabaseConfig::from_url("sqlite::memory:").unwrap(),
    )
    .await
    .unwrap();
    database.migrate().await.unwrap();
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("not-a-directory");
    std::fs::write(&file_path, b"file").unwrap();
    let storage = crate::jobs::JobStorageConfig::new(&file_path, 1024).unwrap();
    let app = router(AppState::from_database(database, storage));

    let (status, body) = request(app, Method::GET, "/readyz", None).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["status"], "not_ready");
    assert_eq!(body["checks"]["spool"]["ready"], false);
}

#[tokio::test]
async fn metrics_redacts_tenant_ids_and_reports_required_series() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("metrics-acme", "Metrics Acme")
        .await
        .unwrap();
    let _subscription = state.printer_events().track_subscription(tenant.id).await;
    state
        .metrics()
        .record_print_report(crate::metrics::PrintReportMetric::Accepted);
    state
        .metrics()
        .record_print_report(crate::metrics::PrintReportMetric::Rejected);
    let issued = state.printer_events().issue_ticket(tenant.id).await;
    assert!(
        state
            .printer_events()
            .consume_ticket(tenant.id, &issued.ticket)
            .await
    );
    assert!(
        !state
            .printer_events()
            .consume_ticket(tenant.id, "missing-ticket")
            .await
    );
    let tenant_id_hash = crate::metrics::tenant_id_hash(tenant.id);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();

    for metric in [
        "pandar_agent_sessions",
        "pandar_commands_total",
        "pandar_websocket_subscriptions",
        "pandar_websocket_tickets_total",
        "pandar_jobs_total",
        "pandar_print_reports_total",
        "pandar_readyz",
    ] {
        assert!(text.contains(metric), "missing metric {metric}: {text}");
    }
    assert!(text.contains("pandar_websocket_tickets_total{result=\"issued\"} 1"));
    assert!(text.contains("pandar_websocket_tickets_total{result=\"consumed\"} 1"));
    assert!(text.contains("pandar_websocket_tickets_total{result=\"invalid\"} 1"));
    assert!(text.contains("pandar_print_reports_total{result=\"accepted\"} 1"));
    assert!(text.contains("pandar_print_reports_total{result=\"rejected\"} 1"));
    assert!(text.contains(&format!(
        "pandar_websocket_subscriptions{{tenant_id_hash=\"{tenant_id_hash}\"}} 1"
    )));
    assert!(!text.contains(&tenant.id.to_string()));
    assert!(!text.contains(&issued.ticket));
}
