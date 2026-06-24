use std::{collections::HashMap, sync::Arc};

use crate::{
    artifacts::{
        ArtifactBody, ArtifactStorage, ArtifactStorageBackend, FilesystemArtifactStorage,
        StoreArtifactInput, StoredArtifact,
    },
    repositories::AuditActor,
};
use axum::http::{
    HeaderMap,
    header::{AUTHORIZATION, CONTENT_TYPE},
};
use tokio::sync::Mutex;

use super::*;

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
    assert_eq!(body.as_ref(), br#"{"error":"unauthorized"}"#);
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
    assert_eq!(body.as_ref(), br#"{"error":"unauthorized"}"#);
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
    assert_eq!(body.as_ref(), br#"{"error":"unauthorized"}"#);
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
    assert_eq!(body.as_ref(), br#"{"error":"forbidden"}"#);
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
            plate_id: 1,
            use_ams: true,
            flow_cali: false,
            timelapse: false,
            ams_mapping_json: None,
            ams_mapping2_json: None,
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
    assert_eq!(body.as_ref(), br#"{"error":"forbidden"}"#);
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
    assert_eq!(body.as_ref(), br#"{"error":"unauthorized"}"#);
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
    assert_eq!(body.as_ref(), br#"{"error":"artifact_not_found"}"#);
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
    assert_eq!(body.as_ref(), br#"{"error":"artifact_unavailable"}"#);
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
    assert_eq!(body.as_ref(), br#"{"error":"artifact_not_found"}"#);
}

async fn state_with_storage(storage: impl crate::artifacts::IntoArtifactStorage) -> AppState {
    AppState::connect_with_config_values("sqlite::memory:", storage, None, None, None, None)
        .await
        .unwrap()
}

struct ArtifactFixture {
    tenant_id: pandar_core::TenantId,
    agent_id: pandar_core::AgentId,
    other_agent_id: pandar_core::AgentId,
}

async fn artifact_fixture(state: &AppState) -> ArtifactFixture {
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let actor = AuditActor::tenant_token(None, "artifact-route-test", vec!["*"]);
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    state
        .agents()
        .rotate_credential(tenant.id, agent.id, AGENT_CREDENTIAL, actor.clone())
        .await
        .unwrap();
    let other = state.agents().create(tenant.id, "other").await.unwrap();
    state
        .agents()
        .rotate_credential(tenant.id, other.id, OTHER_CREDENTIAL, actor)
        .await
        .unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();
    state
        .jobs()
        .create_print_job(crate::repositories::CreatePrintJob {
            tenant_id: tenant.id,
            printer_id,
            agent_id: agent.id,
            artifact_id: "artifact-1".to_string(),
            artifact_filename: "plate.3mf".to_string(),
            artifact_content_type: "model/3mf".to_string(),
            artifact_size_bytes: 14,
            artifact_storage_path: "storage/plate.3mf".to_string(),
            plate_id: 1,
            use_ams: true,
            flow_cali: false,
            timelapse: false,
            ams_mapping_json: None,
            ams_mapping2_json: None,
        })
        .await
        .unwrap();

    ArtifactFixture {
        tenant_id: tenant.id,
        agent_id: agent.id,
        other_agent_id: other.id,
    }
}

async fn artifact_request(
    app: Router,
    agent_id: &str,
    artifact_id: &str,
    credential: Option<&str>,
) -> (StatusCode, HeaderMap, axum::body::Bytes) {
    let mut builder = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/v1/agents/{agent_id}/artifacts/{artifact_id}"));
    if let Some(credential) = credential {
        builder = builder.header(AUTHORIZATION, format!("Bearer {credential}"));
    }
    let response = app
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, headers, body)
}

#[derive(Clone, Default)]
struct FakeArtifactStorage {
    artifacts: Arc<HashMap<String, Vec<u8>>>,
    opens: Arc<Mutex<Vec<String>>>,
    backend_error: bool,
}

impl FakeArtifactStorage {
    fn with_artifacts(artifacts: impl IntoIterator<Item = (&'static str, &'static [u8])>) -> Self {
        Self {
            artifacts: Arc::new(
                artifacts
                    .into_iter()
                    .map(|(key, bytes)| (key.to_string(), bytes.to_vec()))
                    .collect(),
            ),
            opens: Arc::new(Mutex::new(Vec::new())),
            backend_error: false,
        }
    }

    fn backend_error() -> Self {
        Self {
            backend_error: true,
            ..Self::default()
        }
    }
}

#[async_trait::async_trait]
impl ArtifactStorage for FakeArtifactStorage {
    async fn put_artifact(&self, _input: StoreArtifactInput<'_>) -> anyhow::Result<StoredArtifact> {
        unimplemented!("route tests do not upload artifacts")
    }

    async fn open_artifact(&self, storage_key: &str) -> anyhow::Result<ArtifactBody> {
        self.opens.lock().await.push(storage_key.to_string());
        if self.backend_error {
            anyhow::bail!("backend unavailable");
        }
        let bytes = self
            .artifacts
            .get(storage_key)
            .ok_or_else(|| anyhow::anyhow!("fake artifact not found"))?;
        let mut file = tempfile::NamedTempFile::new()?;
        std::io::Write::write_all(&mut file, bytes)?;
        Ok(tokio::fs::File::open(file.into_temp_path()).await?)
    }

    async fn delete_artifact(&self, _storage_key: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn check_ready(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn max_artifact_bytes(&self) -> usize {
        1024
    }

    fn backend(&self) -> ArtifactStorageBackend {
        ArtifactStorageBackend::Filesystem
    }

    fn is_not_found(&self, err: &anyhow::Error) -> bool {
        err.to_string() == "fake artifact not found"
    }
}
