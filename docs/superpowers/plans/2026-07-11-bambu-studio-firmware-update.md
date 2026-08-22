# Bambu Studio Native Firmware Update Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Every task starts with failing tests, is implemented by a fresh subagent, and receives an independent fresh spec/quality review before the next task starts.

**Goal:** Make Bambu Studio's native Firmware page consume real printer-main and printer-reported AMS-family version/status telemetry, refresh versions from the printer, and pass Studio's four firmware commands through to the exact current capable Agent without inventing packages or replaying an update.

**Architecture:** Add a shared presence-preserving firmware model and additive protobuf events/commands. The Agent's long-lived report stream owns generation/revision-tagged firmware telemetry, while fresh single-use MQTT sessions execute bounded version refreshes and two-phase controls. Hub persists only typed URL-free telemetry and command bookkeeping behind SQLite/PostgreSQL-neutral repositories, and keeps prepared commands, package URLs, and result waiters process-local. The Rust network-plugin layer owns Studio JSON parsing, Hub HTTP, status/catalog rendering, invalidation resets, and delayed acknowledgement scheduling; `shim.cpp` remains a thin ABI/callback adapter.

**Tech Stack:** Rust 2024, serde, axum, SeaORM, tonic/prost, tokio, rumqttc, C++17/MSVC ABI probe, cargo-nextest.

## Global Constraints

- Source design: `docs/superpowers/specs/2026-07-11-bambu-studio-firmware-update-design.md`.
- Approved design SHA-256: `0B66032238056FAB99512509B1A83601ADE884DADBA14FB68D44040F37F369A4`.
- Baseline commit: `b8a490478dac68d8f914acad126234a73d403ea9`.
- Scope is B only: Bambu Studio's native Firmware page, printer-main firmware, and every AMS-family module reported by the printer. Web/Android remote OTA and package staging remain C and are not implemented here.
- Preserve Studio's exact MQTT shapes: `upgrade_confirm`, `consistency_confirm`, `start { url, module, version }`, and `mc_for_ams_firmware_upgrade { id }`; preserve Studio's string `sequence_id` and numeric `src_id` without regeneration.
- Parse known JSON with typed serde structs/enums. `serde_json::Value` is allowed only at the MQTT envelope/delta-merge boundary and arbitrary-JSON test assertions.
- Preserve the existing transport bounds rather than adding firmware policy: Hub/plugin JSON requests remain under the existing 64 KiB router/body boundary, and every fresh Agent MQTT option retains the existing 256 KiB inbound/outbound maximum packet size. Oversized Studio command input or printer telemetry fails at that existing boundary before Hub contact/publish/persistence.
- Do not add model, printer-state, homed-state, module-prefix, package-host, URL-host, `fun`, or device-feature restrictions. Validate only the external typed boundary: required strings, numeric type/range, tenant/printer ownership, exact session, generation, and Agent capability.
- `start.url` may exist only in Studio input, the plugin's one execute request, Hub process-local pending memory, the live protobuf execute message, Agent process memory, and the printer MQTT payload. It must never enter Pandar durable command payload/result JSON, audit metadata, printer firmware state, metrics, traces, panic strings, HTTP errors, or documentation examples copied into runtime output.
- Firmware refresh/control commands are live-only. Never enqueue, redispatch, reconstruct, retry, or replay them from durable command rows after disconnect, Agent replacement, Hub restart, or process failover.
- Keep exact-current-session semantics. Firmware state is presentable only when persisted session/generation matches the printer owner's current local Agent session and that session advertises `AGENT_CAPABILITY_FIRMWARE_CONTROL`.
- Only the Agent's long-lived report stream writes durable upgrade status. Fresh command clients may return transient state to the originating Studio callback but may not mutate the shared status reducer.
- Every report stream establishes a per-serial generation and emits invalidation before snapshots or commands for that generation. Module and status revisions are independent, monotonic within that generation, and compared strictly.
- Every Studio `info.get_version` uses the bounded live refresh path: one per-serial coordinator, a fresh MQTT session, at most three internal attempts, exact Studio sequence, and no successful cached/empty fallback after failure.
- Every mutating command uses Agent prepare then execute. Hub sends no action or URL in prepare. Agent rechecks generation immediately before publish. Reservations expire after one second.
- Every plugin mutation uses Hub prepare then execute. Execute is attempted at most once. A typed explicit `pre_publish_failure` is an ABI failure; an ambiguous HTTP failure after execute was attempted is URL-free `firmware_outcome_unknown`, ABI success, and no synthetic acknowledgement.
- Each refresh/control attempt uses a clean MQTT client with a unique bounded client id and a subscribe-confirmed receive pump. The pump serializes `barrier -> enqueue publish`, observes its own `rumqttc::Outgoing::Publish`, and only then marks the command published and admits post-barrier reports. Match only the exact command plus exact sequence; document the unavoidable delayed same-command/same-sequence ambiguity.
- `PublishedWithoutAcknowledgement` means only that MQTT publish completed. It is ABI success without a synthetic callback. Flash completion comes only from later authoritative long-lived `upgrade_state` telemetry.
- The firmware callback is per-originating ABI invocation. C++ performs a final return handoff for that invocation; Rust schedules the callback no earlier than handoff + 1.1 seconds and no later than handoff + 2 seconds. Unrelated sends cannot alter that window.
- Emit the exact design-specified local-unavailable reset on current-to-invalid transitions and repeat it until at least one reset has been emitted after three seconds, unless fresh current state arrives first.
- Extend the existing two-second batch plugin-printer refresh with firmware state; do not add N-per-printer heartbeat polling.
- Retain the documented one-active-Hub ownership restriction for live Agent sessions, prepared URLs, and result waiters. A non-owning replica returns unavailable and never forwards or reconstructs.
- No fabricated catalog record and no empty-URL selectable package. The initial catalog is empty. Tests may inject typed real records to verify Studio envelope mapping.
- Studio itself hides the update-available button for a true LAN-mode `MachineObject`; do not override it. Cloud/tunnel is the end-to-end native-button path; LAN ABI coverage proves telemetry and command transport only.
- `crates/pandar-network-plugin/src/shim.cpp` remains ABI plumbing only. Parsing, policy, status construction, HTTP behavior, redaction, outcome classification, and callback scheduling stay in Rust.
- Preserve complete lower-level cause chains after redacting secrets. Use `{error:#}` / `{err:#}` or equivalent.
- All touched/new production Rust modules must stay at or below 400 LOC. Extract from files already near the limit; never use `include!`. The pre-existing 2058-line `shim.cpp` is exempt only from the Rust LOC guard.
- SQLite and PostgreSQL must have equivalent schema, queries, transitions, and behavior. Run real PostgreSQL coverage when `PANDAR_TEST_POSTGRES_URL` is configured and record the exact skip otherwise.
- Do not modify, delete, or stage pre-existing `crates/pandar-network-plugin/probe-*` paths.
- Do not download an external firmware package and do not send a live firmware MQTT command during verification.
- No task-level commits. After all task gates, final dual review, documentation, and fresh verification, create one Conventional Commit for the implementation and push both it and the already-approved spec commit.

## File Structure

### Shared model and wire contract

- Create `crates/pandar-core/src/firmware.rs`: shared typed firmware modules, upgrade state, AMS switch state, catalog records, URL-free command metadata, acknowledgement, and terminal outcomes.
- Modify `crates/pandar-core/src/lib.rs`: export the firmware types.
- Create `crates/pandar-core/src/tests/firmware.rs` and modify `crates/pandar-core/src/tests.rs`: presence/serde/ordering tests.
- Modify `proto/pandar/agent/v1/agent.proto`: capability 5, firmware telemetry events, prepare/execute/refresh commands, prepared/published events, and typed terminal result.
- Modify `crates/pandar-agent/src/protocol.rs` only if generated-protocol test helpers need module wiring.

### Agent observation and control

- Create `crates/pandar-agent/src/machine/firmware.rs` plus focused `firmware/cache.rs`, `firmware/reducer.rs`, and `firmware/types.rs`: per-serial generation/transition leases, independent revisions, full/delta reconstruction, and reservation coordination.
- Create `crates/pandar-agent/src/machine/mqtt/firmware.rs` plus `firmware/schema.rs`, `firmware/commands.rs`, and `firmware/session.rs`: typed report parsing, exact payloads, unique clean clients, SUBACK/ordinal pump, refresh retry, and acknowledgement matching.
- Modify `crates/pandar-agent/src/machine/mqtt.rs`: reuse typed `get_version` parsing for model discovery and firmware modules; expose focused firmware modules without growing the 332-line root.
- Modify `crates/pandar-agent/src/machine/mqtt/reports.rs`, first extracting its existing diagnostic/value helpers to `mqtt/reports/diagnostics.rs`, and create `mqtt/reports/firmware.rs`: feed accepted long-stream firmware data into the sole durable reducer while preserving sibling print state. Do not touch the existing 398-line `mqtt/reports/schema.rs`.
- Modify `crates/pandar-agent/src/machine/mqtt/transport.rs` only to expose thin resolved-topic/TLS option support used by `mqtt/firmware/session.rs`; do not create a second firmware client/pump module. Leave persistent command/report clients unchanged.
- Do not grow `machine/mqtt/commands.rs` (394 LOC) or `machine/mqtt/fake.rs` (353 LOC). Put payloads in `mqtt/firmware/commands.rs` and deterministic fakes in `mqtt/firmware/tests/fake.rs`.
- Create `crates/pandar-agent/src/machine/firmware_gateway.rs`: independent `FirmwareMachineGateway` trait and request/result types so the existing main gateway trait and all of its fakes do not grow.
- Modify `crates/pandar-agent/src/machine/mod.rs` and `machine/types.rs` only for focused exports.
- Modify `crates/pandar-agent/src/machine/runtime.rs`, first extracting its existing `BambuMachineGateway` implementation to `machine/runtime/gateway.rs`, and create `machine/runtime/firmware.rs`: non-blocking endpoint lookup, generation lease, report-task replacement, and firmware coordinator ownership. Put test-only implementation in `runtime/test_support/firmware.rs`, not the 385-line root.
- Create `crates/pandar-agent/src/commands/firmware.rs`: protobuf conversion and refresh/prepare/execute handling.
- Modify `crates/pandar-agent/src/commands.rs`, `crates/pandar-agent/src/lib.rs`, and create `crates/pandar-agent/src/session_tasks.rs`: capability advertisement and a session-owned concurrent command task set that preserves existing non-firmware ordering while allowing firmware prepare/refresh to bypass blocked normal work.
- Create focused tests under `crates/pandar-agent/src/machine/firmware/tests.rs`, `machine/mqtt/firmware/tests.rs`, `machine/mqtt/firmware/tests/fake.rs`, `commands/tests/firmware.rs`, and `tests/firmware_session.rs`; wire them from existing test roots.

### Hub persistence, live lifecycle, and plugin API

- Create equivalent `20260711010000_printer_firmware.sql` migrations in `crates/pandar-hub/migrations/sqlite/` and `postgres/`.
- Modify `crates/pandar-hub/src/entities/printers.rs`: nullable typed-JSON/session/generation/cfg fields plus non-null independent revision columns defaulting to zero.
- Create `crates/pandar-hub/src/repositories/printers/firmware.rs`; modify `repositories/printers.rs` and adapters only for wiring: exact-session/generation CAS and typed hydration.
- Create focused tests under `crates/pandar-hub/src/repositories/tests/printer_firmware/{schema,cas,postgres}.rs`; wire them from test roots.
- Create `crates/pandar-hub/src/grpc/printer_firmware.rs`: protobuf validation plus authenticated invalidation/module/status/prepared/published/result handling.
- Create `crates/pandar-hub/src/repositories/commands/firmware.rs` and `repositories/commands/firmware/audit.rs`: URL-free live command records and terminal transitions; explicitly reject durable conversion/replay.
- Create `crates/pandar-hub/src/sessions/firmware.rs`: a registry owned by `SessionRegistry` (not each `AgentSession`) for process-local prepared tokens, exact session/generation entries, phase tracking, expiry tasks, one-shot waiters, cancellation, and URL scrubbing.
- Modify `crates/pandar-hub/src/sessions.rs`, `sessions/live_commands.rs`, `grpc.rs`, `grpc/inbound.rs`, `grpc/commands.rs`, `grpc/inbound/commands.rs`, `grpc/commands/conversion.rs`, `repositories/commands/transitions.rs`, and `runtime.rs` only for focused lifecycle wiring and live-only rejection.
- Create `crates/pandar-hub/src/firmware_control.rs` plus focused `firmware_control/prepare.rs`, `execute.rs`, and `refresh.rs`: service-level ownership checks and two-phase orchestration.
- Create `crates/pandar-hub/src/routes/plugin/firmware.rs` plus `firmware/types.rs`: plugin-authenticated state, refresh, prepare, and execute handlers.
- Modify `crates/pandar-hub/src/routes.rs`, `routes/plugin.rs`, and `routes/plugin/studio_devices.rs`: route wiring and current firmware in the existing batch response.
- Add focused tests under `repositories/tests/commands/firmware.rs`, `sessions/tests/firmware.rs`, `grpc/tests/firmware/{events,results,lifecycle}.rs`, `routes/tests/plugin/firmware/{state,refresh,control,redaction}.rs`, and `runtime/tests/firmware.rs`.
- Create `crates/pandar-hub/src/redaction/firmware.rs`; add only module/re-export wiring to the 372-line `redaction.rs`. Keep `repositories/printers.rs` (391), `repositories/commands.rs` (383), `routes/plugin.rs` (369), and Hub `lib.rs` (377) wiring-only, and do not touch the unrelated 399-line `routes/printer_operations.rs`.

### Rust network plugin and C++ ABI

- Modify `crates/pandar-network-plugin/Cargo.toml`: add `pandar-core.workspace = true` for Task 1 shared types.
- Create `crates/pandar-network-plugin/src/firmware.rs` with focused `firmware/model.rs`, `parser.rs`, `http.rs`, `catalog.rs`, `status.rs`, `callbacks.rs`, `session.rs`, and `ffi.rs`.
- Keep generic `src/http.rs` unchanged for firmware phase policy. `firmware/http.rs` alone owns prepare/execute classification, no-retry behavior, and URL-safe diagnostics.
- Keep `printer_refresh.rs` as the existing 750 ms batch transport. After successful refresh, C++ passes the untouched batch body to `pandar_plugin_firmware_observe_printers`; `FirmwarePluginSession` is the sole firmware cache/reset/generation owner.
- Modify `crates/pandar-network-plugin/src/studio_status/input.rs` and `studio_status/list.rs` to validate the typed batch firmware member; create `studio_status/firmware.rs` and leave the 353-line `studio_status/device.rs` wiring-only.
- Modify `crates/pandar-network-plugin/src/studio_status/request.rs`: return typed `get_version` request metadata; the live refresh itself stays in `firmware`.
- Modify `crates/pandar-network-plugin/src/lib.rs`: module declarations and flat FFI exports only; extract enough existing exports if needed to keep it below 400 LOC.
- Modify `crates/pandar-network-plugin/src/shim.cpp`: opaque firmware session handle, thin FFI calls, final per-call handoff, one callback dispatcher, one callback invocation mutex, Cloud/LAN routing, logout/destroy cancellation, and removal of hardcoded version/cfg/catalog behavior.
- Create `crates/pandar-network-plugin/tests/firmware_parser.rs`, `firmware_status.rs`, and `firmware_callbacks.rs`; extend `tests/http_boundary/` with focused firmware mock-Hub tests.
- Modify `crates/pandar-network-plugin/tests/fixtures/studio_abi_probe.cpp`; create `tests/studio_abi_probe/firmware.rs` and `tests/studio_abi_probe/mock_hub/firmware.rs`. Wire the probe module from `tests/studio_abi_probe.rs` and the mock module from its parent `tests/studio_abi_probe/mock_hub.rs`.

---

### Task 1: Add the Shared Typed Model and Additive Protobuf Contract

**Files:**

- Create: `crates/pandar-core/src/firmware.rs`
- Modify: `crates/pandar-core/src/lib.rs`
- Create: `crates/pandar-core/src/tests/firmware.rs`
- Modify: `crates/pandar-core/src/tests.rs`
- Modify: `proto/pandar/agent/v1/agent.proto`
- Create/modify focused generated-wire tests in Agent and Hub protocol test modules

**Interfaces:**

- Produces `PrinterFirmwareModule`, `PrinterFirmwareVersion`, `AmsFirmwareDescriptor`, `AmsFirmwareSwitchState`, `PrinterUpgradeState`, `PrinterFirmwareState`, `FirmwareCatalogEntry`, `FirmwareCatalogTarget`, `FirmwareControlMetadata`, `FirmwareCommand`, `FirmwareAcknowledgement`, and `FirmwareTerminalOutcome`.
- Produces `AgentCapability::FirmwareControl = 5`.
- Produces exact Agent event oneof tags: modules snapshot `18`, status snapshot `19`, invalidated `20`, prepared `21`, and published `22`.
- Produces exact Hub command oneof tags: refresh version `18`, prepare firmware control `19`, and execute firmware control `20`.
- Extends `CommandResult` with `optional FirmwareCommandResult firmware_result = 5`; refresh results carry both generation and module revision. Prepared, published, and terminal results all carry command id, serial, and generation.
- Consumes no behavior from later tasks.

- [ ] **Step 1: Add failing shared-model and wire tests**

Test all exact JSON keys and presence rules:

```rust
let module = PrinterFirmwareModule {
    name: "n3s/0".into(),
    software_version: Some("01.02.03.04".into()),
    software_new_version: Some("01.02.04.00".into()),
    new_version: Some("01.02.05.00".into()),
    visible: Some(false),
    product_name: Some("AMS HT".into()),
    serial_number: Some("AMS-HT-SN".into()),
    hardware_version: Some("N3S".into()),
    firmware_flag: Some(5),
};
```

Assert serde emits/accepts `sw_ver`, `sw_new_ver`, `new_ver`, `visible`, `product_name`, `sn`,
`hw_ver`, and `flag` without conflation. Assert `progress` remains a string. Assert absent,
present-empty, explicit zero, false, and empty string survive round trips for every upgrade field,
`new_ver_list`, and nested `mc_for_ams_firmware.firmware`.

Add protobuf round-trip tests with duplicate ordered modules, wrapper-present empty collections,
negative AMS ids, optional cfg, generation/revisions, all command variants, URL-free acknowledgement,
and refresh `module_revision`. Assert `FirmwareCommand::Start` has a hand-written URL-hiding `Debug`
and is not serializable, while its URL-free metadata is serializable. Keep existing serialized legacy
fixtures byte-identical. Round-trip a printer rejection carrying every acknowledgement field through
core JSON and protobuf without loss.

- [ ] **Step 2: Run the focused tests and prove RED**

```powershell
cargo nextest run -p pandar-core -E 'test(~firmware)'
cargo nextest run -p pandar-agent -E 'test(~firmware_wire) | test(hello_event_has_agent_identity_version_and_exact_capability)'
```

Expected: compile/test failures because the shared types, capability 5, and additive messages do not
exist. Record the exact failures.

- [ ] **Step 3: Implement the shared types**

Use typed serde renames, preserving wire keys. Define the list records exactly:

```rust
pub struct PrinterFirmwareVersion {
    pub name: String,
    pub current_version: Option<String>, // cur_ver
    pub new_version: Option<String>,     // new_ver
}

pub struct AmsFirmwareDescriptor {
    pub id: i32,
    pub name: String,
    pub version: String,
}

pub struct AmsFirmwareSwitchState {
    pub firmware: Option<Vec<AmsFirmwareDescriptor>>,
    pub current_firmware_id: Option<i32>,
    pub current_run_firmware_id: Option<i32>,
    pub status: Option<String>,
}
```

Make `FirmwareCommand` a closed enum without `Serialize`. `Start` owns its URL only in transient
instances; hand-write `Debug` so the URL is always `[redacted]`.
`FirmwareControlMetadata::from(&FirmwareCommand)` deliberately excludes the URL and is serializable.
Keep acknowledgements and outcomes typed, serializable, and URL-free. Define the exact
acknowledgement shape so real printer rejection details survive every boundary:

```rust
pub struct FirmwareAcknowledgement {
    pub command: String,
    pub sequence_id: String,
    pub result: Option<String>,
    pub error_code: Option<i64>, // MQTT key: err_code
    pub reason: Option<String>,
    pub message: Option<String>,
}
```

Do not collapse these into one generic error string. Define status presence as:

```rust
pub struct PrinterFirmwareStatus {
    pub upgrade_state: Option<PrinterUpgradeState>,
    pub cfg: Option<String>,
}
```

Use `Option<Vec<T>>`, never `#[serde(default)] Vec<T>`, where absent and present-empty differ.

- [ ] **Step 4: Add the protobuf contract without changing existing tags**

Use proto3 `optional` for scalar presence and wrapper messages for optional repeated lists. Keep
ordered module records as repeated fields. The execute oneof contains the closed command variants;
prepare contains only command id, serial, and expected generation. Reject a
`PrepareFirmwareControl.command_id` or `ExecuteFirmwareControl.command_id` wrapper value that does
not equal outer `HubCommand.command_id`. The typed terminal result distinguishes:

- refreshed modules plus `module_revision`;
- exact URL-free top-level upgrade acknowledgement;
- published without acknowledgement;
- optional transient upgrade state/cfg for the initiating callback only.

A failed `CommandResult` before any `FirmwarePublished` event represents Agent pre-publish failure;
do not add a duplicate protobuf phase enum. Hub derives the HTTP phase from its pending registry.

Do not represent a package URL in any telemetry or generic result-json string.

- [ ] **Step 5: Run Task 1 verification**

```powershell
cargo fmt --all -- --check
cargo nextest run -p pandar-core -E 'test(~firmware)'
cargo nextest run -p pandar-agent -E 'test(~firmware_wire) | test(hello_event_has_agent_identity_version_and_exact_capability)'
cargo nextest run -p pandar-hub -E 'test(~firmware_wire)'
cargo clippy -p pandar-core -p pandar-agent -p pandar-hub --all-targets -- -D warnings
cargo nextest run -p pandar-core --test module_size
```

- [ ] **Step 6: Independent Task 1 review gate**

Give a fresh reviewer the Task 1 spec/plan excerpt, complete Task 1 diff including intent-to-add
files, RED/GREEN output, and LOC report. Require literal `VERDICT: APPROVE` for spec compliance and
quality. Revise with new failing tests and re-review until approved.

- [ ] **Step 7: Record evidence without committing**

Append test counts, RED reason, changed paths, reviewer verdict, and LOC evidence to
`.superpowers/sdd/progress.md`. Do not stage, commit, or push.

---

### Task 2: Implement Agent Firmware Observation, Reconstruction, and Generations

**Files:** Agent observation/cache/reducer files listed above, plus focused tests.

**Interfaces:**

- Produces `FirmwareObservationCache` keyed by serial, with a per-serial transition lease, current
  endpoint, generation, module revision, status revision, ordered typed modules/status/cfg, and
  reservation state. The raw reconstructed `Value` is never shared.
- Produces invalidated/modules/status Agent events.
- Reuses the current startup `get_version` request for model discovery and ordered firmware modules.
- Consumes Task 1 types/protobuf only; no firmware control behavior yet and capability 5 remains
  unadvertised until Task 4 completes.

- [ ] **Step 1: Add failing parser/reducer/cache tests**

Use real MQTT-shaped fixtures for `ota`, `ams/0`, `n3f/*`, `n3s/*`, duplicate names, and an unknown
future module. Test exact `hw_ver`, `flag`, `sw_new_ver`, `visible`, and `new_ver` retention.

Test reducer behavior:

- `msg = 1` deep-merges into the prior reconstructed printer object before typed extraction;
- a delta-absent `new_ver_list` preserves the prior list;
- `msg = 0` and legacy no-`msg` replace the object;
- full absent list stays absent, present empty stays present empty;
- malformed firmware fields yield a firmware-scoped error but valid sibling snapshot/job/material
  parsing still succeeds;
- a malformed delta merges/parses on a clone and does not poison the next report;
- a pure `info.get_version` report cannot clear status, and firmware-only reports cannot synthesize
  an empty `PrinterSnapshot` or `PrintJobReport`;
- arrays replace as units during recursive delta merge;
- status revision advances only when typed upgrade state or cfg actually changes;
- only the long-lived report reducer assigns status revisions.

Test reconnect and endpoint replacement establish invalidation first. Exercise late-old-before-new
and new-before-late-old event orderings and lower/equal revision rejection.
At the authenticated protobuf boundary, test generation and both revisions at `i64::MAX` are
accepted and `i64::MAX + 1` are rejected before repository conversion on both database backends.

- [ ] **Step 2: Prove RED**

```powershell
cargo nextest run -p pandar-agent -E 'test(~firmware_observation) | test(~firmware_reducer) | test(~firmware_generation)'
```

- [ ] **Step 3: Implement typed module/status parsing and the reducer**

Extract all known fields through typed structs. Give each long-lived report producer/generation its
own local reducer holding the raw reconstructed printer `Value`; the shared cache receives only a
successfully parsed typed replacement tagged with that producer's generation. Merge and parse on a
clone, then commit locally. A parse error for `upgrade_state` is logged with its cause and does not
prevent normal report forwarding or poison later reduction.

Change model discovery to return:

```rust
pub struct FirmwareVersionObservation {
    pub model: String,
    pub modules: Vec<PrinterFirmwareModule>,
}

pub struct FirmwareModulesObservation {
    pub serial: String,
    pub generation: u64,
    pub revision: u64,
    pub modules: Vec<PrinterFirmwareModule>,
}
```

Route every startup and later version observation through one per-serial serialization lock; each
Studio request still performs its own fresh query rather than sharing a cached first result.
Different serials remain concurrent. Allocate the module revision only after successful completion
and generation validation.

- [ ] **Step 4: Implement generation and report-task linearization**

Under the per-serial transition lease:

1. allocate the next process generation;
2. atomically install the endpoint for that generation and clear typed firmware/reservations;
3. clear revisions;
4. enqueue `PrinterFirmwareInvalidated`;
5. start/replace the report producer with its own local reducer bound to that generation;
6. permit snapshots and future firmware leases for that generation.

An old producer may finish, but its captured generation cannot mutate or emit as the current
producer. Endpoint replacement uses the same lease. A link-validation module observation commits
only after the new endpoint generation is established and still current. Do not alter unrelated
printer state columns or emit a firmware-only report as a synthetic full printer snapshot.

- [ ] **Step 5: Run Task 2 verification**

```powershell
cargo fmt --all -- --check
cargo nextest run -p pandar-agent -E 'test(~firmware_observation) | test(~firmware_reducer) | test(~firmware_generation) | test(~runtime_report)'
cargo nextest run -p pandar-agent
cargo clippy -p pandar-agent --all-targets -- -D warnings
cargo nextest run -p pandar-core --test module_size
```

- [ ] **Step 6: Independent Task 2 review and evidence gate**

Review the entire Task 2 diff against the design's merge/presence/generation rules. Require literal
approval, fix via RED→GREEN, then update progress without committing.

---

### Task 3: Persist and Expose Current Firmware Telemetry in Hub

**Files:** Hub migrations/entity/repository/inbound event/batch response files listed above.

**Interfaces:**

- Produces `PrinterFirmwareUpdateOutcome::{Applied, Stale}` and repository operations:

```rust
pub async fn establish_generation_if_current(
    &self, tenant_id: TenantId, agent_id: AgentId, session_id: &str,
    serial: &str, generation: u64,
) -> RepositoryResult<PrinterFirmwareUpdateOutcome>;

pub async fn replace_modules_if_current(
    &self, tenant_id: TenantId, agent_id: AgentId, session_id: &str,
    serial: &str, generation: u64, revision: u64,
    modules: Vec<PrinterFirmwareModule>,
) -> RepositoryResult<PrinterFirmwareUpdateOutcome>;

pub async fn replace_status_if_current(
    &self, tenant_id: TenantId, agent_id: AgentId, session_id: &str,
    serial: &str, generation: u64, revision: u64,
    state: Option<PrinterUpgradeState>, cfg: Option<String>,
) -> RepositoryResult<PrinterFirmwareUpdateOutcome>;
```

- Produces hydrated `PrinterFirmwareState` with authenticated session id, optional generation,
  independent revisions, `Option<Vec<PrinterFirmwareModule>>` modules, upgrade state, and cfg. Null
  modules mean never observed; `Some(vec![])` means an intentional present-empty observation.
- Extends `/api/v1/plugin/printers` device entries with an optional typed `firmware` object only when
  exact current session/generation/capability rules pass.
- Consumes Task 2 Agent events. It does not yet expose live refresh/control routes.

- [ ] **Step 1: Add failing migration/repository/inbound tests**

Add nullable `firmware_modules_json`, `firmware_upgrade_state_json`, `firmware_cfg`,
`firmware_session_id`, and `firmware_generation`. Add non-null
`firmware_module_revision` and `firmware_status_revision` with default zero and nonnegative checks.
Normalize only PostgreSQL `BIGINT` versus SQLite `INTEGER` in migration parity tests. Test:

- create/hydrate with absent firmware;
- first invalidation in a new authenticated session accepts that session's generation;
- later same-session invalidation must be strictly newer;
- module/status replacement requires exact session, exact generation, and strictly newer respective
  revision;
- malformed stored typed JSON returns the full parse cause;
- invalidation clears only firmware fields and resets revisions;
- every unrelated printer field remains byte-for-byte unchanged;
- SQLite and PostgreSQL behavior are equivalent.

Add inbound tests proving Agent-supplied session ids are ignored; the authenticated reverse-stream
session marker is used.

- [ ] **Step 2: Prove RED**

```powershell
cargo nextest run -p pandar-hub -E 'test(~printer_firmware) | test(~firmware_event) | test(~firmware_migration)'
```

- [ ] **Step 3: Implement schema and repository CAS**

Use backend-neutral repository methods and explicit backend adapters only where SQL syntax differs.
Call `begin_current_agent_transaction(...)` first, then lock the printer row to preserve the
existing Agent-to-printer lock order. Return `Stale` for mismatched session/generation/revision
rather than silently applying.

- [ ] **Step 4: Implement authenticated inbound event handling**

Wire invalidated/modules/status events through `grpc/printer_firmware.rs`. Persistence failure keeps
the complete cause. Stale old-session/generation events are ignored as stale, not applied or
re-emitted. Modules replace only modules; status replaces only upgrade state/cfg.
Validate every protobuf `u64` generation/revision is at most `i64::MAX`, then use checked conversion
before repository calls; never wrap into signed storage.

- [ ] **Step 5: Extend the batch Studio-printer response**

Hydrate firmware from the already-loaded `printers::Model` into `PrinterWithLiveStatus` (or use one
bulk query); do not introduce an N-per-printer database query. For each printer, resolve the current
local session with capability 5. Emit `firmware: Option<PrinterFirmwareState>` only when the
persisted firmware session id equals that token and a generation is established. Include session
marker, generation, revisions, optional modules/state/cfg. Otherwise emit `None`; the plugin's sole
cache owner detects a previously-current device becoming invalid. Never expose stale state.

- [ ] **Step 6: Run Task 3 verification**

```powershell
cargo fmt --all -- --check
cargo nextest run -p pandar-hub -E 'test(~printer_firmware) | test(~firmware_event) | test(~firmware_migration) | test(~plugin_firmware_batch)'
cargo nextest run -p pandar-hub
cargo clippy -p pandar-hub --all-targets -- -D warnings
cargo nextest run -p pandar-core --test module_size
```

If `$env:PANDAR_TEST_POSTGRES_URL` is set, run the focused real-PostgreSQL firmware tests serially.
Otherwise record exactly: `PANDAR_TEST_POSTGRES_URL is unset; real PostgreSQL firmware verification skipped`.

```powershell
cargo nextest run -p pandar-hub -E 'test(~firmware) & test(~postgres)' --test-threads=1
```

- [ ] **Step 7: Independent Task 3 review and evidence gate**

Require literal approval for migration parity, CAS semantics, stale ordering, unrelated-column
preservation, and batch exposure. Fix and re-review; do not commit.

---

### Task 4: Implement Agent Fresh Refresh and Two-Phase Firmware Control

**Files:** Agent firmware command/session/runtime/transport files listed above.

**Interfaces:**

- Produces the independent Agent interface:

```rust
#[async_trait]
pub trait FirmwareMachineGateway: Send + Sync {
    async fn refresh_firmware_version(
        &self,
        request: FirmwareRefreshRequest,
    ) -> anyhow::Result<FirmwareModulesObservation>;

    async fn prepare_firmware_control(
        &self,
        request: FirmwarePrepareRequest,
    ) -> anyhow::Result<FirmwarePreparedObservation>;

    async fn execute_firmware_control(
        &self,
        request: FirmwareExecuteRequest,
        phases: mpsc::UnboundedSender<FirmwareControlPhase>,
    ) -> anyhow::Result<FirmwareControlOutcome>;

    async fn cancel_firmware_session(&self, session_epoch: u64);
}
```

- Produces `FirmwareMqttSession` with a background receive pump, SUBACK completion, monotonic receive
  ordinals, serialized barrier/publish, `Outgoing::Publish` observation, a unique bounded client id,
  and explicit shutdown/join.
- Produces URL-redacted Agent results and advertised capability 5 only after all handlers exist.
- Consumes Task 2 generation leases and Task 1 protobuf commands.

- [ ] **Step 1: Add failing exact-payload and fresh-session tests**

For all commands, assert byte-equivalent nested MQTT JSON and exact Studio sequence/src values.
For `start`, assert exact URL/module/version reach the fake MQTT publish once and that logs/results do
not contain the unique sentinel URL.

Test every attempt creates a different client id and receives SUBACK. Pause the pump around the
barrier to prove no report can interleave between barrier establishment and publish enqueue. Reject
a pre-barrier queued match and post-barrier wrong-command or wrong-sequence report. Do not mark
published or accept a match until the pump observes the command's own `Outgoing::Publish`; then
accept only the exact command+sequence acknowledgement.
Assert persistent command/report clients remain connected.

Add a production-boundary TCP loopback broker test using the real `Rumqttc` `AsyncClient/EventLoop`,
not only the fake pump. Prove clean-session CONNECT, SUBSCRIBE/SUBACK before publish, the exact wire
payload, own `Outgoing::Publish` phase, pre-barrier and wrong-response rejection, correct matching,
cancellation/join, unique client ids, 256 KiB packet options, and no disconnect of the persistent
command/report client ids.

Parse an acknowledgement containing exact `command`, `sequence_id`, `result`, `err_code`, `reason`,
and `message`, including printer rejection, and assert no field is lost.

- [ ] **Step 2: Add failing coordinator/lifecycle tests**

Test:

- refresh performs at most three attempts, returns exact modules plus assigned module revision, and
  never returns successful cached/empty data after all attempts fail;
- same-printer refreshes serialize but each Studio request issues its own fresh query; different
  printers run concurrently;
- prepare contains no action/URL and succeeds only after reserving exact generation;
- same-printer busy, stale generation, ending session, and one-second expiry fail before publish;
- execute claims the exact reservation, reacquires transition lease, rechecks generation immediately
  before publish, and holds an in-flight execution lease until terminal acknowledgement,
  published-without-acknowledgement, or cancellation;
- while one same-printer execute waits for acknowledgement, a second prepare remains busy;
- acknowledged, printer-rejected, and two-second published-without-acknowledgement terminal results;
- transient command-client upgrade state never changes the long-stream cache;
- a blocked normal Agent command does not delay reading/handling firmware prepare;
- a new local reverse-session epoch cannot claim an older epoch's preparation even when generation
  matches;
- stream end/replacement/shutdown cancels and joins all firmware tasks, then clears that epoch's
  prepared/in-flight state, then clears the sender, and cannot emit into a later session.

- [ ] **Step 3: Prove RED**

```powershell
cargo nextest run -p pandar-agent -E 'test(~firmware_refresh) | test(~firmware_control) | test(~firmware_session) | test(~firmware_mqtt)'
```

- [ ] **Step 4: Implement the clean firmware MQTT session**

Create the client id from `pandar-agent-fw`, a bounded serial-derived component, and a UUID. The
event-loop pump assigns an ordinal to every received publish and completes the subscribe waiter on
SUBACK. Give the pump one request that atomically establishes the current ordinal barrier and
enqueues the publish, then waits until the event loop observes this request's own
`rumqttc::Outgoing::Publish`. `AsyncClient::publish().await` alone means only queued, so errors before
that outgoing event are pre-publish; failures after it are outcome unknown. Shutdown cancels and
joins the pump. Construct fresh options through the same helper that sets the existing 256 KiB
inbound/outbound maximum packet size and clean session; never use unbounded defaults.

Do not claim to eliminate a delayed old acknowledgement that arrives after publish with the same
command and reused sequence. Document that wire ambiguity in the module and test only the stated
pre-barrier and command-plus-sequence guarantees.

- [ ] **Step 5: Implement refresh and control coordinators**

Refresh owns up to three publish/wait attempts inside one bounded command result and assigns module
revision in completion order. Control stores prepared reservations without action/URL, bound to the
local reverse-session epoch plus printer generation, expires them after one second, and accepts
execute only for the same command/session/generation reservation.

`claim_execute()` converts prepared state to a `FirmwareExecutionLease` that remains in-flight until
the operation reaches a terminal outcome or session/generation cancellation. Its `Drop` removes
only the still-matching command/epoch/generation entry.

Immediately after the pump observes the command's own outgoing publish, send `FirmwarePublished`;
then wait at most two seconds for the typed top-level acknowledgement. Redact the exact pending URL
from every error before it can
leave Agent process memory. Return optional transient state separately and never call the durable
status reducer from a command client.

- [ ] **Step 6: Make the reverse-session task set concurrent and owned**

Create a local reverse-session epoch in every `run_once`. Change the stream handler to receive an
`Arc<G>` constrained by `BambuMachineGateway + FirmwareMachineGateway`. Continuously read the Hub
stream while tasks run. Preserve existing non-firmware ordering with a session-local normal-command
mutex, but spawn firmware work outside it. Firmware endpoint/generation lookup comes from the
firmware cache and never waits on `RuntimeBambuMachineGateway.inner`, whose mutex may span normal
printer I/O. Endpoint replacement updates endpoint, generation, invalidation, and reservations under
one transition lease. On stream end, abort/cancel and join the task set, cancel that epoch, then clear
the sender. Own and join the heartbeat task too. Advertise capability 5 only now.

- [ ] **Step 7: Run Task 4 verification**

```powershell
cargo fmt --all -- --check
cargo nextest run -p pandar-agent -E 'test(~firmware_refresh) | test(~firmware_control) | test(~firmware_session) | test(~firmware_mqtt)'
cargo nextest run -p pandar-agent
cargo clippy -p pandar-agent --all-targets -- -D warnings
cargo nextest run -p pandar-core --test module_size
```

- [ ] **Step 8: Independent Task 4 review and evidence gate**

Require literal approval for publish safety, cancellation, retry bounds, barrier semantics, URL
redaction, and capability timing. Fix via failing tests and re-review; do not commit.

---

### Task 5: Implement Hub Live Firmware Command Ownership and Result Lifecycle

**Files:** Hub command repository, sessions firmware registry, firmware control service, gRPC inbound/result files, redaction, and focused tests.

**Interfaces:**

- Produces URL-free durable `firmware_refresh` and `firmware_control` command records.
- Produces a process-local `PendingFirmwareCommands` registry owned once by `SessionRegistry` (not
  embedded in `AgentSession`), keyed by command id and opaque one-use prepared token, storing exact
  tenant/agent/session/generation, URL-free metadata, optional transient URL only after execute
  claim, phase, expiry, and one-shot waiters.
- Produces internal `prepare_control`, `execute_control`, and `refresh_version` service methods for
  Task 6 routes.
- Consumes Task 4 Agent events/results.

- [ ] **Step 1: Add failing URL-free persistence and non-replay tests**

Persist every action's metadata and audit information, then assert a unique signed-URL sentinel is
absent from command payload/result JSON, audit readback, logs, errors, and API readback. Assert
durable command conversion rejects both firmware kinds and startup/replacement pumps never dispatch
them. A Hub restart must not reconstruct a prepared token or URL.

- [ ] **Step 2: Add failing exact waiter/phase tests**

Test exact current session and capability resolution under the transition boundary; non-owner Hub,
stale session, replacement during prepare, generation invalidation, disconnect, channel full/closed,
shutdown, and late results. A waiter resolves only for exact command/session/generation.

Test phases:

- prepare timeout/busy/stale => safe pre-publish failure, no execute;
- prepared token is one-use and expires even when the plugin vanishes and makes no later request;
- a prepared-never-executed timer removes only that matching command/token/waiter and fails its
  URL-free durable command; it relies on Agent's own command-id-scoped one-second reservation expiry
  and must not cancel unrelated entries in the same session/generation;
- expiry of one prepared entry cannot cancel a different same-generation reservation or in-flight
  command;
- execute atomically claims token and records `ExecuteSent` before live dispatch;
- Agent pre-publish terminal failure remains explicit pre-publish failure;
- `FirmwarePublished` advances phase;
- acknowledgement/rejection/published-without-ack resolves URL-free;
- cancellation after execute may have reached Agent => outcome unknown, never a retry suggestion;
- late old result cannot resolve a new waiter or mutate a new generation.

- [ ] **Step 3: Prove RED**

```powershell
cargo nextest run -p pandar-hub -E 'test(~firmware_command) | test(~firmware_lifecycle) | test(~firmware_redaction)'
```

- [ ] **Step 4: Implement URL-free command records and live-only rejection**

Create command/audit payloads from `FirmwareControlMetadata`, never `FirmwareCommand::Start.url`.
Add dedicated terminal transition methods that accept only typed URL-free outcomes. Explicitly reject
firmware kinds in generic protobuf conversion, queued fallback, startup cleanup dispatch, and late
generic result handling.

Wire every lifecycle surface explicitly:

- `grpc.rs`: replacement cleanup;
- `grpc/inbound.rs`: stream-close cleanup and firmware event dispatch;
- `runtime.rs`: stale-session/control-plane-close cleanup and pending-id ownership;
- `sessions/live_commands.rs`: explicit close cleanup;
- `repositories/commands/transitions.rs`: both firmware kinds as live-only stale candidates;
- `grpc/commands/conversion.rs`: durable conversion rejection;
- `grpc/inbound/commands.rs`: typed firmware result handling before generic result handling and
  `durable_fallback_allowed = false` for both firmware kinds.

- [ ] **Step 5: Implement the process-local registry and service**

Implement `begin_firmware_refresh`, `begin_firmware_prepare`, `complete_firmware_prepared`,
`begin_firmware_execute`, `mark_firmware_published`, `claim_firmware_result`,
`cancel_firmware_generation`, `cancel_firmware_session`, and `expire_firmware_prepare` on the
registry. Every insert/claim/cancel runs under `transition_lease_for_session`.

Scope that transition lease only across validation, registry mutation, and exact-session dispatch.
Drop it before awaiting `FirmwarePrepared` or any terminal one-shot; inbound completion reacquires
the same lease. Add a test using the real shared transition lock that completes normally rather than
timing out, proving no outbound waiter holds the lease needed by inbound claiming.

Prepare resolves the exact local session/capability/generation, writes the URL-free record, inserts
the exact pending entry/waiter, starts a real one-second expiry task, sends URL-free prepare, and
waits at most one second. Execute compares
the full command's URL-free projection with prepared metadata, claims the token once, stores the URL
only in memory, and sends execute exactly once.

Inbound `FirmwarePrepared`, `FirmwarePublished`, and typed terminal results must claim the same entry
under the session transition boundary before changing phase, persisting result, applying refresh
module CAS, or resolving a waiter. `grpc/printer_firmware.rs` owns protobuf-to-core validation and all
firmware events/results. A refresh terminal result must successfully apply its generation/revision
module CAS before its HTTP waiter resolves. Never persist generic `result_json` for firmware; encode
only the typed URL-free outcome through dedicated transitions. Scrub the URL before every
removal/drop/log path.

- [ ] **Step 6: Run Task 5 verification**

```powershell
cargo fmt --all -- --check
cargo nextest run -p pandar-hub -E 'test(~firmware_command) | test(~firmware_lifecycle) | test(~firmware_redaction)'
cargo nextest run -p pandar-hub
cargo clippy -p pandar-hub --all-targets -- -D warnings
cargo nextest run -p pandar-core --test module_size
```

Run/skip real PostgreSQL exactly as in Task 3.

- [ ] **Step 7: Independent Task 5 review and evidence gate**

Require literal approval for at-most-once execute, phase safety, non-replay, exact waiter claims,
one-active-Hub behavior, and URL absence. Fix/re-review; do not commit.

---

### Task 6: Add Hub Plugin Firmware State, Refresh, Prepare, and Execute APIs

**Files:** Hub plugin firmware route/types, routes wiring, batch response integration tests.

**Interfaces:**

- `GET /api/v1/plugin/printers/{printer_id}/firmware`: current typed state and typed catalog; catalog initially empty.
- `POST /api/v1/plugin/printers/{printer_id}/firmware/refresh`: `{ "sequence_id": "..." }` and a fresh typed version result.
- `POST /api/v1/plugin/printers/{printer_id}/firmware/prepare`: URL-free `FirmwareControlMetadata`; returns `{command_id, prepared_token}`.
- `POST /api/v1/plugin/printers/{printer_id}/firmware/execute`: one-use token plus full typed `FirmwareCommand`; returns a typed URL-free phase/outcome and optional transient status.
- Existing `/api/v1/plugin/printers` entries gain exactly
  `firmware: Option<PrinterFirmwareState>` with session marker, generation, independent revisions,
  optional modules, optional upgrade state, and optional cfg.
- All routes use plugin Studio auth and exact tenant/printer/Agent ownership.

- [ ] **Step 1: Add failing route boundary tests**

Cover auth, tenant isolation, missing printer, wrong Agent owner, non-capable/current/stale Agent,
non-owning Hub replica, generation replacement during each phase, malformed JSON, wrong types, empty
required strings, execute metadata mismatch, and the existing 64 KiB request-body boundary. Confirm
an oversized sequence/module/version/URL body is rejected before service dispatch or Agent contact,
without adding field-specific size limits or extra model/state/url-host restrictions.

For refresh, assert a fresh live command, exact Studio sequence, module revision CAS before HTTP
success, and typed failure after Agent's bounded attempts. Never return an empty successful cached
response.

- [ ] **Step 2: Add failing ambiguous/persistence fault tests**

Inject failure before prepare persistence, after prepare dispatch, before execute dispatch, after
execute dispatch, after Agent publish, and during terminal persistence. Responses must carry an
explicit phase when known; after execute may have been attempted, never downgrade ambiguity to a
safe retry response.

- [ ] **Step 3: Prove RED**

```powershell
cargo nextest run -p pandar-hub -E 'test(~plugin_firmware) | test(~firmware_refresh_route) | test(~firmware_prepare) | test(~firmware_execute)'
```

- [ ] **Step 4: Implement route DTOs and handlers**

Keep route DTOs typed and `deny_unknown_fields` only where existing plugin boundary conventions use
it; Studio command forward compatibility is handled in the plugin parser, not by accepting arbitrary
Hub commands. Map stable unavailable/pre-publish/unknown outcomes without exposing causes or URL.

One helper produces the exact-current firmware projection used by both direct state and batch routes;
it compares capability 5 plus persisted session id/generation. The state response includes current
modules, optional upgrade state/cfg, and typed catalog records. Its default catalog is an empty list.

- [ ] **Step 5: Run Task 6 verification**

```powershell
cargo fmt --all -- --check
cargo nextest run -p pandar-hub -E 'test(~plugin_firmware) | test(~firmware_refresh_route) | test(~firmware_prepare) | test(~firmware_execute)'
cargo nextest run -p pandar-hub
cargo clippy -p pandar-hub --all-targets -- -D warnings
cargo nextest run -p pandar-core --test module_size
```

- [ ] **Step 6: Independent Task 6 review and evidence gate**

Require literal approval for boundary validation, auth/ownership, phase classification, fresh-read
semantics, batch parity, and URL-free output. Fix/re-review; do not commit.

---

### Task 7: Implement Rust Plugin Firmware Parsing, HTTP, Status, Catalog, and Callback Queue

**Files:** Rust network-plugin firmware modules, printer refresh/status/http integrations, focused tests.

**Interfaces:**

- Produces `StudioFirmwareParse::{NotFirmware, Firmware(StudioFirmwareCommand), InvalidFirmware}`
  through a presence-preserving top-level `upgrade` wrapper.
- Produces a per-plugin-generation `FirmwarePluginSession` containing Hub credentials, typed cache,
  invalidation/reset lifecycle, HTTP state, pending callback tokens, and stop-aware callback queue.
  It is the sole firmware generation/cache authority; `PrinterRefreshSession` owns only batch HTTP.
- Produces flat FFI for session create/update/cancel/destroy, catalog fetch, live version refresh,
  firmware send, originating-call handoff, and next-ready callback retrieval.
- Consumes Task 6 HTTP endpoints; C++ wiring is Task 8.

```rust
enum StudioFirmwareParse {
    NotFirmware,
    Firmware(StudioFirmwareCommand),
    InvalidFirmware,
}

enum StudioFirmwareCommand {
    UpgradeConfirm { sequence_id: String, src_id: i64 },
    ConsistencyConfirm { sequence_id: String, src_id: i64 },
    Start {
        sequence_id: String,
        src_id: i64,
        url: String,
        module: String,
        version: String,
    },
    McForAmsFirmwareUpgrade { sequence_id: String, src_id: i64, id: i32 },
}

enum FirmwareSendOutcome {
    Acknowledged,
    Rejected,
    PublishedWithoutAcknowledgement,
    OutcomeUnknown,
    PrePublishFailure,
}
```

- [ ] **Step 1: Add failing typed parser tests**

Accept only a top-level `upgrade` object with one exact known command, string sequence id, numeric
`src_id`, and variant fields. Tolerate unknown sibling fields. An absent top-level `upgrade` is
`NotFirmware`; a present `null`, wrong shape, missing/wrong field, unknown command, empty `start`
URL/module/version, or AMS id outside signed printer range is `InvalidFirmware`. Assert every
accepted field remains exact. Firmware parsing must precede the existing semantic/G-code parser;
non-firmware inputs preserve current behavior.

Exercise the existing plugin input/body ceiling with over-limit sequence and all `start` string
fields; reject before Hub HTTP while accepting values just inside the shared boundary. Do not add
firmware-specific length or URL-host policy.

Do not derive `Debug` for `StudioFirmwareCommand`. Hand-write URL-hiding formatting for its `Start`
variant (or reuse a shared non-serializable redacted URL wrapper) and add this boundary regression:

```rust
let command = StudioFirmwareCommand::Start {
    sequence_id: "9001".into(),
    src_id: 1,
    url: "https://user:secret@example.invalid/fw.bin?sig=SENTINEL".into(),
    module: "ota".into(),
    version: "01.02.03.04".into(),
};
assert!(!format!("{command:?}").contains("SENTINEL"));
```

- [ ] **Step 2: Add failing HTTP and ambiguity tests**

Assert prepare is URL-free, execute contains the exact URL once, and execute is never retried. Cut
the mock HTTP connection:

- before prepare completes => ABI failure, safely pre-publish;
- after a prepared token exists and before/after Hub receives execute => ABI success,
  `firmware_outcome_unknown`, no callback, no retry;
- typed explicit execute `pre_publish_failure` => ABI failure;
- matching acknowledgement/rejection => ABI success plus delayed typed callback token;
- published-without-ack => ABI success without callback.

Capture stderr/diagnostics and assert the URL sentinel and query credentials never appear.

- [ ] **Step 3: Add failing status/catalog/reset tests**

Test exact upgrade-state keys and presence, progress string, cfg, complete AMS switch structure,
ordered modules, and catalog envelope:

```json
{ "devices": [{ "dev_id": "SERIAL", "firmware": [], "ams": [] }] }
```

Inject typed real main and AMS-target records and prove only records with non-empty real URLs become
selectable entries; do not add filename/URL-structure/host policy. All AMS-family targets map to the
first `ams[].firmware[]` collection.

Start with fully populated upgrading/force/consistency/AMS `SWITCHING` state, transition to invalid,
and assert the exact design reset JSON clears every scalar/list/nested AMS status. With a controllable
clock, assert immediate reset and repetition through at least one emission after three seconds,
then stop; fresh current state cancels reset repetition.

- [ ] **Step 4: Add failing callback queue tests**

Create two unrelated originating tokens. Delay one handoff and overlap the other submission. Assert
each token is ineligible before its own handoff, becomes eligible only at its own handoff +1.1s,
expires at +2s, preserves dev id/tunnel/body, and cannot be moved by the other token. Logout/session
generation change cancels pending entries. Destroy wakes blocked consumers and joins cleanly.

- [ ] **Step 5: Prove RED**

```powershell
cargo nextest run -p pandar-network-plugin -E 'test(~firmware_parser) | test(~firmware_http) | test(~firmware_status) | test(~firmware_callback)'
```

- [ ] **Step 6: Implement parser and prepare/execute HTTP client**

Project `FirmwareCommand` to URL-free prepare metadata. Once prepare returns a token, call execute
exactly once. Classify only a decoded typed `pre_publish_failure` as safe failure. Any connection,
decode, 5xx, or persistence ambiguity after the execute attempt begins becomes local URL-free
outcome unknown and ABI success.

- [ ] **Step 7: Implement current firmware presentation and catalog**

Keep `PrinterRefreshSession` unchanged as the 750 ms per-request-timeout batch transport; C++ still
invokes it on its existing two-second heartbeat cadence. After a successful batch,
C++ gives the untouched body to `pandar_plugin_firmware_observe_printers`; typed validation in
`studio_status/list.rs` rejects malformed firmware members. `FirmwarePluginSession` alone updates
per-dev current/reset state. `studio_status/firmware.rs` renders exact current state or reset and
removes the shim's hardcoded cfg. C++ never parses firmware JSON.

Fresh `info.get_version` calls the live refresh endpoint and renders exact module fields and original
sequence. Failure renders typed `info.command = get_version`, original sequence, `result = fail`, and
an empty module list only as an explicit failure, never as success.

- [ ] **Step 8: Implement the per-origin callback queue and flat FFI**

Use an opaque `u64` return token. `pandar_plugin_firmware_return_handoff(session, token,
origin_monotonic_tick)` records the C++ tick for correlation and anchors Rust `Instant::now()` as the
handoff instant. Store not-before/deadline as +1.1s/+2s. A stop-aware blocking
`pandar_plugin_firmware_next_callback` returns separate pointer/length/capacity fields for dev id and
message plus a tunnel enum; C++ must not parse JSON. Export this exact flat surface:

```text
pandar_plugin_firmware_session_create(hub, token, generation) -> void*
pandar_plugin_firmware_session_update(session, hub, token, generation) -> int32
pandar_plugin_firmware_observe_printers(session, batch_json, generation) -> int32
pandar_plugin_firmware_catalog(session, studio_dev_id, pandar_printer_id) -> PluginHttpResult
pandar_plugin_firmware_refresh_version(session, studio_dev_id, pandar_printer_id, sequence_id) -> PluginHttpResult
pandar_plugin_firmware_send(session, studio_dev_id, pandar_printer_id, message, tunnel, token_out) -> PluginHttpResult
pandar_plugin_firmware_return_handoff(session, token, origin_tick) -> int32
pandar_plugin_firmware_next_status_override(session, studio_dev_id) -> PluginHttpResult
pandar_plugin_firmware_next_callback(session, timeout_ms) -> PluginFirmwareCallbackResult
pandar_plugin_firmware_cancel_generation(session, generation) -> void
pandar_plugin_firmware_stop(session) -> void
pandar_plugin_firmware_session_destroy(session) -> void
```

`pandar_plugin_firmware_stop` cancels entries and wakes a blocked callback consumer. Rust anchors
scheduling with its own `Instant::now()` inside handoff; the C++ tick is correlation evidence only.

- [ ] **Step 9: Run Task 7 verification**

```powershell
cargo fmt --all -- --check
cargo nextest run -p pandar-network-plugin --test firmware_parser
cargo nextest run -p pandar-network-plugin --test firmware_status
cargo nextest run -p pandar-network-plugin --test firmware_callbacks
cargo nextest run -p pandar-network-plugin --test http_boundary -E 'test(~firmware)'
cargo nextest run -p pandar-network-plugin --test printer_refresh
cargo nextest run -p pandar-network-plugin --test status_request
cargo nextest run -p pandar-network-plugin
cargo clippy -p pandar-network-plugin --all-targets -- -D warnings
cargo nextest run -p pandar-core --test module_size
```

- [ ] **Step 10: Independent Task 7 review and evidence gate**

Require literal approval for typed parsing, ambiguous HTTP safety, no retry, exact reset/catalog,
callback ownership/deadlines, URL redaction, and module sizes. Fix/re-review; do not commit.

---

### Task 8: Wire the Thin C++ ABI and Prove Cloud/LAN Native Behavior

**Files:** `shim.cpp`, compiled probe fixture/harness/mock Hub firmware modules.

**Interfaces:**

- Consumes Task 7 flat FFI only.
- Produces no firmware policy in C++; only opaque handle/token ownership, string transfer, callback
  selection, mutex/thread lifecycle, and ABI return-code mapping.

- [ ] **Step 1: Extend the compiled ABI probe first and prove RED**

The MSVC probe must:

- call `bambu_network_get_printer_firmware` and inspect empty plus injected main/AMS catalog records;
- request Cloud and LAN `info.get_version` and assert all main/AMS/AMS-HT/unknown module fields and
  original sequences, with no hardcoded versions;
- emit all four command shapes through both send entrypoints and inspect exact fake-Hub prepare and
  execute bodies;
- delay the originating call return, overlap an unrelated send, and assert no firmware callback
  before return or during Studio's first one-second guard, then exact callback at 1.1–2s relative to
  that call's own return handoff;
- overlap heartbeat/status and command callbacks and detect concurrent callback entry;
- logout/destroy with a pending callback and prove no callback after object release and all threads
  joined.
- drive a printer-rejected acknowledgement with `result`, `err_code`, `reason`, and `message` from
  the mock MQTT/Hub result through protobuf/HTTP to the exact delayed Studio callback, preserving all
  fields and the top-level command/sequence.

```powershell
cargo nextest run -p pandar-network-plugin --test studio_abi_probe
```

Expected RED: missing native firmware FFI wiring/timing evidence.

- [ ] **Step 2: Add thin Agent-handle plumbing**

Create/update/destroy the opaque Rust firmware session alongside the existing printer refresh
session. On login/config changes, update its Hub URL/token and increment/cancel the prior plugin
generation. Store one callback invocation mutex and one dispatcher thread/stop flag in `Agent`.

- [ ] **Step 3: Replace hardcoded catalog/version/status behavior**

`bambu_network_get_printer_firmware` delegates to Rust with the Studio serial and Pandar printer id.
Status requests delegate fresh version rendering to Rust. Remove `printer_version_report` and
hardcoded `cfg:""`; use only Rust-produced telemetry fragments. Both Cloud and LAN use the same
typed path.

- [ ] **Step 4: Wire firmware sends before existing operation parsing**

For each send call, invoke the Rust tri-state firmware handler first. Non-firmware falls through
unchanged. Firmware failure maps to the existing invalid-result ABI code; handled success maps to
ABI success. If Rust returns a callback token, perform no callback inline. As the final non-blocking
epilogue, call the per-token handoff with C++ `steady_clock` tick, then return.

- [ ] **Step 5: Implement one serialized callback dispatcher**

The dispatcher blocks in Rust's next-ready FFI, copies/frees returned strings, selects Cloud/local
callback by copying it while holding `status_mutex`, releases that mutex, and invokes under the
dedicated callback-invocation mutex. Heartbeat, status-request, connect-status, and firmware
dispatcher `OnMessageFn` invocations all use that mutex. Logout cancels pending Rust entries.
Destroy order is exact: stop heartbeat; call `pandar_plugin_firmware_stop` to cancel and wake the
blocked consumer; join dispatcher; destroy firmware and printer-refresh Rust sessions; clear
callbacks; delete `Agent`.

- [ ] **Step 6: Run Task 8 verification**

```powershell
cargo fmt --all -- --check
cargo nextest run -p pandar-network-plugin --test studio_abi_probe
cargo nextest run -p pandar-network-plugin --test firmware_parser
cargo nextest run -p pandar-network-plugin --test firmware_status
cargo nextest run -p pandar-network-plugin --test firmware_callbacks
cargo nextest run -p pandar-network-plugin --test http_boundary -E 'test(~firmware)'
cargo nextest run -p pandar-network-plugin
cargo clippy -p pandar-network-plugin --all-targets -- -D warnings
cargo nextest run -p pandar-core --test module_size
```

On Windows, the compiled probe must actually use MSVC. On a non-MSVC host, preserve the existing
explicit platform skip; do not replace the compiled probe with a Rust-only assertion.

- [ ] **Step 7: Independent Task 8 review and evidence gate**

Require literal approval for thin-shim boundaries, exact Cloud/LAN values, callback serialization
and timing, destructor ordering, no hardcoded versions/cfg, and full probe evidence. Fix/re-review;
do not commit.

---

### Task 9: Final Reviews, Documentation, Verification, Commit, and Push

- [ ] **Step 1: Audit the complete implementation diff**

Use `git diff --stat`, `git diff --check`, `git status --short`, production LOC checks, migration
parity checks, and explicit searches for the sentinel URL, hardcoded old versions, raw firmware JSON
parsing in `shim.cpp`, firmware durable replay arms, `include!`, and touched `probe-*` paths.

- [ ] **Step 2: First complete implementation dual-review gate**

Prepare a complete review package from baseline `b8a49047...` through the working tree, including all
intent-to-add files and verification evidence. Dispatch:

1. a fresh independent Codex reviewer for spec compliance and code quality;
2. a default-model OpenCode reviewer through `$opencode-agent`.

Require literal `VERDICT: APPROVE` from both with no Critical/Important findings. For every revision,
add a reproducing failing test, fix minimally, rerun affected full crates, refresh the package, and
repeat both reviews.

- [ ] **Step 3: Update documentation**

Modify:

- `docs/compatibility/bambu-studio-plugin.md`;
- `docs/development.md`;
- `docs/roadmap.md`.

Document printer-main/all reported AMS-family support, native Cloud/tunnel behavior, LAN Studio
button policy, authoritative telemetry and live-only lifecycle, two-phase/no-replay semantics, URL
redaction and empty initial catalog, one-active-Hub ownership, deterministic evidence versus no live
flash, rollout/rollback order, and C as future Web/Android OTA/package staging.

- [ ] **Step 4: Second complete implementation-plus-docs dual-review gate**

Repeat both fresh reviewers against the complete final diff including documentation. Require literal
approval from both. Do not commit before this gate passes.

- [ ] **Step 5: Run fresh delivery verification**

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run -p pandar-core --test module_size
cargo nextest run -p pandar-core -E 'test(~firmware)'
cargo nextest run -p pandar-agent -E 'test(~firmware)'
cargo nextest run -p pandar-hub -E 'test(~firmware)'
cargo nextest run -p pandar-network-plugin --test firmware_parser
cargo nextest run -p pandar-network-plugin --test firmware_status
cargo nextest run -p pandar-network-plugin --test firmware_callbacks
cargo nextest run -p pandar-network-plugin --test http_boundary -E 'test(~firmware)'
cargo nextest run -p pandar-network-plugin --test studio_abi_probe
cargo nextest run --manifest-path Cargo.toml --workspace
git diff --check
```

Run real PostgreSQL focused tests when configured; otherwise record the exact skip. Record that no
external package was downloaded and no live firmware command was sent.

- [ ] **Step 6: Stage only approved paths and create one final Conventional Commit**

Explicitly stage the plan, implementation, tests, and docs. Confirm no `probe-*` path is staged.
Create:

```text
feat(studio): support native firmware updates
```

- [ ] **Step 7: Push and verify remote readback**

Push `main`, then verify local HEAD, tracking ref, and remote branch SHA match. Record commit SHA,
remote readback, final test counts, PostgreSQL result/skip, both final review verdicts, and untouched
probe paths in `.superpowers/sdd/progress.md`.

## Acceptance Checklist

- [ ] Bambu Studio Cloud/tunnel Firmware page receives fresh real module versions and current upgrade status for printer main and every printer-reported AMS-family module.
- [ ] `info.get_version` uses a bounded fresh printer read with exact sequence and no successful cache-only fallback.
- [ ] All four Studio firmware commands preserve exact values and reach only the exact current capable Agent/generation.
- [ ] Hub↔Agent and plugin↔Hub two-phase flows prevent known pre-publish failures from publishing late and never retry an ambiguous execute.
- [ ] URL sentinel is absent from all Pandar durable/log/error/readback surfaces and reaches fake MQTT exactly once only during transient execute.
- [ ] Long-lived report stream is the sole durable status writer; reconnect/invalidation/revision races cannot restore stale state.
- [ ] Exact local reset clears Studio's retained main/AMS state and survives the three-second AMS guard.
- [ ] Firmware callbacks are serialized, per-origin, post-return, within 1.1–2 seconds, and cancelled/joined on logout/destroy.
- [ ] Empty catalog is valid; no fake/empty-URL package is selectable.
- [ ] Cloud and LAN ABI entrypoints are compiled and value-exact; no false claim that Studio shows the LAN update button.
- [ ] SQLite/PostgreSQL behavior is equivalent; all touched production Rust modules pass the 400-LOC guard.
- [ ] No live printer firmware command or external package download was used for verification.
