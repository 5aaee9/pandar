# Phase 26 Production Soak, HA, And Failure Injection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add repeatable local HA/failure evidence, focused failure tests, subsystem metrics, and operator runbooks for Pandar's scaled Hub model.

**Architecture:** Extend the existing scaled artifact smoke tool into a small modular Phase 26 harness while keeping default verification free of Docker and external credentials. Add focused Hub metrics/tests for control-plane and storage failure classes, and document what local evidence proves versus what still requires a live PostgreSQL/NATS/object-storage soak.

**Tech Stack:** Rust workspace, Axum route tests, Tokio, SeaORM-backed repositories, existing `AppState`/`ControlPlane`/`ArtifactStorage` boundaries, Next.js build only if frontend contracts change.

---

## File Structure

- Modify: `tools/scaled-artifact-smoke/src/main.rs`
  - Keep only CLI entrypoint, argument parsing, and top-level result printing.
- Create: `tools/scaled-artifact-smoke/src/harness.rs`
  - Own dry-run orchestration, scenario selection, iterations, and concurrency.
- Create: `tools/scaled-artifact-smoke/src/fixture.rs`
  - Seed tenants, users, agents, printers, plugin tokens, and shared Hub state.
- Create: `tools/scaled-artifact-smoke/src/http.rs`
  - Own multipart route calls, loopback Hub serving, WebSocket connection helpers, and body builders.
- Create: `tools/scaled-artifact-smoke/src/storage.rs`
  - Own fake S3-like shared storage plus deterministic failure injection storage.
- Create: `tools/scaled-artifact-smoke/src/scenarios.rs`
  - Own individual dry-run scenarios: artifact dispatch/download, websocket fanout, restart convergence, storage failures, terminal report idempotence.
- Modify: `crates/pandar-hub/src/metrics.rs`
  - Add control-plane counters with bounded labels.
- Modify: `crates/pandar-hub/src/metrics_export.rs`
  - Export control-plane counters.
- Modify: `crates/pandar-hub/src/lib.rs`
  - Record control-plane publish success/failure in `wake_agent`, `close_agent`, and `publish_printer_event`.
- Modify: `crates/pandar-hub/src/runtime.rs`
  - Record control-plane receive/handle success/failure without changing message behavior.
- Modify: `crates/pandar-hub/src/routes/tests/readiness_metrics.rs`
  - Assert new metrics, no raw tenant ids, and existing readiness labels.
- Modify: `crates/pandar-hub/src/routes/tests/jobs/create.rs`
  - Assert print job creation still commits command/job state when control-plane publish fails and records failure metrics.
- Modify: `crates/pandar-hub/src/cluster/tests.rs`
  - Pin lag/decode continuation and publish metrics where appropriate.
- Modify: `crates/pandar-hub/src/routes/tests/artifacts.rs`
  - Add real route coverage for backend read failure classification if current tests do not already prove it.
- Modify: `crates/pandar-hub/src/routes/tests/printer_events_ws.rs`
  - Add cross-replica ticket safety matrix for valid, reused, wrong-tenant, and expired tickets.
- Modify: `crates/pandar-hub/src/repositories/tests/cleanup/storage.rs`
  - Ensure cleanup delete failure preserves artifact rows and is covered by Phase 26 verification.
- Modify: `crates/pandar-hub/src/grpc/tests/print_reports.rs`
  - Add terminal print-report idempotence/no-regression coverage if no equivalent exists.
- Modify: `docs/development.md`
  - Document local Phase 26 harness commands and optional live variables.
- Modify: `docs/release-installation.md`
  - Add operations runbook checks for SQLite and PostgreSQL+NATS+object-storage deployments.
- Create: `docs/compatibility/phase-26-soak-evidence.md`
  - Record local dry-run evidence template and empty live evidence rows.
- Modify: `docs/roadmap.md`
  - Mark completed local Phase 26 scaffolding and any remaining live soak gap.

## Task 1: Split And Extend The Scaled Smoke CLI

**Files:**

- Modify: `tools/scaled-artifact-smoke/src/main.rs`
- Create: `tools/scaled-artifact-smoke/src/harness.rs`
- Create: `tools/scaled-artifact-smoke/src/scenarios.rs`
- Create: `tools/scaled-artifact-smoke/src/fixture.rs`
- Create: `tools/scaled-artifact-smoke/src/http.rs`
- Create: `tools/scaled-artifact-smoke/src/storage.rs`

- [ ] **Step 1: Move existing helper code into modules without changing behavior**

Keep `main.rs` short:

```rust
mod fixture;
mod harness;
mod http;
mod scenarios;
mod storage;

use std::{env, process::ExitCode};

use anyhow::bail;
use harness::{HarnessConfig, ScenarioFilter};

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
    let config = parse_args(env::args().skip(1))?;
    harness::run(config).await?;
    Ok(())
}

fn parse_args(args: impl IntoIterator<Item = String>) -> anyhow::Result<HarnessConfig> {
    let mut config = HarnessConfig::default();
    let mut args = args.into_iter();
    let Some(mode) = args.next() else {
        bail!("usage: pandar-scaled-artifact-smoke --dry-run [--iterations N] [--concurrency N] [--scenario all|artifact|fanout|restart|storage|terminal]");
    };
    if mode != "--dry-run" {
        bail!("usage: pandar-scaled-artifact-smoke --dry-run [--iterations N] [--concurrency N] [--scenario all|artifact|fanout|restart|storage|terminal]");
    }
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--iterations" => {
                let value = args.next().ok_or_else(|| anyhow::anyhow!("--iterations requires a value"))?;
                config.iterations = value.parse().map_err(|_| anyhow::anyhow!("--iterations must be a positive integer"))?;
            }
            "--concurrency" => {
                let value = args.next().ok_or_else(|| anyhow::anyhow!("--concurrency requires a value"))?;
                config.concurrency = value.parse().map_err(|_| anyhow::anyhow!("--concurrency must be a positive integer"))?;
            }
            "--scenario" => {
                let value = args.next().ok_or_else(|| anyhow::anyhow!("--scenario requires a value"))?;
                config.scenario = ScenarioFilter::parse(&value)?;
            }
            _ => bail!("unknown argument {flag}"),
        }
    }
    config.validate()?;
    Ok(config)
}
```

Move the existing `SharedObjectStorage` implementation to `storage.rs`, existing fixture setup to `fixture.rs`, multipart and loopback helpers to `http.rs`, and the current dry-run path to `scenarios::artifact_dispatch_download`.

`SmokeWorld::new()` must construct Hub A and Hub B with public APIs only:

```rust
let control_plane = pandar_hub::cluster::ControlPlane::in_process();
let hub_a = AppState::from_database_with_control_plane(
    database.clone(),
    storage.clone(),
    control_plane.clone(),
);
let hub_b = AppState::from_database_with_control_plane(database, storage, control_plane);
```

Do not call `AppState::sibling_for_tests()` from the smoke tool; it is a `#[cfg(test)] pub(crate)` helper and is unavailable to `tools/scaled-artifact-smoke`.
Likewise, route-test helpers can be copied as patterns only; do not import helpers from `crates/pandar-hub/src/routes/tests/*` into the smoke binary.

- [ ] **Step 2: Run the existing smoke command to prove behavior survived the split**

Run:

```bash
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run
```

Expected: `PASS scaled artifact smoke: dry-run scenarios passed` or an equivalent PASS line listing the artifact scenario.

- [ ] **Step 3: Add scenario configuration**

In `harness.rs`, define:

```rust
#[derive(Debug, Clone)]
pub struct HarnessConfig {
    pub iterations: usize,
    pub concurrency: usize,
    pub scenario: ScenarioFilter,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            iterations: 1,
            concurrency: 2,
            scenario: ScenarioFilter::All,
        }
    }
}

impl HarnessConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.iterations > 0, "iterations must be greater than zero");
        anyhow::ensure!(self.concurrency > 0, "concurrency must be greater than zero");
        anyhow::ensure!(self.concurrency <= 16, "concurrency must be 16 or less for local dry-run");
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioFilter {
    All,
    Artifact,
    Fanout,
    Restart,
    Storage,
    Terminal,
}

impl ScenarioFilter {
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "all" => Ok(Self::All),
            "artifact" => Ok(Self::Artifact),
            "fanout" => Ok(Self::Fanout),
            "restart" => Ok(Self::Restart),
            "storage" => Ok(Self::Storage),
            "terminal" => Ok(Self::Terminal),
            other => anyhow::bail!("unknown scenario {other}"),
        }
    }

    pub fn includes(self, scenario: Self) -> bool {
        self == Self::All || self == scenario
    }
}
```

- [ ] **Step 4: Add basic iteration output**

In `harness.rs`, have `run` print stable scenario evidence:

```rust
pub async fn run(config: HarnessConfig) -> anyhow::Result<()> {
    for iteration in 1..=config.iterations {
        if config.scenario.includes(ScenarioFilter::Artifact) {
            crate::scenarios::artifact_dispatch_download(iteration, &config).await?;
            println!("PASS scenario=artifact iteration={iteration}");
        }
        if config.scenario.includes(ScenarioFilter::Fanout) {
            crate::scenarios::websocket_fanout(iteration, &config).await?;
            println!("PASS scenario=fanout iteration={iteration}");
        }
        if config.scenario.includes(ScenarioFilter::Restart) {
            crate::scenarios::restart_convergence(iteration, &config).await?;
            println!("PASS scenario=restart iteration={iteration}");
        }
        if config.scenario.includes(ScenarioFilter::Storage) {
            crate::scenarios::storage_failures(iteration, &config).await?;
            println!("PASS scenario=storage iteration={iteration}");
        }
        if config.scenario.includes(ScenarioFilter::Terminal) {
            crate::scenarios::terminal_report_idempotence(iteration, &config).await?;
            println!("PASS scenario=terminal iteration={iteration}");
        }
    }
    println!(
        "PASS scaled artifact smoke: dry-run scenarios passed iterations={} concurrency={}",
        config.iterations, config.concurrency
    );
    Ok(())
}
```

## Task 2: Add Phase 26 Harness Scenarios

**Files:**

- Modify: `tools/scaled-artifact-smoke/src/scenarios.rs`
- Modify: `tools/scaled-artifact-smoke/src/fixture.rs`
- Modify: `tools/scaled-artifact-smoke/src/http.rs`
- Modify: `tools/scaled-artifact-smoke/src/storage.rs`

- [ ] **Step 1: Preserve artifact dispatch/download scenario**

Keep the existing artifact behavior and add the wake assertion inside this scenario so default `--dry-run` proves command dispatch and wake convergence:

```rust
pub async fn artifact_dispatch_download(
    _iteration: usize,
    _config: &crate::harness::HarnessConfig,
) -> anyhow::Result<()> {
    let fixture = crate::fixture::SmokeWorld::new().await?;
    let (_control_plane, ready) = pandar_hub::runtime::spawn_control_plane_ready(fixture.hub_b.clone());
    ready
        .await
        .context("Hub B control-plane readiness task was cancelled")?
        .context("Hub B control-plane failed to subscribe")?;
    let (mut wake_receiver, _close_receiver) = fixture.register_agent_session_on_hub_b().await?;
    fixture.create_print_through_hub_a().await?;
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .context("Hub B agent session did not receive wake from Hub A")?
        .context("Hub B wake channel closed before wake")?;
    let command = fixture.dequeue_print_command_from_hub_b().await?;
    fixture.assert_agent_downloads_artifact(command).await?;
    Ok(())
}
```

`SmokeWorld` should own `temp_dir`, `hub_a`, `hub_b`, tenant/agent/printer/plugin credentials, and helper methods so scenarios do not duplicate setup.

- [ ] **Step 2: Add WebSocket ticket/fanout scenario**

Implement `websocket_fanout` to:

1. issue `config.concurrency` WebSocket tickets through Hub A;
2. connect `config.concurrency` WebSockets to Hub B with those tickets;
3. start Hub B control-plane subscriber with `spawn_control_plane_ready`;
4. publish at least one `printer_snapshot` event from Hub A;
5. create a print/job progress transition and publish at least one `job_progress` event from Hub A;
6. assert every Hub B WebSocket receives both event types across the shared control plane.

Use existing route and websocket helper patterns from `crates/pandar-hub/src/routes/tests/printer_events_ws.rs`.
Hub A and Hub B must be the `SmokeWorld` states built from the same shared `ControlPlane::in_process()` instance; otherwise Hub A's `publish_printer_event` messages stay local and Hub B subscribers cannot receive them.

Expected checks:

```rust
anyhow::ensure!(seen_snapshot, "Hub B websocket subscribers did not receive printer_snapshot");
anyhow::ensure!(seen_job_progress, "Hub B websocket subscribers did not receive job_progress");
```

This scenario must use `config.concurrency` directly for subscriber count and event assertions so the CLI concurrency option is not cosmetic. The upper bound remains enforced by `HarnessConfig::validate()`.

- [ ] **Step 3: Add restart convergence scenario**

Implement `restart_convergence` to:

1. create world and enqueue a print through Hub A;
2. drop the original Hub B state/subscriber task;
3. create a restarted Hub B with `AppState::from_database_with_control_plane(fixture.database.clone(), fixture.storage.clone(), fixture.control_plane.clone())`;
4. dequeue the queued command through the restarted state;
5. assert command status becomes `Sent`;
6. issue and consume a new WebSocket ticket across original/restarted states.

This simulates Hub process replacement while preserving the shared database and storage.
The restarted state must also reuse `fixture.control_plane.clone()` so wakes and printer events published by the original Hub state reach the restarted consumer state.

- [ ] **Step 3a: Add command wake convergence scenario**

Fold command wake convergence into `artifact_dispatch_download` so it runs for `--dry-run`, `--scenario all`, and `--scenario artifact`:

1. start Hub B control-plane subscriber with `spawn_control_plane_ready`;
2. create `let (wake_sender, mut wake_receiver) = tokio::sync::mpsc::channel(1)` and a second channel for `close_sender`;
3. build `AgentSession { token: SessionToken::new(), tenant_id, agent_id, name, version, connected_at, last_heartbeat_at, wake_sender, close_sender }`;
4. register that session with `hub_b.sessions().register(session).await`;
5. create a print through Hub A;
6. assert the Hub B wake receiver receives a wake within one second;
7. assert the command is visible and drainable through Hub B.

Use public `pandar_hub::sessions::{AgentSession, SessionToken}` plus `tokio::sync::mpsc::channel`; do not introduce a test-only dependency into the smoke binary. `SessionRegistry::register` returns the previous session, not the receiver, so the fixture helper must retain the `wake_receiver` from the channel it created. This proves that Hub A's wake publish reaches the agent session owned by Hub B, not only that both Hub states see the same durable command rows.

- [ ] **Step 4: Add storage failure scenario**

In `storage.rs`, add a wrapper:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureMode {
    None,
    Put,
    Open,
    Delete,
}
```

The failure storage must implement `ArtifactStorage` and delegate to `SharedObjectStorage` except for the selected mode.

Scenario requirements:

- Put failure through plugin multipart returns a stable non-201 error and creates no command row.
- Open failure on agent artifact download returns `artifact_unavailable`.
- Delete failure is verified by existing cleanup tests; the harness may just call the storage delete path directly if wiring full cleanup would duplicate repository tests.

- [ ] **Step 5: Add concurrent plugin-client pressure**

Update either `artifact_dispatch_download` or a dedicated helper in `scenarios.rs` so `--concurrency N` creates `N` plugin multipart submissions against Hub A and drains `N` commands from Hub B. Use bounded `futures_util::future::try_join_all` or a `tokio::task::JoinSet`; do not spawn unbounded work.

The scenario must assert:

```rust
anyhow::ensure!(
    fixture.hub_b.commands().count().await? == config.concurrency as i64,
    "expected one command per concurrent plugin client"
);
```

The default `--concurrency 2` should therefore exercise simulated plugin clients and command drains. The fanout scenario exercises simulated WebSocket subscribers. The existing Hub-mediated download assertion exercises the agent artifact reader; for `concurrency > 1`, drain and download at least the first and last command so both creation and agent-download paths are covered without excessive runtime.

- [ ] **Step 6: Add terminal report idempotence scenario**

Implement `terminal_report_idempotence` with repository or gRPC helper calls:

1. create a print job;
2. apply a `FINISH` report twice;
3. assert job print status remains `completed`;
4. assert terminal machine event count does not increase on the second identical report;
5. apply an older/stale running report;
6. assert terminal state does not regress.

If direct repository APIs are clearer than gRPC streaming in the harness, use `state.jobs().apply_print_report(...)` and keep gRPC-specific behavior covered by hub tests.

- [ ] **Step 7: Add explicit live PostgreSQL/NATS latency/conflict evidence path**

Do not require live services in default verification. Add a documented optional runbook path in `docs/development.md` and the evidence file for disposable live dependencies:

```bash
PANDAR_SOAK_DATABASE_URL=postgres://... \
PANDAR_SOAK_NATS_URL=nats://... \
PANDAR_SOAK_ARTIFACT_S3_BUCKET=... \
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --iterations 10 --concurrency 8
```

This command remains a local dry-run unless a future live mode is implemented; the variables are recorded as the disposable live dependency contract, not auto-consumed by the dry-run. The local dry-run should still exercise concurrent SQLite repository access and control-plane fanout. The live evidence table must have columns for PostgreSQL latency/conflict notes and NATS reconnect notes. If live variables are absent, record `not run` and keep the roadmap explicit that PostgreSQL latency/conflict evidence remains live-only.

- [ ] **Step 8: Run scenario-specific commands**

Run:

```bash
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --scenario artifact
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --scenario fanout
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --scenario restart
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --scenario storage
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --scenario terminal
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --iterations 2 --concurrency 2
```

Expected: each command prints `PASS`.

## Task 3: Add Control-Plane Metrics

**Files:**

- Modify: `crates/pandar-hub/src/metrics.rs`
- Modify: `crates/pandar-hub/src/metrics_export.rs`
- Modify: `crates/pandar-hub/src/lib.rs`
- Modify: `crates/pandar-hub/src/runtime.rs`
- Modify: `crates/pandar-hub/src/routes/tests/readiness_metrics.rs`
- Modify: `crates/pandar-hub/src/routes/tests/jobs/create.rs`

- [ ] **Step 1: Add counters to `MetricsState`**

Add:

```rust
#[derive(Debug, Default)]
struct ControlPlaneCounters {
    publish_ok: AtomicI64,
    publish_failed: AtomicI64,
    receive_ok: AtomicI64,
    receive_failed: AtomicI64,
}

#[derive(Debug, Clone, Copy)]
pub enum ControlPlaneMetric {
    PublishOk,
    PublishFailed,
    ReceiveOk,
    ReceiveFailed,
}
```

Add `control_plane: Arc<ControlPlaneCounters>` to `MetricsState`, initialize it, and implement:

```rust
pub fn record_control_plane(&self, metric: ControlPlaneMetric) {
    let counter = match metric {
        ControlPlaneMetric::PublishOk => &self.control_plane.publish_ok,
        ControlPlaneMetric::PublishFailed => &self.control_plane.publish_failed,
        ControlPlaneMetric::ReceiveOk => &self.control_plane.receive_ok,
        ControlPlaneMetric::ReceiveFailed => &self.control_plane.receive_failed,
    };
    counter.fetch_add(1, Ordering::Relaxed);
}

pub fn control_plane_snapshot(&self) -> [(&'static str, i64); 4] {
    [
        ("publish_ok", self.control_plane.publish_ok.load(Ordering::Relaxed)),
        ("publish_failed", self.control_plane.publish_failed.load(Ordering::Relaxed)),
        ("receive_ok", self.control_plane.receive_ok.load(Ordering::Relaxed)),
        ("receive_failed", self.control_plane.receive_failed.load(Ordering::Relaxed)),
    ]
}
```

- [ ] **Step 2: Export metrics**

In `metrics_export.rs`, append:

```rust
fn append_control_plane(output: &mut String, state: &AppState) {
    for (result, count) in state.metrics().control_plane_snapshot() {
        output.push_str(&format!(
            "pandar_control_plane_messages_total{{result=\"{result}\"}} {count}\n"
        ));
    }
}
```

Call it from `prometheus_metrics` after websocket metrics and before jobs.

- [ ] **Step 3: Record publish metrics**

In `AppState::wake_agent`, `close_agent`, and `publish_printer_event`, record `PublishOk` after successful `publish`, and `PublishFailed` in the error branch. Preserve existing logging and route behavior.

- [ ] **Step 4: Record receive metrics**

In `runtime::spawn_control_plane_inner`, record `ReceiveOk` after `handle_control_message` returns for valid messages and `ReceiveFailed` when the stream yields `Err(err)`. Keep the loop alive after errors.

- [ ] **Step 5: Test metrics output**

Extend `metrics_redacts_tenant_ids_and_reports_required_series` to record at least one `PublishOk`, `PublishFailed`, `ReceiveOk`, and `ReceiveFailed` directly through `state.metrics().record_control_plane(...)`, then assert:

```rust
assert!(text.contains("pandar_control_plane_messages_total{result=\"publish_ok\"} 1"));
assert!(text.contains("pandar_control_plane_messages_total{result=\"publish_failed\"} 1"));
assert!(text.contains("pandar_control_plane_messages_total{result=\"receive_ok\"} 1"));
assert!(text.contains("pandar_control_plane_messages_total{result=\"receive_failed\"} 1"));
```

Run:

```bash
cargo test -p pandar-hub metrics_redacts_tenant_ids_and_reports_required_series -- --nocapture
```

Expected: test passes and no raw tenant ids appear in metrics text.

- [ ] **Step 6: Test publish-failure success contract with metrics**

Extend `crates/pandar-hub/src/routes/tests/jobs/create.rs::print_job_returns_created_when_agent_wake_publish_fails` so it also asserts the control-plane publish-failure metric increments while the durable command row remains committed:

```rust
let metrics = state.metrics().control_plane_snapshot();
assert!(metrics.contains(&("publish_failed", 1)));
assert_eq!(state.commands().count().await.unwrap(), 1);
```

If direct tuple lookup is clearer, collect the snapshot into a map in the test. Keep the existing `StatusCode::CREATED` and command kind assertions. This pins the Phase 26 contract that a post-commit wake publish failure is observable but does not roll back already committed job/command state.

Run:

```bash
cargo test -p pandar-hub print_job_returns_created_when_agent_wake_publish_fails -- --nocapture
```

Expected: test passes and proves both committed durable state and failure observability.

## Task 4: Add Focused Failure Tests

**Files:**

- Modify: `crates/pandar-hub/src/cluster/tests.rs`
- Modify: `crates/pandar-hub/src/routes/tests/jobs/create.rs`
- Modify: `crates/pandar-hub/src/routes/tests/artifacts.rs`
- Modify: `crates/pandar-hub/src/routes/tests/printer_events_ws.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/print_reports.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/cleanup/storage.rs`

- [ ] **Step 1: Control-plane receive failure continues**

Add or extend a `cluster/tests.rs` test that feeds an invalid payload followed by a valid payload through `NatsControlPlane::subscribe()` and asserts the stream yields an error and then a valid `HubControlMessage`. Existing `nats_control_plane_subscribe_reports_decode_errors_and_continues` may already satisfy this; if so, leave it and reference it in final verification.

- [ ] **Step 2: Control-plane publish failure remains post-commit success**

Use the Task 3 metrics work to extend `routes/tests/jobs/create.rs::print_job_returns_created_when_agent_wake_publish_fails`.

The test must inject a wake publish failure after print job and command state have been committed, then assert:

1. the route still returns `StatusCode::CREATED`;
2. the durable command row remains committed;
3. the command kind is still the expected print command;
4. `state.metrics().control_plane_snapshot()` contains `("publish_failed", 1)`.

This focused test pins the Phase 26 contract that publish failure is observable without rolling back committed job/command state or crashing the serving route. Keep the existing full-chain `{err:#}` logging in the production error branch; do not add brittle log-capture assertions unless the repository already has a local log-capture helper for this pattern.

- [ ] **Step 3: Cross-replica WebSocket ticket safety matrix**

Add `printer_events_cross_replica_ticket_safety_matrix` in `routes/tests/printer_events_ws.rs`.

The test must use one shared database and two Hub states to prove ticket behavior across replicas:

1. issue a valid ticket through Hub A and consume it through Hub B successfully;
2. try to consume the same ticket through Hub B again and assert it is rejected before upgrade;
3. issue a wrong-tenant ticket through Hub A and assert Hub B rejects it for the printer route tenant;
4. seed an already expired ticket in the shared ticket repository or database, then assert Hub B rejects it before upgrade.

Use the existing sibling-state patterns in this file. Route-level issuance should go through `router(state.clone())`; route-level consumption should go through `router(sibling_state(&state))` or the existing equivalent helper. For the expired case, either expose a small local seed helper in this test module or insert the hashed ticket with an expired timestamp the same way repository ticket tests do. Keep the assertion at the HTTP/WebSocket boundary, not only at the repository boundary.

- [ ] **Step 4: Artifact storage write failure creates no durable print state**

Add or extend a focused route test in `routes/tests/jobs/create.rs` for plugin multipart print creation with a storage `put` failure.

The test must assert:

1. the route returns a stable non-201 error;
2. no print job row is created;
3. no command row is created;
4. no control-plane publish success is recorded.

Use the existing fake artifact storage pattern in route tests. If the exact external error label is already established by the route, assert that label; otherwise assert the status and zero durable rows without introducing a new public error contract.

- [ ] **Step 5: Artifact route backend read failure returns unavailable**

If `routes/tests/artifacts.rs::artifact_storage_failure_returns_unavailable_not_not_found` already proves this with a fake backend, keep it. Otherwise add:

```rust
#[tokio::test]
async fn artifact_backend_read_failure_returns_unavailable() {
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
```

- [ ] **Step 6: Cleanup delete failure preserves rows**

If `repositories/tests/cleanup/storage.rs` already covers this with a failing storage fake, keep it. Otherwise add a test that:

1. creates a stale artifact row;
2. runs cleanup execute with storage delete failure;
3. asserts the artifact row still exists;
4. asserts cleanup reports the storage failure.

- [ ] **Step 7: Terminal print report idempotence**

Add a focused test in `grpc/tests/print_reports.rs` or repository lifecycle tests:

```rust
#[tokio::test]
async fn grpc_print_job_terminal_report_is_idempotent_and_not_regressed() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (created, serial) = create_print_job(&state, tenant_id, agent_id, ARTIFACT_ID).await;
    let (_stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    let mut finish = report(serial.clone(), created.job.id.to_string(), created.artifact.id);
    finish.gcode_state = "FINISH".to_string();
    sender.send(Ok(report_event(tenant_id, agent_id, finish.clone()))).await.unwrap();
    sender.send(Ok(report_event(tenant_id, agent_id, finish))).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let finished = state.jobs().get_for_tenant(tenant_id, created.job.id).await.unwrap().unwrap().job;
    assert_eq!(finished.print.status, pandar_core::PrintStatus::Completed);

    let mut stale = report(serial, created.job.id.to_string(), created.artifact.id);
    stale.gcode_state = "RUNNING".to_string();
    stale.observed_at = "2026-06-22T09:59:00Z".to_string();
    sender.send(Ok(report_event(tenant_id, agent_id, stale))).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let after_stale = state.jobs().get_for_tenant(tenant_id, created.job.id).await.unwrap().unwrap().job;
    assert_eq!(after_stale.print.status, pandar_core::PrintStatus::Completed);
}
```

If this exact shape does not match repository semantics, adapt to the existing lifecycle helpers but keep the same assertions.

- [ ] **Step 8: Run targeted tests**

Run:

```bash
cargo test -p pandar-hub nats_control_plane_subscribe_reports_decode_errors_and_continues -- --nocapture
cargo test -p pandar-hub print_job_returns_created_when_agent_wake_publish_fails -- --nocapture
cargo test -p pandar-hub printer_events_cross_replica_ticket_safety_matrix -- --nocapture
cargo test -p pandar-hub plugin_print_storage_put_failure_creates_no_job_or_command -- --nocapture
cargo test -p pandar-hub artifact_storage_failure_returns_unavailable_not_not_found -- --nocapture
cargo test -p pandar-hub cleanup -- --nocapture
cargo test -p pandar-hub terminal_report -- --nocapture
```

Expected: all targeted tests pass. If `terminal_report` does not match test names, run the exact new test name.

## Task 5: Update Operations Documentation And Evidence

**Files:**

- Modify: `docs/development.md`
- Modify: `docs/release-installation.md`
- Create: `docs/compatibility/phase-26-soak-evidence.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Document local harness commands**

Add to `docs/development.md` under Operations:

````markdown
Phase 26 local HA/failure smoke:

```bash
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --iterations 2 --concurrency 2
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --scenario storage
```
````

The default mode uses local process fixtures and loopback HTTP only. It does not require Docker, PostgreSQL, NATS, MinIO, or cloud S3 credentials. Treat it as local convergence evidence, not as live deployment soak evidence.

````

- [ ] **Step 2: Document live soak variables**

Add:

```markdown
Optional live soak variables:

- `PANDAR_SOAK_DATABASE_URL`: disposable PostgreSQL database.
- `PANDAR_SOAK_NATS_URL`: disposable NATS server.
- `PANDAR_SOAK_ARTIFACT_S3_BUCKET`, `PANDAR_SOAK_ARTIFACT_S3_REGION`, `PANDAR_SOAK_ARTIFACT_S3_ENDPOINT`, `PANDAR_SOAK_ARTIFACT_S3_ACCESS_KEY_ID`, `PANDAR_SOAK_ARTIFACT_S3_SECRET_ACCESS_KEY`: disposable object-storage bucket.

Do not point live soak at production data. The live soak evidence path is skipped unless these variables are explicitly provided. When live dependencies are available, record PostgreSQL latency or transaction-conflict observations, NATS reconnect behavior, object-storage behavior, command output, and commit SHA in `docs/compatibility/phase-26-soak-evidence.md`.
````

- [ ] **Step 3: Add runbook checks**

In `docs/release-installation.md`, add a short operations runbook:

- SQLite single-node: check `/readyz`, `pandar_readyz`, artifact filesystem backup.
- PostgreSQL+NATS+object storage: check database readiness, control-plane metrics, artifact storage readiness, session metrics, command/job counts.
- Recovery steps:
  - Hub restart: verify agents reconnect or receive next wake, commands remain queued/sent in database.
  - NATS interruption: verify durable command state remains; restart broker/Hub subscriber; issue another wake-producing action if needed.
  - Storage outage: verify `/readyz` `artifact_storage=0`, upload/download failures are `artifact_unavailable` or stable upload labels, cleanup rows remain for retry.
  - Printer/report issues: inspect print report counters and full-chain agent logs.

- [ ] **Step 4: Create evidence file**

Create `docs/compatibility/phase-26-soak-evidence.md`:

```markdown
# Phase 26 Soak Evidence

## Local Dry-Run Evidence

| Date       | Commit                     | Command                                                                         | Scenarios                     | Result                                               | Notes                                                                   |
| ---------- | -------------------------- | ------------------------------------------------------------------------------- | ----------------------------- | ---------------------------------------------------- | ----------------------------------------------------------------------- |
| 2026-06-24 | working tree before commit | `cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run` | all default dry-run scenarios | to be updated during Task 6 after the command is run | Replace this row with the actual verification result before committing. |

## Live PostgreSQL + NATS + Object Storage Evidence

| Date    | Commit        | PostgreSQL   | PostgreSQL latency/conflict notes | NATS         | NATS reconnect notes | Object storage | Command | Result   | Notes                                                                      |
| ------- | ------------- | ------------ | --------------------------------- | ------------ | -------------------- | -------------- | ------- | -------- | -------------------------------------------------------------------------- |
| not run | not committed | not provided | not run                           | not provided | not run              | not provided   | not run | untested | Requires disposable live dependencies and must not target production data. |
```

Update the local row after verification with the actual command result before committing.

- [ ] **Step 5: Update roadmap**

Add Phase 26 completed bullets under `## Completed` and update `## Phase 26` to distinguish local evidence from live evidence. Do not mark live PostgreSQL/NATS/S3 soak complete unless it actually ran.

## Task 6: Final Verification, Review Gate, Commit, Push

**Files:**

- All files touched above.

- [ ] **Step 1: Run formatting and targeted checks**

Run:

```bash
cargo fmt --check
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run
cargo test -p pandar-hub metrics_redacts_tenant_ids_and_reports_required_series -- --nocapture
cargo test -p pandar-hub nats_control_plane_subscribe_reports_decode_errors_and_continues -- --nocapture
cargo test -p pandar-hub artifact_storage_failure_returns_unavailable_not_not_found -- --nocapture
cargo test -p pandar-hub cleanup -- --nocapture
```

Expected: all pass.

- [ ] **Step 2: Run full required verification**

Run:

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path Cargo.toml --workspace
npm --prefix frontend run build
git diff --check
```

Expected: all pass. If frontend files did not change, still run the build because Phase 26 verifies the current product surface before the commit.

- [ ] **Step 3: Clean generated targets**

Run:

```bash
rm -rf tools/scaled-artifact-smoke/target
find tools/scaled-artifact-smoke -maxdepth 2 -type d -name target -print
```

Expected: no output from `find`.

- [ ] **Step 4: Final SDD implementation review**

Dispatch independent Codex reviewer and opencode reviewer with:

- spec path;
- plan path;
- current diff or base/head SHA;
- verification commands and results;
- instruction to judge spec compliance only.

Required result from both:

```text
VERDICT: APPROVE
```

If either returns `REVISE`, fix the issue, rerun relevant verification, and re-run both reviewers.

- [ ] **Step 5: Commit and push**

Use Lore commit protocol:

```text
Prove scaled Hub recovery before new printer controls

Constraint: Phase 26 must produce repeatable local HA/failure evidence without requiring Docker or live external services.
Rejected: Treating Phase 25 local artifact smoke as production soak | It does not cover fanout, restart, storage failures, or operator diagnostics.
Confidence: high
Scope-risk: moderate
Directive: Keep live PostgreSQL/NATS/object-storage soak explicit and disposable; do not run it against production data.
Tested: cargo fmt --check; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --manifest-path Cargo.toml --workspace; npm --prefix frontend run build; cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run; git diff --check
Not-tested: live PostgreSQL/NATS/object-storage soak when disposable live dependencies are unavailable.
```

Replace the `Tested:` and `Not-tested:` lines with the exact commands that ran and the exact live dependencies that were unavailable before committing.

Then:

```bash
git add -A
git commit
git push origin main
```

Expected: push succeeds to `main`.
