# Phase 9 Print Report Reconciliation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist and display physical Bambu print progress from MQTT reports while keeping dispatch command status separate.

**Architecture:** Add a new `PrintJobReport` gRPC event from agent to hub, normalize Bambu MQTT reports in the agent, reconcile them in hub repositories into print-lifecycle columns and `machine_events`, then surface the nested print state through HTTP, tenant WebSocket events, and the existing frontend job table. `jobs.status` remains the dispatch status; `jobs.print_status` is the physical lifecycle.

**Tech Stack:** Rust 2024, tonic/prost protobuf generation, tokio, rumqttc, SQLx SQLite/PostgreSQL, axum WebSockets, Next.js 16 server components.

---

## File Map

- Modify `proto/pandar/agent/v1/agent.proto`: add `PrintJobReport` and `MachineDiagnostic`.
- Modify `crates/pandar-core/src/job.rs`: add `PrintStatus`, `JobPrintState`, and print fields on `Job`.
- Modify `crates/pandar-agent/src/machine/mqtt.rs` and `crates/pandar-agent/src/machine/mqtt/tests.rs`: normalize print reports and diagnostics.
- Modify `crates/pandar-agent/src/lib.rs`: start configured-printer report forwarding while reverse gRPC is connected.
- Modify `crates/pandar-agent/src/commands.rs` and tests only if shared event construction is needed.
- Add `crates/pandar-hub/migrations/sqlite/20260622020000_phase_9_print_reports.sql`.
- Add `crates/pandar-hub/migrations/postgres/20260622020000_phase_9_print_reports.sql`.
- Modify `crates/pandar-hub/src/repositories/jobs.rs`, `rows.rs`, `create.rs`; add focused `print_reports.rs` under the jobs repository module.
- Modify repository tests in `crates/pandar-hub/src/repositories/tests/jobs.rs` and `postgres.rs`.
- Add `crates/pandar-hub/src/grpc/print_reports.rs`; modify `grpc.rs` and gRPC tests.
- Modify `crates/pandar-hub/src/printer_events.rs`, `routes/printer_events.rs`, `routes/jobs.rs`, and route tests.
- Modify `frontend/app/page.tsx`.
- Update `docs/architecture.md` and `docs/roadmap.md` after implementation review approval.

## Milestone 1: Protocol And Core Models

**Files:**
- Modify: `proto/pandar/agent/v1/agent.proto`
- Modify: `crates/pandar-core/src/job.rs`
- Test: `crates/pandar-core` unit coverage via workspace tests

- [x] **Step 1: Extend protobuf event contract**

Change `AgentEvent` to include:

```proto
    PrintJobReport print_job_report = 15;
```

Add:

```proto
message PrintJobReport {
  string serial = 1;
  string job_id = 2;
  string artifact_id = 3;
  string subtask_id = 4;
  string gcode_file = 5;
  string subtask_name = 6;
  string gcode_state = 7;
  uint32 percent = 8;
  bool has_percent = 9;
  uint32 remaining_time_minutes = 10;
  bool has_remaining_time_minutes = 11;
  uint32 current_layer = 12;
  bool has_current_layer = 13;
  uint32 total_layers = 14;
  bool has_total_layers = 15;
  repeated MachineDiagnostic diagnostics = 16;
  string observed_at = 17;
}

message MachineDiagnostic {
  string kind = 1;
  string severity = 2;
  string code = 3;
  string message = 4;
  string payload_json = 5;
}
```

Use `has_*` booleans because proto3 scalar integers cannot distinguish absent from zero.

- [x] **Step 2: Add core print lifecycle types**

In `crates/pandar-core/src/job.rs`, add:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrintStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}
```

Implement `as_str`, `Display`, and `FromStr` with strings `pending`, `running`, `completed`, `failed`, `cancelled`. Add a `CoreError::InvalidPrintStatus(String)` variant if it does not already exist.

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobPrintState {
    pub status: PrintStatus,
    pub printer_state: Option<String>,
    pub progress_percent: Option<u8>,
    pub remaining_time_minutes: Option<u32>,
    pub current_layer: Option<u32>,
    pub total_layers: Option<u32>,
    pub active_file: Option<String>,
    pub last_progress_percent: Option<u8>,
    pub last_layer: Option<u32>,
    pub error: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub updated_at: Option<String>,
}
```

Add `print: JobPrintState` to `Job` and matching fields to `JobParts`. Default new jobs to pending in repository creation.

- [x] **Step 3: Regenerate protobuf through Cargo, without committing generated output**

Run:

```bash
cargo test -p pandar-agent protocol --no-run
cargo test -p pandar-hub protocol --no-run
```

Expected: both commands compile or fail only on downstream code that has not yet been updated. Generated `.pb.rs` or `.tonic.rs` files must remain under `target/` and absent from `git status`.

## Milestone 2: Agent Report Normalization And Forwarding

**Files:**
- Modify: `crates/pandar-agent/src/machine/mqtt.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/tests.rs`
- Modify: `crates/pandar-agent/src/lib.rs`
- Optional modify: `crates/pandar-agent/src/commands.rs`
- Test: agent MQTT and runtime tests

- [ ] **Step 1: Add normalized report structs**

In `mqtt.rs`, add public structs:

```rust
pub struct PrintReportProgress {
    pub serial: String,
    pub job_id: Option<String>,
    pub artifact_id: Option<String>,
    pub subtask_id: Option<String>,
    pub gcode_state: Option<String>,
    pub percent: Option<u8>,
    pub remaining_time_minutes: Option<u32>,
    pub current_layer: Option<u32>,
    pub total_layers: Option<u32>,
    pub gcode_file: Option<String>,
    pub subtask_name: Option<String>,
    pub diagnostics: Vec<MachineReportDiagnostic>,
    pub observed_at: String,
}

pub struct MachineReportDiagnostic {
    pub kind: String,
    pub severity: String,
    pub code: Option<String>,
    pub message: String,
    pub payload: serde_json::Value,
}
```

Use existing `pandar_core::created_at_now()` for agent-side `observed_at`.

- [ ] **Step 2: Normalize Bambu report JSON**

Add `print_report_from_report(endpoint, report) -> PrintReportProgress`.

Extraction rules:

- `print.task_id` -> `job_id`.
- `print.subtask_id` -> both `artifact_id` and `subtask_id` when non-blank.
- `print.gcode_state`, `print.mc_percent`, `print.mc_remaining_time`, `print.layer_num`, `print.total_layer_num`, `print.gcode_file`, `print.subtask_name`.
- Accept numeric JSON numbers and numeric strings.
- Drop out-of-range numeric values using the spec ranges.
- Convert `print.print_error` into a `MachineReportDiagnostic { kind: "print_error", severity: "error" }` when present and not empty.
- Convert array/object HMS fields discovered in reports into diagnostics with `kind: "hms"` when they expose a code/message. Keep this simple and test representative shapes; do not add a broad raw-MQTT parser.

- [ ] **Step 3: Convert normalized reports into proto events**

Add an event builder that maps `Option` numeric fields into `value + has_*` proto fields. Keep blank optional strings empty for proto transport; the hub trims them.

Expected event variant:

```rust
agent_event::Event::PrintJobReport(report)
```

- [ ] **Step 4: Forward continuous reports while connected**

In `run_once`, after the reverse stream opens and when printers are configured, spawn one task per configured printer:

- Create a fresh `RumqttcBambuMqttTransport` for the report loop.
- Subscribe to `device/{serial}/report`.
- Loop on `next_report(DEFAULT_REPORT_TIMEOUT)`; timeout should log and continue instead of ending the reverse connection.
- Send normalized `PrintJobReport` events to the existing agent event sender.
- Stop naturally when the sender is closed.

Do not change the no-printer path.

- [ ] **Step 5: Agent tests**

Add tests proving:

- `print_report_from_report` extracts `task_id`, `subtask_id`, percent, remaining time, layers, `gcode_file`, `subtask_name`, `print_error`, and representative HMS diagnostic.
- Out-of-range numeric values become `None`.
- The proto event builder sets `has_*` booleans correctly.
- Report forwarding uses fake MQTT transport and emits `PrintJobReport` without opening sockets.

Run:

```bash
cargo test -p pandar-agent machine::mqtt commands --no-fail-fast
```

Expected: targeted agent tests pass.

## Milestone 3: Hub Schema And Repository Reconciliation

**Files:**
- Add: `crates/pandar-hub/migrations/sqlite/20260622020000_phase_9_print_reports.sql`
- Add: `crates/pandar-hub/migrations/postgres/20260622020000_phase_9_print_reports.sql`
- Modify: `crates/pandar-hub/src/repositories/jobs.rs`
- Modify: `crates/pandar-hub/src/repositories/jobs/create.rs`
- Modify: `crates/pandar-hub/src/repositories/jobs/rows.rs`
- Add: `crates/pandar-hub/src/repositories/jobs/print_reports.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/jobs.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/postgres.rs`

- [ ] **Step 1: Add equivalent migrations**

SQLite migration:

```sql
ALTER TABLE jobs ADD COLUMN print_status TEXT NOT NULL DEFAULT 'pending'
    CHECK (print_status IN ('pending', 'running', 'completed', 'failed', 'cancelled'));
ALTER TABLE jobs ADD COLUMN printer_state TEXT;
ALTER TABLE jobs ADD COLUMN progress_percent INTEGER;
ALTER TABLE jobs ADD COLUMN remaining_time_minutes INTEGER;
ALTER TABLE jobs ADD COLUMN current_layer INTEGER;
ALTER TABLE jobs ADD COLUMN total_layers INTEGER;
ALTER TABLE jobs ADD COLUMN active_file TEXT;
ALTER TABLE jobs ADD COLUMN last_progress_percent INTEGER;
ALTER TABLE jobs ADD COLUMN last_layer INTEGER;
ALTER TABLE jobs ADD COLUMN print_error TEXT;
ALTER TABLE jobs ADD COLUMN print_started_at TEXT;
ALTER TABLE jobs ADD COLUMN print_finished_at TEXT;
ALTER TABLE jobs ADD COLUMN print_updated_at TEXT;

CREATE TABLE machine_events (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    printer_id TEXT NOT NULL REFERENCES printers(id) ON DELETE CASCADE,
    job_id TEXT REFERENCES jobs(id) ON DELETE SET NULL,
    event_key TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('print_progress', 'print_terminal', 'print_error', 'hms')),
    severity TEXT NOT NULL CHECK (severity IN ('info', 'warning', 'error')),
    message TEXT NOT NULL,
    code TEXT,
    payload_json TEXT NOT NULL,
    observed_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE (tenant_id, event_key)
);
CREATE INDEX idx_machine_events_tenant_id ON machine_events(tenant_id);
CREATE INDEX idx_machine_events_printer_id ON machine_events(printer_id);
CREATE INDEX idx_machine_events_job_id ON machine_events(job_id);
```

PostgreSQL migration:

```sql
ALTER TABLE jobs ADD COLUMN print_status TEXT NOT NULL DEFAULT 'pending'
    CHECK (print_status IN ('pending', 'running', 'completed', 'failed', 'cancelled'));
ALTER TABLE jobs ADD COLUMN printer_state TEXT;
ALTER TABLE jobs ADD COLUMN progress_percent INTEGER;
ALTER TABLE jobs ADD COLUMN remaining_time_minutes INTEGER;
ALTER TABLE jobs ADD COLUMN current_layer INTEGER;
ALTER TABLE jobs ADD COLUMN total_layers INTEGER;
ALTER TABLE jobs ADD COLUMN active_file TEXT;
ALTER TABLE jobs ADD COLUMN last_progress_percent INTEGER;
ALTER TABLE jobs ADD COLUMN last_layer INTEGER;
ALTER TABLE jobs ADD COLUMN print_error TEXT;
ALTER TABLE jobs ADD COLUMN print_started_at TEXT;
ALTER TABLE jobs ADD COLUMN print_finished_at TEXT;
ALTER TABLE jobs ADD COLUMN print_updated_at TEXT;

CREATE TABLE machine_events (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    printer_id TEXT NOT NULL REFERENCES printers(id) ON DELETE CASCADE,
    job_id TEXT REFERENCES jobs(id) ON DELETE SET NULL,
    event_key TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('print_progress', 'print_terminal', 'print_error', 'hms')),
    severity TEXT NOT NULL CHECK (severity IN ('info', 'warning', 'error')),
    message TEXT NOT NULL,
    code TEXT,
    payload_json TEXT NOT NULL,
    observed_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE (tenant_id, event_key)
);
CREATE INDEX idx_machine_events_tenant_id ON machine_events(tenant_id);
CREATE INDEX idx_machine_events_printer_id ON machine_events(printer_id);
CREATE INDEX idx_machine_events_job_id ON machine_events(job_id);
```

- [ ] **Step 2: Update repository row mapping**

Select all new `jobs.print_*` columns in list/get/by-command queries. Rehydrate `JobPrintState` and map invalid `print_status` into a new repository error like `InvalidPersistedPrintStatus(String)`.

New job creation sets `print_status = 'pending'`; it can rely on migration default or bind the value explicitly. Prefer explicit binding in `create.rs` so tests prove the behavior.

- [ ] **Step 3: Add reconciliation input/output types**

In `jobs.rs` expose:

```rust
pub struct ApplyPrintReport {
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub serial: String,
    pub job_id: Option<JobId>,
    pub artifact_id: Option<String>,
    pub subtask_id: Option<String>,
    pub gcode_file: Option<String>,
    pub subtask_name: Option<String>,
    pub gcode_state: Option<String>,
    pub percent: Option<u8>,
    pub remaining_time_minutes: Option<u32>,
    pub current_layer: Option<u32>,
    pub total_layers: Option<u32>,
    pub diagnostics: Vec<PrintReportDiagnostic>,
    pub observed_at: String,
}

pub struct PrintReportDiagnostic {
    pub kind: String,
    pub severity: String,
    pub code: Option<String>,
    pub message: String,
    pub payload_json: String,
}

pub struct AppliedPrintReport {
    pub job: Option<JobWithArtifact>,
    pub changed: bool,
    pub inserted_job_events: bool,
    pub inserted_printer_events: bool,
}
```

Keep proto types out of repositories. Use core ids and plain strings/integers.

- [ ] **Step 4: Implement correlation**

In `print_reports.rs`:

- Resolve printer by tenant, agent, and serial number.
- Prefer exact `job_id`.
- Then exact `artifact_id`/`subtask_id`.
- Then active-file fallback:
  - same tenant/agent/printer.
  - `print_status IN ('pending', 'running')`.
  - created in last 24 hours by lexicographic RFC3339 comparison.
  - filename equals basename of `gcode_file` or filename stem equals `subtask_name`.
  - exactly one match required; zero or multiple means no job update.
- When no job correlates, persist any diagnostics as printer-level `machine_events` with `job_id = NULL`; do not update any job fields.

- [ ] **Step 5: Implement idempotent updates and machine event inserts**

Status mapping:

- `RUNNING` -> `running`.
- `FINISH` -> `completed`.
- `FAILED` -> `failed`.
- `IDLE` -> `cancelled` only when persisted status is `running`.

Rules:

- Terminal states never regress.
- `last_progress_percent` and `last_layer` are max(existing, observed).
- `print_started_at` is set only once.
- `print_finished_at` is set only once on terminal.
- `print_error` is set for failed/cancelled terminal reports and not overwritten by later non-terminal reports.
- `machine_events` inserts use `ON CONFLICT DO NOTHING` / SQLite `INSERT OR IGNORE`.
- Event keys are deterministic and must match the approved spec:
  - Progress: `print-progress:{job_id}:{observed_at}:{gcode_state}:{percent}:{current_layer}:{total_layers}`.
  - Terminal: `print-terminal:{job_id}:{terminal_status}`.
  - Print error: `print-error:{job_id}:{code_or_message_hash}:{observed_at}`.
  - HMS: `hms:{printer_id}:{code}:{observed_at}`.
  - Uncorrelated diagnostic: `machine:{printer_id}:{kind}:{code_or_message_hash}:{observed_at}`.
- `code_or_message_hash` is `code` when a non-blank code exists; otherwise use a stable short hash of normalized message plus `payload_json`.
- If an insert conflicts on `(tenant_id, event_key)`, treat it as idempotent success and do not update the existing row.
- `AppliedPrintReport.changed` is true when job print columns changed.
- `AppliedPrintReport.inserted_job_events` is true when at least one job-scoped `machine_events` row was newly inserted.
- `AppliedPrintReport.inserted_printer_events` is true when at least one printer-level uncorrelated diagnostic row was newly inserted.

- [ ] **Step 6: Repository tests**

Add SQLite tests in `repositories/tests/jobs.rs`:

- new job starts with `print.status == pending` while dispatch `status == queued`.
- exact job id report updates print status/progress but leaves dispatch status unchanged.
- exact artifact/subtask id report correlates.
- file fallback handles zero, one, and ambiguous matches.
- `RUNNING -> FINISH` sets started/finished and completed.
- `RUNNING -> IDLE` sets cancelled.
- `FAILED` stores error.
- replayed terminal report does not duplicate machine events or regress status.
- uncorrelated diagnostic report inserts printer-level `machine_events` with `job_id = NULL` and returns no job.
- replayed uncorrelated diagnostic report does not duplicate machine events.
- result flags distinguish job updates, newly inserted job-scoped events, and newly inserted printer-level events.

Mirror representative PostgreSQL coverage in `repositories/tests/postgres.rs`, respecting existing environment-dependent skip behavior.

Run:

```bash
cargo test -p pandar-hub repositories::tests::jobs -- --nocapture
```

Expected: SQLite repository tests pass.

## Milestone 4: Hub gRPC, HTTP, And Tenant WebSocket Events

**Files:**
- Add: `crates/pandar-hub/src/grpc/print_reports.rs`
- Modify: `crates/pandar-hub/src/grpc.rs`
- Modify: `crates/pandar-hub/src/printer_events.rs`
- Modify: `crates/pandar-hub/src/routes/jobs.rs`
- Modify: `crates/pandar-hub/src/routes/printer_events.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/mod.rs`
- Add or modify: `crates/pandar-hub/src/grpc/tests/print_reports.rs`
- Modify: `crates/pandar-hub/src/routes/tests/jobs.rs`
- Modify: `crates/pandar-hub/src/routes/tests/printers.rs`

- [ ] **Step 1: Validate and handle gRPC reports**

`grpc/print_reports.rs` should:

- Trim `serial`; reject blank serial with `Status::invalid_argument`.
- Parse `observed_at` as RFC3339; reject invalid timestamps.
- Treat blank optional strings as absent.
- Reject invalid present `job_id` and `artifact_id`.
- Convert `has_*` numeric fields into `Option`.
- Drop diagnostics with blank kind, default blank severity to `info`, unknown severity to `warning`.
- Call `state.jobs().apply_print_report(input)`.
- Publish a tenant `job_progress` WebSocket event only when a job was updated or terminal diagnostics were inserted for a job.
- Do not publish `job_progress` for uncorrelated printer-level diagnostics because there is no job payload to send.

- [ ] **Step 2: Wire current-session handling**

Add `PrintJobReport` handling to `grpc.rs` inside the same `sessions().while_current(...)` pattern used by snapshots and command events. Stale replaced streams must not mutate repository state.

- [ ] **Step 3: Extend tenant event hub**

In `printer_events.rs`, add:

```rust
#[serde(rename = "job_progress")]
JobProgress { job: JobEvent }
```

Use a route-facing/event DTO that includes the nested `print` object and does not expose credentials. Reuse conversion logic with `routes/jobs.rs` where practical without creating circular dependencies.

- [ ] **Step 4: Extend job HTTP response**

`routes/jobs.rs` keeps `status` and `command.status` as dispatch status. Add:

```rust
print: JobPrintResponse
```

with the fields from the approved spec.

- [ ] **Step 5: Hub tests**

Add tests proving:

- malformed blank serial and invalid timestamp report events close the stream with `InvalidArgument`.
- invalid `job_id` rejects.
- stale replaced stream report does not mutate jobs.
- valid report updates nested job print state and broadcasts a `job_progress` event.
- report that only inserts a job-scoped terminal diagnostic broadcasts `job_progress`.
- uncorrelated diagnostic report persists a printer-level event but does not broadcast `job_progress`.
- job list/detail JSON includes dispatch status and nested print status.

Run:

```bash
cargo test -p pandar-hub grpc::tests::print_reports routes::tests::jobs routes::tests::printers -- --nocapture
```

Expected: targeted hub gRPC/routes tests pass.

## Milestone 5: Frontend And Documentation

**Files:**
- Modify: `frontend/app/page.tsx`
- Modify: `docs/architecture.md`
- Modify: `docs/roadmap.md`
- Modify: this plan checklist as tasks complete

- [ ] **Step 1: Update frontend job types**

Add nested `print` to the `Job` type in `frontend/app/page.tsx`:

```ts
print: {
  status: string
  printer_state: string | null
  progress_percent: number | null
  remaining_time_minutes: number | null
  current_layer: number | null
  total_layers: number | null
  active_file: string | null
  last_progress_percent: number | null
  last_layer: number | null
  error: string | null
  started_at: string | null
  finished_at: string | null
  updated_at: string | null
}
```

- [ ] **Step 2: Render dispatch and print states separately**

In the job table:

- Rename current status column content to show `Dispatch`.
- Add `Print` information in the same row or a new column:
  - status badge from `job.print.status`.
  - percent when present.
  - `current_layer / total_layers` when present.
  - remaining minutes when present.
  - terminal `job.print.error` when present.

Keep styling compact and consistent with the existing operational dashboard.

- [ ] **Step 3: Update architecture and roadmap**

`docs/architecture.md`:

- Mark Phase 9 as implemented.
- Document `PrintJobReport`, `job.print` response shape, `job_progress` tenant WebSocket event, physical lifecycle fields, and machine event dedupe.
- Preserve the statement that dispatch `succeeded` is not physical print completion.

`docs/roadmap.md`:

- Move Phase 9 to completed.
- Set Immediate Next to Phase 10 external identity authentication.
- Keep later phases 11-15 intact unless Phase 9 completion changes wording.

- [ ] **Step 4: Frontend build**

Run:

```bash
npm --prefix frontend run build
```

Expected: Next.js build succeeds.

## Milestone 6: Final Verification, Review, Commit, Push

**Files:**
- No feature files unless fixes are required.

- [ ] **Step 1: Required generated-output check**

Run:

```bash
find . -path ./target -prune -o \( -name '*.pb.rs' -o -name '*.tonic.rs' \) -print
```

Expected: no output.

- [ ] **Step 2: Rust formatting**

Run:

```bash
cargo fmt --check
```

Expected: exit 0.

- [ ] **Step 3: Rust lint**

Run:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: exit 0.

- [ ] **Step 4: Rust tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: all workspace tests pass.

- [ ] **Step 5: Frontend build**

Run:

```bash
npm --prefix frontend run build
```

Expected: exit 0.

- [ ] **Step 6: Diff hygiene**

Run:

```bash
git diff --check
git status --short
```

Expected: no whitespace errors; only intended files changed; no generated protobuf output.

- [ ] **Step 7: Independent final implementation review**

Dispatch an independent reviewer with the approved spec, this plan, final diff, and verification output. Required verdict:

```text
VERDICT: APPROVE
```

Fix and re-review until approved.

- [ ] **Step 8: Commit and push**

Use Lore commit protocol. Suggested intent line:

```text
Reconcile physical print state from Bambu reports
```

Include trailers for constraints, rejected alternatives, tests, and not-tested live printer coverage. Push to current `main` upstream.

## Plan Self-Review

- Spec coverage: covers proto, agent normalization/forwarding, hub schema/repository reconciliation, event dedupe, validation, HTTP/WS API, frontend job table, docs, and verification.
- Placeholder scan: no unfinished marker text remains.
- Type consistency: physical lifecycle is named `PrintStatus`/`JobPrintState` in core, `PrintJobReport` in proto, and nested `print` in HTTP/WS/frontend.
- Scope: Phase 9 stays focused on physical print progress and machine events. Auth, discovery, AMS, SeaORM migration, and rich browser WebSocket consumption remain later phases.
