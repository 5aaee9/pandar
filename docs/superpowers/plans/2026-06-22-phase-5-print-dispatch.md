# Phase 5 Print Dispatch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first durable print-dispatch path from hub HTTP job creation to agent file upload and MQTT `project_file` publish, without real Bambu sockets in tests.

**Architecture:** Keep job/artifact identity in `pandar-core`, backend-neutral persistence in hub repositories, protocol conversion in gRPC command modules, agent execution behind artifact/file-transfer/MQTT boundaries, and frontend as a server-rendered HTTP-only operator view. Split files that are already near 400 LOC before adding behavior.

**Tech Stack:** Rust 2024, axum, tonic/protobuf, SQLx SQLite/PostgreSQL, tokio, serde/base64, Next.js 16 server components and server actions.

---

## File Structure

- Mandatory split-first order: split `crates/pandar-core/src/lib.rs`, `crates/pandar-hub/src/grpc.rs`, and `crates/pandar-hub/src/repositories/commands.rs` before adding Phase 5 behavior to those areas.
- Modify `crates/pandar-core/src/lib.rs` only to re-export new modules; split existing domain records into focused files before adding job types so all source files stay under 400 LOC.
- Create `crates/pandar-core/src/ids.rs`, `tenant.rs`, `agent.rs`, `printer.rs`, `command.rs`, and `job.rs`.
- Modify root `Cargo.toml` to add workspace dependency `base64 = "0.22"` and modify `crates/pandar-hub/Cargo.toml` to use it.
- Modify `proto/pandar/agent/v1/agent.proto` to add `PrintProjectFile` and hub command variant. Do not commit generated `.pb.rs` or `.tonic.rs` files.
- Modify hub migrations under both `crates/pandar-hub/migrations/sqlite` and `crates/pandar-hub/migrations/postgres`.
- Create `crates/pandar-hub/src/repositories/jobs.rs` and `crates/pandar-hub/src/repositories/tests/jobs.rs`.
- Modify `crates/pandar-hub/src/repositories/commands.rs` and create `crates/pandar-hub/src/repositories/commands/inserts.rs` plus `commands/ownership.rs` so command repository files stay under 400 LOC.
- Modify `crates/pandar-hub/src/repositories/mod.rs`, `tests/mod.rs`, `tests/phase1.rs`, and `tests/postgres.rs`.
- Create `crates/pandar-hub/src/jobs.rs` for hub spool configuration, filename sanitization, size limits, and file writes.
- Modify `crates/pandar-hub/src/lib.rs` and `main.rs` to wire `PANDAR_SPOOL_DIR` and `PANDAR_MAX_ARTIFACT_BYTES`.
- Create `crates/pandar-hub/src/grpc/commands.rs` for outbound command conversion and job status coupling; shrink `grpc.rs` below 400 LOC.
- Create `crates/pandar-hub/src/grpc/tests/print_jobs.rs`.
- Create `crates/pandar-hub/src/routes/jobs.rs` and `crates/pandar-hub/src/routes/tests/jobs.rs`; keep `routes.rs` below 400 LOC.
- Create `crates/pandar-agent/src/artifacts.rs` for artifact reader path safety.
- Create `crates/pandar-agent/src/machine/print.rs` for print dispatch over file transfer and MQTT.
- Modify `crates/pandar-agent/src/machine/file_transfer.rs` to add a runtime `UnavailableMachineFileTransfer` adapter behind the existing `MachineFileTransfer` trait; real FTPS runtime remains out of scope for Phase 5.
- Modify `crates/pandar-agent/src/machine/mod.rs`, `commands.rs`, and command tests.
- Modify `crates/pandar-agent/src/lib.rs` to add `PANDAR_ARTIFACT_ROOT` config and runtime gateway wiring.
- Modify `frontend/app/page.tsx`; create `frontend/app/actions.ts`, `frontend/app/jobs.tsx`, and `frontend/app/types.ts` so `page.tsx` stays thin and under 400 LOC.
- Update `README.md`, `docs/architecture.md`, and `docs/roadmap.md` after implementation review.

## Milestone 1: Core Domain, Proto Contract, And File Splits

**Files:**

- Modify: `crates/pandar-core/src/lib.rs`
- Create: `crates/pandar-core/src/ids.rs`, `tenant.rs`, `agent.rs`, `printer.rs`, `command.rs`, `job.rs`
- Modify: `crates/pandar-core/src/tests.rs`
- Modify: `proto/pandar/agent/v1/agent.proto`

- [ ] **Step 1: Split core before adding job types**

Move existing ID newtypes, tenant, agent, printer, command, and shared error code into focused modules. Keep public names unchanged by re-exporting from `lib.rs`:

```rust
pub mod agent;
pub mod command;
pub mod ids;
pub mod job;
pub mod printer;
pub mod tenant;

pub use agent::{Agent, AgentStatus};
pub use command::{CommandRecord, CommandRecordParts, CommandStatus};
pub use ids::{AgentId, CommandId, JobId, TenantId};
pub use job::{Job, JobArtifact, JobArtifactParts, JobParts, JobStatus};
pub use printer::{Printer, PrinterParts};
pub use tenant::Tenant;
```

Keep `CoreError`, `created_at_now`, and the shared `required` helper accessible to modules as `pub(crate)` where needed. Do not change existing behavior. This split is completed before any job type additions.

Run:

```bash
cargo test -p pandar-core --no-fail-fast
```

Expected: existing core tests still pass before adding Phase 5 behavior.

- [ ] **Step 2: Add core job tests**

Add tests in `crates/pandar-core/src/tests.rs` for:

```rust
#[test]
fn job_status_round_trips_persisted_strings() {
    for (status, value) in [
        (JobStatus::Queued, "queued"),
        (JobStatus::Sent, "sent"),
        (JobStatus::Acknowledged, "acknowledged"),
        (JobStatus::Succeeded, "succeeded"),
        (JobStatus::Failed, "failed"),
    ] {
        assert_eq!(status.as_str(), value);
        assert_eq!(value.parse::<JobStatus>(), Ok(status));
    }
    assert_eq!(
        "printing".parse::<JobStatus>(),
        Err(CoreError::InvalidJobStatus("printing".to_string()))
    );
}

#[test]
fn job_and_artifact_from_parts_validate_required_fields() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let command_id = CommandId::new();
    let job = Job::from_parts(JobParts {
        id: JobId::new(),
        tenant_id,
        printer_id: uuid::Uuid::new_v4().to_string(),
        agent_id,
        artifact_id: JobId::new().to_string(),
        command_id,
        status: "queued".to_string(),
        error: None,
        created_at: "2026-06-22T00:00:00Z".to_string(),
        updated_at: "2026-06-22T00:00:00Z".to_string(),
    })
    .unwrap();
    assert_eq!(job.status, JobStatus::Queued);

    assert_eq!(
        JobArtifact::from_parts(JobArtifactParts {
            id: JobId::new().to_string(),
            tenant_id,
            filename: " ".to_string(),
            content_type: "application/octet-stream".to_string(),
            size_bytes: 1,
            storage_path: "tenant/artifact/file.3mf".to_string(),
            created_at: "2026-06-22T00:00:00Z".to_string(),
        })
        .unwrap_err(),
        CoreError::EmptyArtifactFilename
    );
}
```

Cover empty `printer_id`, `artifact_id`, `filename`, `content_type`, and `storage_path`, plus zero `size_bytes`.

- [ ] **Step 3: Implement job domain types**

Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(Uuid);
```

with `new`, `parse`, `Default`, and `Display` matching existing ID types.

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus { Queued, Sent, Acknowledged, Succeeded, Failed }
```

with `as_str`, `Display`, and `FromStr` for the five persisted strings.

Add `Job`, `JobParts`, `JobArtifact`, and `JobArtifactParts`. Validate blank job `printer_id`, blank `artifact_id`, blank artifact fields, zero artifact size, and invalid persisted status. Extend `CoreError` with `InvalidJobId`, `InvalidJobStatus(String)`, `EmptyJobPrinterId`, `EmptyJobArtifactId`, `EmptyArtifactId`, `EmptyArtifactFilename`, `EmptyArtifactContentType`, `EmptyArtifactStoragePath`, and `EmptyArtifactBody`.

- [ ] **Step 4: Extend protobuf command contract**

Change `proto/pandar/agent/v1/agent.proto`:

```proto
message HubCommand {
  string command_id = 1;
  oneof command {
    RefreshPrinters refresh_printers = 10;
    PrintProjectFile print_project_file = 11;
  }
}

message RefreshPrinters {}

message PrintProjectFile {
  string job_id = 1;
  string artifact_id = 2;
  string printer_id = 3;
  string serial_number = 4;
  string filename = 5;
  string storage_path = 6;
  uint64 size_bytes = 7;
  uint32 plate_id = 8;
  bool use_ams = 9;
  bool flow_cali = 10;
  bool timelapse = 11;
}
```

Run:

```bash
cargo test -p pandar-core --no-fail-fast
find . -path ./target -prune -o \( -name '*.pb.rs' -o -name '*.tonic.rs' \) -print
```

Expected: core tests pass and find prints nothing.

## Milestone 2: Hub Job Persistence, Spool Storage, And Command Ledger

**Files:**

- Create: `crates/pandar-hub/migrations/sqlite/20260622000000_phase_5_print_jobs.sql`
- Create: `crates/pandar-hub/migrations/postgres/20260622000000_phase_5_print_jobs.sql`
- Create: `crates/pandar-hub/src/jobs.rs`
- Create: `crates/pandar-hub/src/repositories/jobs.rs`
- Modify: `crates/pandar-hub/src/repositories/commands.rs`
- Create: `crates/pandar-hub/src/repositories/commands/inserts.rs`
- Create: `crates/pandar-hub/src/repositories/commands/ownership.rs`
- Modify: `crates/pandar-hub/src/repositories/mod.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/mod.rs`
- Create: `crates/pandar-hub/src/repositories/tests/jobs.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/phase1.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/postgres.rs`
- Modify: `crates/pandar-hub/src/lib.rs`
- Modify: `crates/pandar-hub/src/main.rs`

- [ ] **Step 1: Add migrations**

SQLite and PostgreSQL migration contents should be equivalent:

```sql
CREATE TABLE job_artifacts (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    storage_path TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE jobs (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    printer_id TEXT NOT NULL REFERENCES printers(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    artifact_id TEXT NOT NULL REFERENCES job_artifacts(id) ON DELETE CASCADE,
    command_id TEXT NOT NULL REFERENCES commands(id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (command_id)
);

CREATE INDEX idx_job_artifacts_tenant_id ON job_artifacts(tenant_id);
CREATE INDEX idx_jobs_tenant_id ON jobs(tenant_id);
CREATE INDEX idx_jobs_printer_id ON jobs(printer_id);
CREATE INDEX idx_jobs_agent_id ON jobs(agent_id);
CREATE INDEX idx_jobs_command_id ON jobs(command_id);
```

Extend SQLite migration tests to assert both new tables exist and `jobs.command_id` exists.

- [ ] **Step 2: Add hub spool config and writer tests**

Create `crates/pandar-hub/src/jobs.rs` with:

```rust
#[derive(Debug, Clone)]
pub struct JobStorageConfig {
    spool_dir: Arc<PathBuf>,
    max_artifact_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct StoredArtifact {
    pub filename: String,
    pub storage_path: String,
    pub size_bytes: u64,
}
```

Implement:

- `JobStorageConfig::from_env()` reads `PANDAR_SPOOL_DIR` default `pandar-spool` and `PANDAR_MAX_ARTIFACT_BYTES` default `10485760`.
- Invalid max bytes returns an `anyhow` error with env var context.
- `sanitize_filename(input: &str) -> String` per spec.
- `write_artifact(&self, tenant_id, artifact_id, filename, bytes)` rejects bytes longer than max and empty bytes, creates parent dirs, writes bytes, and returns relative storage path.
- `remove_artifact(&self, storage_path)` best-effort removes a relative path under spool root.

Add unit tests for filename sanitization, env parsing, oversized rejection, empty rejection, and actual write under a tempdir. Use `tempfile` if already present; otherwise add it as a dev-dependency only for hub tests.

- [ ] **Step 3: Wire storage config and JobRepository into AppState**

Modify `AppState` to include:

```rust
jobs: JobRepository,
job_storage: JobStorageConfig,
```

Add `AppState::connect_with_config(database_url, job_storage)` and keep `connect(database_url)` calling `JobStorageConfig::from_env()`. Test helpers should use a temp spool config so tests do not write project-root files.

Expose:

```rust
pub fn jobs(&self) -> &JobRepository;
pub fn job_storage(&self) -> &JobStorageConfig;
```

Update `main.rs` startup to preserve full context on invalid storage config.

- [ ] **Step 4: Add command payload type and print enqueue tests**

Add a serializable hub payload type near command repository or a small shared hub module:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrintProjectFilePayload {
    pub job_id: String,
    pub artifact_id: String,
    pub printer_id: String,
    pub serial_number: String,
    pub filename: String,
    pub storage_path: String,
    pub size_bytes: u64,
    pub plate_id: u32,
    pub use_ams: bool,
    pub flow_cali: bool,
    pub timelapse: bool,
}
```

Add repository tests:

- `command_enqueue_print_project_file_persists_payload_and_printer_id`
- `command_enqueue_print_project_file_rejects_missing_printer`
- `command_enqueue_print_project_file_rejects_wrong_agent`

- [ ] **Step 5: Implement print command enqueue**

Add `CommandRepository::enqueue_print_project_file(tenant_id, agent_id, printer_id, payload)`.

It must:

- Verify agent belongs to tenant.
- Verify printer row exists with the same tenant and agent.
- Insert command with `printer_id = Some(printer_id)`, `kind = "print_project_file"`, `status = queued`, and payload JSON.
- Return the inserted `CommandRecord`.

Preserve existing `enqueue_refresh_printers` behavior. Before adding print enqueue, move command ownership checks into `commands/ownership.rs` and insert SQL into `commands/inserts.rs`.

`commands/inserts.rs` must expose concrete helpers instead of a pool-only API:

```rust
pub struct InsertCommand<'a> {
    pub id: CommandId,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub printer_id: Option<&'a str>,
    pub kind: &'a str,
    pub payload_json: &'a str,
    pub created_at: &'a str,
}

pub async fn insert_sqlite(/* pool or transaction executor */, input: InsertCommand<'_>) -> RepositoryResult<()>;
pub async fn insert_postgres(/* pool or transaction executor */, input: InsertCommand<'_>) -> RepositoryResult<()>;
```

Use backend-specific helper signatures that compile cleanly with both pool and transaction executors. It is acceptable to provide separate pool and transaction variants when SQLx lifetime constraints make one generic signature noisy, as long as the SQL shape is shared and tested. `CommandRepository::enqueue_refresh_printers` and `enqueue_print_project_file` call the helpers with pool executors. `JobRepository::create_print_job` calls the transaction helper variants inside the same transaction as artifact metadata and job insertion. This keeps command/job atomicity identical on SQLite and PostgreSQL without a generic repository transaction abstraction.

- [ ] **Step 6: Add JobRepository tests**

Add tests for:

- `job_repository_create_print_job_links_artifact_command_and_job`
- `job_repository_list_returns_newest_first`
- `job_repository_get_returns_none_for_unknown_job`
- `job_repository_rejects_missing_tenant_on_list`
- `job_repository_rejects_wrong_tenant_printer`
- `job_repository_mark_for_command_tracks_ack_success_failure`
- `invalid_persisted_job_status_is_reported`
- `job_repository_create_rolls_back_command_when_job_insert_fails`

Use SQLite default repository harness. Add guarded PostgreSQL coverage under the existing `PANDAR_TEST_POSTGRES_URL` harness for create/list/get, rollback/no orphan command, mark status transitions, and invalid persisted status.

- [ ] **Step 7: Implement JobRepository**

Add `JobRepository` with:

- No job count is added to the existing summary endpoint in Phase 5; `JobRepository` does not need a `count()` method.
- `create_print_job(input: CreatePrintJob)` using a DB transaction for artifact metadata, command insert, and job insert. Insert the print command inside this transaction via `commands::inserts::{insert_sqlite, insert_postgres}`; do not call the non-transactional `CommandRepository` path from inside this method. Do not commit a command without a linked job.
- `list_for_tenant(tenant_id)` with missing tenant check.
- `get_for_tenant(tenant_id, job_id)`.
- `mark_for_command(command_id, status, error)` with idempotent terminal handling.

`CreatePrintJob` is repository input for an artifact that was already decoded and written by the HTTP route:

```rust
pub struct CreatePrintJob {
    pub tenant_id: TenantId,
    pub printer_id: String,
    pub agent_id: AgentId,
    pub artifact_id: String,
    pub artifact_filename: String,
    pub artifact_content_type: String,
    pub artifact_size_bytes: u64,
    pub artifact_storage_path: String,
    pub plate_id: u32,
    pub use_ams: bool,
    pub flow_cali: bool,
    pub timelapse: bool,
}
```

The repository verifies tenant/printer/agent ownership before insertion, allocates `job_id` and `command_id`, builds `PrintProjectFilePayload` with those IDs plus printer serial, and returns the inserted `Job` with its `JobArtifact`. The route owns the spool file lifecycle and removes `artifact_storage_path` if `create_print_job` returns an error.

Use row rehydration helpers that call `Job::from_parts` and `JobArtifact::from_parts`, preserving context chains.

Run:

```bash
cargo test -p pandar-hub repositories::tests::jobs --no-fail-fast
cargo test -p pandar-hub repositories::tests::commands --no-fail-fast
cargo test -p pandar-hub repositories::tests::phase1::sqlite_migrations_create_phase_1_schema --no-fail-fast
cargo test -p pandar-hub repositories::tests::postgres --no-fail-fast
```

Expected: all targeted repository and migration tests pass; PostgreSQL tests execute when `PANDAR_TEST_POSTGRES_URL` is set and otherwise skip using the existing guard.

## Milestone 3: Hub HTTP API And gRPC Print Command Flow

**Files:**

- Create: `crates/pandar-hub/src/routes/jobs.rs`
- Modify: `crates/pandar-hub/src/routes.rs`
- Create: `crates/pandar-hub/src/routes/tests/jobs.rs`
- Modify: `crates/pandar-hub/src/routes/tests.rs`
- Create/modify: `crates/pandar-hub/src/grpc/commands.rs`
- Modify: `crates/pandar-hub/src/grpc.rs`
- Create: `crates/pandar-hub/src/grpc/tests/print_jobs.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/mod.rs`

- [ ] **Step 1: Split gRPC command conversion out of `grpc.rs`**

Create `grpc/commands.rs` containing:

```rust
pub fn hub_command_from_record(command: CommandRecord) -> anyhow::Result<HubCommand>;
pub async fn mark_sent_and_job(...);
pub async fn handle_ack_and_job(...);
pub async fn handle_result_and_job(...);
```

Move existing refresh command conversion into this module before adding print behavior. `grpc.rs` should call these helpers and shrink below 400 LOC before Step 2 starts.

- [ ] **Step 2: Add gRPC print command tests**

Tests:

- `grpc_dispatch_print_project_file_sends_payload_and_marks_job_sent`
- `grpc_print_ack_and_result_update_linked_job`
- `grpc_stale_print_result_does_not_update_job`
- `grpc_malformed_print_payload_streams_internal_error`

Use existing `connect_live` helpers and repository fixture setup.

- [ ] **Step 3: Implement gRPC print command conversion and job coupling**

`hub_command_from_record`:

- `refresh_printers` => existing `RefreshPrinters`.
- `print_project_file` => deserialize `PrintProjectFilePayload`, build proto `PrintProjectFile`.
- Unknown kind => error with command id/kind context.

Outbound pump marks command sent then marks linked job sent. Inbound ack/result update command ledger and linked job only inside the existing `while_current` token guard.

- [ ] **Step 4: Add HTTP route tests**

Route tests:

- `job_create_writes_artifact_queues_command_and_returns_created_job`
- `job_create_rejects_invalid_tenant_printer_and_job_ids`
- `job_create_rejects_missing_printer`
- `job_create_rejects_empty_artifact`
- `job_create_rejects_invalid_base64`
- `job_create_rejects_oversized_artifact`
- `job_list_returns_tenant_jobs`
- `job_detail_returns_one_job`
- `missing_job_detail_returns_not_found`

Use temp spool config in test state.

- [ ] **Step 5: Implement HTTP routes**

Add routes:

```rust
.route("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs", post(jobs::create_job))
.route("/api/v1/tenants/{tenant_id}/jobs", get(jobs::list_jobs))
.route("/api/v1/tenants/{tenant_id}/jobs/{job_id}", get(jobs::get_job))
```

Create route request/response structs in `routes/jobs.rs`. Parse UUID-like printer/job IDs with stable `400` errors. Decode base64 with the modern `base64::Engine` API. Use `413` for oversized decoded payload.

The create route must execute this order:

1. parse tenant/printer IDs and request JSON,
2. decode base64 and size-check before opening a DB transaction,
3. allocate `artifact_id`, sanitize filename, and call `JobStorageConfig::write_artifact`,
4. call `JobRepository::create_print_job(CreatePrintJob { artifact_id, artifact_filename, artifact_content_type, artifact_size_bytes, artifact_storage_path, plate_id, use_ams, flow_cali, timelapse, ... })`,
5. if repository creation fails, call `JobStorageConfig::remove_artifact(&artifact_storage_path)` and log cleanup failures as `tracing::warn!(error = %format!("{err:#}"), ...)` or equivalent full-chain formatting, then return the original repository error.

The route must only remove the file after repository failure, and must remove the exact storage path returned by `write_artifact`.

Run:

```bash
cargo test -p pandar-hub routes::tests::jobs --no-fail-fast
cargo test -p pandar-hub grpc::tests::print_jobs --no-fail-fast
cargo test -p pandar-hub --no-fail-fast
```

Expected: hub route and gRPC tests pass.

## Milestone 4: Agent Print Execution

**Files:**

- Create: `crates/pandar-agent/src/artifacts.rs`
- Create: `crates/pandar-agent/src/machine/print.rs`
- Modify: `crates/pandar-agent/src/machine/mod.rs`
- Modify: `crates/pandar-agent/src/machine/file_transfer.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt.rs`
- Modify: `crates/pandar-agent/src/commands.rs`
- Modify: `crates/pandar-agent/src/commands/tests.rs`
- Modify: `crates/pandar-agent/src/lib.rs`

- [ ] **Step 1: Add artifact reader tests**

Tests should cover:

- Relative safe path joins under `PANDAR_ARTIFACT_ROOT`.
- Absolute path rejected.
- Parent component path rejected.
- Missing file error includes storage path context.
- In-memory reader returns configured bytes.

- [ ] **Step 2: Implement artifact readers**

Create:

```rust
#[async_trait]
pub trait ArtifactReader: Send + Sync {
    async fn read(&self, storage_path: &str) -> anyhow::Result<Vec<u8>>;
}

pub struct LocalArtifactReader { root: PathBuf }
```

Add `LocalArtifactReader::new(root)` and a test-only `MemoryArtifactReader`. Path validation rejects absolute paths, parent dirs, and prefixes before IO.

- [ ] **Step 3: Add machine print dispatch tests**

Use fake MQTT and fake file transfer to assert:

- Upload path is command filename.
- Uploaded bytes match artifact reader bytes.
- MQTT publish is `project_file` with `task_id = job_id`, `subtask_id = artifact_id`, `plate_id`, `use_ams`, `flow_cali`, `timelapse`.
- Unknown serial fails before artifact read/upload/publish.
- File upload failure preserves context and skips MQTT publish.
- MQTT publish failure preserves context after upload.

- [ ] **Step 4: Implement file-transfer boundary and print dispatch**

Add `UnavailableMachineFileTransfer` in `machine/file_transfer.rs` behind the existing `MachineFileTransfer` trait. It returns a full-context error that states real Bambu FTPS upload is not implemented in Phase 5. This is the default runtime adapter so Phase 5 does not add real FTPS sockets, matching the approved spec. Unit tests exercise successful upload/publish behavior through `FakeMachineFileTransfer`.

Add a `PrintProjectFileRequest` domain struct in `machine/print.rs` mirroring proto fields needed by the agent. Add a gateway implementation that selects endpoint/transport/file transfer by serial and executes:

1. read artifact bytes,
2. `run_with_transfer_mode(endpoint, cache, false, |mode| transfer.upload(filename, &bytes, mode))`,
3. publish `BambuMqttCommand::ProjectFile(ProjectFileCommand { ... })` to `device/{serial}/request` with QoS 1.

Use existing fake transports in tests. Do not add real Bambu socket tests.

- [ ] **Step 5: Add agent command tests**

Extend command tests for:

- `print_project_file_success_emits_ack_then_success`
- `print_project_file_unknown_serial_emits_rejected_ack_only`
- `print_project_file_gateway_failure_emits_ack_then_failed_result_with_context`

- [ ] **Step 6: Implement agent command handling**

Update `BambuMachineGateway` with `print_project_file`. `NoopMachineGateway` rejects print commands with context. Add a reusable print executor that takes an artifact reader, file-transfer boundary, MQTT transport, transfer-mode cache, and endpoint list. Runtime construction uses the same executor with configured MQTT transports and `UnavailableMachineFileTransfer`; tests use fake file-transfer and fake MQTT implementations to verify upload and publish behavior without opening real sockets. With the default runtime adapter, a print command is acknowledged and then reported failed before MQTT publish; real FTPS upload remains a later phase.

Update `handle_command_with_gateway` to match `hub_command::Command::PrintProjectFile`.

Add `PANDAR_ARTIFACT_ROOT` to `AgentConfig` with default `.` and wire it to `LocalArtifactReader`.

Run:

```bash
cargo test -p pandar-agent --no-fail-fast
```

Expected: agent tests pass without real Bambu sockets.

## Milestone 5: Frontend Dashboard And Docs

**Files:**

- Modify: `frontend/app/page.tsx`
- Create: `frontend/app/actions.ts`
- Create: `frontend/app/jobs.tsx`
- Create: `frontend/app/types.ts`
- Modify: `README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Add frontend job fetch types and UI tests by build**

No separate frontend test harness exists. Keep validation through TypeScript/Next build. Extend page data fetches for selected tenant:

- `/api/v1/tenants/{tenant_id}/jobs`

Add types for job responses.

- [ ] **Step 2: Add server action for debug print dispatch**

Add a server action that receives standard form fields and posts JSON to:

```text
/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs
```

Do not add client state. The form accepts selected printer id, filename, artifact base64 text, plate id, use AMS, flow calibration, and timelapse.

- [ ] **Step 3: Render job form and history**

Render a compact job dispatch form only when a tenant has printers. Render job history with status, printer id, artifact filename, created time, and error. Keep operational styling and avoid nested cards or marketing sections.

- [ ] **Step 4: Update docs**

README:

- Document print job endpoints, JSON base64 artifact limit, `PANDAR_SPOOL_DIR`, `PANDAR_MAX_ARTIFACT_BYTES`, and `PANDAR_ARTIFACT_ROOT`.
- Clarify Phase 5 `succeeded` means dispatch/MQTT publish success, not physical print completion.

Architecture:

- Add Phase 5 hub/agent/frontend behavior and the local-spool/shared-filesystem caveat.

Roadmap:

- Move Phase 5 implemented items to completed.
- Update Immediate Next toward live-printer compatibility, artifact download path, and physical progress reconciliation.

- [ ] **Step 5: Verify frontend**

Run:

```bash
npm run build
```

from `frontend/`.

Expected: Next.js build and TypeScript pass.

## Milestone 6: Final Verification, Review, Commit, Push

**Files:**

- All changed files.

- [ ] **Step 1: Run full verification**

Run:

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
npm run build
find . -path ./target -prune -o \( -name '*.pb.rs' -o -name '*.tonic.rs' \) -print
git diff --check
```

Expected: all commands pass and protobuf find prints nothing.

- [ ] **Step 2: Check file sizes**

Run:

```bash
wc -l crates/pandar-core/src/*.rs crates/pandar-hub/src/grpc.rs crates/pandar-hub/src/grpc/*.rs crates/pandar-hub/src/repositories/*.rs crates/pandar-hub/src/routes.rs crates/pandar-hub/src/routes/*.rs crates/pandar-agent/src/*.rs crates/pandar-agent/src/machine/*.rs frontend/app/*.tsx
```

Expected: touched source files stay below 400 LOC. Split any file that exceeds or clearly approaches the limit.

- [ ] **Step 3: Independent final implementation review**

Dispatch a fresh reviewer with:

- spec path,
- reviewed plan path,
- final diff or base/head SHAs,
- verification output.

Do not commit until reviewer returns:

```text
VERDICT: APPROVE
```

- [ ] **Step 4: Commit and push**

Use Lore commit protocol. Suggested intent line:

```text
Dispatch print jobs through agent machine boundaries
```

Push branch `codex/phase-5-print-dispatch`.

## Self-Review

- Spec coverage: plan covers core job types, proto command contract, SQLite/PostgreSQL migrations, spool storage, job repository, command ledger integration, HTTP API, gRPC stream delivery/status coupling, agent artifact/file-transfer/MQTT execution, frontend job form/history, docs, and final verification.
- Placeholder scan: no TBD/TODO placeholders are used in implementation steps; implementation scope uses concrete files and concrete runtime boundaries.
- Type consistency: `JobId`, `JobStatus`, `Job`, `JobArtifact`, `PrintProjectFilePayload`, and `PrintProjectFile` are introduced before use.
- Scope check: auth, object storage, real FTPS runtime, real printer tests, slicing, progress reconciliation, and polished upload UX remain out of scope.
