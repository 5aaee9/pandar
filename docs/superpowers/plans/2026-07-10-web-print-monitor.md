# Web Print Monitor and Build-Plate Recovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show Studio-originated print name, percentage, layers, and remaining time in the Web, and provide server-authoritative native build-plate mismatch recovery with the same Studio action semantics.

**Architecture:** Extend the additive Agent report and persistence contract first, then linearize Agent sessions and merge versioned printer state in the Hub. Publish the enriched state through the existing REST/WebSocket discriminator, add a capability-gated live recovery route, and make the Web reconcile an authoritative REST baseline around future-only WebSocket events before rendering the monitor and mismatch dialog.

**Tech Stack:** Rust 2024, Tokio, tonic/prost, serde, SeaORM/sqlx, SQLite, PostgreSQL, axum WebSocket, rumqttc QoS 1, Next.js 16, React 19, TypeScript 5.9, Vitest, next-intl, Kotlin serialization, cargo-nextest.

## Global Constraints

- The reviewed source of truth is `docs/superpowers/specs/2026-07-10-web-print-monitor-design.md`; preserve every fail-closed rule, generation rule, reconnect rule, and rollout constraint in that design.
- Use `reference/BambuStudio` for direct Studio payload and `job_state` behavior. For the focused `05008051` runtime catalog, `reference/bambuddy/backend/app/data/hms_actions.json` contains six exact entries (`093`, `094`, `20P`, `22E`, `239`, `31B`) with `PROBLEM_SOLVED_RESUME`, `IGNORE_RESUME`, and `STOP_PRINTING`; `reference/bambuddy/scripts/update_hms_actions.py` maps Studio IDs `28`, `27`, and `5` to those names, while `reference/BambuStudio/src/slic3r/GUI/DeviceErrorDialog.hpp` defines the same numeric enum. The packaged Studio `094`/`239` JSON is only the older baseline; `reference/BambuStudio/src/slic3r/GUI/HMS.cpp` proves Studio loads local data then downloads newer `GetActionImage.php` data.
- Preserve protobuf compatibility: add `AgentCapability` value `2` and `PrintJobReport` tags `25/26`; do not renumber or reuse any existing tag.
- Web recovery sends Studio's native `err`/`job_id`/`param:"reserve"` request with sequence ID `0`, but only a fresh-connection QoS1 PUBACK is transport confirmation; no application-level sequence-zero response completes an attempt.
- Never issue Resume, Ignore, or Stop to a real printer during tests or smoke checks. Use typed fixtures and fake/local MQTT event drivers only.
- Every known JSON shape uses typed serde/TypeScript/Kotlin structures. `serde_json::Value` remains only at genuinely open printer-payload boundaries.
- SQLite and PostgreSQL are first-class. New schema, defaults, constraints, lock ordering, transactions, race tests, and observable behavior must match.
- Preserve complete lower-level error chains with `{err:#}` or equivalent at runtime boundaries; do not expose credentials, access codes, bearer tokens, request bodies, or response bodies.
- Keep every modified production Rust module below 400 LOC. Extract the focused modules listed below instead of extending files already near the limit; never use `include!`.
- Do not touch, read, stage, delete, or rename any untracked `crates/pandar-network-plugin/probe-*` directory.
- The Studio plugin remains on the old `HandlePrintError` capability and nonzero sequence/application-result path. The Web path requires `HandlePrintErrorSequenceZeroPubackOnly`.
- Web recovery remains live-only and requires exactly one active Hub. It never enters the durable outbound queue.
- Maintain the checklist in this plan during implementation. Per the invoked `sdd-workflow`, do not create task commits; commit once only after final independent implementation approval, documentation updates, and fresh verification.
- Do not update user-facing architecture/development/Android documentation before the final implementation review gate. Update `docs/roadmap.md` together with those docs immediately after that gate, before verification and commit.

---

## File Structure

- `proto/pandar/agent/v1/agent.proto` owns additive Agent capability and report fields.
- `crates/pandar-agent/src/machine/mqtt/reports/{schema.rs,protocol.rs}` owns typed `job_attr` decode and presence-preserving protobuf mapping.
- `crates/pandar-agent/src/machine/mqtt/recovery.rs` owns the fresh recovery-only rumqttc connection and bounded PUBACK state machine; `machine/runtime.rs` owns the exact sequence-zero dispatch split.
- `crates/pandar-hub/migrations/{sqlite,postgres}/20260710000000_web_print_monitor.sql` owns equivalent revision/generation/session schema.
- `crates/pandar-hub/src/repositories/agents/connections.rs` and `sessions/transitions.rs` own persistent exact-session claims and stable per-Agent process leases.
- `crates/pandar-hub/src/repositories/printers/live_status/{merge.rs,persistence.rs}` owns pure task/error generation and atomic printer-row revision updates.
- `crates/pandar-hub/src/repositories/printers/queries.rs` owns enriched list/get hydration so `printers.rs` stays below 400 LOC.
- `crates/pandar-hub/src/printer_events.rs`, `runtime.rs`, and `routes/printer_events.rs` own the existing-discriminator enriched event plus process epoch invalidation.
- `crates/pandar-hub/src/repositories/commands/audit/printer_operations/recovery.rs` owns locked server-side recovery validation and shared native single-flight; `routes/printer_operations/web_recovery.rs` owns tenant orchestration.
- `frontend/app/printer-live-types.ts` owns additive/legacy boundary types; `frontend/app/printer-reconciliation.ts` owns pure revision/material merge rules; `frontend/app/use-dashboard-runtime-events.ts` owns socket, REST, deadlines, cadence, and command-result notification.
- `frontend/app/printer-print-status.tsx` owns the card monitor; `frontend/app/printer-mismatch-dialog.tsx` owns occurrence coordination/dialog state; `frontend/app/plate-mismatch-actions.ts` owns presentation eligibility; `frontend/app/printer-recovery-actions.ts` owns the non-redirecting server action.

## Execution Preflight After Plan Approval

- [x] Run `git status --short --branch` and confirm that only the approved spec/plan and the pre-existing `probe-*` directories are untracked.
- [x] Run `git fetch origin` followed by `git rebase origin/main`, as explicitly requested by the user.
- [x] Run `git status --short` again; stop on any tracked conflict and never use `reset --hard`, `checkout --`, or an operation that removes the probe directories.

### Task 1: Add the Agent Wire Capability and Presence-Preserving `job_attr`

**Files:**
- Modify: `proto/pandar/agent/v1/agent.proto:25-28,83-108`
- Modify: `crates/pandar-agent/src/lib.rs:183-194`
- Modify: `crates/pandar-agent/src/machine/mqtt/commands.rs:104-123`
- Modify: `crates/pandar-agent/src/machine/mqtt/reports/schema.rs:16-44`
- Modify: `crates/pandar-agent/src/machine/mqtt/reports.rs:57-108`
- Modify: `crates/pandar-agent/src/machine/mqtt/reports/protocol.rs:10-68`
- Modify: `crates/pandar-agent/src/machine/mqtt/tests/print_error.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/tests.rs:1002-1038`
- Modify: `crates/pandar-agent/src/tests.rs:96-114`

**Interfaces:**
- Produces `AgentCapability::HandlePrintErrorSequenceZeroPubackOnly` from protobuf enum value `2`.
- Adds `job_attr: Option<u32>` to `PrintReportProgress` and protobuf fields `job_attr/has_job_attr` at tags `25/26`.
- Preserves explicit `job_attr = 0`; absence never overwrites Hub state.
- Later tasks require both capabilities in Agent hello and consume `PrintJobReport.has_job_attr`.

- [x] **Step 1: Add failing typed MQTT and protobuf presence tests**

Extend `crates/pandar-agent/src/machine/mqtt/tests/print_error.rs` with exact zero/nonzero/absent cases:

```rust
#[test]
fn job_attr_preserves_zero_nonzero_and_absence() {
    let cases = [
        (serde_json::json!({"print": {"job_attr": 0}}), Some(0)),
        (serde_json::json!({"print": {"job_attr": 0x21}}), Some(0x21)),
        (serde_json::json!({"print": {"mc_percent": 7}}), None),
        (serde_json::json!({"print": {"job_attr": -1}}), None),
        (serde_json::json!({"print": {"job_attr": "invalid"}}), None),
    ];
    for (report, expected) in cases {
        let progress = print_report_from_report(&endpoint(), &report);
        assert_eq!(progress.job_attr, expected);
    }
}

#[test]
fn job_attr_presence_round_trips_to_agent_report() {
    let explicit_zero = print_job_report_event(&config(), progress_with_job_attr(Some(0)));
    let absent = print_job_report_event(&config(), progress_with_job_attr(None));
    let Some(agent_event::Event::PrintJobReport(explicit_zero)) = explicit_zero.event else {
        panic!("expected print report");
    };
    let Some(agent_event::Event::PrintJobReport(absent)) = absent.event else {
        panic!("expected print report");
    };
    assert!(explicit_zero.has_job_attr);
    assert_eq!(explicit_zero.job_attr, 0);
    assert!(!absent.has_job_attr);
}
```

Also assert that an invalid `job_attr` does not discard other valid fields from the same report. Update the exact hello test to expect both enum values in stable order:

```rust
assert_eq!(
    hello.capabilities,
    vec![
        AgentCapability::HandlePrintError as i32,
        AgentCapability::HandlePrintErrorSequenceZeroPubackOnly as i32,
    ],
);
```

- [x] **Step 2: Run the focused tests and confirm the expected compile failure**

Run:

```powershell
cargo nextest run -p pandar-agent -E 'test(hello_event_has_agent_identity_version_and_exact_capability) | test(/machine::mqtt::tests::print_error/)'
```

Expected: compile failures for missing `job_attr`, `has_job_attr`, and `HandlePrintErrorSequenceZeroPubackOnly`.

- [x] **Step 3: Add the exact additive protobuf contract**

Append only these values/tags:

```proto
enum AgentCapability {
  AGENT_CAPABILITY_UNSPECIFIED = 0;
  AGENT_CAPABILITY_HANDLE_PRINT_ERROR = 1;
  AGENT_CAPABILITY_HANDLE_PRINT_ERROR_SEQUENCE_ZERO_PUBACK_ONLY = 2;
}

message PrintJobReport {
  // Existing fields 1 through 24 remain byte-for-byte numbered as they are.
  uint32 job_attr = 25;
  bool has_job_attr = 26;
}
```

- [x] **Step 4: Thread typed `job_attr` through MQTT, progress, and protobuf**

Add the field at the typed boundary and map it without `Value` extraction. The second struct below is specifically the existing `PrintReportProgress` in `crates/pandar-agent/src/machine/mqtt/commands.rs`:

```rust
#[derive(Debug, Default, Deserialize)]
pub(super) struct PrintReportSection {
    // existing fields
    #[serde(default)]
    pub(super) job_attr: Option<NumericValue>,
}

pub struct PrintReportProgress {
    // existing fields
    pub job_attr: Option<u32>,
}
```

In `print_report_from_parsed_report`, use the existing bounded numeric helper with the full protobuf range:

```rust
job_attr: bounded_u32(print.job_attr.as_ref(), 0, u32::MAX),
```

In `print_job_report_event`, capture presence before unwrapping:

```rust
let has_job_attr = progress.job_attr.is_some();
let job_attr = progress.job_attr.unwrap_or_default();
// ... inside PrintJobReport
job_attr,
has_job_attr,
```

Advertise both capabilities in `hello_event` in the order used by the test.

- [x] **Step 5: Run Agent protocol/report tests and module-size guard**

Run:

```powershell
cargo nextest run -p pandar-agent -E 'test(hello_event_has_agent_identity_version_and_exact_capability) | test(/machine::mqtt::tests::print_error/)'
cargo nextest run -p pandar-core workspace_production_rust_modules_stay_under_line_limit
```

Expected: all selected tests pass; every production Rust module remains below 400 LOC.

### Task 2: Dispatch Sequence-Zero Recovery on a Fresh MQTT Connection

**Files:**
- Create: `crates/pandar-agent/src/machine/mqtt/recovery.rs`
- Create: `crates/pandar-agent/src/machine/mqtt/tests/recovery.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt.rs:3-45`
- Modify: `crates/pandar-agent/src/machine/mqtt/tests.rs:14-18`
- Modify: `crates/pandar-agent/src/machine/mqtt/transport.rs:72-87,202-237`
- Modify: `crates/pandar-agent/src/machine/mod.rs:239-255`
- Modify: `crates/pandar-agent/src/machine/operations.rs:79-171`
- Modify: `crates/pandar-agent/src/machine/runtime.rs:213-223`
- Modify: `crates/pandar-agent/src/machine/types.rs:53-69`
- Test: `crates/pandar-agent/src/machine/tests/print_error.rs`

**Interfaces:**
- Produces `dispatch_sequence_zero_recovery(endpoint, command) -> anyhow::Result<PrinterOperationDispatchResult>`.
- Uses one unique client ID, no subscription, one QoS1 PUBLISH, and one five-second end-to-end deadline per call.
- Returns `sequence_id: Some("0")` only after the matching connection-scoped PUBACK.
- Leaves every nonzero Studio `HandlePrintError` on `dispatch_printer_operation` and its application-result correlation path.

- [x] **Step 1: Add failing recovery-connection state-machine tests**

Declare `mod recovery;` in the MQTT test parent. In the new test file, drive an injectable event sequence with rumqttc `Event` values and assert the invariants:

```rust
#[tokio::test]
async fn recovery_waits_for_its_own_publish_puback_and_ignores_reports() {
    let attempt = FakeRecoveryAttempt::new([
        Event::Incoming(Packet::Publish(application_report("0", "resume"))),
        Event::Outgoing(Outgoing::Publish(41)),
        Event::Incoming(Packet::PubAck(PubAck::new(7))),
        Event::Incoming(Packet::PubAck(PubAck::new(41))),
    ]);
    let result = dispatch_with_attempt(attempt, request_payload()).await.unwrap();
    assert_eq!(result.sequence_id.as_deref(), Some("0"));
}

#[tokio::test]
async fn timed_out_attempt_is_dropped_and_retry_has_a_distinct_client() {
    let first = FakeRecoveryAttempt::pending("recovery-a");
    let second = FakeRecoveryAttempt::acked("recovery-b", 3);
    assert!(dispatch_with_deadline(first, Duration::from_millis(5)).await.is_err());
    assert_eq!(dispatch_with_attempt(second.clone(), request_payload()).await.unwrap().sequence_id.as_deref(), Some("0"));
    assert_ne!(second.client_id(), "recovery-a");
    assert!(second.did_not_observe_old_puback());
}
```

Add cases for: no EventLoop poll means no completion; reusable command connection already has queued/unacknowledged work; connection replay; queue/connect/poll/protocol errors; cancellation; delayed old PUBACK; application PUBLISH before and after PUBACK; no Subscribe request.
For Resume, Ignore, and Stop, decode the one captured publish and assert it is the existing Studio-derived `print/handle_print_error` payload with current `err`, `job_id`, action parameter, `param: "reserve"`, and sequence `0`; do not introduce a second payload builder.

- [x] **Step 2: Run the focused recovery tests and confirm failure**

Run:

```powershell
cargo nextest run -p pandar-agent -E 'test(/machine::mqtt::tests::recovery/)'
```

Expected: compile failure because the recovery-only module and test driver do not exist.

- [x] **Step 3: Implement the clean-connection PUBACK primitive**

Create the production entry point with a deadline that starts before topic identity resolution:

```rust
const RECOVERY_DEADLINE: Duration = Duration::from_secs(5);

pub(super) async fn dispatch_sequence_zero_recovery(
    endpoint: &BambuPrinterEndpoint,
    command: BambuMqttCommand,
) -> anyhow::Result<PrinterOperationDispatchResult> {
    tokio::time::timeout(RECOVERY_DEADLINE, dispatch_attempt(endpoint, command))
        .await
        .context("timed out dispatching sequence-zero recovery through MQTT PUBACK")??;
    Ok(PrinterOperationDispatchResult {
        sequence_id: Some("0".to_owned()),
        error: None,
        mqtt_report: None,
        mqtt_summary: None,
    })
}
```

The inner attempt must create and drop its own client/EventLoop:

```rust
let suffix = format!("recovery-{}", uuid::Uuid::new_v4());
let mut options = bambu_lan_mqtt_options(endpoint, Some(&suffix));
options.set_clean_session(true);
let (client, mut event_loop) = rumqttc::AsyncClient::new(options, 1);
let topic = resolved_request_topic(endpoint).await?;
client.publish(topic, QoS::AtLeastOnce, false, payload).await?;
let mut own_packet_id = None;
loop {
    match event_loop.poll().await.context("poll recovery MQTT event loop")? {
        Event::Outgoing(Outgoing::Publish(packet_id)) => own_packet_id = Some(packet_id),
        Event::Incoming(Packet::PubAck(ack)) if own_packet_id == Some(ack.pkid) => break,
        Event::Incoming(Packet::Publish(_)) | Event::Incoming(_) | Event::Outgoing(_) => {}
    }
}
```

Expose only a narrow topic-resolution helper from `transport.rs`; do not reuse the persistent `RumqttcBambuMqttTransport` or call `next_report`. The unique ID plus explicit clean session prevents broker state from crossing attempts; enqueue exactly one publish request and never subscribe.

- [x] **Step 4: Route exactly sequence-zero native recovery through the fresh connection**

Add a narrow endpoint lookup to `ConfiguredBambuMachineGateway` and split before taking the reusable path:

```rust
pub fn endpoint(&self, serial_number: &str) -> Option<BambuPrinterEndpoint> {
    self.printers.iter()
        .find(|(endpoint, _, _)| endpoint.serial == serial_number)
        .map(|(endpoint, _, _)| endpoint.clone())
}
```

In `RuntimeBambuMachineGateway::operate_printer`:

```rust
if matches!(operation, PrinterOperation::HandlePrintError { sequence_id: 0, .. }) {
    let endpoint = self.inner.lock().await.endpoint(serial_number)
        .with_context(|| format!("no configured Bambu printer matches serial {serial_number}"))?;
    let command = mqtt_command_for_printer_operation(operation)?;
    return dispatch_sequence_zero_recovery(&endpoint, command).await;
}
self.inner.lock().await.operate_printer(serial_number, operation).await
```

Make only `mqtt_command_for_printer_operation` visible to its parent so both paths use the existing exact Studio payload builder.

- [x] **Step 5: Run recovery and nonzero Studio regression tests**

Run:

```powershell
cargo nextest run -p pandar-agent -E 'test(/machine::mqtt::tests::recovery/) | test(native_print_error_dispatch_preserves_transport_and_result_correlation) | test(native_print_error_all_actions_convert_to_typed_machine_operations)'
cargo nextest run -p pandar-agent
cargo nextest run -p pandar-core workspace_production_rust_modules_stay_under_line_limit
```

Expected: clean recovery tests pass, nonzero Studio correlation remains unchanged, and all Agent tests pass.

### Task 3: Add Equivalent Revision, Generation, and Session Schema

**Files:**
- Create: `crates/pandar-hub/migrations/sqlite/20260710000000_web_print_monitor.sql`
- Create: `crates/pandar-hub/migrations/postgres/20260710000000_web_print_monitor.sql`
- Modify: `crates/pandar-hub/src/entities/printers.rs:5-35`
- Modify: `crates/pandar-hub/src/entities/agents.rs:5-16`
- Modify: `crates/pandar-hub/src/repositories/printers/live_status.rs:18-100`
- Modify: `crates/pandar-hub/src/repositories/tests/printer_live_status.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/postgres.rs:426-432`

**Interfaces:**
- Produces `state_revision >= 1`, task/error generation counters, raw `print_job_attr`, exact error marker fields, and `agents.current_session_id` on both backends.
- Extends `PrinterWithLiveStatus` with `state_revision: u64`; extends `PrinterLiveStatus` with generation/raw marker data needed by later repository code.
- Database default `state_revision = 1` is authoritative for legacy inserts that omit every new column.

- [x] **Step 1: Add failing migration/default/backfill tests**

Add a SQLite test that inserts a printer with a raw legacy-style statement omitting every new column, then hydrates it:

```rust
#[tokio::test]
async fn legacy_printer_insert_gets_revision_one_and_zero_generations() {
    let fixture = fixture().await;
    insert_legacy_printer(&fixture.database, fixture.tenant.id, fixture.agent.id).await;
    let printer = fixture.printers
        .list_with_live_status_for_tenant(fixture.tenant.id)
        .await.unwrap().remove(0);
    assert_eq!(printer.state_revision, 1);
    assert_eq!(printer.live_status.task_generation, 0);
    assert_eq!(printer.live_status.error_generation, 0);
}
```

Add migration-text assertions for `DEFAULT 1`, `CHECK (state_revision >= 1)`, matching counter defaults, and nullable session/error markers. Seed one legacy row for each existing task-evidence column and prove generation `1`; seed a positive legacy error and prove error generation `1` is bound to that task generation while session/time markers remain null and unrecoverable. Add the same legacy insert/backfill assertions to the configured PostgreSQL repository test.

- [x] **Step 2: Run the schema tests and confirm failure**

Run:

```powershell
cargo nextest run -p pandar-hub -E 'test(/printer_live_status/) | test(postgres_print_reports_merge_printer_live_status_without_a_job_when_configured)'
```

Expected: migration/field assertions fail because the new columns do not exist.

- [x] **Step 3: Add the two explicit backend migrations**

Use the same logical SQL with backend integer types. The SQLite file contains:

```sql
ALTER TABLE printers ADD COLUMN state_revision INTEGER NOT NULL DEFAULT 1 CHECK (state_revision >= 1);
ALTER TABLE printers ADD COLUMN print_task_generation INTEGER NOT NULL DEFAULT 0 CHECK (print_task_generation >= 0);
ALTER TABLE printers ADD COLUMN print_error_generation INTEGER NOT NULL DEFAULT 0 CHECK (print_error_generation >= 0);
ALTER TABLE printers ADD COLUMN print_job_attr INTEGER;
ALTER TABLE printers ADD COLUMN print_error_task_generation INTEGER;
ALTER TABLE printers ADD COLUMN print_error_session_id TEXT;
ALTER TABLE printers ADD COLUMN print_error_received_at TEXT;
ALTER TABLE agents ADD COLUMN current_session_id TEXT;

UPDATE printers
SET print_task_generation = 1
WHERE print_task_id IS NOT NULL
   OR print_subtask_id IS NOT NULL
   OR print_progress_percent IS NOT NULL
   OR print_remaining_time_minutes IS NOT NULL
   OR print_current_layer IS NOT NULL
   OR print_total_layers IS NOT NULL
   OR print_gcode_file IS NOT NULL
   OR print_subtask_name IS NOT NULL
   OR print_job_id IS NOT NULL
   OR print_error > 0
   OR print_gcode_state IN ('PREPARE', 'SLICING', 'RUNNING', 'PAUSE', 'FINISH', 'FAILED');

UPDATE printers
SET print_error_generation = 1,
    print_error_task_generation = print_task_generation
WHERE print_error > 0;
```

The PostgreSQL file uses `BIGINT` for the three counters and `print_job_attr`, retains the same defaults/checks, and leaves `print_error_session_id`/`print_error_received_at` null during backfill.

- [x] **Step 4: Extend SeaORM entities and live-status hydration**

Add the exact entity fields and checked conversions:

```rust
pub state_revision: i64,
pub print_task_generation: i64,
pub print_error_generation: i64,
pub print_job_attr: Option<i64>,
pub print_error_task_generation: Option<i64>,
pub print_error_session_id: Option<String>,
pub print_error_received_at: Option<String>,
```

Add `current_session_id: Option<String>` to `agents::Model`. Hydrate public counters as `u64` and raw `job_attr` as `u32`, returning contextual repository errors for invalid database values:

```rust
pub struct PrinterWithLiveStatus {
    pub state_revision: u64,
    pub printer: Printer,
    pub live_status: PrinterLiveStatus,
}

pub struct PrinterLiveStatus {
    pub task_generation: u64,
    pub error_generation: u64,
    pub job_attr: Option<u32>,
    pub error_task_generation: Option<u64>,
    pub error_session_id: Option<String>,
    pub error_received_at: Option<String>,
    // existing live fields
}
```

- [x] **Step 5: Run SQLite/PostgreSQL schema tests and the Hub module-size guard**

Run:

```powershell
cargo nextest run -p pandar-hub -E 'test(/printer_live_status/) | test(postgres_print_reports_merge_printer_live_status_without_a_job_when_configured)'
cargo nextest run -p pandar-core workspace_production_rust_modules_stay_under_line_limit
```

Expected: SQLite always passes; PostgreSQL executes and passes when `PANDAR_TEST_POSTGRES_URL` is configured.

### Task 4: Linearize Agent Session Claims and Agent-Owned Printer State Mutations

**Files:**
- Create: `crates/pandar-hub/src/repositories/agents/connections.rs`
- Create: `crates/pandar-hub/src/sessions/transitions.rs`
- Modify: `crates/pandar-hub/src/repositories/agents.rs` (split connection methods out; keep below 400 LOC)
- Modify: `crates/pandar-hub/src/repositories/mod.rs`
- Modify: `crates/pandar-hub/src/sessions.rs:29-173,213-244`
- Modify: `crates/pandar-hub/src/grpc.rs:88-145`
- Modify: `crates/pandar-hub/src/grpc/inbound.rs:24-140`
- Modify: `crates/pandar-hub/src/grpc/printer_snapshots.rs`
- Modify: `crates/pandar-hub/src/grpc/print_reports.rs`
- Modify: `crates/pandar-hub/src/grpc/printer_materials.rs`
- Modify: `crates/pandar-hub/src/repositories/adapters/printers.rs`
- Modify: `crates/pandar-hub/src/repositories/jobs/print_reports.rs`
- Modify: `crates/pandar-hub/src/repositories/materials.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/lifecycle.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/printer_snapshots.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/print_reports.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/printer_materials.rs`

**Interfaces:**
- Produces `SessionRegistry::transition_lease(agent_id) -> OwnedMutexGuard<()>`, stable across AgentSession replacement.
- Produces repository session claim/heartbeat/offline helpers and `begin_current_agent_transaction` with SQLite immediate / PostgreSQL Agent-row-first locking.
- Changes snapshot/report/material handlers to accept the authenticated `SessionToken` and commit only if `agents.current_session_id` still matches.
- Later Web recovery holds the same lease and transaction order through persistence and live enqueue.

- [x] **Step 1: Add deterministic replacement-race tests**

Add two deterministic interleavings. First, pause A before it acquires the stable lease, let B claim/install, then resume A and prove its now-stale token cannot mutate. Second, pause A after it owns the lease, start B replacement and prove B cannot become current until A's already-linearized mutation completes:

```rust
#[tokio::test]
async fn replacement_session_blocks_old_snapshot_report_material_and_heartbeat_commits() {
    let fixture = connected_fixture().await;
    let session_a = fixture.current_session();
    let paused = fixture.pause_before_transition_lease(session_a.token);
    let old_writes = fixture.spawn_all_mutations(session_a.token);
    paused.wait_until_reached().await;
    let session_b = fixture.replace_session().await;
    paused.resume();
    old_writes.await.unwrap();
    assert_eq!(fixture.persisted_session_id().await, session_b.token.persisted_id());
    assert_eq!(fixture.printer_state().await, fixture.state_before_old_writes());
}

#[tokio::test]
async fn replacement_waits_for_a_mutation_that_already_owns_the_lease() {
    let fixture = connected_fixture().await;
    let session_a = fixture.current_session();
    let paused = fixture.pause_after_transition_lease(session_a.token);
    let old_write = fixture.spawn_snapshot(session_a.token);
    paused.wait_until_reached().await;
    let replacement = fixture.spawn_replacement_session();
    assert!(!replacement.is_finished());
    paused.resume();
    old_write.await.unwrap();
    let session_b = replacement.await.unwrap();
    assert_eq!(fixture.persisted_session_id().await, session_b.token.persisted_id());
    assert_eq!(fixture.printer_state().await, fixture.snapshot_from_session_a());
}

#[tokio::test]
async fn old_disconnect_cannot_clear_replacement_session() {
    let fixture = connected_fixture().await;
    let session_a = fixture.current_session();
    let session_b = fixture.replace_session().await;
    fixture.disconnect(session_a.token).await;
    assert_eq!(fixture.persisted_session_id().await, session_b.token.persisted_id());
    assert_eq!(fixture.agent_status().await, "online");
}
```

Cover A/B concurrent registration, old heartbeat, snapshot, print report, material snapshot, stale expiry, and exact-session offline clear.

- [x] **Step 2: Run lifecycle and mutation tests and confirm the race failure**

Run:

```powershell
cargo nextest run -p pandar-hub -E 'test(/grpc::tests::lifecycle/) | test(/grpc::tests::printer_snapshots/) | test(/grpc::tests::print_reports/) | test(/grpc::tests::printer_materials/)'
```

Expected: new interleaving assertions fail under the current `while_current` check-before/check-after implementation.

- [x] **Step 3: Add the stable process lease and persisted token representation**

Move transition ownership out of `AgentSession`:

```rust
#[derive(Debug, Clone, Default)]
pub struct AgentTransitions {
    leases: Arc<Mutex<HashMap<AgentId, Arc<Mutex<()>>>>>,
}

impl AgentTransitions {
    pub async fn lease(&self, agent_id: AgentId) -> OwnedMutexGuard<()> {
        let lease = self.leases.lock().await
            .entry(agent_id)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        lease.lock_owned().await
    }
}

impl SessionToken {
    pub fn persisted_id(self) -> String { self.0.to_string() }
}
```

`SessionRegistry` owns `AgentTransitions`; replacement never replaces the lease. Keep the existing per-session live-command transition only for pending command result ownership.

- [x] **Step 4: Implement database claim and exact-session transaction helpers**

In `repositories/agents/connections.rs`, implement the fixed lock order:

```rust
pub async fn begin_current_agent_transaction(
    database: &Database,
    tenant_id: TenantId,
    agent_id: AgentId,
    session_id: &str,
) -> RepositoryResult<DatabaseTransaction> {
    let tx = begin_agent_transaction(database).await?; // SQLite IMMEDIATE, PostgreSQL normal
    let agent = locked_agent(&tx, agent_id).await?
        .ok_or(RepositoryError::MissingAgent)?;
    if agent.tenant_id != tenant_id.to_string()
        || agent.current_session_id.as_deref() != Some(session_id)
    {
        return Err(RepositoryError::AgentSessionNotCurrent);
    }
    Ok(tx)
}
```

`begin_agent_transaction` branches by backend: SQLite starts `SqliteTransactionMode::Immediate`; PostgreSQL starts a normal transaction and `locked_agent` performs an Agent `SELECT ... FOR UPDATE` before any printer query. Every snapshot/report/material/recovery repository then locks the printer row second with PostgreSQL `FOR UPDATE`; SQLite already owns the write reservation. Add `AgentSessionNotCurrent` to `RepositoryError`, then add `claim_online_session`, `heartbeat_if_current`, and `mark_offline_if_current`; each locks Agent first, compares the persisted session ID, and commits its status/token update in the same transaction. Run the deterministic replacement/lock-order cases against SQLite and the configured PostgreSQL backend.

- [x] **Step 5: Reorder registration, inbound mutation, and disconnect flows**

Registration becomes:

```rust
let token = SessionToken::new();
let _lease = state.sessions().transition_lease(agent_id).await;
state.agents().claim_online_session(
    tenant_id, agent_id, &token.persisted_id(), &hello.version, &now,
).await.map_err(repository_status)?;
let replaced = state.sessions().register(session).await;
```

Every inbound mutation takes the same lease, confirms the registry token, and passes `token.persisted_id()` into the repository transaction. Snapshot upsert must accept a transaction instead of executing directly against a pool. Print and material transactions begin with `begin_current_agent_transaction`, then lock/mutate printer rows. Disconnect/expiry removes and marks offline only under the stable lease and only for the exact token.

- [x] **Step 6: Run all exact-session tests and module-size guard**

Run:

```powershell
cargo nextest run -p pandar-hub -E 'test(/grpc::tests::lifecycle/) | test(/grpc::tests::printer_snapshots/) | test(/grpc::tests::print_reports/) | test(/grpc::tests::printer_materials/) | test(/sessions::tests/)'
cargo nextest run -p pandar-core workspace_production_rust_modules_stay_under_line_limit
```

Expected: every old-session write is rejected before commit; B remains online/current; production modules stay below 400 LOC.

### Task 5: Merge Task/Error Generations and Atomically Revise Printer State

**Files:**
- Create: `crates/pandar-hub/src/repositories/printers/live_status/merge.rs`
- Create: `crates/pandar-hub/src/repositories/printers/live_status/persistence.rs`
- Create: `crates/pandar-hub/src/repositories/printers/queries.rs`
- Modify: `crates/pandar-hub/src/repositories/printers/live_status.rs`
- Modify: `crates/pandar-hub/src/repositories/printers.rs` (move enriched queries; remain below 400 LOC)
- Modify: `crates/pandar-hub/src/repositories/jobs/print_reports.rs`
- Modify: `crates/pandar-hub/src/repositories/jobs/print_reports/correlation.rs`
- Modify: `crates/pandar-hub/src/repositories/adapters/printers.rs`
- Modify: `crates/pandar-hub/src/grpc/print_reports.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/printer_live_status.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/postgres.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/print_reports/live_status.rs`

**Interfaces:**
- Produces pure `merge_live_report(stored, patch, session_id, received_at) -> MergedPrinterLiveStatus`.
- Produces `AppliedPrintReport { printer_id, live_status_changed, ... }` and post-commit enriched reload.
- Atomically increments `state_revision` for snapshot, print/last-seen, and user-visible printer edits; material-only writes retain independent ordering.
- Clears task/error/recovery fields exactly according to the reviewed identity/state truth table.

- [x] **Step 1: Add the full pure merge truth-table tests**

Create table-driven tests covering all reviewed transitions. The core shape is:

```rust
#[test]
fn task_identity_and_error_occurrence_matrix_is_fail_closed() {
    for case in cases() {
        let merged = merge_live_report(&case.stored, &case.patch, &case.session, &case.received_at);
        assert_eq!(merged.state, case.expected, "{}", case.name);
        assert_eq!(merged.live_status_changed, case.changed, "{}", case.name);
    }
}
```

The explicit cases are: every common-slot identity conflict; equality plus enrichment; missing identity partial; first enrichment; no-common-slot ambiguity; blank identity plus task/subtask sentinel `"0"`; inactive/terminal-to-live; explicit IDLE highest precedence; FINISH retention; explicit zero/empty values; absent fields; positive/clear/same-positive ABA; different error; task change while positive; job-ID change; missing/different task/session marker; same error from replacement session with omitted `job_attr/job_id`; explicit `job_attr = 0`; repeated same occurrence. Generate `received_at` inside the Hub handler/repository boundary and use that Hub receive time for error markers; never authorize from Agent-provided `observed_at`.

- [x] **Step 2: Add atomic revision and concurrent-writer tests**

Test that last-seen-only reports increment the stored revision without publishing, while user edit, snapshot, and live changes each increment exactly once. Add two concurrent PostgreSQL writers and assert the final revision advances twice, not once.

- [x] **Step 3: Run the focused tests and confirm current merge/revision failures**

Run:

```powershell
cargo nextest run -p pandar-hub -E 'test(/printer_live_status/) | test(/grpc::tests::print_reports::live_status/) | test(postgres_print_reports_merge_printer_live_status_without_a_job_when_configured)'
```

Expected: generation/revision assertions fail because current persistence performs partial ActiveModel updates without generations or atomic revision expressions.

- [x] **Step 4: Implement the pure identity/state/error reducer**

Use typed slots and exhaustive state classification:

```rust
enum NativePrintState { Live, Terminal, Idle, Unknown }

fn classify(value: Option<&str>) -> NativePrintState {
    match value {
        Some("PREPARE" | "SLICING" | "RUNNING" | "PAUSE") => NativePrintState::Live,
        Some("FINISH" | "FAILED") => NativePrintState::Terminal,
        Some("IDLE") => NativePrintState::Idle,
        Some(_) | None => NativePrintState::Unknown,
    }
}
```

Compare every trusted common identity slot. On a proven boundary, clear all task-scoped fields before applying the patch. On no-common-slot ambiguity, preserve display/generation but clear marker, `printer_job_id`, and `job_attr`. Before establishing a positive marker because it is absent or task/session-mismatched, clear those same recovery fields, then apply only fields present in the current frame. Explicit IDLE ignores same-frame stale task/error/job fields.

- [x] **Step 5: Persist complete merged state with one atomic revision expression**

Write the full merged print state under the already locked transaction and increment with a database expression:

```rust
printers::Entity::update_many()
    .filter(printers::Column::Id.eq(printer_id))
    .set(merged_active_model(&merged.state, observed_at))
    .col_expr(
        printers::Column::StateRevision,
        Expr::col(printers::Column::StateRevision).add(1),
    )
    .exec(transaction)
    .await
    .context("failed to persist merged printer live status")?;
```

Add the same expression to user edits. In both backend-specific snapshot `ON CONFLICT` statements, explicitly insert `state_revision = 1` and set `state_revision = printers.state_revision + 1` on conflict while executing through the exact-session transaction introduced in Task 4. Return `live_status_changed` excluding last-seen/revision-only changes, commit, reload `PrinterWithLiveStatus`, and publish only after commit.

- [x] **Step 6: Run merge, concurrency, and PostgreSQL tests**

Run:

```powershell
cargo nextest run -p pandar-hub -E 'test(/printer_live_status/) | test(/grpc::tests::print_reports/) | test(postgres_print_reports_merge_printer_live_status_without_a_job_when_configured)'
cargo nextest run -p pandar-core workspace_production_rust_modules_stay_under_line_limit
```

Expected: every truth-table case and atomic increment passes on SQLite and configured PostgreSQL.

### Task 6: Publish Enriched REST/WebSocket State and Repair Event Loss

**Files:**
- Modify: `crates/pandar-hub/src/printer_events.rs:17-220`
- Modify: `crates/pandar-hub/src/routes/printers.rs:70-119`
- Modify: `crates/pandar-hub/src/routes/printers/responses.rs`
- Modify: `crates/pandar-hub/src/routes/printers/update.rs`
- Modify: `crates/pandar-hub/src/repositories/printers.rs`
- Modify: `crates/pandar-hub/src/grpc/printer_snapshots.rs`
- Modify: `crates/pandar-hub/src/grpc/printer_materials.rs`
- Modify: `crates/pandar-hub/src/grpc/print_reports.rs`
- Modify: `crates/pandar-hub/src/routes/printer_events.rs:73-125`
- Modify: `crates/pandar-hub/src/runtime.rs:32-84`
- Modify: `crates/pandar-hub/src/lib.rs:320-339`
- Modify: `crates/pandar-hub/src/cluster/tests.rs`
- Create: `crates/pandar-hub/src/runtime/tests/printer_event_epoch.rs`
- Modify: `crates/pandar-hub/src/runtime/tests.rs`
- Modify: `crates/pandar-hub/src/routes/tests/printers.rs`
- Modify: `crates/pandar-hub/src/routes/tests/printer_events_ws.rs`
- Modify: `crates/pandar-hub/src/routes/tests/plugin/live_status.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/printer_snapshots.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/printer_materials.rs`

**Interfaces:**
- Produces optional-compatible `PrinterEventPrinter.state_revision` and `PrinterEventPrinter.print` while every upgraded producer supplies `Some`.
- Produces `PrinterEventPrint` with required generations/HMS and nullable device fields; raw `job_attr` and authorization markers remain private.
- Produces `PrinterEventHub::{subscribe_epoch,invalidate_epoch}`; lag/publish/receive/EOF faults close sockets without adding a public event discriminator.
- REST list/detail and `printer_snapshot` share the same enriched builder.

- [x] **Step 1: Add failing public-contract and compatibility tests**

Decode exact expected output from list, detail, and WebSocket:

```rust
#[derive(Deserialize)]
struct EnrichedPrinter {
    state_revision: u64,
    print: EnrichedPrint,
}

#[derive(Deserialize)]
struct EnrichedPrint {
    task_generation: u64,
    error_generation: u64,
    hms: Vec<PrinterHms>,
    job_state: Option<u32>,
    gcode_state: Option<String>,
    task_id: Option<String>,
    subtask_id: Option<String>,
    progress_percent: Option<u8>,
    remaining_time_minutes: Option<u32>,
    current_layer: Option<u32>,
    total_layers: Option<u32>,
    gcode_file: Option<String>,
    subtask_name: Option<String>,
    print_error: Option<u32>,
    printer_job_id: Option<String>,
}
```

Assert `(job_attr >> 4) & 0x0f`, no host/access code/raw attr/marker leakage, explicit nulls, same REST/event shape, legacy events with both fields absent decode to `None`, and an old-shape serde fixture ignores the additive fields. Add mixed-replica control-plane fixtures that decode both legacy and enriched snapshots without a new discriminator or duplicate local delivery. Keep the Studio plugin live-status regression on its existing flat keys and prove it does not receive the nested tenant-Web contract.

- [x] **Step 2: Add failing lag/epoch/retry tests**

Cover tenant `broadcast::RecvError::Lagged`, process epoch change, printer-event publish failure, control-plane item error, EOF, subscribe failure, one-second resubscribe, and no duplicate delivery after resubscribe. A lag or epoch change must terminate the socket rather than wait for another event.

- [x] **Step 3: Run contract/event tests and confirm failures**

Run:

```powershell
cargo nextest run -p pandar-hub -E 'test(/routes::tests::printers/) | test(/printer_events_ws/) | test(/plugin::live_status/) | test(/grpc::tests::printer_snapshots/) | test(/runtime::tests::printer_event_epoch/)'
```

Expected: missing fields and current `Lagged => continue` / single-subscribe behavior fail the new assertions.

- [x] **Step 4: Define and populate the enriched public type**

Add the compatible shape:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrinterEventPrint {
    pub task_generation: u64,
    pub error_generation: u64,
    pub job_state: Option<u32>,
    pub gcode_state: Option<String>,
    pub task_id: Option<String>,
    pub subtask_id: Option<String>,
    pub progress_percent: Option<u8>,
    pub remaining_time_minutes: Option<u32>,
    pub current_layer: Option<u32>,
    pub total_layers: Option<u32>,
    pub gcode_file: Option<String>,
    pub subtask_name: Option<String>,
    pub print_error: Option<u32>,
    pub printer_job_id: Option<String>,
    pub hms: Vec<PrinterHms>,
}

pub struct PrinterEventPrinter {
    // existing safe shell fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub print: Option<PrinterEventPrint>,
}
```

Change `printer_event_printer` to accept `PrinterWithLiveStatus` and always produce `Some` on upgraded paths. Compute `job_state` from raw attr and never serialize raw/marker/credentials.

- [x] **Step 5: Reload enriched state in every producer**

Add `get_with_live_status_for_tenant` and use enriched list/get queries. After snapshot, material patch, print report, and user edit commit, reload `PrinterWithLiveStatus` plus the latest materials before building an event. Make the audited delete transaction hydrate and return the locked pre-delete `PrinterWithLiveStatus`, because the common response builder can no longer accept a plain row after deletion. Coalesce a print report that changes both live state and materials into one enriched snapshot; a last-seen-only revision emits none.

- [x] **Step 6: Add the internal epoch and resilient control-plane loop**

Extend `PrinterEventHub` with a watch channel:

```rust
pub fn subscribe_epoch(&self) -> watch::Receiver<u64> { self.epoch.subscribe() }

pub fn invalidate_epoch(&self) {
    self.epoch.send_modify(|value| *value = value.wrapping_add(1));
}
```

Use `tokio::select!` in `forward_events`; `Lagged`, epoch change, serializer failure, send failure, or channel close exits. Wrap control-plane subscription in an outer loop: item errors invalidate and continue; EOF invalidates then resubscribes after one second; later subscribe failures invalidate/log full context/sleep/retry. `publish_printer_event` invalidates on publish failure.

- [x] **Step 7: Run REST/WS/epoch and compatibility tests**

Run:

```powershell
cargo nextest run -p pandar-hub -E 'test(/routes::tests::printers/) | test(/printer_events_ws/) | test(/plugin::live_status/) | test(/grpc::tests::printer_snapshots/) | test(/grpc::tests::printer_materials/) | test(/runtime::tests::printer_event_epoch/)'
cargo nextest run -p pandar-core workspace_production_rust_modules_stay_under_line_limit
```

Expected: enriched shape, no leaks, socket invalidation, and retry semantics all pass.

### Task 7: Add Server-Authoritative Web Build-Plate Recovery

**Files:**
- Create: `crates/pandar-hub/src/routes/printer_operations/plate_mismatch.rs`
- Create: `crates/pandar-hub/src/routes/printer_operations/web_recovery.rs`
- Create: `crates/pandar-hub/src/repositories/commands/audit/printer_operations/recovery.rs`
- Modify: `crates/pandar-hub/src/routes/printer_operations.rs:17-315`
- Modify: `crates/pandar-hub/src/routes/printer_operations/live.rs`
- Modify: `crates/pandar-hub/src/routes/printers.rs:322-345`
- Modify: `crates/pandar-hub/src/repositories/commands/audit/printer_operations.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/operations.rs`
- Modify: `crates/pandar-hub/src/repositories/mod.rs`
- Modify: `crates/pandar-hub/src/routes/tests/printer_commands.rs`
- Create or extend: `crates/pandar-hub/src/routes/tests/printer_commands/print_error.rs`
- Modify: `crates/pandar-hub/src/routes/tests/plugin/operations.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/commands/print_error.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/postgres_commands/print_error.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/commands/print_error.rs`

**Interfaces:**
- Adds tenant request `{action:"handle_print_error", error_action, error_generation}` and rejects all client transport/state fields.
- Produces `create_web_print_error_sent_with_audit` that locks Agent then printer, validates exact current occurrence/session/state/catalog, performs shared native single-flight, and returns a typed `PersistedLivePrinterOperation` containing the command, locked serial number, and server-owned sequence `0` operation.
- Generalizes the existing exact-token live dispatcher into `dispatch_persisted_live_command`, which calls the existing `SessionRegistry::try_dispatch_live_command_with_capability`; Web holds the stable transition lease through transaction and enqueue.
- Leaves plugin input/old capability/nonzero sequence behavior intact while sharing only the in-flight native exclusion.

- [x] **Step 1: Add failing parser, authorization, catalog, and dispatch tests**

Use typed request fixtures. The accepted body is exactly:

```json
{"action":"handle_print_error","error_action":"resume","error_generation":9}
```

Add rejection cases for Viewer, wrong tenant, missing printer, unknown/extra/missing fields, client `print_error/job_id/job_attr/job_state/task_generation/sequence_id`, wrong/cleared/different error, stale generation, marker task/session mismatch, missing receive time, native state outside `PREPARE|SLICING|RUNNING|PAUSE`, coarse IDLE/OFFLINE/FAILED, family miss, Resume/Ignore with missing or `>1` job state, missing new capability, offline/replaced Agent, ownership race, publish failure, and duplicate native command.

Add success cases for Resume/Ignore/Stop on all six supported families, current `20P`, explicit empty job ID, and Stop with unknown job state. Assert the typed command and audit metadata contain the locked server action/error/job/sequence, protobuf tag 25 carries `sequence_id = 0`, and no durable queue/wake occurs.

- [x] **Step 2: Run recovery route/repository tests and confirm failures**

Run:

```powershell
cargo nextest run -p pandar-hub -E 'test(/printer_commands::print_error/) | test(/commands::print_error/) | test(/plugin::operations/)'
```

Expected: tenant parser rejects all native actions and no Web recovery repository method exists.

- [x] **Step 3: Implement the exact focused action catalog and request split**

Use a closed catalog, not an inferred general HMS system:

```rust
const BUILD_PLATE_MISMATCH: u32 = 83_918_929;
const SUPPORTED_FAMILIES: [&str; 6] = ["093", "094", "20P", "22E", "239", "31B"];

pub fn supports(serial: &str, action: PrintErrorAction) -> bool {
    let family = serial.get(..3).map(str::to_ascii_uppercase);
    family.as_deref().is_some_and(|value| SUPPORTED_FAMILIES.contains(&value))
        && matches!(action, PrintErrorAction::Resume | PrintErrorAction::Ignore | PrintErrorAction::Stop)
}
```

Lock this exact six-family/three-action matrix in table tests. The implementation evidence remains the checked-in bambuddy snapshot plus Studio's `DeviceErrorDialog.hpp`, updater mapping, and runtime download path; production/tests do not load the reference projects dynamically.

Add `error_generation: RequestField<u64>` and return:

```rust
enum TenantPrinterOperation {
    Queued(PrinterOperationKind),
    HandlePrintError { error_action: PrintErrorAction, error_generation: u64 },
}
```

Ordinary actions require every native field missing. Web recovery requires only semantic action/generation. Plugin recovery continues to require current transport fields and rejects Web generation.

- [x] **Step 4: Implement the locked validation and shared single-flight transaction**

Define the repository input:

```rust
pub struct WebPrintErrorRecovery {
    pub action: PrintErrorAction,
    pub error_generation: u64,
    pub expected_agent_id: AgentId,
    pub expected_session_id: String,
}

pub struct PersistedLivePrinterOperation {
    pub command: CommandRecord,
    pub serial_number: String,
    pub operation: PrinterOperationKind,
}
```

Begin the exact-session transaction, lock Agent then printer, and revalidate all approved predicates. Derive `job_state = (job_attr >> 4) & 0x0f`; Resume/Ignore require `Some(0 | 1)`, Stop does not. Query `sent`/`acknowledged` printer-operation commands for the locked printer, deserialize `PrinterOperationPayload`, and reject any `HandlePrintError`. Construct the operation only from locked server values:

```rust
PrinterOperationKind::HandlePrintError {
    error_action: input.action,
    print_error: BUILD_PLATE_MISMATCH,
    printer_job_id: printer.print_job_id.unwrap_or_default(),
    sequence_id: 0,
}
```

Call the same in-flight helper from plugin sent persistence, without applying Web occurrence/job-state rules to plugin input.

- [x] **Step 5: Hold the stable lease through persistence and live enqueue**

Add this concrete helper in `routes/printer_operations/live.rs`:

```rust
async fn dispatch_persisted_live_command(
    state: &AppState,
    persisted: &PersistedLivePrinterOperation,
    token: SessionToken,
    capability: AgentCapability,
) -> Result<(), LiveDispatchError> {
    let hub_command = live_printer_operation_hub_command(
        persisted.command.id,
        persisted.serial_number.clone(),
        persisted.operation.clone(),
    );
    state.sessions().try_dispatch_live_command_with_capability(
        persisted.command.tenant_id,
        persisted.command.agent_id,
        token,
        capability,
        persisted.command.id,
        hub_command,
    ).await
}
```

The Web orchestrator must execute in this order using the existing exact-capability token lookup:

```rust
let _lease = state.sessions().transition_lease(agent_id).await;
let capability = AgentCapability::HandlePrintErrorSequenceZeroPubackOnly;
let token = state.sessions()
    .current_token_for_capability(tenant_id, agent_id, capability)
    .await
    .ok_or_else(printer_operation_unavailable)?;
let persisted = state.commands().create_web_print_error_sent_with_audit(
    tenant_id, printer_id, recovery(token.persisted_id()), actor,
).await.map_err(web_recovery_error)?;
if let Err(error) = dispatch_persisted_live_command(
    state, &persisted, token, capability,
).await {
    fail_live_dispatch(state, &persisted.command, error).await;
    return Err(printer_operation_unavailable());
}
```

Replacement waits on the same lease. Dispatch still rechecks exact token/capability. Any failure marks the persisted command failed with full context and returns stable `400 printer_operation_unavailable`.

- [x] **Step 6: Prove single-flight and mixed-version behavior on both backends**

Run:

```powershell
cargo nextest run -p pandar-hub -E 'test(/printer_commands::print_error/) | test(/commands::print_error/) | test(/plugin::operations/) | test(/grpc::tests::commands::print_error/)'
cargo nextest run -p pandar-hub -E 'test(/postgres_commands::print_error/)'
cargo nextest run -p pandar-core workspace_production_rust_modules_stay_under_line_limit
```

Expected: one concurrent request persists/sends, plugin/Web overlap is excluded, terminal retry is allowed, old Agent fails Web closed, and plugin remains available with the old capability.

### Task 8: Reconcile Web Printer State Across Future-Only and Lost Events

**Files:**
- Create: `frontend/app/printer-live-types.ts`
- Create: `frontend/app/printer-reconciliation.ts`
- Create: `frontend/app/printer-reconciliation.test.ts`
- Create: `frontend/app/use-dashboard-runtime-events.ts`
- Create: `frontend/app/use-dashboard-runtime-events.test.tsx`
- Modify: `frontend/app/dashboard-types.ts`
- Modify: `frontend/app/dashboard-runtime.tsx`
- Modify: `frontend/app/dashboard-runtime.test.tsx`
- Create: `frontend/app/api/tenants/[tenantId]/printers/route.ts`
- Create: `frontend/app/api/tenants/[tenantId]/printers/route.test.ts`

**Interfaces:**
- Models the optional-compatible Hub envelope while treating upgraded `state_revision` and `print` as present on authoritative responses.
- Adds a pure revision/observation merge reducer plus a token-owned reconciliation coordinator.
- Moves dashboard event ownership out of the over-limit runtime component and implements socket-first buffered bootstrap, serialized periodic repair, bounded fetches, visibility repair, and fail-closed enriched state.
- Keeps browser credentials on same-origin proxy routes; the browser never calls the Rust API directly.

- [x] **Step 1: Add failing pure reducer tests for ordering and whole replacement**

Define the narrow client contract:

```ts
export type PrinterPrintState = {
  task_generation: number
  error_generation: number
  hms: Array<{ attr: number; code: number }>
  job_state: number | null
  gcode_state: string | null
  task_id: string | null
  subtask_id: string | null
  subtask_name: string | null
  gcode_file: string | null
  progress_percent: number | null
  remaining_time_minutes: number | null
  current_layer: number | null
  total_layers: number | null
  print_error: number | null
  printer_job_id: string | null
}
```

Import `PrinterPrintState` into `dashboard-types.ts` and add `state_revision?: number` and `print?: PrinterPrintState | null` directly to the existing `Printer`. Optionality exists only for rolling decode; upgraded authoritative responses must supply both fields.

Cover: whole-list replace removes deleted printers; versioned shell/print data applies only when its revision is greater; duplicate/older revisions cannot regress a clear; materials merge independently by `observed_at` even from a lower-revision or legacy snapshot; a legacy snapshot cannot overwrite an enriched known printer's shell/print; an unknown enriched printer requests exactly one authoritative repair instead of being inserted; a still-absent printer is discarded; a newly present REST printer becomes baseline before a still-higher buffered event applies; legacy rows remain displayable but cannot expose recovery.

- [x] **Step 2: Run the reducer tests and confirm failure**

Run:

```powershell
npm --prefix frontend test -- printer-reconciliation.test.ts
```

Expected: modules do not exist.

- [x] **Step 3: Implement the pure state reducer and monotonic tokens**

Use explicit reducer outputs rather than side effects:

```ts
type MergeResult =
  | { kind: "applied"; printers: Printer[] }
  | { kind: "ignored"; printers: Printer[] }
  | { kind: "resync"; printers: Printer[] }

export function mergePrinterEvent(
  current: Printer[],
  incoming: Printer,
): MergeResult
```

Represent each authoritative fetch with a monotonically increasing token. Immediately before replacing state, require that token to still own the coordinator. Do the same immediately before replaying buffered events so an older response cannot overwrite a newer reconnect.

- [x] **Step 4: Add failing coordinator tests with fake timers and abortable fetches**

Cover the exact state machine:

1. Wait for the WebSocket `open` callback, install/activate its snapshot buffer, and only then start the initial REST `PrinterList` fetch; continue handling `job_progress` and `command_result` through their typed paths, and do not mark the channel `live` yet.
2. Replace the whole printer list unconditionally, retaining only browser-local dialog dismissal keys, then replay only buffered shell/print revisions greater than the fetched row while independently merging newer materials.
3. On reconnect, invalidate/abort the old fetch and repeat bootstrap.
4. Serialize 30-second start-to-start repairs; never overlap them.
5. Keep one 10-second `AbortController` deadline active through response body read, JSON decode, and a `performance.now()` pre-apply check; a synchronous decode that crosses the monotonic deadline is rejected even if the abort callback was delayed.
6. A snapshot for an unknown printer ID triggers at most one confirmation fetch: add it only from that REST response, then replay a still-higher buffered revision; if still absent, discard the event. One cycle is therefore at most two sequential 10-second attempts.
7. `visibilitychange` to visible and `pageshow` trigger an immediate repair and reset the cadence; if a cycle is active, coalesce triggers into exactly one pending rerun that starts immediately after it terminates.
8. Failed fetch/decode closes the socket, clears `state_revision`/`print` and recovery eligibility while retaining safe coarse inventory, and exposes unavailable/retry state.
9. Cleanup aborts fetches, timers, and socket callbacks without stale React updates.
10. A legacy REST baseline whole-replaces inventory and clears enriched data; subsequent legacy socket events may update only coarse shell/material state and cannot re-enable recovery.

- [x] **Step 5: Implement the same-origin printer-list proxy**

Follow the existing ticket proxy's authenticated server-side forwarding. Forward the Hub status and JSON body without caching:

```ts
export const dynamic = "force-dynamic"

export async function GET(
  request: Request,
  context: { params: Promise<{ tenantId: string }> },
): Promise<Response>
```

Forward `request.signal` to the upstream fetch so the browser's deadline cancels proxy work. Tests must prove tenant path encoding, authenticated API forwarding, abort propagation, no-store semantics, upstream status preservation, and no credential/API URL exposure in the browser response.

- [x] **Step 6: Extract and implement the runtime event coordinator**

Move WebSocket/notification ownership from `dashboard-runtime.tsx` into `use-dashboard-runtime-events.ts`. Return only the state the runtime renders:

```ts
type DashboardRuntimeEvents = {
  liveState: LiveState
  lastEventAt: string | null
  notifications: RuntimeNotification[]
  printers: Printer[]
  jobs: Job[]
  retry: () => void
}
```

Pass the existing `apiUrl`, auth source, selected tenant, initial printers, and initial jobs into the hook. The timer is serialized and measured start-to-start. Its 40-second silent-loss guarantee is algorithmic only while the page scheduler is active and a REST attempt completes inside its full-body deadline; the immediate visibility/pageshow repair covers browser suspension. Do not display a stronger wall-clock claim.

- [x] **Step 7: Verify the coordinator and the extracted module size**

Run:

```powershell
npm --prefix frontend test -- printer-reconciliation.test.ts use-dashboard-runtime-events.test.tsx "app/api/tenants/[tenantId]/printers/route.test.ts"
npm --prefix frontend exec tsc -- --noEmit
cargo nextest run -p pandar-core workspace_production_rust_modules_stay_under_line_limit
```

Expected: ordering/race tests pass, `dashboard-runtime.tsx` is below 400 LOC, and no production Rust module exceeds the repository limit.

### Task 9: Render Native-Style Print Status and Build-Plate Recovery in Web

**Files:**
- Create: `frontend/app/printer-print-status.tsx`
- Create: `frontend/app/printer-print-status.test.tsx`
- Create: `frontend/app/printer-mismatch-dialog.tsx`
- Create: `frontend/app/printer-mismatch-dialog.test.tsx`
- Create: `frontend/app/plate-mismatch-actions.ts`
- Create: `frontend/app/plate-mismatch-actions.test.ts`
- Create: `frontend/app/printer-recovery-actions.ts`
- Create: `frontend/app/printer-recovery-actions.test.ts`
- Create: `frontend/app/printer-operation-payload.ts`
- Create: `frontend/app/printer-operation-payload.test.ts`
- Modify: `frontend/app/use-dashboard-runtime-events.ts`
- Modify: `frontend/app/use-dashboard-runtime-events.test.tsx`
- Modify: `frontend/app/dashboard-printer-card.tsx`
- Modify: `frontend/app/dashboard-inventory.tsx`
- Modify: `frontend/app/dashboard-inventory.test.tsx`
- Modify: `frontend/app/dashboard-view-content.tsx`
- Modify: `frontend/app/dashboard-runtime.test.tsx`
- Modify: `frontend/app/dashboard-types.ts`
- Modify: `frontend/app/job-format.ts`
- Modify: `frontend/messages/en.json`
- Modify: `frontend/messages/zh.json`
- Modify: `mobile/android/app/src/test/kotlin/zip/iptables/pandar/android/data/remote/dto/PrinterEventsDecoderTest.kt`

**Interfaces:**
- Replaces the card's coarse-only Running presentation with native live details when enriched data exists, while retaining the coarse status as an availability/state fallback.
- Centralizes one build-plate mismatch dialog for the selected occurrence and uses a dedicated `useActionState` server action that submits only semantic action plus `error_generation` upstream.
- Derives actions from the same closed native Studio catalog encoded on the Hub and never clears the warning optimistically.
- Converts the successful sequence-zero `command_result` into the exact bilingual “sent, awaiting printer confirmation” toast.

- [x] **Step 1: Add failing print-status presentation tests**

Cover:

- task display precedence: nonblank `subtask_name`, then the basename of `gcode_file`, then the localized unknown-task label;
- progress clamped only for presentation and rendered as a percentage;
- current/total layer rendering when both are known, plus sensible one-sided display;
- remaining minutes formatted through `job-format.ts` without duplicating duration logic;
- running/paused presentation for `RUNNING|PRINTING|PAUSE|PAUSED`, preparing/slicing presentation for `PREPARE|SLICING`, and a finished view for `FINISH`; the display aliases never widen recovery eligibility or task-boundary classification;
- `PREPARE|SLICING` retains the task name but renders progress, layers, and remaining time unavailable even if a stale numeric field is present;
- coarse `IDLE|OFFLINE|FAILED` suppresses progress/finished content even if stale live data exists;
- missing optional enriched fields preserves the existing coarse status instead of fabricating zeroes.

Run the presentation cases under both English and Chinese message providers so every new state, warning, action, unavailable guidance, and success string is proven in both catalogs.

- [x] **Step 2: Add failing native action and dialog tests**

Implement one pure client helper mirroring the server catalog for presentation only:

```ts
export type PlateMismatchAction = "resume" | "ignore" | "stop"

export function plateMismatchActions(
  serialNumber: string,
  print: PrinterPrintState,
): PlateMismatchAction[]
```

Test exact `print_error` code `83918929`, families `093|094|20P|22E|239|31B`, empty catalog for `26A`/unknown, Resume/Ignore only with present `job_state <= 1`, Stop independent of job state, active exact state `PREPARE|SLICING|RUNNING|PAUSE`, and coarse `IDLE|OFFLINE|FAILED` as a complete veto. Unsupported combinations render localized printer-only guidance. Stop uses destructive styling, with the mismatch dialog itself as its confirmation surface rather than a second nested confirmation. The Hub remains authoritative and must reject any stale or forged request.

Dialog tests cover the displayed code `0500-8051` and approved English/Chinese mismatch explanation, auto-open once per `printer.id:error_generation`, dismissal across unavailable/reconnect with the same generation, clear/reappear auto-open with a higher generation, inline-warning reopen, clear/different-error close, stable printer-list selection when multiple printers are affected, Resume/Ignore/Stop labels and native ordering, no action for an unsupported state, busy-state deduplication, accessible focus/close behavior, and the exact upstream request body:

```json
{"action":"handle_print_error","error_action":"resume","error_generation":9}
```

Assert that the body contains no HMS code, printer job ID, job state, task generation, or sequence ID. The server action validates its `FormData` boundary, calls `requireAuth`, encodes tenant/printer path segments, does not redirect, and returns a typed `idle | sent | error` state; its success wording is `sent`, never `queued` or `completed`.

The action's only upstream payload is constructed server-side:

```ts
const response = await fetch(
  `${apiUrl}/api/v1/tenants/${encodeURIComponent(tenantId)}/printers/${encodeURIComponent(printerId)}/controls`,
  {
    method: "POST",
    headers: await apiHeaders("application/json"),
    body: JSON.stringify({
      action: "handle_print_error",
      error_action: errorAction,
      error_generation: errorGeneration,
    }),
  },
)
return response.ok ? { status: "sent" } : { status: "error", error: await responseError(response) }
```

- [x] **Step 3: Run the component/helper tests and confirm failure**

Run:

```powershell
npm --prefix frontend test -- printer-print-status.test.tsx printer-mismatch-dialog.test.tsx plate-mismatch-actions.test.ts printer-recovery-actions.test.ts printer-operation-payload.test.ts
```

Expected: the new modules do not exist and the card still renders only coarse state.

- [x] **Step 4: Implement live card details without duplicating formatting policy**

Adapt the existing `job-format.ts` progress/layer/time helpers to the nullable live contract. Take the basename of `gcode_file` without exposing a device path. Keep the status component presentational:

```tsx
<PrinterPrintStatus
  coarseStatus={printer.status}
  print={printer.print ?? null}
/>
```

Do not derive command eligibility here. Replace only the current status area in `dashboard-printer-card.tsx`; keep card navigation, temperatures, and materials unchanged.

- [x] **Step 5: Implement the page-level mismatch coordinator**

Mount one provider/coordinator in the dashboard inventory boundary and let cards register/select an occurrence key of `printer.id:error_generation`. Preserve stable authoritative printer-list order for simultaneous unresolved occurrences; dismissing/submitting one advances to the next, while an inline reopen explicitly selects that printer. Each affected card keeps a red inline warning until an authoritative snapshot clears/changes that occurrence, including unsupported families/states. Disable duplicate submission while the request is in flight. A synchronous action failure keeps the dialog open and restores controls; a successful HTTP response closes the dialog but leaves the inline warning and does not consume its reopen action.

The successful HTTP response is only command acceptance. Do not show a completion toast there; await the typed command-result event.

- [x] **Step 6: Add the exact post-PUBACK toast and translations**

At the untrusted JSON boundary, parse the already-delivered `command.payload_json` with `parsePrinterOperationPayload` into a new discriminated TypeScript `PrinterOperationPayload` matching the Rust `{printer_id, serial_number, operation:{type,...}}` shape. Reject malformed/unknown payloads rather than treating them as recovery. For a succeeded `operation.type === "handle_print_error"` with numeric `sequence_id === 0`, show exactly:

```text
Recovery command sent; waiting for printer status confirmation
恢复指令已发送，等待打印机状态确认
```

Do not reuse a generic “completed” toast. Failed results retain the existing error toast, including the server's preserved cause/context. Nonzero Studio/plugin command results retain current behavior.

- [x] **Step 7: Prove Android ignores the additive enriched fields**

Extend `PrinterEventsDecoderTest.kt` with a `printer_snapshot` fixture containing `state_revision` and the complete nested `print` object. Assert the current DTO still decodes the printer/material data under `ignoreUnknownKeys = true`; do not add unused Android model fields.

- [x] **Step 8: Verify UI, localization, compatibility, and production builds**

Run:

```powershell
npm --prefix frontend test
npm --prefix frontend run lint
npm --prefix frontend exec tsc -- --noEmit
npm --prefix frontend run build
.\mobile\android\gradlew.bat -p mobile/android testDebugUnitTest --tests "zip.iptables.pandar.android.data.remote.dto.PrinterEventsDecoderTest"
```

Expected: the Web UI exposes full live details and only native valid mismatch actions, while Android continues decoding additive snapshots.

## Implementation Review Gate (Orchestrator Only)

- [x] Run every targeted command from Tasks 1–9 and inspect the full working-tree diff.
- [x] Invoke `superpowers:requesting-code-review` and send the approved design, this plan, and the exact diff to a fresh independent subagent reviewer.
- [x] Run a separate default-model `opencode-agent` review against the same design, plan, and diff.
- [x] Require both reviewers to return literal `VERDICT: APPROVE`; any `REVISE` reopens implementation, targeted verification, and both reviews.
- [x] Do not update roadmap/development/architecture docs, commit, or push before both approvals.

### Task 10: Update Documentation, Run Fresh Verification, Commit Once, and Push

**Files:**
- Modify: `docs/roadmap.md`
- Modify: `docs/development.md`
- Modify: `docs/architecture.md`
- Modify: `docs/android.md`
- Verify only: all implementation files from Tasks 1–9

**Interfaces:**
- Documents the shipped live-monitor/reconciliation/recovery behavior, operational rollout order, and failure semantics only after the implementation is independently approved.
- Performs a fresh, evidence-producing final verification with no physical printer recovery command.
- Creates one Conventional Commit for the reviewed change set and pushes the current branch.

- [x] **Step 1: Update the required documentation after final approval**

Record:

- completed Web print name/progress/layers/time/HMS visibility and next roadmap work;
- socket-first REST reconciliation, 30-second repair, 10-second full-body deadline, visibility repair, and fail-closed unavailable behavior;
- sequence-zero recovery-only MQTT/PUBACK semantics and the fact that PUBACK is transport confirmation, not printer recovery confirmation;
- native build-plate mismatch catalog/guards and server-authoritative occurrence validation;
- schema order and mixed-version rollout: database, dual-capability Agents, all Hubs, confirmation, then Web;
- rollback order: disable Web/server action, drain commands to terminal states with no local pending dispatch, then Hub, then Agent;
- single-active-Hub limitation for recovery and Android additive-field compatibility.

Do not claim the 40-second repair bound while a browser page is suspended.

- [ ] **Step 2: Read and apply the final-verification skill, then run format/lint/type checks**

Read `superpowers:verification-before-completion` immediately before these fresh commands:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
npm --prefix frontend run lint
npm --prefix frontend exec tsc -- --noEmit
```

Fix any failure, rerun the affected targeted tests, and then restart this final verification step from the first command.

- [x] **Step 3: Run the full Rust, Web, PostgreSQL, and Android suites**

Run:

```powershell
cargo nextest run --manifest-path Cargo.toml --workspace
npm --prefix frontend test
npm --prefix frontend run build
.\mobile\android\gradlew.bat -p mobile/android testDebugUnitTest --tests "zip.iptables.pandar.android.data.remote.dto.PrinterEventsDecoderTest"
```

For PostgreSQL parity, use `PANDAR_TEST_POSTGRES_URL` when already configured. Otherwise download the pinned expert-user Windows archive linked from PostgreSQL's official Windows download page into a unique temporary directory, initialize a trust-authenticated cluster bound only to loopback, and stop it after the tests:

```powershell
$temporaryPostgres = $null
try {
  if (-not $env:PANDAR_TEST_POSTGRES_URL) {
    $temporaryPostgres = Join-Path $env:TEMP ("pandar-postgres-" + [guid]::NewGuid())
    New-Item -ItemType Directory -Path $temporaryPostgres | Out-Null
    $archive = Join-Path $temporaryPostgres "postgresql.zip"
    Invoke-WebRequest -Uri "https://get.enterprisedb.com/postgresql/postgresql-17.10-2-windows-x64-binaries.zip" -OutFile $archive
    Expand-Archive -LiteralPath $archive -DestinationPath $temporaryPostgres
    $pgBin = Join-Path $temporaryPostgres "pgsql\bin"
    $data = Join-Path $temporaryPostgres "data"
    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $port = ([Net.IPEndPoint]$listener.LocalEndpoint).Port
    $listener.Stop()
    & (Join-Path $pgBin "initdb.exe") -D $data -U postgres -A trust --encoding=UTF8
    & (Join-Path $pgBin "pg_ctl.exe") -D $data -l (Join-Path $temporaryPostgres "postgres.log") -o "-h 127.0.0.1 -p $port" -w start
    & (Join-Path $pgBin "createdb.exe") -h 127.0.0.1 -p $port -U postgres pandar_test
    $env:PANDAR_TEST_POSTGRES_URL = "postgres://postgres@127.0.0.1:$port/pandar_test"
  }
  cargo nextest run -p pandar-hub -E 'test(/postgres/) | test(/postgres_commands/)'
} finally {
  if ($temporaryPostgres -and (Test-Path -LiteralPath $data)) {
    & (Join-Path $pgBin "pg_ctl.exe") -D $data -m fast -w stop
    Remove-Item Env:PANDAR_TEST_POSTGRES_URL -ErrorAction SilentlyContinue
  }
}
```

Before any later recursive cleanup, resolve the unique directory and verify it is a child of the resolved `$env:TEMP`; cleanup never targets the workspace or a computed unverified path.

- [ ] **Step 4: Smoke-test the local Hub/Agent/Web fixture without a printer action**

Start the repository's documented local fixture with synthetic report/event inputs. Verify:

1. REST and WebSocket expose identical enriched printer state and increasing revisions.
2. The Web card renders task/progress/layers/time.
3. A synthetic `05008051` occurrence presents the correct dialog/actions.
4. Stop before submitting any recovery action; no MQTT command reaches a physical printer.
5. Stop all fixture processes and retain their logs as verification evidence.

- [x] **Step 5: Inspect and stage only the reviewed files**

Run:

```powershell
git status --short
git diff --check
git diff --stat
git diff -- docs/superpowers/specs/2026-07-10-web-print-monitor-design.md docs/superpowers/plans/2026-07-10-web-print-monitor.md
```

Review every changed path. Stage explicit intended paths only with `git add -- <path...>`; never use `git add .`, and never read, modify, or stage `crates/pandar-network-plugin/probe-*`.

- [ ] **Step 6: Create one Conventional Commit and push**

Read the `conventional-commits` skill immediately before committing. Confirm the remote base has not moved:

```powershell
git fetch origin
git merge-base --is-ancestor origin/main HEAD
```

If `origin/main` advanced, rebase the reviewed commit/change set onto it and rerun the complete fresh verification before pushing. Otherwise commit exactly once:

```powershell
git commit -m "feat(printing): add web print monitoring and plate recovery"
git push
```

If push is rejected because the remote moved, rebase, rerun the complete fresh verification, and push again. Report the commit SHA, branch, remote ref, and verification commands only after the push succeeds.
