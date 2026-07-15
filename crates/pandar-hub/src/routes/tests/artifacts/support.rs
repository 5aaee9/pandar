use std::{collections::HashMap, sync::Arc};

use crate::{
    AppState,
    artifacts::{
        ArtifactBody, ArtifactStorage, ArtifactStorageBackend, StoreArtifactInput, StoredArtifact,
    },
    repositories::{AuditActor, CreatePrintJob, test_helpers::insert_printer_fixture},
};
use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Method, Request, StatusCode, header::AUTHORIZATION},
};
use http_body_util::BodyExt;
use tokio::sync::Mutex;
use tower::ServiceExt;

use super::{AGENT_CREDENTIAL, OTHER_CREDENTIAL};

pub(super) async fn state_with_storage(
    storage: impl crate::artifacts::IntoArtifactStorage,
) -> AppState {
    AppState::connect_with_config_values("sqlite::memory:", storage, None, None, None, None)
        .await
        .unwrap()
}

pub(super) struct ArtifactFixture {
    pub(super) tenant_id: pandar_core::TenantId,
    pub(super) agent_id: pandar_core::AgentId,
    pub(super) other_agent_id: pandar_core::AgentId,
}

pub(super) async fn artifact_fixture(state: &AppState) -> ArtifactFixture {
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
        .create_print_job(CreatePrintJob {
            tenant_id: tenant.id,
            printer_id,
            agent_id: agent.id,
            artifact_id: "artifact-1".to_string(),
            artifact_filename: "plate.3mf".to_string(),
            artifact_content_type: "model/3mf".to_string(),
            artifact_size_bytes: 14,
            artifact_storage_path: "storage/plate.3mf".to_string(),
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

    ArtifactFixture {
        tenant_id: tenant.id,
        agent_id: agent.id,
        other_agent_id: other.id,
    }
}

pub(super) async fn artifact_request(
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
pub(super) struct FakeArtifactStorage {
    artifacts: Arc<HashMap<String, Vec<u8>>>,
    pub(super) opens: Arc<Mutex<Vec<String>>>,
    backend_error: bool,
}

impl FakeArtifactStorage {
    pub(super) fn with_artifacts(
        artifacts: impl IntoIterator<Item = (&'static str, &'static [u8])>,
    ) -> Self {
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

    pub(super) fn backend_error() -> Self {
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
