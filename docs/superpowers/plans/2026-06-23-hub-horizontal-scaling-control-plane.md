# Hub Horizontal Scaling Control Plane Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add explicit SQLite/in-process and PostgreSQL/NATS Hub control-plane modes so horizontally scaled Hub replicas can wake agent streams, close sessions, fan out live events, and share WebSocket tickets without changing agent/client authentication.

**Architecture:** Add a small `cluster` control-plane module with an in-process broadcast implementation and a NATS implementation. `AppState` owns the control plane; `run_from_env` starts one subscriber task that handles control messages against local sessions and WebSocket subscribers. Browser WebSocket tickets move from process memory into a backend-neutral repository/table.

**Tech Stack:** Rust 2024, tokio, axum, tonic, SeaORM/sqlx migrations, async-nats, serde JSON, existing SQLite/PostgreSQL repository patterns.

---

## File Structure

- Create `crates/pandar-hub/src/cluster.rs`: control-plane config, message enum, in-process bus, NATS bus, subscriber loop.
- Modify `crates/pandar-hub/src/lib.rs`: add `cluster` module, `AppState` control-plane and ticket repository fields, config-aware constructors, startup subscriber task.
- Modify `crates/pandar-hub/src/sessions.rs`: keep local session registry, add local-only wake/close methods, move command enqueue wake responsibility out to callers.
- Modify `crates/pandar-hub/src/printer_events.rs`: remove in-memory ticket map, add local-only publish method, keep subscription metrics.
- Create `crates/pandar-hub/src/entities/printer_event_tickets.rs`: SeaORM entity.
- Create `crates/pandar-hub/src/repositories/printer_event_tickets.rs`: issue/consume short-lived hashed one-use WebSocket tickets.
- Modify `crates/pandar-hub/src/entities/mod.rs` and `crates/pandar-hub/src/repositories/mod.rs`: export new entity/repository.
- Add migrations:
  - `crates/pandar-hub/migrations/sqlite/20260623030000_hub_control_plane_tickets.sql`
  - `crates/pandar-hub/migrations/postgres/20260623030000_hub_control_plane_tickets.sql`
- Modify `crates/pandar-hub/src/routes/printer_events.rs`, `routes/printers.rs`, `routes/jobs.rs`, `routes/jobs/material.rs`, `routes/plugin.rs`, `grpc/printer_snapshots.rs`, and `grpc/print_reports.rs`: publish control messages after durable changes and make event payload responses deserializable.
- Modify route/gRPC tests under `crates/pandar-hub/src/routes/tests` and `crates/pandar-hub/src/grpc/tests`: add cross-replica fixtures and red/green coverage.
- Modify `Cargo.toml` and `crates/pandar-hub/Cargo.toml`: add `async-nats`, add `futures-util`, and enable `tokio-stream` `sync`. Let Cargo regenerate `Cargo.lock`; do not hand-edit it.
- Update `docs/architecture.md`, `docs/development.md`, `docker-compose.postgres.yml`, and `docs/roadmap.md`.

## Task 1: Control Plane Configuration, Dependency, And Startup Wiring

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/pandar-hub/Cargo.toml`
- Create: `crates/pandar-hub/src/cluster.rs`
- Modify: `crates/pandar-hub/src/lib.rs`

- [ ] **Step 1: Write failing configuration tests**

Add tests in `crates/pandar-hub/src/cluster.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DatabaseBackend;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &'static str) -> Self {
            let previous = std::env::var(key).ok();
            unsafe { std::env::set_var(key, value) };
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => unsafe { std::env::set_var(self.key, value) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    #[test]
    fn default_control_plane_is_in_process() {
        let config = ControlPlaneConfig::from_env_values(DatabaseBackend::Sqlite, None, None, None)
            .unwrap();

        assert!(matches!(config, ControlPlaneConfig::InProcess));
    }

    #[test]
    fn sqlite_rejects_nats_control_plane() {
        let err = ControlPlaneConfig::from_env_values(
            DatabaseBackend::Sqlite,
            Some("nats"),
            Some("nats://localhost:4222"),
            None,
        )
        .unwrap_err();

        assert!(err.to_string().contains("SQLite cannot use NATS control plane"));
    }

    #[test]
    fn nats_requires_url() {
        let err = ControlPlaneConfig::from_env_values(
            DatabaseBackend::Postgres,
            Some("nats"),
            Some(" "),
            None,
        )
        .unwrap_err();

        assert!(err.to_string().contains("PANDAR_NATS_URL"));
    }

    #[test]
    fn postgres_nats_uses_default_subject() {
        let config = ControlPlaneConfig::from_env_values(
            DatabaseBackend::Postgres,
            Some("nats"),
            Some("nats://localhost:4222"),
            None,
        )
        .unwrap();

        assert_eq!(
            config,
            ControlPlaneConfig::Nats {
                url: "nats://localhost:4222".to_string(),
                subject: "pandar.hub.control".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn sqlite_connect_rejects_nats_control_plane_config() {
        let temp_dir = tempfile::tempdir().unwrap().keep();
        let job_storage = crate::jobs::JobStorageConfig::new(
            temp_dir,
            crate::jobs::DEFAULT_MAX_ARTIFACT_BYTES,
        )
        .unwrap();
        let err = crate::AppState::connect_with_config_values(
            "sqlite::memory:",
            job_storage,
            None,
            Some("nats"),
            Some("nats://localhost:4222"),
            None,
        )
        .await
        .unwrap_err();

        assert!(format!("{err:#}").contains("SQLite cannot use NATS control plane"));
    }

    #[tokio::test]
    async fn sqlite_connect_defaults_to_in_process_without_broker_config() {
        let temp_dir = tempfile::tempdir().unwrap().keep();
        let job_storage = crate::jobs::JobStorageConfig::new(
            temp_dir,
            crate::jobs::DEFAULT_MAX_ARTIFACT_BYTES,
        )
        .unwrap();
        let state = crate::AppState::connect_with_config_values(
            "sqlite::memory:",
            job_storage,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(state.database().backend(), DatabaseBackend::Sqlite);
    }

    #[tokio::test]
    async fn sqlite_connect_with_auth_config_reads_env_and_rejects_nats() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = EnvVarGuard::set("PANDAR_CONTROL_PLANE", "nats");
        let _url_guard = EnvVarGuard::set("PANDAR_NATS_URL", "nats://localhost:4222");
        let temp_dir = tempfile::tempdir().unwrap().keep();
        let job_storage = crate::jobs::JobStorageConfig::new(
            temp_dir,
            crate::jobs::DEFAULT_MAX_ARTIFACT_BYTES,
        )
        .unwrap();
        let err = crate::AppState::connect_with_auth_config(
            "sqlite::memory:",
            job_storage,
            None,
        )
        .await
        .unwrap_err();

        assert!(format!("{err:#}").contains("SQLite cannot use NATS control plane"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test -p pandar-hub cluster::tests:: -- --nocapture
```

Expected: compile failure because `cluster`, `ControlPlaneConfig`, and `AppState::connect_with_config_values` do not exist.

- [ ] **Step 3: Add minimal control-plane config and dependency**

Add workspace dependencies. `tokio-stream` already exists in the workspace table, so update that existing entry to enable `sync`; do not add a duplicate TOML key:

```toml
async-nats = "0.49.1"
futures-util = "0.3.32"
tokio-stream = { version = "0.1", features = ["sync"] }
```

Add hub dependencies:

```toml
async-nats.workspace = true
futures-util.workspace = true
```

Create `crates/pandar-hub/src/cluster.rs` with:

```rust
use anyhow::{bail, Context};
use crate::db::DatabaseBackend;

pub const DEFAULT_NATS_SUBJECT: &str = "pandar.hub.control";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlPlaneConfig {
    InProcess,
    Nats { url: String, subject: String },
}

impl ControlPlaneConfig {
    pub fn from_env(backend: DatabaseBackend) -> anyhow::Result<Self> {
        Self::from_env_values(
            backend,
            std::env::var("PANDAR_CONTROL_PLANE").ok().as_deref(),
            std::env::var("PANDAR_NATS_URL").ok().as_deref(),
            std::env::var("PANDAR_NATS_SUBJECT").ok().as_deref(),
        )
    }

    pub(crate) fn from_env_values(
        backend: DatabaseBackend,
        control_plane: Option<&str>,
        nats_url: Option<&str>,
        nats_subject: Option<&str>,
    ) -> anyhow::Result<Self> {
        let control_plane = control_plane
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("in-process");
        match control_plane {
            "in-process" => Ok(Self::InProcess),
            "nats" => {
                if backend == DatabaseBackend::Sqlite {
                    bail!("SQLite cannot use NATS control plane");
                }
                let url = nats_url
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .context("PANDAR_NATS_URL is required when PANDAR_CONTROL_PLANE=nats")?
                    .to_string();
                let subject = nats_subject
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(DEFAULT_NATS_SUBJECT)
                    .to_string();
                Ok(Self::Nats { url, subject })
            }
            other => bail!("unsupported PANDAR_CONTROL_PLANE {other}"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum HubControlMessage {
    AgentWake { tenant_id: String, agent_id: String },
    AgentClose { tenant_id: String, agent_id: String },
    PrinterEvent {
        tenant_id: String,
        event: crate::printer_events::PrinterEvent,
    },
}

#[derive(Debug, Clone)]
pub struct ControlPlane;

impl ControlPlane {
    pub async fn from_config(config: ControlPlaneConfig) -> anyhow::Result<Self> {
        match config {
            ControlPlaneConfig::InProcess => Ok(Self::in_process()),
            ControlPlaneConfig::Nats { .. } => {
                bail!("NATS control plane implementation is added in Task 2")
            }
        }
    }

    pub fn in_process() -> Self {
        Self
    }
}
```

Add `pub mod cluster;` in `lib.rs`.

Add `control_plane: cluster::ControlPlane` to `AppState`, add `control_plane(&self)`, add `from_database_with_control_plane`, and route `from_database` through the in-process control plane:

```rust
pub fn from_database(database: Database, job_storage: JobStorageConfig) -> Self {
    Self::from_database_with_control_plane(database, job_storage, cluster::ControlPlane::in_process())
}

pub fn from_database_with_control_plane(
    database: Database,
    job_storage: JobStorageConfig,
    control_plane: cluster::ControlPlane,
) -> Self {
    let metrics = MetricsState::new();
    Self {
        database: database.clone(),
        tenants: TenantRepository::new(database.clone()),
        auth: AuthRepository::new(database.clone()),
        audit_events: AuditEventRepository::new(database.clone()),
        agents: AgentRepository::new(database.clone()),
        printers: PrinterRepository::new(database.clone()),
        commands: CommandRepository::new(database.clone()),
        jobs: JobRepository::new(database.clone()),
        materials: MaterialRepository::new(database),
        job_storage,
        external_auth: None,
        bootstrap_token: None,
        printer_events: PrinterEventHub::with_metrics(metrics.clone()),
        sessions: SessionRegistry::new(),
        control_plane,
        metrics,
    }
}
```

Add `AppState::connect_with_config_values` and route existing `connect*` constructors through it:

```rust
pub async fn connect_with_config_values(
    database_url: impl Into<String>,
    job_storage: JobStorageConfig,
    external_auth: Option<JwtVerifier>,
    control_plane: Option<&str>,
    nats_url: Option<&str>,
    nats_subject: Option<&str>,
) -> anyhow::Result<Self> {
    let database_url = database_url.into();
    let config = DatabaseConfig::from_url(database_url)?;
    let control_config = cluster::ControlPlaneConfig::from_env_values(
        config.backend(),
        control_plane,
        nats_url,
        nats_subject,
    )?;
    let control_plane = cluster::ControlPlane::from_config(control_config).await?;
    let database = Database::connect(&config).await?;
    database.migrate().await?;

    let bootstrap_token = std::env::var("PANDAR_BOOTSTRAP_TOKEN")
        .ok()
        .filter(|value| !value.trim().is_empty());

    Ok(Self::from_database_with_control_plane(database, job_storage, control_plane)
        .with_external_auth_option(external_auth)
        .with_bootstrap_token_option(bootstrap_token))
}
```

Production env wiring must not bypass NATS settings. Implement `connect_with_auth_config` so it reads `PANDAR_CONTROL_PLANE`, `PANDAR_NATS_URL`, and `PANDAR_NATS_SUBJECT` and forwards those values into `connect_with_config_values`; it must not call `connect_with_config_values(..., None, None, None)` for production:

```rust
pub async fn connect_with_auth_config(
    database_url: impl Into<String>,
    job_storage: JobStorageConfig,
    external_auth: Option<JwtVerifier>,
) -> anyhow::Result<Self> {
    let control_plane = std::env::var("PANDAR_CONTROL_PLANE").ok();
    let nats_url = std::env::var("PANDAR_NATS_URL").ok();
    let nats_subject = std::env::var("PANDAR_NATS_SUBJECT").ok();
    Self::connect_with_config_values(
        database_url,
        job_storage,
        external_auth,
        control_plane.as_deref(),
        nats_url.as_deref(),
        nats_subject.as_deref(),
    )
    .await
}
```

Keep the existing production constructor chain intact: `connect(database_url)` calls `connect_with_config(database_url, JobStorageConfig::from_env()?)`, `connect_with_config(...)` builds `ExternalAuthConfig::from_env()?.map(JwtVerifier::remote)`, and `connect_with_auth_config(...)` reads the control-plane environment as shown above. This preserves the current `run_from_env` shape, because production already calls `AppState::connect(database_url)` and will reach the env-aware `connect_with_auth_config` path through the existing chain.

Also update `sqlite_for_tests` so test fixtures never read process-global control-plane environment variables. It should call:

```rust
Self::connect_with_config_values(
    "sqlite::memory:",
    job_storage,
    None,
    None,
    None,
    None,
)
.await
```

This prevents the env-mutating `sqlite_connect_with_auth_config_reads_env_and_rejects_nats` test from racing unrelated tests that create SQLite fixtures.

In `crates/pandar-hub/src/lib.rs`, update `run_from_env` to start:

```rust
let _control_plane = runtime::spawn_control_plane(state.clone());
let _session_expiry = runtime::spawn_session_expiry(state.clone());
```

The subscriber task can be added as a no-op compile placeholder in this task and fully implemented in Task 2:

```rust
pub fn spawn_control_plane(_state: AppState) -> JoinHandle<()> {
    tokio::spawn(async {})
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
cargo test -p pandar-hub cluster::tests:: -- --nocapture
```

Expected: all seven tests pass, including SQLite+NATS rejection, SQLite default no-broker startup through `AppState::connect_with_config_values`, and env-driven production path rejection through `connect_with_auth_config`.

## Task 2: In-Process/NATS Control Plane And Runtime Subscriber

**Files:**
- Modify: `crates/pandar-hub/src/cluster.rs`
- Modify: `crates/pandar-hub/src/lib.rs`
- Modify: `crates/pandar-hub/src/runtime.rs`
- Modify: `crates/pandar-hub/src/sessions.rs`
- Modify: `crates/pandar-hub/src/printer_events.rs`
- Modify: `crates/pandar-hub/src/routes/printers.rs`
- Modify: `crates/pandar-hub/src/routes/jobs.rs`
- Modify: `crates/pandar-hub/src/routes/jobs/material.rs`
- Modify: `crates/pandar-hub/src/grpc/printer_snapshots.rs`
- Modify: `crates/pandar-hub/src/grpc/print_reports.rs`

- [ ] **Step 1: Write failing cross-replica control-plane tests**

Add tests in `crates/pandar-hub/src/grpc/tests/lifecycle.rs`:

```rust
#[tokio::test]
async fn sibling_instance_can_wake_connected_agent() {
    let connected_state = fixture_state().await;
    let dispatch_state = sibling_state(&connected_state);
    let _control_task = crate::runtime::spawn_control_plane_ready(dispatch_state.clone()).await;
    let _connected_task = crate::runtime::spawn_control_plane_ready(connected_state.clone()).await;
    let (tenant_id, agent_id) = tenant_agent(&connected_state).await;
    let (mut stream, _sender) =
        connect_live(&connected_state, vec![hello_event(tenant_id, agent_id)])
            .await
            .unwrap();

    let command = dispatch_state
        .commands()
        .enqueue_refresh_printers(tenant_id, agent_id)
        .await
        .unwrap();
    dispatch_state.wake_agent(tenant_id, agent_id).await;

    let hub_command = tokio::time::timeout(std::time::Duration::from_millis(250), stream.next())
        .await
        .expect("connected sibling did not receive command wake")
        .unwrap()
        .unwrap();
    assert_eq!(hub_command.command_id, command.id.to_string());
}

#[tokio::test]
async fn sibling_instance_can_close_connected_agent() {
    let connected_state = fixture_state().await;
    let admin_state = sibling_state(&connected_state);
    let _admin_task = crate::runtime::spawn_control_plane_ready(admin_state.clone()).await;
    let _connected_task = crate::runtime::spawn_control_plane_ready(connected_state.clone()).await;
    let (tenant_id, agent_id) = tenant_agent(&connected_state).await;
    let (mut stream, _sender) =
        connect_live(&connected_state, vec![hello_event(tenant_id, agent_id)])
            .await
            .unwrap();

    admin_state
        .agents()
        .revoke_credential(tenant_id, agent_id, test_audit_actor())
        .await
        .unwrap();
    admin_state.close_agent(tenant_id, agent_id).await;

    let closed = tokio::time::timeout(std::time::Duration::from_millis(250), stream.next())
        .await
        .expect("connected sibling did not receive close wake");
    assert!(closed.is_none());
}

#[tokio::test]
async fn sibling_agent_wake_ignores_wrong_tenant_and_agent() {
    let connected_state = fixture_state().await;
    let dispatch_state = sibling_state(&connected_state);
    let _control_task = crate::runtime::spawn_control_plane_ready(dispatch_state.clone()).await;
    let _connected_task = crate::runtime::spawn_control_plane_ready(connected_state.clone()).await;
    let (tenant_id, agent_id) = tenant_agent(&connected_state).await;
    let (mut stream, _sender) =
        connect_live(&connected_state, vec![hello_event(tenant_id, agent_id)])
            .await
            .unwrap();

    dispatch_state.wake_agent(TenantId::new(), agent_id).await;
    dispatch_state.wake_agent(tenant_id, AgentId::new()).await;

    let unexpected = tokio::time::timeout(std::time::Duration::from_millis(100), stream.next()).await;
    assert!(unexpected.is_err(), "wrong tenant or agent woke the stream");
}

#[tokio::test]
async fn sibling_agent_close_ignores_wrong_tenant_and_agent() {
    let connected_state = fixture_state().await;
    let admin_state = sibling_state(&connected_state);
    let _admin_task = crate::runtime::spawn_control_plane_ready(admin_state.clone()).await;
    let _connected_task = crate::runtime::spawn_control_plane_ready(connected_state.clone()).await;
    let (tenant_id, agent_id) = tenant_agent(&connected_state).await;
    let (mut stream, _sender) =
        connect_live(&connected_state, vec![hello_event(tenant_id, agent_id)])
            .await
            .unwrap();

    admin_state.close_agent(TenantId::new(), agent_id).await;
    admin_state.close_agent(tenant_id, AgentId::new()).await;

    let unexpected = tokio::time::timeout(std::time::Duration::from_millis(100), stream.next()).await;
    assert!(unexpected.is_err(), "wrong tenant or agent closed the stream");
}
```

Add helper in `grpc/tests/mod.rs`:

```rust
pub(super) fn sibling_state(state: &AppState) -> AppState {
    state.sibling_for_tests()
}
```

Use `sibling_for_tests()` for pure control-plane/session fanout tests where a shared in-process control plane is the thing under test. Do not add a SQLite-file sibling helper to `grpc/tests/mod.rs`; Task 3 adds that helper in route tests where it is used for ticket/database persistence coverage.

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test -p pandar-hub sibling_instance -- --nocapture
cargo test -p pandar-hub sibling_agent_wake_ignores_wrong_tenant_and_agent -- --nocapture
cargo test -p pandar-hub sibling_agent_close_ignores_wrong_tenant_and_agent -- --nocapture
cargo test -p pandar-hub sessions_wake_local_agent_wakes_matching_online_agent -- --nocapture
cargo test -p pandar-hub grpc_dispatch_to_online_agent_yields_refresh_and_marks_sent -- --nocapture
cargo test -p pandar-hub grpc_ack_and_result_update_command_status -- --nocapture
cargo test -p pandar-hub grpc_live_stream_ack_and_result_update_command_ledger -- --nocapture
```

Expected: compile failure for missing `spawn_control_plane_ready`, `wake_agent`, `close_agent`, and `sibling_for_tests`.

- [ ] **Step 3: Implement control-plane runtime**

Extend `cluster.rs` with:

```rust
use std::{pin::Pin, sync::Arc};
use async_trait::async_trait;
use futures_util::{Stream, StreamExt};
use pandar_core::{AgentId, TenantId};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Mutex};

type ControlStream = Pin<Box<dyn Stream<Item = anyhow::Result<HubControlMessage>> + Send>>;

#[async_trait]
trait ControlPlaneBackend: Send + Sync {
    async fn publish(&self, message: HubControlMessage) -> anyhow::Result<()>;
    async fn subscribe(&self) -> anyhow::Result<ControlStream>;
}

#[derive(Clone)]
pub struct ControlPlane {
    backend: Arc<dyn ControlPlaneBackend>,
}

impl std::fmt::Debug for ControlPlane {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("ControlPlane").finish_non_exhaustive()
    }
}

impl ControlPlane {
    pub async fn from_config(config: ControlPlaneConfig) -> anyhow::Result<Self> {
        match config {
            ControlPlaneConfig::InProcess => Ok(Self::in_process()),
            ControlPlaneConfig::Nats { url, subject } => {
                Ok(Self {
                    backend: Arc::new(NatsControlPlane::connect(&url, subject).await?),
                })
            }
        }
    }

    pub fn in_process() -> Self {
        Self {
            backend: Arc::new(InProcessControlPlane::new()),
        }
    }

    pub async fn publish(&self, message: HubControlMessage) -> anyhow::Result<()> {
        self.backend.publish(message).await
    }

    pub async fn subscribe(&self) -> anyhow::Result<ControlStream> {
        self.backend.subscribe().await
    }
}

#[derive(Debug)]
struct InProcessControlPlane {
    sender: broadcast::Sender<HubControlMessage>,
}

impl InProcessControlPlane {
    fn new() -> Self {
        Self {
            sender: broadcast::channel(256).0,
        }
    }
}

#[async_trait]
impl ControlPlaneBackend for InProcessControlPlane {
    async fn publish(&self, message: HubControlMessage) -> anyhow::Result<()> {
        let _ = self.sender.send(message);
        Ok(())
    }

    async fn subscribe(&self) -> anyhow::Result<ControlStream> {
        let stream = tokio_stream::wrappers::BroadcastStream::new(self.sender.subscribe())
            .filter_map(|message| async move {
                match message {
                Ok(message) => Some(Ok(message)),
                Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(skipped)) => {
                    Some(Err(anyhow::anyhow!("control plane subscriber lagged by {skipped} messages")))
                }
                }
            });
        Ok(Box::pin(stream))
    }
}

struct NatsControlPlane {
    transport: Arc<dyn NatsTransport>,
    subject: String,
}

impl NatsControlPlane {
    async fn connect(url: &str, subject: String) -> anyhow::Result<Self> {
        let client = async_nats::connect(url)
            .await
            .with_context(|| format!("failed to connect to NATS control plane at {url}"))?;
        Ok(Self {
            transport: Arc::new(AsyncNatsTransport { client }),
            subject,
        })
    }
}

type NatsPayloadStream = Pin<Box<dyn Stream<Item = anyhow::Result<Vec<u8>>> + Send>>;

#[async_trait]
trait NatsTransport: Send + Sync {
    async fn publish(&self, subject: String, payload: Vec<u8>) -> anyhow::Result<()>;
    async fn subscribe(&self, subject: String) -> anyhow::Result<NatsPayloadStream>;
}

struct AsyncNatsTransport {
    client: async_nats::Client,
}

#[async_trait]
impl NatsTransport for AsyncNatsTransport {
    async fn publish(&self, subject: String, payload: Vec<u8>) -> anyhow::Result<()> {
        self.client
            .publish(subject, payload.into())
            .await
            .context("failed to publish control message to NATS")
    }

    async fn subscribe(&self, subject: String) -> anyhow::Result<NatsPayloadStream> {
        let subscriber = self
            .client
            .subscribe(subject)
            .await
            .context("failed to subscribe to NATS control subject")?;
        Ok(Box::pin(subscriber.map(|message| Ok(message.payload.to_vec()))))
    }
}

#[async_trait]
impl ControlPlaneBackend for NatsControlPlane {
    async fn publish(&self, message: HubControlMessage) -> anyhow::Result<()> {
        let payload = serde_json::to_vec(&message).context("failed to encode control message")?;
        self.transport.publish(self.subject.clone(), payload).await
    }

    async fn subscribe(&self) -> anyhow::Result<ControlStream> {
        let stream = self.transport.subscribe(self.subject.clone()).await?.map(|payload| {
            let payload = payload?;
            serde_json::from_slice::<HubControlMessage>(&payload)
                .context("failed to decode control message")
        });
        Ok(Box::pin(stream))
    }
}
```

The `tokio-stream` `sync` feature is required so `BroadcastStream` is available. `futures-util::StreamExt` is required for both the NATS subscriber stream and `filter_map`. Define `RecordingNatsTransport`, `PublishedMessage`, and the `tokio::sync::Mutex` import inside the `#[cfg(test)] mod tests` block only; production code should not import `Mutex` for the fake transport.

In this task, update `HubControlMessage` from the Task 1 stub to derive serde for the NATS JSON contract:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HubControlMessage {
    #[serde(rename = "agent_wake")]
    AgentWake { tenant_id: String, agent_id: String },
    #[serde(rename = "agent_close")]
    AgentClose { tenant_id: String, agent_id: String },
    #[serde(rename = "printer_event")]
    PrinterEvent {
        tenant_id: String,
        event: crate::printer_events::PrinterEvent,
    },
}
```

Add default-suite NATS boundary tests using a fake `NatsTransport`, not a live NATS server:

```rust
#[derive(Clone, Default)]
struct RecordingNatsTransport {
    published: Arc<Mutex<Vec<PublishedMessage>>>,
    subscribed_subjects: Arc<Mutex<Vec<String>>>,
    payloads: Arc<Mutex<Vec<Vec<u8>>>>,
}

#[derive(Clone)]
struct PublishedMessage {
    subject: String,
    payload: Vec<u8>,
}

impl RecordingNatsTransport {
    fn with_payloads(payloads: Vec<Vec<u8>>) -> Self {
        Self {
            payloads: Arc::new(Mutex::new(payloads)),
            ..Self::default()
        }
    }

    async fn published(&self) -> Vec<PublishedMessage> {
        self.published.lock().await.clone()
    }

    async fn subscribed_subjects(&self) -> Vec<String> {
        self.subscribed_subjects.lock().await.clone()
    }
}

#[async_trait]
impl NatsTransport for RecordingNatsTransport {
    async fn publish(&self, subject: String, payload: Vec<u8>) -> anyhow::Result<()> {
        self.published.lock().await.push(PublishedMessage { subject, payload });
        Ok(())
    }

    async fn subscribe(&self, subject: String) -> anyhow::Result<NatsPayloadStream> {
        self.subscribed_subjects.lock().await.push(subject);
        let payloads = self.payloads.lock().await.clone();
        Ok(Box::pin(futures_util::stream::iter(payloads.into_iter().map(Ok))))
    }
}

#[tokio::test]
async fn nats_control_plane_publishes_subject_and_json_payload() {
    let transport = RecordingNatsTransport::default();
    let control = NatsControlPlane {
        transport: Arc::new(transport.clone()),
        subject: "pandar.test.control".to_string(),
    };
    let expected_tenant_id = TenantId::new().to_string();
    let expected_agent_id = AgentId::new().to_string();
    let message = HubControlMessage::AgentWake {
        tenant_id: expected_tenant_id.clone(),
        agent_id: expected_agent_id.clone(),
    };

    control.publish(message.clone()).await.unwrap();

    let published = transport.published().await;
    assert_eq!(published[0].subject, "pandar.test.control");
    match serde_json::from_slice::<HubControlMessage>(&published[0].payload).unwrap() {
        HubControlMessage::AgentWake { tenant_id, agent_id } => {
            assert_eq!(tenant_id, expected_tenant_id);
            assert_eq!(agent_id, expected_agent_id);
        }
        other => panic!("unexpected control message: {other:?}"),
    }
}

#[tokio::test]
async fn nats_control_plane_subscribe_decodes_json_payloads() {
    let expected_tenant_id = TenantId::new().to_string();
    let expected_agent_id = AgentId::new().to_string();
    let message = HubControlMessage::AgentClose {
        tenant_id: expected_tenant_id.clone(),
        agent_id: expected_agent_id.clone(),
    };
    let transport = RecordingNatsTransport::with_payloads(vec![
        serde_json::to_vec(&message).unwrap(),
    ]);
    let control = NatsControlPlane {
        transport: Arc::new(transport.clone()),
        subject: "pandar.test.control".to_string(),
    };

    let mut stream = control.subscribe().await.unwrap();
    match stream.next().await.unwrap().unwrap() {
        HubControlMessage::AgentClose { tenant_id, agent_id } => {
            assert_eq!(tenant_id, expected_tenant_id);
            assert_eq!(agent_id, expected_agent_id);
        }
        other => panic!("unexpected control message: {other:?}"),
    }
    assert_eq!(transport.subscribed_subjects().await, vec!["pandar.test.control"]);
}
```

The fake transport boundary proves the NATS adapter uses the configured subject and JSON payload contract without requiring a broker in the default test suite. Live NATS cluster/load tests remain outside this implementation.

Replace the Task 1 `ControlPlane` stub with the full trait-backed implementation above. Keep the `AppState` control-plane field and `from_database_with_control_plane` constructor from Task 1; add only the sibling/test helpers and publish methods here:

```rust
#[cfg(test)]
pub fn sibling_for_tests(&self) -> Self {
    Self::from_database_with_control_plane(
        self.database.clone(),
        self.job_storage.clone(),
        self.control_plane.clone(),
    )
    .with_external_auth_option(self.external_auth.clone())
    .with_bootstrap_token_option(self.bootstrap_token.clone())
}

pub async fn wake_agent(&self, tenant_id: TenantId, agent_id: AgentId) {
    if let Err(err) = self.control_plane.publish(HubControlMessage::AgentWake {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
    }).await {
        tracing::error!(error = %format!("{err:#}"), "failed to publish agent wake");
    }
}

pub async fn close_agent(&self, tenant_id: TenantId, agent_id: AgentId) {
    if let Err(err) = self.control_plane.publish(HubControlMessage::AgentClose {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
    }).await {
        tracing::error!(error = %format!("{err:#}"), "failed to publish agent close");
    }
}

pub async fn publish_printer_event(&self, tenant_id: TenantId, event: PrinterEvent) {
    if let Err(err) = self.control_plane.publish(HubControlMessage::PrinterEvent {
        tenant_id: tenant_id.to_string(),
        event,
    }).await {
        tracing::error!(error = %format!("{err:#}"), "failed to publish printer event");
    }
}
```

Add `publish_printer_event` in this task, not Task 5, so the subscriber, control message type, and producer call sites move together.

Update `grpc/printer_snapshots.rs` and `grpc/print_reports.rs` in this task to call `state.publish_printer_event(tenant_id, event).await` instead of `state.printer_events().publish(...)`.

In `runtime.rs`, add a shared runner plus a test-ready spawn helper. The ready signal is sent only after subscribing succeeds, so tests do not publish into the in-process broadcast channel before subscribers exist:

```rust
use futures_util::StreamExt;
use pandar_core::{AgentId, TenantId};

pub fn spawn_control_plane(state: AppState) -> JoinHandle<()> {
    tokio::spawn(run_control_plane(state, None))
}

#[cfg(test)]
pub async fn spawn_control_plane_ready(state: AppState) -> JoinHandle<()> {
    let (ready_sender, ready_receiver) = tokio::sync::oneshot::channel();
    let handle = tokio::spawn(run_control_plane(state, Some(ready_sender)));
    ready_receiver
        .await
        .expect("control plane subscriber task should report readiness")
        .expect("control plane subscriber should start");
    handle
}

async fn run_control_plane(
    state: AppState,
    ready_sender: Option<tokio::sync::oneshot::Sender<anyhow::Result<()>>>,
) {
    let mut stream = match state.control_plane().subscribe().await {
        Ok(stream) => stream,
        Err(err) => {
            if let Some(ready_sender) = ready_sender {
                let _ = ready_sender.send(Err(anyhow::anyhow!("{err:#}")));
            }
            tracing::error!(error = %format!("{err:#}"), "failed to subscribe to hub control plane");
            return;
        }
    };
    if let Some(ready_sender) = ready_sender {
        let _ = ready_sender.send(Ok(()));
    }
    while let Some(message) = stream.next().await {
        match message {
            Ok(HubControlMessage::AgentWake { tenant_id, agent_id }) => {
                match crate::cluster::parse_agent_identity(&tenant_id, &agent_id) {
                    Ok((tenant_id, agent_id)) => {
                        state.sessions().wake_local_agent(tenant_id, agent_id).await;
                    }
                    Err(err) => {
                        tracing::error!(
                            tenant_id,
                            agent_id,
                            error = %format!("{err:#}"),
                            "invalid agent wake control message identity"
                        );
                    }
                }
            }
            Ok(HubControlMessage::AgentClose { tenant_id, agent_id }) => {
                match crate::cluster::parse_agent_identity(&tenant_id, &agent_id) {
                    Ok((tenant_id, agent_id)) => {
                        state.sessions().close_local_agent(tenant_id, agent_id).await;
                    }
                    Err(err) => {
                        tracing::error!(
                            tenant_id,
                            agent_id,
                            error = %format!("{err:#}"),
                            "invalid agent close control message identity"
                        );
                    }
                }
            }
            Ok(HubControlMessage::PrinterEvent { tenant_id, event }) => {
                match crate::cluster::parse_tenant_id(&tenant_id) {
                    Ok(tenant_id) => {
                        state.printer_events().publish_local(tenant_id, event).await;
                    }
                    Err(err) => {
                        tracing::error!(
                            tenant_id,
                            error = %format!("{err:#}"),
                            "invalid printer event control message tenant"
                        );
                    }
                }
            }
            Err(err) => {
                tracing::error!(error = %format!("{err:#}"), "failed to receive hub control message");
            }
        }
    }
}
```

Rename current `SessionRegistry::wake_agent` to `wake_local_agent`; add `close_local_agent`; delete `dispatch_refresh_printers`. Production command-producing code must enqueue commands through `CommandRepository` and then publish `AgentWake` through `AppState::wake_agent`; it must not call a local-only session helper after this change.

`close_local_agent` should mirror the old route close behavior but filter tenant ownership:

```rust
pub async fn close_local_agent(&self, tenant_id: TenantId, agent_id: AgentId) {
    let session = {
        let mut sessions = self.sessions.lock().await;
        if sessions
            .get(&agent_id)
            .is_some_and(|session| session.tenant_id == tenant_id)
        {
            sessions.remove(&agent_id)
        } else {
            None
        }
    };

    if let Some(session) = session {
        let _ = session.close_sender.try_send(());
    }
}
```

Update every existing `dispatch_refresh_printers` caller:

- `crates/pandar-hub/src/sessions.rs::sessions_dispatch_wakes_matching_online_agent`: rename to `sessions_wake_local_agent_wakes_matching_online_agent`, enqueue through `state.commands().enqueue_refresh_printers(...)`, then call `state.sessions().wake_local_agent(...)`.
- `crates/pandar-hub/src/grpc/tests/commands.rs::grpc_live_stream_ack_and_result_update_command_ledger`: enqueue through `state.commands()`, then call `state.sessions().wake_local_agent(...)` before reading from the stream.
- `crates/pandar-hub/src/grpc/tests/lifecycle.rs::replacement_stream_receives_commands_after_old_stream_closes`: enqueue through `state.commands()`, then call `state.sessions().wake_local_agent(...)`.
- `crates/pandar-hub/src/grpc/tests/mod.rs::grpc_dispatch_to_online_agent_yields_refresh_and_marks_sent`: enqueue through `state.commands()`, then call `state.sessions().wake_local_agent(...)`.
- `crates/pandar-hub/src/grpc/tests/mod.rs::grpc_ack_and_result_update_command_status`: enqueue through `state.commands()`, then call `state.sessions().wake_local_agent(...)`.

Use `state.wake_agent(...)` only where a test intentionally exercises the control-plane subscriber; use `wake_local_agent(...)` for existing single-registry unit/gRPC stream tests.

Before editing, confirm the complete current call-site list with:

```bash
rg -n 'dispatch_refresh_printers|sessions\(\)\.wake_agent|sessions\(\)\.remove\(agent_id\)|close_sender\.try_send' crates/pandar-hub/src
```

The expected non-test production replacements are:

- `crates/pandar-hub/src/routes/printers.rs`: replace every `state.sessions().wake_agent(...)` with `state.wake_agent(...)` in this task so the `SessionRegistry` rename does not break compilation.
- `crates/pandar-hub/src/routes/provisioning/agents.rs`: Task 4 replaces the rotate and revoke route branches that remove a session and call `session.close_sender.try_send(())` with `state.close_agent(tenant_id, agent_id).await` after the repository operation succeeds.
- `crates/pandar-hub/src/sessions.rs::register`: keep the `previous.close_sender.try_send(())` behavior local-only. It closes an already-replaced stream inside one process during registration, not a route-triggered cross-replica credential close.

The expected test replacements are the five `dispatch_refresh_printers` tests listed above plus the route tests listed in Task 4. If `rg` finds any additional call site, update it in the same task or record why it is intentionally local-only before running tests.

Also update `crates/pandar-hub/src/routes/printers.rs` in this task so the workspace compiles after the `SessionRegistry::wake_agent` rename. Replace the three current `state.sessions().wake_agent(tenant_id, agent_id).await` route call sites with `state.wake_agent(tenant_id, agent_id).await`. Task 4 will add and update the route-level wake assertions for these subscriber-driven paths.

This creates a deliberate transient test state until Task 4: route-level wake tests in `routes/tests/printer_commands.rs` that still expect synchronous local `try_recv()` delivery may fail after this production change and before Task 4 starts the subscriber and switches those assertions to `timeout(..., recv())`. Do not run `cargo test --workspace` or `cargo nextest run --workspace` between Task 2 and Task 4; Task 2 verification is limited to the control-plane and gRPC tests named below.

Because `HubControlMessage::PrinterEvent` crosses NATS as JSON, make the full event payload graph deserializable. Change `JobCommandResponse.kind` from `&'static str` to `String`; keep the serialized field name and value unchanged by updating the `JobResponse::from_parts` construction site from `kind: "print_project_file"` to `kind: "print_project_file".to_string()`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PrinterEvent { ... }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResponse { ... }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobPrintResponse { ... }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobArtifactResponse { ... }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobCommandResponse {
    id: String,
    kind: String,
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobMaterialResponse { ... }

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JobFilamentUsageResponse { ... }
```

The complete nested list is: `PrinterEvent`, `JobResponse`, `JobPrintResponse`, `JobArtifactResponse`, `JobCommandResponse`, `JobMaterialResponse`, and `JobFilamentUsageResponse`. `pandar_core::Printer` is already `Serialize + Deserialize`.

`JobMaterialResponse` and `JobFilamentUsageResponse` live in `crates/pandar-hub/src/routes/jobs/material.rs`; add `Deserialize` there in the same Task 2 edit.

In Task 2, add `PrinterEventHub::publish_local` with the same body as the current `publish`. After `grpc/printer_snapshots.rs` and `grpc/print_reports.rs` are updated to use `AppState::publish_printer_event`, remove the old direct `PrinterEventHub::publish` method in this same task.

```rust
pub async fn publish_local(&self, tenant_id: TenantId, event: PrinterEvent) {
    let sender = self.sender(tenant_id).await;
    let _ = sender.send(event);
}
```

Update existing gRPC printer snapshot and print report tests that previously called `state.printer_events().subscribe(...)` and then triggered the gRPC handler directly: start `crate::runtime::spawn_control_plane_ready(state.clone()).await` before triggering the event, because event delivery now goes through `AppState::publish_printer_event` and the subscriber path.

Add small parser helpers in `cluster.rs` and route the `AgentWake`, `AgentClose`, and `PrinterEvent` subscriber branches through them so malformed identities are tested without depending on log capture:

```rust
pub(crate) fn parse_tenant_id(tenant_id: &str) -> anyhow::Result<TenantId> {
    TenantId::parse(tenant_id)
        .with_context(|| format!("invalid tenant id in control message: {tenant_id}"))
}

pub(crate) fn parse_agent_identity(
    tenant_id: &str,
    agent_id: &str,
) -> anyhow::Result<(TenantId, AgentId)> {
    let tenant_id = parse_tenant_id(tenant_id)?;
    let agent_id = AgentId::parse(agent_id)
        .with_context(|| format!("invalid agent id in control message: {agent_id}"))?;
    Ok((tenant_id, agent_id))
}
```

Add the parser-level regression test:

```rust
#[test]
fn invalid_control_message_identity_is_detected() {
    assert!(parse_agent_identity("bad", "also-bad").is_err());
}

#[test]
fn invalid_control_message_tenant_is_detected() {
    assert!(parse_tenant_id("bad").is_err());
}
```

The runtime subscriber must log parse failures instead of silently skipping malformed control messages.

Add a NATS decode-boundary test using `RecordingNatsTransport` with one invalid JSON payload followed by one valid `AgentWake` payload. Subscribe through `NatsControlPlane`, collect two stream items, assert the first is an error containing `failed to decode control message`, and assert the second is the valid wake message. This proves malformed NATS payloads are skipped by the subscriber loop without terminating later delivery.

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
cargo test -p pandar-hub sibling_instance -- --nocapture
cargo test -p pandar-hub sibling_agent_wake_ignores_wrong_tenant_and_agent -- --nocapture
cargo test -p pandar-hub sibling_agent_close_ignores_wrong_tenant_and_agent -- --nocapture
cargo test -p pandar-hub sessions_wake_local_agent_wakes_matching_online_agent -- --nocapture
cargo test -p pandar-hub grpc_dispatch_to_online_agent_yields_refresh_and_marks_sent -- --nocapture
cargo test -p pandar-hub grpc_ack_and_result_update_command_status -- --nocapture
cargo test -p pandar-hub grpc_live_stream_ack_and_result_update_command_ledger -- --nocapture
cargo test -p pandar-hub replacement_stream_receives_commands_after_old_stream_closes -- --nocapture
cargo test -p pandar-hub invalid_control_message_identity_is_detected -- --nocapture
cargo test -p pandar-hub invalid_control_message_tenant_is_detected -- --nocapture
cargo test -p pandar-hub nats_control_plane_publishes_subject_and_json_payload -- --nocapture
cargo test -p pandar-hub nats_control_plane_subscribe_decodes_json_payloads -- --nocapture
cargo test -p pandar-hub nats_control_plane_subscribe_reports_decode_errors_and_continues -- --nocapture
cargo test -p pandar-hub printer_events_websocket_receives_snapshot_from_grpc_stream -- --nocapture
cargo test -p pandar-hub printer_events_websocket_receives_job_progress_from_grpc_stream -- --nocapture
```

Expected: sibling wake/close tests, wrong-tenant/wrong-agent negative tests, parser/NATS boundary tests, gRPC event fanout tests, and every dispatch-refactor gRPC/session regression pass.

## Task 3: Database-Backed Printer Event Tickets

**Files:**
- Create: `crates/pandar-hub/src/entities/printer_event_tickets.rs`
- Modify: `crates/pandar-hub/src/entities/mod.rs`
- Create: `crates/pandar-hub/src/repositories/printer_event_tickets.rs`
- Modify: `crates/pandar-hub/src/repositories/mod.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/mod.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/postgres.rs`
- Create: `crates/pandar-hub/src/repositories/tests/printer_event_tickets.rs`
- Add migrations for SQLite/PostgreSQL
- Modify: `crates/pandar-hub/src/printer_events.rs`
- Modify: `crates/pandar-hub/src/routes/printer_events.rs`
- Modify: `crates/pandar-hub/src/routes/tests/readiness_metrics.rs`

- [ ] **Step 1: Write failing cross-state ticket test**

Add in `routes/tests/printer_events_ws.rs`:

```rust
#[tokio::test]
async fn printer_events_websocket_accepts_browser_ticket_from_sibling_instance() {
    let (issuing_state, consuming_state) = sibling_sqlite_file_states().await;
    let issuing_app = router(issuing_state.clone());
    let consuming_app = router(consuming_state);
    let tenant = issuing_state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &issuing_state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "sibling-ticket-ws-token",
    )
    .await;
    let http_addr = serve_http(consuming_app).await;
    let (status, body) = request_as(
        issuing_app,
        Method::POST,
        &format!("/api/v1/tenants/{}/printer-events/tickets", tenant.id),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ticket = body["ticket"].as_str().unwrap();

    let (ws, _) = tokio_tungstenite::connect_async(format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events?ticket={ticket}",
        tenant.id
    ))
    .await
    .unwrap();
    drop(ws);
}
```

Add a `sibling_sqlite_file_states` helper for route tests (in `routes/tests.rs` or local to `printer_events_ws.rs`) that creates two `AppState::connect_with_config_values` instances against the same temporary SQLite file and shared temporary job-storage directory. Use this helper for database-persistence sibling tests; keep `sibling_for_tests()` only for in-process control-plane fanout tests.

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p pandar-hub printer_events_websocket_accepts_browser_ticket_from_sibling_instance -- --nocapture
```

Expected: unauthorized/invalid ticket because tickets are process-local.

- [ ] **Step 3: Add table/entity/repository**

Migration SQL for both backends:

```sql
CREATE TABLE printer_event_tickets (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    ticket_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at TEXT
);
CREATE INDEX idx_printer_event_tickets_tenant_id ON printer_event_tickets(tenant_id);
CREATE INDEX idx_printer_event_tickets_hash ON printer_event_tickets(ticket_hash);
CREATE INDEX idx_printer_event_tickets_expires_at ON printer_event_tickets(expires_at);
```

Because sqlx compile-time migrations are embedded from `crates/pandar-hub/migrations/{sqlite,postgres}`, adding the SQL files with the timestamped filenames above is the migration registration step; no separate Rust registry exists.

Entity:

```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "printer_event_tickets")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub tenant_id: String,
    pub ticket_hash: String,
    pub created_at: String,
    pub expires_at: String,
    pub used_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

Repository skeleton:

```rust
pub(super) fn format_ticket_timestamp(value: time::OffsetDateTime) -> RepositoryResult<String> {
    value
        .format(&time::format_description::well_known::Rfc3339)
        .context("failed to format printer event ticket timestamp")
        .map_err(RepositoryError::from)
}

pub(super) fn ticket_timestamp_now() -> RepositoryResult<String> {
    format_ticket_timestamp(time::OffsetDateTime::now_utc())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IssuedPrinterEventTicket {
    pub ticket: String,
    pub expires_at: String,
}

#[derive(Debug, Clone)]
pub struct PrinterEventTicketRepository {
    database: Database,
}

impl PrinterEventTicketRepository {
    pub fn new(database: Database) -> Self { Self { database } }

    pub async fn issue(&self, tenant_id: TenantId) -> RepositoryResult<IssuedPrinterEventTicket> {
        let plaintext = format!("pandar_ws_{}", uuid::Uuid::new_v4().simple());
        let now_dt = time::OffsetDateTime::now_utc();
        let now = format_ticket_timestamp(now_dt)?;
        let expires_at = format_ticket_timestamp(now_dt + time::Duration::seconds(60))?;
        printer_event_tickets::ActiveModel {
            id: Set(uuid::Uuid::new_v4().to_string()),
            tenant_id: Set(tenant_id.to_string()),
            ticket_hash: Set(hash_secret(&plaintext)),
            created_at: Set(now),
            expires_at: Set(expires_at.clone()),
            used_at: Set(None),
        }
        .insert(&self.database.sea_orm_connection())
        .await
        .context("failed to insert printer event ticket")?;
        Ok(IssuedPrinterEventTicket {
            ticket: plaintext,
            expires_at,
        })
    }

    pub async fn consume(
        &self,
        tenant_id: TenantId,
        plaintext_ticket: &str,
    ) -> RepositoryResult<TicketConsumeStatus> {
        let now = ticket_timestamp_now()?;
        let ticket_hash = hash_secret(plaintext_ticket);
        let result = printer_event_tickets::Entity::update_many()
            .set(printer_event_tickets::ActiveModel {
                used_at: Set(Some(now.clone())),
                ..Default::default()
            })
            .filter(printer_event_tickets::Column::TenantId.eq(tenant_id.to_string()))
            .filter(printer_event_tickets::Column::TicketHash.eq(ticket_hash.clone()))
            .filter(printer_event_tickets::Column::UsedAt.is_null())
            .filter(printer_event_tickets::Column::ExpiresAt.gt(now))
            .exec(&self.database.sea_orm_connection())
            .await
            .context("failed to consume printer event ticket")?;
        if result.rows_affected == 1 {
            return Ok(TicketConsumeStatus::Consumed);
        }

        let expired = printer_event_tickets::Entity::find()
            .filter(printer_event_tickets::Column::TenantId.eq(tenant_id.to_string()))
            .filter(printer_event_tickets::Column::TicketHash.eq(ticket_hash))
            .filter(printer_event_tickets::Column::UsedAt.is_null())
            .filter(printer_event_tickets::Column::ExpiresAt.lte(now))
            .one(&self.database.sea_orm_connection())
            .await
            .context("failed to inspect expired printer event ticket")?;
        Ok(match expired {
            Some(_) => TicketConsumeStatus::Expired,
            None => TicketConsumeStatus::Invalid,
        })
    }
}

pub enum TicketConsumeStatus {
    Consumed,
    Invalid,
    Expired,
}
```

Use `crate::repositories::hash_secret` for hashes. `consume` must use `update_many` filtered by `tenant_id`, `ticket_hash`, `used_at IS NULL`, and `expires_at > now`, and succeed only when `rows_affected == 1`. If the update affects 0 rows, issue one read filtered by the same tenant/hash with `used_at IS NULL` and `expires_at <= now`; return `Expired` only for that case. Wrong-tenant, reused, and missing tickets return `Invalid`.

Move the public ticket response type into `crates/pandar-hub/src/repositories/printer_event_tickets.rs` as `IssuedPrinterEventTicket` and re-export it from `repositories/mod.rs` beside `PrinterEventTicketRepository` and `TicketConsumeStatus`. Update `routes/printer_events.rs` to import the type from the repository module if it needs the concrete response type. Do not leave a duplicate `IssuedPrinterEventTicket` definition in `printer_events.rs`.

Ticket timestamps are persisted as RFC3339 UTC text produced by one canonical formatter, `format_ticket_timestamp(value)`, plus `ticket_timestamp_now()` for the current time. This uses the same RFC3339 UTC format as `pandar_core::created_at_now()`, but returns `RepositoryResult<String>` so formatting errors keep context instead of panicking. In `issue`, capture one `now_dt`, derive both `created_at` and `expires_at` from that value through `format_ticket_timestamp`; in `consume`, use `ticket_timestamp_now()` for both `used_at` and the comparison `now`; in expired-row tests, generate the past `expires_at` through `format_ticket_timestamp(past_dt)`. The repository relies on one consistent UTC RFC3339 text shape for lexicographic comparisons across SQLite and PostgreSQL. If inserting a ticket hits the unique `ticket_hash` constraint, return `RepositoryError::Database` with the context chain intact; do not retry or silently mint a second token in this task because UUID-generated duplicate plaintext is not a normal runtime condition.

Add `printer_event_tickets` to `crates/pandar-hub/src/repositories/tests/mod.rs` and add repository tests that run against both `sqlite_database().await` and `super::postgres::postgres_database().await`. Extract a helper such as `async fn ticket_repository_semantics(database: Database)` and call it from concrete test functions named `sqlite_ticket_repository_semantics` and `postgres_ticket_repository_semantics`. The PostgreSQL test should use the repo's existing optional `PANDAR_TEST_POSTGRES_URL` fixture: if the URL is absent, the test may skip the live PostgreSQL connection the same way existing repository PostgreSQL tests do, but the helper and test function must exist and must run the same consume/reuse/wrong-tenant/expired semantics when the URL is configured. Also add a route-level sibling SQLite-file WebSocket ticket test using a shared temporary SQLite file fixture rather than cloned in-memory state. Cover:

- sibling state can consume a freshly issued ticket once
- a reused ticket returns `Invalid`
- a wrong-tenant consume returns `Invalid`
- an expired, unused ticket returns `Expired`

For the expired-ticket case, seed the row directly in the repository test instead of using `issue`, because the public API intentionally only mints live tickets. Insert a `printer_event_tickets::ActiveModel` with `tenant_id`, `ticket_hash: Set(hash_secret(&plaintext))`, `created_at: Set(ticket_timestamp_now().unwrap())`, `expires_at: Set(format_ticket_timestamp(time::OffsetDateTime::now_utc() - time::Duration::seconds(1)).unwrap())`, and `used_at: Set(None)`, then call `consume(tenant_id, &plaintext)` and assert `TicketConsumeStatus::Expired`. Use the same helper for SQLite and PostgreSQL so the expired-branch behavior is proven on both backends when PostgreSQL is configured. Do not hand-write a semantically equivalent timestamp string such as `+00:00`; use the canonical formatter so lexicographic comparison against `...Z` timestamps is well-defined.

Do not add a background expired-ticket cleanup loop in this task. The old in-memory hub opportunistically swept expired entries because process memory needed pruning; the database-backed implementation preserves caller behavior through one-use consume semantics and `Expired` on attempted consume, while leaving row retention/cleanup as an operational follow-up.

When removing ticket storage from `PrinterEventHub`, delete the old in-memory ticket pieces completely: `TICKET_TTL`, `PrinterEventTicket`, the `tickets` field, and the `issue_ticket` / `consume_ticket` methods. This keeps `cargo clippy --all-targets -- -D warnings` from finding dead code after routes switch to `PrinterEventTicketRepository`.

Update `crates/pandar-hub/src/repositories/tests/postgres.rs::clear_postgres` so the TRUNCATE list includes `printer_event_tickets` before `tenants` or in the same PostgreSQL TRUNCATE statement:

```rust
"TRUNCATE audit_events, api_tokens, user_identities, tenant_tokens, plugin_login_tickets, printer_event_tickets, job_filament_usages, printer_material_snapshots, jobs, job_artifacts, commands, printers, agents, users, tenants"
```

- [ ] **Step 4: Wire routes to repository**

In `AppState`, add `printer_event_tickets: PrinterEventTicketRepository`.

Initialize the field anywhere an `AppState` is constructed. In `from_database_with_control_plane`, create it with the same database handle as the other repositories:

```rust
printer_event_tickets: PrinterEventTicketRepository::new(database.clone()),
```

Because `from_database` routes through `from_database_with_control_plane`, this also covers `from_database`, `sqlite_for_tests`, and `sibling_for_tests`. Add an accessor:

```rust
pub fn printer_event_tickets(&self) -> &PrinterEventTicketRepository {
    &self.printer_event_tickets
}
```

In `create_printer_event_ticket`, call:

```rust
let issued = state.printer_event_tickets().issue(tenant_id).await?;
state.metrics().record_ticket(TicketMetric::Issued);
```

In WebSocket ticket auth, call:

```rust
let status = state.printer_event_tickets().consume(tenant_id, &ticket).await?;
match status {
    TicketConsumeStatus::Consumed => state.metrics().record_ticket(TicketMetric::Consumed),
    TicketConsumeStatus::Expired => {
        state.metrics().record_ticket(TicketMetric::Expired);
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "invalid_auth_token"));
    }
    TicketConsumeStatus::Invalid => {
        state.metrics().record_ticket(TicketMetric::Invalid);
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "invalid_auth_token"));
    }
}
```

Remove ticket storage from `PrinterEventHub`.

Update `routes/tests/readiness_metrics.rs::metrics_redacts_tenant_ids_and_reports_required_series`: replace `state.printer_events().issue_ticket(...)` / `consume_ticket(...)` with `state.printer_event_tickets().issue(...)` / `consume(...)`, and explicitly record ticket metrics through `state.metrics().record_ticket(TicketMetric::Issued | TicketMetric::Consumed | TicketMetric::Invalid)` so the existing `pandar_websocket_tickets_total{result="issued|consumed|invalid"}` assertions remain meaningful after ticket metric recording moves out of `PrinterEventHub`.

- [ ] **Step 5: Run ticket tests**

Run:

```bash
cargo test -p pandar-hub sqlite_ticket_repository_semantics -- --nocapture
cargo test -p pandar-hub postgres_ticket_repository_semantics -- --nocapture
cargo test -p pandar-hub printer_events_websocket_accepts_browser_ticket_once -- --nocapture
cargo test -p pandar-hub printer_events_websocket_accepts_browser_ticket_from_sibling_instance -- --nocapture
cargo test -p pandar-hub metrics_redacts_tenant_ids_and_reports_required_series -- --nocapture
```

Expected: repository semantics pass for SQLite and for PostgreSQL when `PANDAR_TEST_POSTGRES_URL` is configured; ticket auth tests pass, including one-use and sibling consumption.

## Task 4: Route Command Wake Coverage

**Files:**
- Modify: `crates/pandar-hub/src/routes/jobs.rs`
- Modify: `crates/pandar-hub/src/routes/plugin.rs`
- Modify: `crates/pandar-hub/src/routes/provisioning/agents.rs`
- Modify: `crates/pandar-hub/src/routes/tests/printer_commands.rs`
- Modify: `crates/pandar-hub/src/routes/tests/printers.rs`
- Modify: `crates/pandar-hub/src/routes/tests/jobs/create.rs`
- Modify: `crates/pandar-hub/src/routes/tests/jobs/recovery.rs`
- Modify: `crates/pandar-hub/src/routes/tests/provisioning/agents.rs`

- [ ] **Step 1: Write failing print command cross-replica wake test**

Add a focused route test named `print_job_wakes_agent_on_sibling_instance` in `crates/pandar-hub/src/routes/tests/jobs/create.rs`. It should register a local `AgentSession` on one sibling state, post a print job through the other sibling state, and expect the connected sibling's wake receiver to fire through the control-plane subscriber. Use the same request payload shape as the existing successful create-job route test in that file, but build the app from the dispatching sibling state and register the session on the connected sibling state. The final assertions are:

```rust
tokio::time::timeout(std::time::Duration::from_millis(250), wake_receiver.recv())
    .await
    .expect("connected sibling did not receive print command wake")
    .expect("wake signal should be sent");
let command = connected_state
    .commands()
    .next_queued_for_agent(tenant_id, agent_id)
    .await
    .unwrap()
    .unwrap();
assert_eq!(command.kind, "print_project_file");
```

Add focused route-level wake assertions for the other command-producing job paths in the same task, using the same sibling-state pattern where the connected sibling owns the `AgentSession` and the route request goes through the dispatching sibling:

- `refresh_printers_wakes_agent_on_sibling_instance` in `crates/pandar-hub/src/routes/tests/printers.rs`: enqueue through the refresh-printers HTTP route on the dispatch sibling, wait for the connected sibling wake receiver, then assert `connected_state.commands().next_queued_for_agent(tenant_id, agent_id)` returns a refresh-printers command.
- `retry_dispatch_wakes_agent_on_sibling_instance` in `crates/pandar-hub/src/routes/tests/jobs/recovery.rs`: create a failed dispatchable job using the existing retry test fixture shape, call the retry route, wait for the connected sibling wake receiver, then assert the next queued command for the agent is `print_project_file`.
- `reprint_wakes_agent_on_sibling_instance` in `crates/pandar-hub/src/routes/tests/jobs/recovery.rs` or the existing reprint test module: call the reprint route from a completed/source job fixture, wait for the connected sibling wake receiver, then assert the queued command kind.
- `duplicate_and_print_wakes_agent_on_sibling_instance` in the existing duplicate-job route test module: call the duplicate-and-print route, wait for the connected sibling wake receiver, then assert the queued command kind.
- `plugin_print_wakes_agent_on_sibling_instance` in the plugin route tests: use the existing plugin print creation payload and auth fixture, register the session on a connected sibling, issue the plugin print request through the dispatch sibling, wait for the connected sibling wake receiver, then assert the queued command kind.

These tests must prove that command creation is durable before wake delivery by reading the command through `connected_state.commands().next_queued_for_agent(...)` after the wake signal. Do not satisfy this requirement with broad `retry`, `reprint`, `duplicate`, or plugin filters that only assert HTTP status.

- [ ] **Step 2: Run the new test to verify it fails**

Run:

```bash
cargo test -p pandar-hub print_job_wakes_agent_on_sibling_instance -- --nocapture
cargo test -p pandar-hub refresh_printers_wakes_agent_on_sibling_instance -- --nocapture
cargo test -p pandar-hub retry_dispatch_wakes_agent_on_sibling_instance -- --nocapture
cargo test -p pandar-hub reprint_wakes_agent_on_sibling_instance -- --nocapture
cargo test -p pandar-hub duplicate_and_print_wakes_agent_on_sibling_instance -- --nocapture
cargo test -p pandar-hub plugin_print_wakes_agent_on_sibling_instance -- --nocapture
```

Expected: timeout because print creation does not publish cross-replica wake.

- [ ] **Step 3: Replace local wakes with control-plane wakes**

`routes/printers.rs` wake call sites were already moved to `state.wake_agent(...)` in Task 2 so the `SessionRegistry::wake_agent` rename compiles. In this task, only update the route tests for those printer command paths.

In `routes/jobs.rs`, after successful `create_print_job_with_audit`, `retry_dispatch_with_audit`, `reprint_with_audit`, and `duplicate_and_print_with_audit`, call:

```rust
let wake_tenant_id = job.job.tenant_id;
let wake_agent_id = job.job.agent_id;
state.wake_agent(wake_tenant_id, wake_agent_id).await;
```

before building the response. Capture `tenant_id` and `agent_id` before consuming the `JobWithArtifact` into `JobResponse::try_from(job)?` or any other response conversion.

In `routes/plugin.rs`, after plugin print job creation, capture the IDs before consuming the created job into the response and call:

```rust
let wake_tenant_id = created.job.tenant_id;
let wake_agent_id = created.job.agent_id;
state.wake_agent(wake_tenant_id, wake_agent_id).await;
```

In `routes/provisioning/agents.rs`, replace local session removal after credential rotation/revocation with cross-replica close:

```rust
state.close_agent(tenant_id, agent_id).await;
```

Do this only after `rotate_credential` or `revoke_credential` returns successfully. Remove both direct local close branches that currently do `state.sessions().remove(agent_id).await` and then `session.close_sender.try_send(())` in the rotate and revoke route handlers; the repository operation remains the authority, then `state.close_agent(...)` publishes the close control message.

Add route-level tests in `routes/tests/provisioning/agents.rs`:

```rust
#[tokio::test]
async fn agent_credential_revoke_closes_sibling_session() {
    let connected_state = state().await;
    let admin_state = connected_state.sibling_for_tests();
    let _connected_task = crate::runtime::spawn_control_plane_ready(connected_state.clone()).await;
    let _admin_task = crate::runtime::spawn_control_plane_ready(admin_state.clone()).await;
    let tenant = connected_state.tenants().create("acme", "Acme Labs").await.unwrap();
    let agent = connected_state.agents().create(tenant.id, "live-agent").await.unwrap();
    let credential = "pandar_ac_live";
    connected_state
        .agents()
        .rotate_credential(tenant.id, agent.id, credential, test_audit_actor())
        .await
        .unwrap();
    let (wake_sender, _) = mpsc::channel(1);
    let (close_sender, mut close_receiver) = mpsc::channel(1);
    connected_state
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id: agent.tenant_id,
            agent_id: agent.id,
            name: agent.name.clone(),
            version: "0.1.0".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
        })
        .await;
    let token = all_scope_tenant_token(&admin_state, &tenant.id.to_string(), "close-sibling").await;

    let (status, _) = request_as(
        router(admin_state),
        Method::POST,
        &format!("/api/v1/tenants/{}/agents/{}/credential:revoke", tenant.id, agent.id),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    tokio::time::timeout(std::time::Duration::from_millis(250), close_receiver.recv())
        .await
        .expect("sibling session was not closed")
        .expect("close signal should be sent");
}
```

Use the actual revoke route URI from the existing route tests if it differs; keep the assertion that the HTTP route, not a direct repository call, publishes `AgentClose`.

Update existing route wake tests that register a local `AgentSession` to use the subscriber-driven model:

- In `routes/tests/printer_commands.rs`, start `let _control_task = crate::runtime::spawn_control_plane_ready(state.clone()).await;` in `discover_printers_defaults_timeout_audits_and_wakes_agent` and `diagnose_printer_enqueues_redacted_payload_audits_and_wakes_agent` before the route request.
- Replace immediate `wake_receiver.try_recv()` assertions with a short `tokio::time::timeout(..., wake_receiver.recv()).await` assertion, because wake delivery now goes through the async control-plane subscriber.
- If a refresh-printers route test is extended to assert wake, use the same subscriber-driven pattern.
- In `routes/tests/provisioning/agents.rs`, update `agent_credential_revoke_closes_current_session` and `agent_credential_rotate_closes_current_session` to start `let _control_task = crate::runtime::spawn_control_plane_ready(state.clone()).await;` before the route request, and replace `close_receiver.try_recv()` with `tokio::time::timeout(std::time::Duration::from_millis(250), close_receiver.recv()).await` so the same-instance close tests also use the subscriber path.
- Keep direct `state.sessions().wake_local_agent(...)` only in tests that are specifically testing `SessionRegistry`, not route behavior.

- [ ] **Step 4: Run command wake tests**

Run:

```bash
cargo test -p pandar-hub sibling_instance -- --nocapture
cargo test -p pandar-hub discover_printers_defaults_timeout_audits_and_wakes_agent -- --nocapture
cargo test -p pandar-hub diagnose_printer_enqueues_redacted_payload_audits_and_wakes_agent -- --nocapture
cargo test -p pandar-hub refresh_printers_returns_command_record -- --nocapture
cargo test -p pandar-hub refresh_printers_wakes_agent_on_sibling_instance -- --nocapture
cargo test -p pandar-hub print_job_wakes_agent_on_sibling_instance -- --nocapture
cargo test -p pandar-hub retry_dispatch_wakes_agent_on_sibling_instance -- --nocapture
cargo test -p pandar-hub reprint_wakes_agent_on_sibling_instance -- --nocapture
cargo test -p pandar-hub duplicate_and_print_wakes_agent_on_sibling_instance -- --nocapture
cargo test -p pandar-hub plugin_print_wakes_agent_on_sibling_instance -- --nocapture
cargo test -p pandar-hub agent_credential_revoke_closes_sibling_session -- --nocapture
cargo test -p pandar-hub agent_credential_revoke_closes_current_session -- --nocapture
cargo test -p pandar-hub agent_credential_rotate_closes_current_session -- --nocapture
```

Expected: sibling command wake, print wake, and credential revoke close tests pass.

## Task 5: Cross-Replica Printer Event Fanout Without Duplicates

**Files:**
- Modify: `crates/pandar-hub/src/routes/tests/printer_events_ws.rs`

- [ ] **Step 1: Add sibling fanout regression coverage**

Add in `printer_events_ws.rs`:

```rust
#[tokio::test]
async fn printer_events_websocket_receives_event_from_sibling_instance() {
    let publishing_state = state().await;
    let subscribing_state = publishing_state.sibling_for_tests();
    let _publishing_task = crate::runtime::spawn_control_plane_ready(publishing_state.clone()).await;
    let _subscribing_task = crate::runtime::spawn_control_plane_ready(subscribing_state.clone()).await;
    let tenant = publishing_state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &publishing_state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "sibling-ws-token",
    )
    .await;
    let http_addr = serve_http(router(subscribing_state)).await;
    let mut request = format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events",
        tenant.id
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    publishing_state
        .publish_printer_event(tenant.id, sibling_snapshot_event(tenant.id))
        .await;

    let message = tokio::time::timeout(std::time::Duration::from_millis(250), ws.next())
        .await
        .expect("sibling websocket did not receive printer event")
        .unwrap()
        .unwrap();
    let body: Value = match message {
        Message::Text(text) => serde_json::from_str(&text).unwrap(),
        other => panic!("expected text websocket message, got {other:?}"),
    };
    assert_eq!(body["type"], "printer_snapshot");
    assert_eq!(body["printer"]["serial_number"], "SN-SIBLING");
}

#[tokio::test]
async fn printer_events_websocket_receives_one_event_from_publishing_instance() {
    let state = state().await;
    let _control_task = crate::runtime::spawn_control_plane_ready(state.clone()).await;
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "same-instance-ws-token",
    )
    .await;
    let http_addr = serve_http(router(state.clone())).await;
    let mut request = format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events",
        tenant.id
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    state
        .publish_printer_event(tenant.id, sibling_snapshot_event(tenant.id))
        .await;

    let first = tokio::time::timeout(std::time::Duration::from_millis(250), ws.next())
        .await
        .expect("websocket did not receive first printer event");
    assert!(first.is_some());
    let second = tokio::time::timeout(std::time::Duration::from_millis(100), ws.next()).await;
    assert!(second.is_err(), "websocket received duplicate printer event");
}

#[tokio::test]
async fn printer_events_websocket_ignores_wrong_tenant_event_from_sibling_instance() {
    let publishing_state = state().await;
    let subscribing_state = publishing_state.sibling_for_tests();
    let _publishing_task = crate::runtime::spawn_control_plane_ready(publishing_state.clone()).await;
    let _subscribing_task = crate::runtime::spawn_control_plane_ready(subscribing_state.clone()).await;
    let tenant = publishing_state.tenants().create("acme", "Acme Labs").await.unwrap();
    let other = publishing_state.tenants().create("other", "Other Labs").await.unwrap();
    let token = auth_token_for_role(
        &publishing_state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "wrong-tenant-ws-token",
    )
    .await;
    let http_addr = serve_http(router(subscribing_state)).await;
    let mut request = format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events",
        tenant.id
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    publishing_state
        .publish_printer_event(other.id, sibling_snapshot_event(other.id))
        .await;

    let unexpected = tokio::time::timeout(std::time::Duration::from_millis(100), ws.next()).await;
    assert!(unexpected.is_err(), "wrong tenant printer event reached websocket");
}
```

Add helper:

```rust
fn sibling_snapshot_event(tenant_id: TenantId) -> PrinterEvent {
    PrinterEvent::PrinterSnapshot {
        printer: pandar_core::Printer {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id,
            agent_id: AgentId::new(),
            serial_number: "SN-SIBLING".to_string(),
            name: "Sibling Printer".to_string(),
            model: Some("X1C".to_string()),
            status: "idle".to_string(),
            last_seen_at: pandar_core::created_at_now(),
            created_at: pandar_core::created_at_now(),
        },
    }
}
```

- [ ] **Step 2: Run regression tests to verify they pass**

Run:

```bash
cargo test -p pandar-hub printer_events_websocket_receives_event_from_sibling_instance -- --nocapture
cargo test -p pandar-hub printer_events_websocket_receives_one_event_from_publishing_instance -- --nocapture
cargo test -p pandar-hub printer_events_websocket_ignores_wrong_tenant_event_from_sibling_instance -- --nocapture
cargo test -p pandar-hub printer_events_websocket_receives_snapshot_from_grpc_stream -- --nocapture
cargo test -p pandar-hub printer_events_websocket_receives_job_progress_from_grpc_stream -- --nocapture
```

Expected: these tests should pass because Task 2 already routed gRPC event producers through the control plane. Treat them as regression coverage for sibling delivery, no duplicate local delivery, and wrong-tenant isolation rather than a red phase.

- [ ] **Step 3: Confirm direct event publish API is gone**

Task 2 already added `AppState::publish_printer_event(...)`, `PrinterEventHub::publish_local(...)`, moved `grpc/printer_snapshots.rs` and `grpc/print_reports.rs`, and removed the old direct `PrinterEventHub::publish(...)` method. In this task, add the WebSocket regression tests above and run:

```bash
rg -n 'printer_events\(\)\.publish\(|pub async fn publish\(' crates/pandar-hub/src
```

Expected: no matches for direct `PrinterEventHub::publish`; only `publish_local` and `AppState::publish_printer_event` remain.

- [ ] **Step 4: Run WebSocket event tests**

Run:

```bash
cargo test -p pandar-hub printer_events_websocket_receives -- --nocapture
```

Expected: existing and new WebSocket event tests pass.

## Task 6: Documentation And Deployment Wiring

**Files:**
- Modify: `docker-compose.postgres.yml`
- Modify: `docs/architecture.md`
- Modify: `docs/development.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update compose with optional NATS service**

Add service:

```yaml
  nats:
    image: nats:2.11-alpine
    profiles: ["nats"]
    command: ["-js", "-m", "8222"]
    ports:
      - "4222:4222"
    healthcheck:
      test: ["CMD", "wget", "-qO-", "http://127.0.0.1:8222/healthz"]
      interval: 5s
      timeout: 5s
      retries: 12
```

Set `pandar-api` environment:

```yaml
      PANDAR_CONTROL_PLANE: ${PANDAR_CONTROL_PLANE:-in-process}
      PANDAR_NATS_URL: ${PANDAR_NATS_URL:-nats://nats:4222}
```

Document beside the Compose environment that `PANDAR_NATS_URL` is ignored unless `PANDAR_CONTROL_PLANE=nats`; in-process PostgreSQL deployments do not require NATS even if the URL variable has a default value. Keep `pandar-api` free of `depends_on: nats` in the default graph. Document enabling the optional broker with a Compose profile, for example:

```bash
PANDAR_CONTROL_PLANE=nats docker compose -f docker-compose.postgres.yml --profile nats up
```

- [ ] **Step 2: Update docs**

Add architecture text:

```markdown
Hub supports two coordination modes. SQLite deployments use an in-process control plane and are single-process only. PostgreSQL deployments can enable `PANDAR_CONTROL_PLANE=nats`, which keeps PostgreSQL as the durable fact source and uses NATS only as the internal Hub-to-Hub wake/fanout bus. Agents and browsers still connect only to Hub.

PostgreSQL+NATS scales Hub control-plane delivery and shared metadata. Print artifact payloads still use `PANDAR_SPOOL_DIR`; multi-replica deployments must mount that directory on shared storage or wait for a later object-storage artifact backend before scheduling print creation across arbitrary Hub pods.
```

Add development text:

```markdown
Control plane:

- `PANDAR_CONTROL_PLANE=in-process` or unset: no broker, suitable for SQLite and single-Hub PostgreSQL.
- `PANDAR_CONTROL_PLANE=nats`: requires PostgreSQL and `PANDAR_NATS_URL`.
- `PANDAR_NATS_SUBJECT` defaults to `pandar.hub.control`.

NATS is internal to Hub. Do not expose tenant tokens or agent credentials to NATS.

For horizontally scaled print-job creation, configure `PANDAR_SPOOL_DIR` on shared persistent storage. NATS does not replicate print artifacts.
```

Add roadmap phase:

```markdown
## Phase 22: Hub Horizontal Scaling Control Plane

Goal: support lightweight single-process Hub deployments and large PostgreSQL-backed Hub deployments with an internal NATS control plane.

- Completed explicit control-plane deployment matrix.
- Completed in-process bus for SQLite/single-process deployments.
- Completed NATS-backed Hub internal control plane for PostgreSQL HPA.
- Completed database-backed WebSocket tickets for cross-replica browser connection setup.

Immediate next: load-test 100k-agent control-plane behavior and decide whether durable JetStream replay is needed.
```

- [ ] **Step 3: Run formatting checks for docs/compose**

Run:

```bash
git diff --check
```

Expected: no whitespace errors.

## Task 7: Full Verification, Review, Commit, Push

**Files:**
- All changed files.

Commit and push are included because the user explicitly invoked `$sdd-workflow`, whose completion contract requires documentation updates, fresh verification, a Lore-format commit, and a push. These are downstream SDD completion steps after implementation review approval, not a generic requirement for unrelated plans.

- [ ] **Step 1: Run targeted tests**

Run:

```bash
cargo test -p pandar-hub cluster::tests:: -- --nocapture
cargo test -p pandar-hub sibling_instance -- --nocapture
cargo test -p pandar-hub sibling_agent_wake_ignores_wrong_tenant_and_agent -- --nocapture
cargo test -p pandar-hub sibling_agent_close_ignores_wrong_tenant_and_agent -- --nocapture
cargo test -p pandar-hub sqlite_ticket_repository_semantics -- --nocapture
cargo test -p pandar-hub postgres_ticket_repository_semantics -- --nocapture
cargo test -p pandar-hub printer_events_websocket_accepts_browser_ticket_from_sibling_instance -- --nocapture
cargo test -p pandar-hub printer_events_websocket -- --nocapture
cargo test -p pandar-hub print_job_wakes_agent_on_sibling_instance -- --nocapture
cargo test -p pandar-hub refresh_printers_wakes_agent_on_sibling_instance -- --nocapture
cargo test -p pandar-hub retry_dispatch_wakes_agent_on_sibling_instance -- --nocapture
cargo test -p pandar-hub reprint_wakes_agent_on_sibling_instance -- --nocapture
cargo test -p pandar-hub duplicate_and_print_wakes_agent_on_sibling_instance -- --nocapture
cargo test -p pandar-hub plugin_print_wakes_agent_on_sibling_instance -- --nocapture
cargo test -p pandar-hub agent_credential_revoke_closes_sibling_session -- --nocapture
```

Expected: all targeted tests pass.

- [ ] **Step 2: Run required workspace checks**

Run:

```bash
cargo fmt -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: all commands exit 0. If `cargo nextest` is unavailable, report the exact error and run `cargo test --workspace` as next-best evidence.

- [ ] **Step 3: Review diff scope**

Run:

```bash
git status --short
git diff --stat
git diff --check
```

Expected: only files named in this plan changed; no whitespace errors.

- [ ] **Step 4: Commit with Lore protocol**

Use a commit message shaped like:

```text
Enable Hub replicas to coordinate through an explicit control plane

Constraint: SQLite remains a lightweight single-process deployment with no broker.
Constraint: PostgreSQL HPA uses NATS only as an internal Hub bus; Hub keeps tenant and agent authorization.
Rejected: PostgreSQL LISTEN/NOTIFY | insufficient for the target 100k+ online-agent scale.
Rejected: MQTT/EMQX agent migration | materially changes the current gRPC agent contract.
Confidence: high
Scope-risk: broad
Directive: Do not let agents or browsers connect directly to NATS without a separate auth/ACL design.
Tested: cargo fmt -- --check; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --manifest-path "Cargo.toml" --workspace
Not-tested: Live multi-node NATS cluster load test.
```

- [ ] **Step 5: Push**

Run:

```bash
git push
```

Expected: current branch pushes to its configured upstream. If push fails, report the local commit SHA and exact push error.
