# Phase 2 Agent Reverse Connection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Phase 2 reverse gRPC control channel so a local `pandar-agent` can connect outward to `pandar-hub`, heartbeat, receive refresh-printers commands, and acknowledge/complete them without opening Bambu machine sockets.

**Architecture:** Generate tonic/prost Rust code from `proto/pandar/agent/v1/agent.proto` during Cargo builds, keep generated API behind crate-local protocol modules, extend repositories for connection and command state, then layer a hub session registry and agent client loop on top. Keep Phase 2 state simple: SQLx persists durable agent/command metadata; an in-memory registry owns live streams.

**Tech Stack:** Rust 2024, Tokio, tonic 0.14.6, prost 0.14, tonic-prost, tonic-prost-build, protoc-bin-vendored, tokio-stream, SQLx SQLite/PostgreSQL, Axum for unchanged HTTP.

---

## Approved Spec

- `docs/superpowers/specs/2026-06-20-phase-2-agent-reverse-connection-design.md`
- Independent spec review: `VERDICT: APPROVE`

## File Map

- `Cargo.toml`: workspace dependencies for tonic, prost, tonic-prost, tonic-prost-build, protoc-bin-vendored, tokio-stream, futures-core if needed.
- `proto/pandar/agent/v1/agent.proto`: expanded Phase 2 service messages.
- `crates/pandar-core/src/lib.rs`: command ID/status/record domain types and parsing.
- `crates/pandar-hub/build.rs`: compile proto for hub.
- `crates/pandar-hub/src/protocol.rs`: include generated proto for hub.
- `crates/pandar-hub/src/repositories/agents.rs`: agent lookup and connection metadata updates.
- `crates/pandar-hub/src/repositories/counts.rs`: replace count-only command repository with command state methods or move command logic into `commands.rs`.
- `crates/pandar-hub/src/repositories/commands.rs`: command enqueue/query/update implementation if split from counts.
- `crates/pandar-hub/src/repositories/mod.rs`: repository exports and typed errors.
- `crates/pandar-hub/src/repositories/tests.rs`: repository tests for agent and command transitions.
- `crates/pandar-hub/migrations/sqlite/20260620010000_phase_2_agent_commands.sql`: SQLite index migration.
- `crates/pandar-hub/migrations/postgres/20260620010000_phase_2_agent_commands.sql`: PostgreSQL index migration.
- `crates/pandar-hub/src/sessions.rs`: in-memory session registry and dispatch API.
- `crates/pandar-hub/src/grpc.rs`: tonic AgentControl service implementation and tests.
- `crates/pandar-hub/src/lib.rs`: expose hub gRPC/session modules through `AppState`.
- `crates/pandar-hub/src/main.rs`: start HTTP and gRPC listeners together.
- `crates/pandar-agent/build.rs`: compile proto for agent.
- `crates/pandar-agent/src/protocol.rs`: include generated proto for agent.
- `crates/pandar-agent/src/lib.rs`: config, reconnect loop, heartbeat, command handling.
- `crates/pandar-agent/src/main.rs`: call async agent runner.
- `README.md`, `docs/architecture.md`, `docs/roadmap.md`: Phase 2 docs.

## Task 1: Proto And Generated Code

**Files:**

- Modify: `Cargo.toml`
- Modify: `proto/pandar/agent/v1/agent.proto`
- Modify: `crates/pandar-hub/Cargo.toml`
- Create: `crates/pandar-hub/build.rs`
- Create: `crates/pandar-hub/src/protocol.rs`
- Modify: `crates/pandar-agent/Cargo.toml`
- Create: `crates/pandar-agent/build.rs`
- Create: `crates/pandar-agent/src/protocol.rs`

- [ ] **Step 1: Add dependencies**

Add workspace dependencies:

```toml
prost = "0.14"
protoc-bin-vendored = "3.2.0"
tokio-stream = "0.1"
tonic = "0.14.6"
tonic-prost = "0.14.6"
tonic-prost-build = "0.14.6"
```

Use `tonic.workspace = true`, `tonic-prost.workspace = true`, `prost.workspace = true`, and `tokio-stream.workspace = true` in `pandar-hub` and `pandar-agent`. Use `tonic-prost-build.workspace = true` and `protoc-bin-vendored.workspace = true` in each crate `build-dependencies`.

- [ ] **Step 2: Expand proto**

Replace `proto/pandar/agent/v1/agent.proto` with the approved Phase 2 contract:

```proto
syntax = "proto3";

package pandar.agent.v1;

service AgentControl {
  rpc ReverseConnect(stream AgentEvent) returns (stream HubCommand);
}

message AgentEvent {
  string agent_id = 1;
  string tenant_id = 2;
  string event_id = 3;
  oneof event {
    AgentHello hello = 10;
    AgentHeartbeat heartbeat = 11;
    PrinterSnapshot printer_snapshot = 12;
    CommandAck command_ack = 13;
    CommandResult command_result = 14;
  }
}

message AgentHello {
  string name = 1;
  string version = 2;
}

message AgentHeartbeat {
  string observed_at = 1;
}

message PrinterSnapshot {
  string serial = 1;
  string name = 2;
  string state = 3;
}

message CommandAck {
  string command_id = 1;
  bool accepted = 2;
  string error = 3;
}

message CommandResult {
  string command_id = 1;
  bool success = 2;
  string error = 3;
}

message HubCommand {
  string command_id = 1;
  oneof command {
    RefreshPrinters refresh_printers = 10;
  }
}

message RefreshPrinters {}
```

Use `ReverseConnect` rather than `Connect`; `Connect` collides with tonic's generated client `connect(...)` constructor.

- [ ] **Step 3: Add build scripts**

Each build script compiles the shared proto:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../proto/pandar/agent/v1/agent.proto");
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    unsafe {
        std::env::set_var("PROTOC", protoc);
    }
    tonic_prost_build::configure()
        .compile_protos(&["../../proto/pandar/agent/v1/agent.proto"], &["../../proto"])?;
    Ok(())
}
```

Adjust relative paths per crate. For `crates/pandar-hub/build.rs` and `crates/pandar-agent/build.rs`, `../../proto` is correct from each crate directory.

Generated protobuf Rust files must stay in Cargo `OUT_DIR` under `target/`. Do not commit generated `.rs` files; `target/` remains gitignored.

- [ ] **Step 4: Include generated modules**

Each crate protocol module:

```rust
pub mod agent {
    pub mod v1 {
        tonic::include_proto!("pandar.agent.v1");
    }
}
```

- [ ] **Step 5: Verify generated code compiles**

Run:

```bash
cargo test -p pandar-hub protocol --no-run
cargo test -p pandar-agent protocol --no-run
```

Expected: both commands compile without requiring a live database.

## Task 2: Core Command Domain And Repository Persistence

**Files:**

- Modify: `crates/pandar-core/src/lib.rs`
- Modify: `crates/pandar-hub/src/repositories/mod.rs`
- Modify: `crates/pandar-hub/src/repositories/agents.rs`
- Modify: `crates/pandar-hub/src/repositories/counts.rs`
- Create: `crates/pandar-hub/src/repositories/commands.rs` if splitting command logic from counts
- Modify: `crates/pandar-hub/src/repositories/tests.rs`
- Create: `crates/pandar-hub/migrations/sqlite/20260620010000_phase_2_agent_commands.sql`
- Create: `crates/pandar-hub/migrations/postgres/20260620010000_phase_2_agent_commands.sql`

- [ ] **Step 1: Add core command types**

Add:

```rust
pub struct CommandId(Uuid);
pub enum CommandStatus { Queued, Sent, Acknowledged, Succeeded, Failed }
pub struct CommandRecord {
    pub id: CommandId,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub printer_id: Option<String>,
    pub kind: String,
    pub status: CommandStatus,
    pub payload_json: String,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
```

Implement UUID parsing for `CommandId`, `Display`, `Default`, `CommandStatus::as_str`, `FromStr`, and `CommandRecord::from_parts`. Keep validation limited to empty command kind and invalid status.

- [ ] **Step 2: Add migration indexes**

SQLite and PostgreSQL migration SQL:

```sql
CREATE INDEX idx_commands_agent_status ON commands(agent_id, status);
```

Do not alter existing table names or use backend-native enums.

- [ ] **Step 3: Extend repository errors**

Add typed variants:

```rust
MissingAgent,
MissingCommand,
CommandOwnershipMismatch,
InvalidCommandTransition { from: String, action: &'static str },
InvalidPersistedCommandStatus(String),
```

- [ ] **Step 4: Extend AgentRepository**

Implement:

```rust
pub async fn get(&self, agent_id: AgentId) -> RepositoryResult<Option<Agent>>;
pub async fn update_connection(
    &self,
    agent_id: AgentId,
    status: AgentStatus,
    version: Option<&str>,
    last_seen_at: &str,
) -> RepositoryResult<Agent>;
pub async fn mark_offline(&self, agent_id: AgentId, last_seen_at: &str) -> RepositoryResult<Agent>;
```

Select `version` and `last_seen_at` only for persistence updates; the current `Agent` core type does not need to expose them unless implementation requires it.

- [ ] **Step 5: Implement CommandRepository methods**

Implement:

```rust
pub async fn enqueue_refresh_printers(
    &self,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> RepositoryResult<CommandRecord>;
pub async fn next_queued_for_agent(
    &self,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> RepositoryResult<Option<CommandRecord>>;
pub async fn mark_sent(
    &self,
    command_id: CommandId,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> RepositoryResult<CommandRecord>;
pub async fn mark_acknowledged(
    &self,
    command_id: CommandId,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> RepositoryResult<CommandRecord>;
pub async fn mark_succeeded(
    &self,
    command_id: CommandId,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> RepositoryResult<CommandRecord>;
pub async fn mark_failed(
    &self,
    command_id: CommandId,
    tenant_id: TenantId,
    agent_id: AgentId,
    error: &str,
) -> RepositoryResult<CommandRecord>;
```

Before inserting, verify the agent exists and belongs to tenant. For updates, load command by ID, check tenant/agent ownership, then enforce the approved transition table.

- [ ] **Step 6: Repository tests**

Add SQLite tests for:

- `agent_get_update_connection_and_mark_offline_work`
- `command_enqueue_rejects_missing_agent`
- `command_enqueue_rejects_wrong_tenant`
- `command_queue_filters_by_tenant_and_agent`
- `command_update_rejects_missing_command`
- `command_update_rejects_wrong_tenant`
- `command_update_rejects_wrong_agent`
- `command_sent_ack_success_flow`
- `command_ack_failure_marks_failed`
- `command_result_failure_marks_failed`
- `command_duplicate_terminal_events_are_idempotent`
- `command_stale_events_are_rejected`

Add optional PostgreSQL coverage under `PANDAR_TEST_POSTGRES_URL` for all database-dependent repository behavior:

- agent get/update/offline
- enqueue missing-agent rejection
- enqueue wrong-tenant rejection
- queue filtering by tenant and agent
- missing-command update rejection
- wrong-tenant update rejection
- wrong-agent update rejection
- sent/ack/success transition flow
- ack failure and result failure
- duplicate terminal idempotency
- stale/out-of-order transition rejection

When the env var is absent, PostgreSQL tests must print or otherwise take a clean skip path without failing.

- [ ] **Step 7: Verify repository layer**

Run:

```bash
cargo test -p pandar-core
cargo test -p pandar-hub repositories
cargo fmt --check -p pandar-core
cargo fmt --check -p pandar-hub
```

Expected: all pass.

## Task 3: Hub Session Registry And gRPC Service

**Files:**

- Modify: `crates/pandar-hub/src/lib.rs`
- Create: `crates/pandar-hub/src/sessions.rs`
- Create: `crates/pandar-hub/src/grpc.rs`
- Add tests in `crates/pandar-hub/src/grpc.rs` or `crates/pandar-hub/src/grpc/tests.rs`

- [ ] **Step 1: Extend AppState**

Add a cloned `SessionRegistry` to `AppState`, initialize it in `from_database`, and expose:

```rust
pub fn sessions(&self) -> &SessionRegistry;
```

- [ ] **Step 2: Implement SessionRegistry**

Use an async lock or concurrent map keyed by `AgentId`. Store:

```rust
tenant_id: TenantId,
agent_id: AgentId,
name: String,
version: String,
connected_at: String,
last_heartbeat_at: String,
wake_sender: tokio::sync::mpsc::Sender<()>,
```

Implement:

```rust
register(session) -> previous session is replaced
touch_heartbeat(agent_id, observed_at)
remove(agent_id)
dispatch_refresh_printers(tenant_id, agent_id, commands: &CommandRepository)
expire_stale(now, timeout, agents: &AgentRepository)
```

- [ ] **Step 3: Implement AgentControl service**

In `grpc.rs`, implement generated `AgentControl` trait. Connect flow:

1. Read first inbound message.
2. Require hello.
3. Parse IDs.
4. Load and validate agent/tenant.
5. Persist online status with version and last_seen.
6. Register session.
7. Spawn inbound event handling for heartbeat/ack/result.
8. Return a `ReceiverStream<Result<HubCommand, tonic::Status>>` that drains queued commands after wake-ups.

- [ ] **Step 4: Map errors to tonic statuses**

Map exactly:

- malformed UUID -> `invalid_argument`
- non-hello first event -> `failed_precondition`
- missing agent/command -> `not_found`
- tenant or command ownership mismatch -> `permission_denied`
- invalid command transition -> `failed_precondition`
- unexpected repository/runtime failures -> `internal`, with full error chain logged.

- [ ] **Step 5: Hub gRPC tests**

Add tests for:

- first event not hello rejected.
- malformed IDs rejected.
- missing agent rejected.
- tenant mismatch rejected.
- hello marks agent online.
- heartbeat updates last_seen.
- dispatch to online agent yields refresh command and marks sent.
- ack/result update command status.
- ack/result for another agent's command returns permission denied.
- ack/result for an unknown command returns not found.
- stale/out-of-order ack/result returns failed precondition.
- timeout marks agent offline using shorter test timeout.

Use in-memory SQLite and in-process tonic service where possible.

- [ ] **Step 6: Verify hub gRPC layer**

Run:

```bash
cargo test -p pandar-hub grpc sessions
cargo fmt --check -p pandar-hub
```

If Cargo rejects multiple test filters, run equivalent filters separately.

## Task 4: Hub Runtime Startup

**Files:**

- Modify: `crates/pandar-hub/src/main.rs`
- Modify: `crates/pandar-hub/src/lib.rs` if a helper is needed

- [ ] **Step 1: Add gRPC bind config**

Read:

```rust
let grpc_bind_addr = std::env::var("PANDAR_HUB_GRPC_BIND")
    .unwrap_or_else(|_| "0.0.0.0:50051".to_owned());
```

- [ ] **Step 2: Start HTTP and gRPC together**

Bind both listeners, then run both servers under `tokio::try_join!` or a select that returns the first error with context. Preserve the existing `PANDAR_HUB_BIND` and `PANDAR_DATABASE_URL` behavior.

- [ ] **Step 3: Verify startup compiles**

Run:

```bash
cargo test -p pandar-hub
cargo clippy -p pandar-hub --all-targets -- -D warnings
```

Expected: all pass.

## Task 5: Agent Reverse Client

**Files:**

- Modify: `crates/pandar-agent/Cargo.toml`
- Modify: `crates/pandar-agent/src/lib.rs`
- Modify: `crates/pandar-agent/src/main.rs`
- Add tests in `crates/pandar-agent/src/lib.rs` or split if the file approaches 400 LOC

- [ ] **Step 1: Extend AgentConfig**

Add:

```rust
#[arg(long, env = "PANDAR_AGENT_ID")]
pub agent_id: String,
#[arg(long, env = "PANDAR_TENANT_ID")]
pub tenant_id: String,
#[arg(long, env = "PANDAR_AGENT_VERSION", default_value = env!("CARGO_PKG_VERSION"))]
pub agent_version: String,
```

- [ ] **Step 2: Implement event helpers**

Add pure helpers:

```rust
fn hello_event(config: &AgentConfig) -> AgentEvent;
fn heartbeat_event(config: &AgentConfig) -> AgentEvent;
fn ack_event(config: &AgentConfig, command_id: String) -> AgentEvent;
fn success_event(config: &AgentConfig, command_id: String) -> AgentEvent;
```

- [ ] **Step 3: Implement command handler**

For `RefreshPrinters`, send ack accepted and success result. Do not touch Bambu machine gateway or sockets.

- [ ] **Step 4: Implement reconnect loop**

`run(config)` becomes async and:

- connects to `AgentControlClient::connect(config.hub_grpc_url.clone())`
- calls `reverse_connect(...)` with a stream whose first item is hello
- sends heartbeats every 15 seconds
- reads hub commands and handles refresh-printers
- on stream/connect error, waits 1s, 2s, 4s, up to 30s, then retries

Tests should cover pure backoff calculation and command handling without sleeping for real retry intervals.

- [ ] **Step 5: Update main**

Use `#[tokio::main]` and call async `run(config).await`.

- [ ] **Step 6: Agent tests**

Add tests for:

- CLI parses hub URL, agent name, agent ID, tenant ID, and version.
- hello event is first-event shaped and includes IDs/name/version.
- refresh-printers handler emits accepted ack and success result.
- backoff doubles and caps at 30 seconds.

- [ ] **Step 7: Verify agent**

Run:

```bash
cargo test -p pandar-agent
cargo fmt --check -p pandar-agent
```

Expected: all pass.

## Task 6: Integration Verification And Docs

**Files:**

- Modify: `README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Add local integration test**

If not already covered in Task 3, add one test that creates a SQLite-backed hub state, creates a tenant and agent through repositories, starts the in-process gRPC service, connects an agent stream, dispatches refresh-printers, and verifies sent/ack/succeeded command states.

- [ ] **Step 2: Update docs**

README:

- document `PANDAR_HUB_GRPC_BIND`.
- document `PANDAR_AGENT_ID`, `PANDAR_TENANT_ID`, `PANDAR_AGENT_NAME`, `PANDAR_AGENT_VERSION`, `PANDAR_HUB_GRPC_URL`.
- state Phase 2 does not open Bambu sockets.

Architecture:

- describe reverse gRPC session flow.
- describe session registry and command ledger relationship.

Roadmap:

- mark completed Phase 2 items.
- move Immediate Next to Phase 3 Bambu machine transport.

- [ ] **Step 3: Full verification**

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
git diff --check
```

Expected: all pass.

## Subagent Execution Order

1. Task 1: proto/build plumbing.
2. Task 2: core and repository persistence.
3. Task 3: hub session registry and gRPC service.
4. Task 4: hub runtime startup.
5. Task 5: agent reverse client.
6. Task 6: integration verification and docs.

Do not run implementation tasks in parallel because several tasks intentionally build on generated protocol and repository APIs.

## Review Gates

After each implementation task:

- Run the task's local verification command.
- Inspect the diff.
- Request spec compliance review for that task.
- Request code quality review for that task.
- Fix and re-review before starting the next task.

After all tasks:

- Run full verification.
- Request final implementation review against the approved spec and this plan.
- Commit and push only after `VERDICT: APPROVE`.
