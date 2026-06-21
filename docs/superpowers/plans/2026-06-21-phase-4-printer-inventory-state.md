# Phase 4 Printer Inventory And State Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist agent-reported printer snapshots in the hub, expose tenant-scoped printer inventory/state APIs and WebSocket events, and show a minimal frontend inventory dashboard.

**Architecture:** Keep printer identity and validation in `pandar-core`, hub persistence in a real `PrinterRepository`, gRPC snapshot ingestion in a small handler module, WebSocket fanout in an in-memory tenant broadcaster, and frontend reads through HTTP only. Do not implement print dispatch, auth, credential storage, or live Bambu sockets.

**Tech Stack:** Rust 2024, axum HTTP/WebSocket, tonic gRPC, SQLx SQLite/PostgreSQL migrations, tokio broadcast channels, Next.js 16 server component frontend, Tailwind CSS.

---

## File Structure

- Modify `Cargo.toml` to enable axum WebSocket support with the existing workspace dependency.
- Modify `crates/pandar-core/src/lib.rs` and `crates/pandar-core/src/tests.rs` for printer domain records.
- Modify `proto/pandar/agent/v1/agent.proto` to add `PrinterSnapshot.model`.
- Modify `crates/pandar-agent/src/machine/mod.rs`, `crates/pandar-agent/src/machine/mqtt.rs`, and `crates/pandar-agent/src/commands.rs` to carry model through snapshots.
- Create `crates/pandar-hub/migrations/sqlite/20260621000000_phase_4_printer_state.sql`.
- Create `crates/pandar-hub/migrations/postgres/20260621000000_phase_4_printer_state.sql`.
- Replace `crates/pandar-hub/src/repositories/counts.rs` with `crates/pandar-hub/src/repositories/printers.rs`.
- Modify `crates/pandar-hub/src/repositories/mod.rs` and repository tests to use the new printer repository.
- Create `crates/pandar-hub/src/printer_events.rs` for tenant-scoped WebSocket broadcast state.
- Create `crates/pandar-hub/src/grpc/printer_snapshots.rs` and move snapshot-specific handling out of `grpc.rs`.
- Modify `crates/pandar-hub/src/grpc.rs` to call the snapshot handler and broadcast after persistence.
- Create `crates/pandar-hub/src/grpc/tests/printer_snapshots.rs` for snapshot persistence/stale-session tests.
- Modify `crates/pandar-hub/src/routes.rs` and `crates/pandar-hub/src/routes/tests.rs` for printer list/detail/refresh routes and WebSocket tests. Split route response helpers into small private functions if `routes.rs` approaches 400 LOC.
- Modify `crates/pandar-hub/src/lib.rs` to add `PrinterEventHub` to `AppState`.
- Modify `frontend/app/page.tsx` and `frontend/app/globals.css` for the read-only operational dashboard.
- Modify `README.md`, `docs/architecture.md`, and `docs/roadmap.md` after implementation review.

## Milestone 1: Core, Proto, And Agent Snapshot Model

**Files:**
- Modify: `crates/pandar-core/src/lib.rs`
- Modify: `crates/pandar-core/src/tests.rs`
- Modify: `proto/pandar/agent/v1/agent.proto`
- Modify: `crates/pandar-agent/src/machine/mod.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/tests.rs`
- Modify: `crates/pandar-agent/src/commands.rs`
- Modify: `crates/pandar-agent/src/commands/tests.rs`

- [ ] **Step 1: Add core printer tests**

Add tests for:

```rust
#[test]
fn printer_from_parts_validates_required_fields() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer = Printer::from_parts(PrinterParts {
        id: "printer-1".to_string(),
        tenant_id,
        agent_id,
        serial_number: "01S00EXAMPLE".to_string(),
        name: "garage-a1".to_string(),
        model: Some("A1 Mini".to_string()),
        status: "RUNNING".to_string(),
        last_seen_at: "2026-06-21T00:00:00Z".to_string(),
        created_at: "2026-06-21T00:00:00Z".to_string(),
    })
    .unwrap();

    assert_eq!(printer.tenant_id, tenant_id);
    assert_eq!(printer.agent_id, agent_id);
    assert_eq!(printer.serial_number, "01S00EXAMPLE");
    assert_eq!(printer.model.as_deref(), Some("A1 Mini"));

    let err = Printer::from_parts(PrinterParts {
        serial_number: " ".to_string(),
        ..printer_parts()
    })
    .unwrap_err();
    assert_eq!(err, CoreError::EmptyPrinterSerialNumber);
}
```

Also add helper `printer_parts()` in the test module and cover empty `id`, `name`, and `status`.

- [ ] **Step 2: Implement core printer types**

Add `Printer`, `PrinterParts`, and core errors:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Printer {
    pub id: String,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub serial_number: String,
    pub name: String,
    pub model: Option<String>,
    pub status: String,
    pub last_seen_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterParts {
    pub id: String,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub serial_number: String,
    pub name: String,
    pub model: Option<String>,
    pub status: String,
    pub last_seen_at: String,
    pub created_at: String,
}
```

Validation rejects blank `id`, `serial_number`, `name`, and `status`. Trim optional `model` to `None` when blank.

- [ ] **Step 3: Extend protobuf snapshot**

Change `PrinterSnapshot` to:

```proto
message PrinterSnapshot {
  string serial = 1;
  string name = 2;
  string state = 3;
  string model = 4;
}
```

Do not commit generated Rust protobuf output.

- [ ] **Step 4: Carry model in agent machine snapshots**

Update `MachineSnapshot`:

```rust
pub struct MachineSnapshot {
    pub serial: String,
    pub name: String,
    pub model: Option<String>,
    pub state: String,
}
```

Set `model: endpoint.model.clone()` in MQTT report normalization and configured gateway tests.

- [ ] **Step 5: Emit snapshot model in agent command events**

Update `printer_snapshot_event`:

```rust
PrinterSnapshot {
    serial: snapshot.serial,
    name: snapshot.name,
    state: snapshot.state,
    model: snapshot.model.unwrap_or_default(),
}
```

Update command tests to expect `model` for configured snapshots and an empty model when absent.

- [ ] **Step 6: Verify milestone 1**

Run:

```bash
cargo test -p pandar-core --no-fail-fast
cargo test -p pandar-agent --no-fail-fast
find . -path ./target -prune -o \( -name '*.pb.rs' -o -name '*.tonic.rs' \) -print
```

Expected: tests pass and find prints nothing.

## Milestone 2: Printer Repository And Migrations

**Files:**
- Create: `crates/pandar-hub/migrations/sqlite/20260621000000_phase_4_printer_state.sql`
- Create: `crates/pandar-hub/migrations/postgres/20260621000000_phase_4_printer_state.sql`
- Create: `crates/pandar-hub/src/repositories/printers.rs`
- Modify: `crates/pandar-hub/src/repositories/mod.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/mod.rs`
- Create: `crates/pandar-hub/src/repositories/tests/printers.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/phase1.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/postgres.rs`

- [ ] **Step 1: Add migrations**

SQLite:

```sql
ALTER TABLE printers ADD COLUMN last_seen_at TEXT;
UPDATE printers SET last_seen_at = created_at WHERE last_seen_at IS NULL;
CREATE INDEX idx_printers_tenant_agent ON printers(tenant_id, agent_id);
```

PostgreSQL:

```sql
ALTER TABLE printers ADD COLUMN last_seen_at TEXT;
UPDATE printers SET last_seen_at = created_at WHERE last_seen_at IS NULL;
CREATE INDEX idx_printers_tenant_agent ON printers(tenant_id, agent_id);
```

- [ ] **Step 2: Replace count-only repository with printer repository**

Rename the module export to `printers` and implement:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterSnapshotUpsert {
    pub serial_number: String,
    pub name: String,
    pub model: Option<String>,
    pub status: String,
    pub observed_at: String,
}
```

Methods:

- `count()`
- `list_for_tenant(tenant_id)`
- `get_for_tenant(tenant_id, printer_id)`
- `upsert_snapshot(tenant_id, agent_id, snapshot)`

Use SQL `ON CONFLICT (tenant_id, serial_number) DO UPDATE` for both SQLite and PostgreSQL. On create, generate `uuid::Uuid::new_v4().to_string()`. On update, preserve existing `id` and `created_at`, update `agent_id`, `name`, `model`, `status`, and `last_seen_at`.

- [ ] **Step 3: Add repository tests**

Tests must cover:

- `printer_repository_upserts_and_lists_for_tenant`
- `printer_repository_get_returns_none_for_unknown_printer`
- `printer_repository_list_rejects_missing_tenant`
- `printer_repository_reassigns_serial_to_latest_agent`
- `printer_repository_rejects_missing_agent`
- `invalid_persisted_printer_status_is_reported_with_context`

For PostgreSQL optional tests, add one coverage test under the existing `PANDAR_TEST_POSTGRES_URL` guard for upsert/list behavior.

- [ ] **Step 4: Update migration tests**

Extend SQLite migration schema test to assert `last_seen_at` column exists:

```rust
let count: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM pragma_table_info('printers') WHERE name = 'last_seen_at'",
)
.fetch_one(&pool)
.await
.unwrap();
assert_eq!(count, 1);
```

- [ ] **Step 5: Verify milestone 2**

Run:

```bash
cargo test -p pandar-hub repositories::tests::printers --no-fail-fast
cargo test -p pandar-hub repositories::tests::phase1::sqlite_migrations_create_phase_1_schema --no-fail-fast
```

Expected: targeted hub repository tests pass.

## Milestone 3: Hub Snapshot Ingestion, HTTP API, And WebSocket Broadcast

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/pandar-hub/Cargo.toml`
- Create: `crates/pandar-hub/src/printer_events.rs`
- Modify: `crates/pandar-hub/src/lib.rs`
- Create: `crates/pandar-hub/src/grpc/printer_snapshots.rs`
- Modify: `crates/pandar-hub/src/grpc.rs`
- Create: `crates/pandar-hub/src/grpc/tests/printer_snapshots.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/mod.rs`
- Modify: `crates/pandar-hub/src/routes.rs`
- Modify: `crates/pandar-hub/src/routes/tests.rs`

- [ ] **Step 1: Enable axum WebSocket feature**

Change workspace axum dependency:

```toml
axum = { version = "0.8.9", features = ["ws"] }
```

- [ ] **Step 2: Add printer event broadcaster**

Create `PrinterEventHub` using `tokio::sync::broadcast` and `DashMap` is not required. A simple `Arc<Mutex<HashMap<String, broadcast::Sender<PrinterEvent>>>>` is enough because broadcasts are not hot-path in Phase 4.

Public API:

```rust
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PrinterEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub printer: PrinterEventPrinter,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PrinterEventPrinter {
    // same fields as HTTP PrinterResponse
}

impl PrinterEventHub {
    pub fn new() -> Self;
    pub async fn subscribe(&self, tenant_id: TenantId) -> broadcast::Receiver<PrinterEvent>;
    pub async fn publish_snapshot(&self, printer: Printer);
}
```

- [ ] **Step 3: Wire broadcaster into AppState**

Add `printer_events: PrinterEventHub` to `AppState`, initialize in `from_database`, and expose `printer_events()`.

- [ ] **Step 4: Add gRPC snapshot handler module**

Move snapshot ingestion into `grpc/printer_snapshots.rs`:

```rust
pub async fn handle_snapshot(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    snapshot: PrinterSnapshot,
) -> Result<(), Status>
```

Rules:

- Trim and reject blank `serial`, `name`, or `state` with `invalid_argument`.
- Trim `model`; empty becomes `None`.
- Use `pandar_core::created_at_now()` as `observed_at`.
- Call `state.printers().upsert_snapshot(...)`.
- Call `state.printer_events().publish_snapshot(printer).await`.

In `grpc.rs`, replace ignored `PrinterSnapshot` arm with token-scoped call through `sessions().while_current(...)`.

- [ ] **Step 5: Add gRPC tests**

Tests:

- `grpc_printer_snapshot_persists_printer_state`
- `grpc_printer_snapshot_rejects_empty_serial`
- `grpc_stale_printer_snapshot_does_not_mutate_replacement_session`

Use existing `connect_live` helpers.

- [ ] **Step 6: Add HTTP printer routes**

Add routes:

```rust
.route("/api/v1/tenants/{tenant_id}/printers", get(list_printers))
.route("/api/v1/tenants/{tenant_id}/printers/{printer_id}", get(get_printer))
.route(
    "/api/v1/tenants/{tenant_id}/agents/{agent_id}/refresh-printers",
    post(refresh_printers),
)
.route("/api/v1/tenants/{tenant_id}/printer-events", get(printer_events))
```

Add `PrinterResponse`, `PrinterListResponse`, and `CommandResponse`. Keep conversions private and small.

`refresh_printers` parses tenant/agent IDs and calls `state.sessions().dispatch_refresh_printers(tenant_id, agent_id, state.commands()).await`.

- [ ] **Step 7: Add WebSocket route**

Before upgrade, parse tenant ID and call `state.tenants().exists` or `state.printers().list_for_tenant(tenant_id)` to validate missing tenant. If the tenant is missing, return `404`.

After upgrade, forward `PrinterEvent` JSON from the broadcast receiver:

```rust
while let Ok(event) = receiver.recv().await {
    let payload = serde_json::to_string(&event).context("encode printer event")?;
    if socket.send(Message::Text(payload.into())).await.is_err() {
        break;
    }
}
```

Use an internal helper that logs encoding errors with `{err:#}`.

- [ ] **Step 8: Add route and WebSocket tests**

Route tests:

- list returns tenant printers.
- detail returns one printer.
- detail missing printer returns `printer_not_found`.
- refresh route enqueues command and returns queued command JSON.
- invalid agent ID returns `invalid_agent_id`.
- printer-events invalid tenant ID returns `400` before upgrade.
- printer-events missing tenant returns `404` before upgrade.

WebSocket test:

- Open `/api/v1/tenants/{tenant_id}/printer-events`.
- Connect an agent over the gRPC helper.
- Send a valid `PrinterSnapshot` event through that live gRPC stream.
- Assert received text has `type: printer_snapshot` and expected serial/status.

- [ ] **Step 9: Verify milestone 3**

Run:

```bash
cargo test -p pandar-hub grpc::tests::printer_snapshots --no-fail-fast
cargo test -p pandar-hub routes::tests --no-fail-fast
cargo test -p pandar-hub --no-fail-fast
```

Expected: hub tests pass.

## Milestone 4: Frontend Inventory Dashboard And Docs

**Files:**
- Modify: `frontend/app/page.tsx`
- Modify: `frontend/app/globals.css`
- Modify: `README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Replace landing content with operational dashboard**

Use a server component that fetches:

- `${APP_API_URL}/api/v1/summary`
- `${APP_API_URL}/api/v1/tenants`
- first tenant's `/api/v1/tenants/{tenant_id}/printers`

Use `cache: 'no-store'`. Render quiet dashboard sections for summary, tenant selector, and printer table/cards. Do not add client state or WebSocket usage.

- [ ] **Step 2: Add frontend error and empty states**

Render:

- "No tenants" when tenant list is empty.
- "No printers reported" when selected tenant has no printers.
- A compact error banner when a fetch fails.

- [ ] **Step 3: Keep styling dense and operational**

Use restrained full-page layout, neutral colors, status badges, and stable table/card dimensions. Do not add marketing hero sections or nested cards.

- [ ] **Step 4: Update docs**

README:

- Document new printer HTTP endpoints.
- Document WebSocket endpoint and non-replay behavior.
- Document frontend dashboard reads `APP_API_URL`.

Architecture:

- Mark Phase 4 behavior under hub/agent/frontend sections.
- State that frontend does not yet consume WebSocket events.

Roadmap:

- Move Phase 4 items to completed.
- Update Immediate Next toward Phase 5 print dispatch.

- [ ] **Step 5: Verify milestone 4**

Run:

```bash
npm run build
```

from `frontend/`. Expected: Next.js production build succeeds.

## Milestone 5: Final SDD Review, Workspace Verification, Commit, Push

**Files:**
- All changed files.

- [ ] **Step 1: Run full verification**

Run:

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
npm run build
git diff --check
find . -path ./target -prune -o \( -name '*.pb.rs' -o -name '*.tonic.rs' \) -print
```

Expected:

- Rust formatting/lint/tests pass.
- Frontend builds.
- Diff check passes.
- Find command prints nothing.

- [ ] **Step 2: Check file sizes**

Run:

```bash
wc -l crates/pandar-core/src/lib.rs crates/pandar-hub/src/routes.rs crates/pandar-hub/src/grpc.rs crates/pandar-hub/src/grpc/printer_snapshots.rs crates/pandar-hub/src/repositories/printers.rs crates/pandar-hub/src/printer_events.rs frontend/app/page.tsx
```

Expected: every touched source file is below 400 LOC. Split further before final review if not.

- [ ] **Step 3: Independent final implementation review**

Dispatch a fresh reviewer with the approved spec, approved plan, diff, and verification output. Do not commit until the reviewer returns:

```text
VERDICT: APPROVE
```

- [ ] **Step 4: Commit and push**

Use the Lore commit protocol. Suggested intent line:

```text
Persist printer inventory from agent snapshots
```

Include `Tested:` trailers with the exact successful commands. Push the current branch `codex/phase-4-printer-inventory-state`.

## Self-Review

- Spec coverage: the plan covers core printer model, proto/model propagation, repository/migrations, gRPC ingestion, HTTP APIs, WebSocket broadcast, frontend read-only dashboard, docs, verification, and final review.
- Placeholder scan: no TBD/TODO placeholders are left in implementation steps.
- Type consistency: `Printer`, `PrinterParts`, `PrinterSnapshotUpsert`, `PrinterEvent`, and route response names are defined before use.
- Scope check: auth, credentials, print dispatch, live MQTT subscription, and real printer validation remain out of scope.
