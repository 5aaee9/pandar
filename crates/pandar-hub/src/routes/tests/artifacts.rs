use crate::{artifacts::FilesystemArtifactStorage, repositories::AuditActor};
use axum::http::header::CONTENT_TYPE;

use support::{FakeArtifactStorage, artifact_fixture, artifact_request, state_with_storage};

use super::*;

mod support;

const AGENT_CREDENTIAL: &str = "pandar_ac_download";
const OTHER_CREDENTIAL: &str = "pandar_ac_other";

#[tokio::test]
async fn valid_agent_credential_downloads_owned_artifact() {
    let storage =
        FakeArtifactStorage::with_artifacts([("storage/plate.3mf", b"artifact-bytes".as_slice())]);
    let state = state_with_storage(storage.clone()).await;
    let fixture = artifact_fixture(&state).await;

    let (status, headers, body) = artifact_request(
        router(state),
        &fixture.agent_id.to_string(),
        "artifact-1",
        Some(AGENT_CREDENTIAL),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(CONTENT_TYPE).unwrap(), "model/3mf");
    assert_eq!(body.as_ref(), b"artifact-bytes");
    assert_eq!(
        storage.opens.lock().await.as_slice(),
        &["storage/plate.3mf".to_string()]
    );
}

#[tokio::test]
async fn invalid_agent_credential_returns_401() {
    let state = state_with_storage(FakeArtifactStorage::default()).await;
    let fixture = artifact_fixture(&state).await;

    let (status, _, body) = artifact_request(
        router(state),
        &fixture.agent_id.to_string(),
        "artifact-1",
        Some("pandar_ac_wrong"),
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(error(body), "unauthorized");
}

#[tokio::test]
async fn missing_bearer_credential_returns_401() {
    let state = state_with_storage(FakeArtifactStorage::default()).await;
    let fixture = artifact_fixture(&state).await;

    let (status, _, body) = artifact_request(
        router(state),
        &fixture.agent_id.to_string(),
        "artifact-1",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(error(body), "unauthorized");
}

#[tokio::test]
async fn revoked_agent_credential_returns_401() {
    let state = state_with_storage(FakeArtifactStorage::default()).await;
    let fixture = artifact_fixture(&state).await;
    state
        .agents()
        .revoke_credential(
            fixture.tenant_id,
            fixture.agent_id,
            AuditActor::tenant_token(None, "artifact-route-test", vec!["*"]),
        )
        .await
        .unwrap();

    let (status, _, body) = artifact_request(
        router(state),
        &fixture.agent_id.to_string(),
        "artifact-1",
        Some(AGENT_CREDENTIAL),
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(error(body), "unauthorized");
}

#[tokio::test]
async fn valid_credential_for_another_agent_returns_403() {
    let state = state_with_storage(FakeArtifactStorage::default()).await;
    let fixture = artifact_fixture(&state).await;

    let (status, _, body) = artifact_request(
        router(state),
        &fixture.agent_id.to_string(),
        "artifact-1",
        Some(OTHER_CREDENTIAL),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error(body), "forbidden");
}

#[tokio::test]
async fn valid_agent_gets_403_for_same_tenant_artifact_assigned_to_another_agent() {
    let state = state_with_storage(FakeArtifactStorage::default()).await;
    let fixture = artifact_fixture(&state).await;
    let other_printer_id =
        insert_printer_fixture(state.database(), fixture.tenant_id, fixture.other_agent_id)
            .await
            .unwrap();
    state
        .jobs()
        .create_print_job(crate::repositories::CreatePrintJob {
            tenant_id: fixture.tenant_id,
            printer_id: other_printer_id,
            agent_id: fixture.other_agent_id,
            artifact_id: "artifact-for-other-agent".to_string(),
            artifact_filename: "other.3mf".to_string(),
            artifact_content_type: "model/3mf".to_string(),
            artifact_size_bytes: 3,
            artifact_storage_path: "storage/other.3mf".to_string(),
            artifact_metadata_json: None,
            plate_id: 1,
            use_ams: true,
            bed_leveling: false,
            auto_bed_leveling: pandar_core::PrintCalibrationMode::Off,
            flow_cali: false,
            auto_flow_cali: pandar_core::PrintCalibrationMode::Off,
            auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
            timelapse: false,
            ams_mapping_json: None,
            ams_mapping2_json: None,
            ams_mapping_info_json: None,
        })
        .await
        .unwrap();

    let (status, _, body) = artifact_request(
        router(state),
        &fixture.agent_id.to_string(),
        "artifact-for-other-agent",
        Some(AGENT_CREDENTIAL),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error(body), "forbidden");
}

#[tokio::test]
async fn duplicate_credential_hash_returns_401_without_cross_agent_access() {
    let storage =
        FakeArtifactStorage::with_artifacts([("storage/plate.3mf", b"artifact-bytes".as_slice())]);
    let state = state_with_storage(storage.clone()).await;
    let fixture = artifact_fixture(&state).await;
    state
        .agents()
        .rotate_credential(
            fixture.tenant_id,
            fixture.other_agent_id,
            AGENT_CREDENTIAL,
            AuditActor::tenant_token(None, "artifact-route-test", vec!["*"]),
        )
        .await
        .unwrap();

    let (status, _, body) = artifact_request(
        router(state),
        &fixture.agent_id.to_string(),
        "artifact-1",
        Some(AGENT_CREDENTIAL),
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(error(body), "unauthorized");
    assert!(storage.opens.lock().await.is_empty());
}

#[tokio::test]
async fn missing_artifact_returns_404() {
    let state = state_with_storage(FakeArtifactStorage::default()).await;
    let fixture = artifact_fixture(&state).await;

    let (status, _, body) = artifact_request(
        router(state),
        &fixture.agent_id.to_string(),
        "missing",
        Some(AGENT_CREDENTIAL),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(error(body), "artifact_not_found");
}

#[tokio::test]
async fn artifact_storage_failure_returns_unavailable_not_not_found() {
    let state = state_with_storage(FakeArtifactStorage::backend_error()).await;
    let fixture = artifact_fixture(&state).await;

    let (status, _, body) = artifact_request(
        router(state),
        &fixture.agent_id.to_string(),
        "artifact-1",
        Some(AGENT_CREDENTIAL),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(error(body), "artifact_unavailable");
}

#[tokio::test]
async fn missing_filesystem_artifact_file_returns_404() {
    let temp_dir = tempfile::tempdir().unwrap();
    let storage = FilesystemArtifactStorage::new(temp_dir.path().to_path_buf(), 1024).unwrap();
    let state = state_with_storage(storage).await;
    let fixture = artifact_fixture(&state).await;

    let (status, _, body) = artifact_request(
        router(state),
        &fixture.agent_id.to_string(),
        "artifact-1",
        Some(AGENT_CREDENTIAL),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(error(body), "artifact_not_found");
}

#[derive(serde::Deserialize)]
struct ErrorResponse {
    error: String,
}

fn error(body: axum::body::Bytes) -> String {
    serde_json::from_slice::<ErrorResponse>(&body)
        .unwrap()
        .error
}
