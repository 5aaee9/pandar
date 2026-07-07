use super::*;
use serde::Deserialize;
use std::sync::LazyLock;
use tokio::sync::Mutex;

static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Deserialize)]
struct ReadyzResponse {
    status: String,
    checks: ReadyzChecks,
}

#[derive(Deserialize)]
struct ReadyzChecks {
    database: Option<ReadyzCheck>,
    artifact_storage: ReadyzCheck,
    external_auth: Option<ReadyzCheck>,
    spool: Option<ReadyzCheck>,
}

#[derive(Deserialize)]
struct ReadyzCheck {
    ready: bool,
    detail: Option<String>,
}

#[tokio::test]
async fn readyz_reports_disabled_external_auth_as_ready() {
    let (status, body) = request(router(raw_state().await), Method::GET, "/readyz", None).await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<ReadyzResponse>(body);
    assert_eq!(body.status, "ready");
    assert!(body.checks.database.unwrap().ready);
    assert!(body.checks.artifact_storage.ready);
    assert!(body.checks.spool.is_none());
    let external_auth = body.checks.external_auth.unwrap();
    assert!(external_auth.ready);
    assert_eq!(external_auth.detail.as_deref(), Some("disabled"));
}

#[tokio::test]
async fn readyz_reports_artifact_storage_failure() {
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
    let body = decode::<ReadyzResponse>(body);
    assert_eq!(body.status, "not_ready");
    assert!(!body.checks.artifact_storage.ready);
    assert!(body.checks.spool.is_none());
}

#[tokio::test]
async fn readyz_reports_filesystem_not_shared_for_postgres_nats() {
    let _env_lock = ENV_LOCK.lock().await;
    let _guard = EnvGuard::remove("PANDAR_ARTIFACT_FILESYSTEM_SHARED");
    let state = raw_state()
        .await
        .with_database_backend_for_tests(crate::db::DatabaseBackend::Postgres)
        .with_control_plane_for_tests(crate::cluster::ControlPlane::nats_for_tests());
    let app = router(state);

    let (status, body) = request(app, Method::GET, "/readyz", None).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let body = decode::<ReadyzResponse>(body);
    assert_eq!(body.status, "not_ready");
    assert!(!body.checks.artifact_storage.ready);
    assert_eq!(
        body.checks.artifact_storage.detail.as_deref(),
        Some("filesystem_not_shared")
    );
}

#[tokio::test]
async fn metrics_reports_ready_with_explicit_shared_filesystem_override() {
    let _env_lock = ENV_LOCK.lock().await;
    let _guard = EnvGuard::set("PANDAR_ARTIFACT_FILESYSTEM_SHARED", "true");
    let state = raw_state()
        .await
        .with_database_backend_for_tests(crate::db::DatabaseBackend::Postgres)
        .with_control_plane_for_tests(crate::cluster::ControlPlane::nats_for_tests());
    let app = router(state);

    let (status, body) = request(app.clone(), Method::GET, "/readyz", None).await;
    assert_eq!(status, StatusCode::OK);
    let body = decode::<ReadyzResponse>(body);
    assert!(body.checks.artifact_storage.ready);

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

    assert!(text.contains("pandar_readyz{check=\"artifact_storage\"} 1"));
    assert!(!text.contains("pandar_readyz{check=\"spool\"}"));
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
    let ticket = crate::repositories::generate_secret("pandar_ws");
    let issued = state
        .printer_event_tickets()
        .issue(tenant.id, crate::repositories::hash_secret(&ticket))
        .await
        .unwrap();
    state
        .metrics()
        .record_ticket(crate::metrics::TicketMetric::Issued);
    assert!(matches!(
        state
            .printer_event_tickets()
            .consume(tenant.id, &issued.ticket_hash)
            .await
            .unwrap(),
        crate::repositories::PrinterEventTicketConsumeResult::Consumed(_)
    ));
    state
        .metrics()
        .record_ticket(crate::metrics::TicketMetric::Consumed);
    assert!(matches!(
        state
            .printer_event_tickets()
            .consume(
                tenant.id,
                &crate::repositories::hash_secret("missing-ticket")
            )
            .await
            .unwrap(),
        crate::repositories::PrinterEventTicketConsumeResult::Invalid
    ));
    state
        .metrics()
        .record_ticket(crate::metrics::TicketMetric::Invalid);
    state
        .metrics()
        .record_control_plane(crate::metrics::ControlPlaneMetric::PublishOk);
    state
        .metrics()
        .record_control_plane(crate::metrics::ControlPlaneMetric::PublishFailed);
    state
        .metrics()
        .record_control_plane(crate::metrics::ControlPlaneMetric::ReceiveOk);
    state
        .metrics()
        .record_control_plane(crate::metrics::ControlPlaneMetric::ReceiveFailed);
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
        "pandar_control_plane_messages_total",
        "pandar_jobs_total",
        "pandar_print_reports_total",
        "pandar_readyz",
    ] {
        assert!(text.contains(metric), "missing metric {metric}: {text}");
    }
    assert!(text.contains("pandar_websocket_tickets_total{result=\"issued\"} 1"));
    assert!(text.contains("pandar_websocket_tickets_total{result=\"consumed\"} 1"));
    assert!(text.contains("pandar_websocket_tickets_total{result=\"invalid\"} 1"));
    assert!(text.contains("pandar_control_plane_messages_total{result=\"publish_ok\"} 1"));
    assert!(text.contains("pandar_control_plane_messages_total{result=\"publish_failed\"} 1"));
    assert!(text.contains("pandar_control_plane_messages_total{result=\"receive_ok\"} 1"));
    assert!(text.contains("pandar_control_plane_messages_total{result=\"receive_failed\"} 1"));
    assert!(text.contains("pandar_print_reports_total{result=\"accepted\"} 1"));
    assert!(text.contains("pandar_print_reports_total{result=\"rejected\"} 1"));
    assert!(text.contains("pandar_readyz{check=\"artifact_storage\"} 1"));
    assert!(!text.contains("pandar_readyz{check=\"spool\"}"));
    assert!(text.contains(&format!(
        "pandar_websocket_subscriptions{{tenant_id_hash=\"{tenant_id_hash}\"}} 1"
    )));
    assert!(!text.contains(&tenant.id.to_string()));
    assert!(!text.contains(&ticket));
}

struct EnvGuard {
    name: &'static str,
    previous: Option<String>,
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> T {
    decode_json(value)
}

impl EnvGuard {
    fn set(name: &'static str, value: &str) -> Self {
        let previous = std::env::var(name).ok();
        unsafe {
            std::env::set_var(name, value);
        }
        Self { name, previous }
    }

    fn remove(name: &'static str) -> Self {
        let previous = std::env::var(name).ok();
        unsafe {
            std::env::remove_var(name);
        }
        Self { name, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.previous {
                Some(value) => std::env::set_var(self.name, value),
                None => std::env::remove_var(self.name),
            }
        }
    }
}
