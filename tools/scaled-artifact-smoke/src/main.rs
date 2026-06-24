use std::{env, net::SocketAddr, process::ExitCode, sync::Arc};

use anyhow::{Context, bail, ensure};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pandar_agent::{
    AgentConfig,
    commands::{ArtifactReader, HubArtifactReader},
};
use pandar_core::CommandStatus;
use pandar_hub::{
    AppState,
    artifacts::{
        ArtifactBody, ArtifactStorage, ArtifactStorageBackend, FilesystemArtifactStorage,
        StoreArtifactInput, StoredArtifact,
    },
    db::{Database, DatabaseConfig},
    grpc::commands::next_hub_command_for_agent,
    repositories::{AuditActor, PrinterSnapshotUpsert, TenantTokenScope, UserRole},
    router,
};
use tower::ServiceExt;

const ARTIFACT_BYTES: &[u8] = b"scaled smoke 3mf bytes";

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("scaled artifact smoke failed: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let mode = parse_mode()?;
    match mode {
        Mode::DryRun => run_dry_run().await?,
    }
    println!("PASS scaled artifact smoke: dry-run cross-hub artifact contract");
    Ok(())
}

fn parse_mode() -> anyhow::Result<Mode> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("--dry-run") if args.next().is_none() => Ok(Mode::DryRun),
        _ => bail!("usage: pandar-scaled-artifact-smoke --dry-run"),
    }
}

enum Mode {
    DryRun,
}

async fn run_dry_run() -> anyhow::Result<()> {
    let temp = tempfile::tempdir().context("create smoke temp dir")?;
    let database_url = format!("sqlite://{}", temp.path().join("hub.sqlite").display());
    let database = Database::connect(&DatabaseConfig::from_url(database_url)?).await?;
    database.migrate().await?;

    let storage: Arc<dyn ArtifactStorage> =
        Arc::new(SharedObjectStorage::new(temp.path().join("objects"))?);
    let hub_a = AppState::from_database(database.clone(), storage.clone());
    let hub_b = AppState::from_database(database, storage);

    let fixture = seed_fixture(&hub_a).await?;
    create_print_through_multipart_route(&hub_a, &fixture).await?;

    ensure!(
        hub_b.commands().count().await? == 1,
        "Hub B did not see the queued command in the shared database"
    );
    let command = next_hub_command_for_agent(&hub_b, fixture.tenant_id, fixture.agent_id)
        .await
        .map_err(|status| anyhow::anyhow!("command conversion failed: {status}"))?
        .context("expected queued command")?;
    let print = match command.command.context("expected hub command payload")? {
        pandar_hub::protocol::agent::v1::hub_command::Command::PrintProjectFile(print) => print,
        _ => bail!("expected PrintProjectFile command"),
    };

    ensure!(
        !print.artifact_download_path.trim().is_empty(),
        "PrintProjectFile is missing artifact_download_path"
    );
    ensure!(
        !print.storage_path.contains("pandar-spool"),
        "smoke command used a shared spool path"
    );
    let persisted = hub_b
        .commands()
        .get_for_tenant(fixture.tenant_id, pandar_core::CommandId::parse(&command.command_id)?)
        .await?
        .context("expected persisted command after dispatch")?;
    ensure!(persisted.status == CommandStatus::Sent, "command was not marked sent");

    let base_url = serve_hub_b(hub_b).await?;
    let agent_config = AgentConfig {
        hub_grpc_url: "grpc://unused-in-smoke".to_owned(),
        hub_api_url: Some(base_url),
        agent_name: "scaled-smoke-agent".to_owned(),
        agent_id: fixture.agent_id.to_string(),
        tenant_id: fixture.tenant_id.to_string(),
        agent_credential: fixture.agent_credential,
        agent_version: "scaled-smoke".to_owned(),
        printers: "[]".to_owned(),
        artifact_root: temp.path().join("agent-artifacts"),
    };
    let downloaded = HubArtifactReader::new(&agent_config)
        .read_artifact(&print.artifact_download_path)
        .await?;
    ensure!(downloaded == ARTIFACT_BYTES, "downloaded artifact bytes differ");
    Ok(())
}

async fn seed_fixture(state: &AppState) -> anyhow::Result<SmokeFixture> {
    let tenant = state.tenants().create("scaled-smoke", "Scaled Smoke").await?;
    let admin = state
        .auth()
        .create_user(
            tenant.id,
            "scaled-smoke@example.invalid",
            "Scaled Smoke",
            UserRole::TenantAdmin,
        )
        .await?;
    let agent = state.agents().create(tenant.id, "scaled-smoke-agent").await?;
    let agent_credential = "pandar_agent_scaled_smoke_secret".to_owned();
    state
        .agents()
        .rotate_credential(
            tenant.id,
            agent.id,
            &agent_credential,
            AuditActor::user(admin.id.clone()),
        )
        .await?;
    let printer = state
        .printers()
        .upsert_snapshot(
            tenant.id,
            agent.id,
            PrinterSnapshotUpsert {
                serial_number: "scaled-smoke-serial".to_owned(),
                name: "Scaled Smoke Printer".to_owned(),
                model: Some("X1C".to_owned()),
                status: "online".to_owned(),
                observed_at: pandar_core::created_at_now(),
            },
        )
        .await?;
    let plugin_token = state
        .auth()
        .create_tenant_token_with_audit(
            tenant.id,
            "scaled smoke plugin",
            vec![TenantTokenScope::PluginStudio],
            None,
            AuditActor::user(admin.id),
        )
        .await?;

    Ok(SmokeFixture {
        tenant_id: tenant.id,
        agent_id: agent.id,
        printer_id: printer.id,
        plugin_token: plugin_token.plaintext_token,
        agent_credential,
    })
}

async fn create_print_through_multipart_route(
    state: &AppState,
    fixture: &SmokeFixture,
) -> anyhow::Result<()> {
    let body = multipart_print_body(&fixture.printer_id);
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/plugin/prints")
        .header(header::AUTHORIZATION, format!("Bearer {}", fixture.plugin_token))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={}", body.boundary),
        )
        .body(Body::from(body.body))
        .context("build multipart print request")?;
    let response = router(state.clone())
        .oneshot(request)
        .await
        .context("send multipart print request")?;
    let status = response.status();
    let body = response.into_body().collect().await?.to_bytes();
    ensure!(
        status == StatusCode::CREATED,
        "multipart print route returned {status}: {}",
        String::from_utf8_lossy(&body)
    );
    Ok(())
}

struct MultipartBody {
    boundary: String,
    body: Vec<u8>,
}

fn multipart_print_body(printer_id: &str) -> MultipartBody {
    let boundary = "pandar-scaled-smoke-boundary";
    let mut body = Vec::new();
    for (name, value) in [
        ("printer_id", printer_id),
        ("filename", "scaled-smoke.3mf"),
        ("content_type", "model/3mf"),
        ("plate_id", "1"),
        ("use_ams", "true"),
        ("flow_cali", "false"),
        ("timelapse", "true"),
        ("ams_mapping", "[]"),
    ] {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(value.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"scaled-smoke.3mf\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: model/3mf\r\n\r\n");
    body.extend_from_slice(ARTIFACT_BYTES);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    MultipartBody {
        boundary: boundary.to_owned(),
        body,
    }
}

async fn serve_hub_b(state: AppState) -> anyhow::Result<String> {
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, router(state)).await {
            eprintln!("scaled smoke hub server failed: {err:#}");
        }
    });
    Ok(format!("http://{addr}"))
}

struct SmokeFixture {
    tenant_id: pandar_core::TenantId,
    agent_id: pandar_core::AgentId,
    printer_id: String,
    plugin_token: String,
    agent_credential: String,
}

struct SharedObjectStorage {
    inner: FilesystemArtifactStorage,
}

impl SharedObjectStorage {
    fn new(root: impl Into<std::path::PathBuf>) -> anyhow::Result<Self> {
        Ok(Self {
            inner: FilesystemArtifactStorage::new(root.into(), 1024 * 1024)?,
        })
    }
}

#[async_trait::async_trait]
impl ArtifactStorage for SharedObjectStorage {
    async fn put_artifact(&self, input: StoreArtifactInput<'_>) -> anyhow::Result<StoredArtifact> {
        self.inner.put_artifact(input).await
    }

    async fn open_artifact(&self, storage_key: &str) -> anyhow::Result<ArtifactBody> {
        self.inner.open_artifact(storage_key).await
    }

    async fn delete_artifact(&self, storage_key: &str) -> anyhow::Result<()> {
        self.inner.delete_artifact(storage_key).await
    }

    async fn check_ready(&self) -> anyhow::Result<()> {
        self.inner.check_ready().await
    }

    fn max_artifact_bytes(&self) -> usize {
        self.inner.max_artifact_bytes()
    }

    fn backend(&self) -> ArtifactStorageBackend {
        ArtifactStorageBackend::S3
    }
}
