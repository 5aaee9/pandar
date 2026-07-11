# Bambu Studio Axis Controls and Device Features Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preserve the printer-reported Bambu `print.fun` bitmap end to end and make Bambu Studio modern and legacy Home/X/Y/Z controls dispatch the exact printer protocol supported by the current Agent session.

**Architecture:** `pandar-core` owns an unsigned typed Bambu feature bitmap; protobuf carries full snapshots, feature-only updates, Agent capability negotiation, and required feature intent. Agent parses and caches the current printer observation, Hub persists last-known bits plus the exact observing session, and Studio receives modern bits only for a matching capable live session. Modern Studio operations carry a typed required feature and fail closed in Hub and Agent; requirement-free legacy operations retain their exact semantic translation.

**Tech Stack:** Rust 2024 workspace, serde, prost/tonic, tokio, SeaORM/sqlx, SQLite, PostgreSQL, C++17 ABI shim, Bambu Studio reference sources, cargo-nextest.

## Global Constraints

- Follow `docs/superpowers/specs/2026-07-11-bambu-studio-axis-controls-device-features-design.md` exactly.
- Do not contact or move a real printer; all protocol verification is deterministic local or loopback testing.
- Do not fabricate bit 32 or 38 from a printer model. Preserve every `u64` bit, including unnamed bits and bit 63.
- Hub stores and transports semantic operations only. Raw Studio JSON and raw G-code never cross Hub.
- `crates/pandar-network-plugin/src/shim.cpp` remains an ABI adapter; typed parsing and status construction stay in Rust.
- Use typed serde structures for known JSON shapes. `IgnoredAny` is allowed only for the field-scoped invalid `print.fun` variant.
- SQLite and PostgreSQL migrations and behavior must be equivalent. If `PANDAR_TEST_POSTGRES_URL` is absent, record the real PostgreSQL test as explicitly skipped.
- Production Rust modules must remain at or below 400 lines; split the named modules before they cross the limit.
- Preserve the complete lower error/cause chain in returned or logged diagnostics.
- Do not modify or delete the pre-existing untracked `crates/pandar-network-plugin/probe-*` directories.
- SDD commit policy overrides per-task commits: keep reviewed task changes in the working tree and create one final commit only after the final implementation gate, documentation, and fresh verification.

## File Structure

New focused production modules:

- `crates/pandar-core/src/device_features.rs`: typed `BambuDeviceFeatures` parser/formatter and named bit queries.
- `crates/pandar-agent/src/machine/device_features.rs`: shared per-serial runtime cache.
- `crates/pandar-agent/src/machine/mqtt/device_features.rs`: field-scoped `fun` observation parsing, feature probe, and feature event builders.
- `crates/pandar-agent/src/machine/mqtt/commands/axis.rs`: existing `gcode_line` plus modern homing/axis command types and typed payload builders, extracted from the already-399-line `commands.rs`.
- `crates/pandar-agent/src/machine/operations/axis.rs`: Home/Move feature selection and modern/legacy MQTT mapping, keeping `operations.rs` below 400 lines.
- `crates/pandar-hub/src/grpc/printer_device_features.rs`: feature-only inbound event handling.
- `crates/pandar-hub/src/grpc/commands/device_features.rs`: queued required-feature dispatch gate.
- `crates/pandar-hub/src/repositories/printers/device_features.rs`: exact-session feature update/invalidation and dispatch lookup.
- `crates/pandar-hub/src/routes/printer_operations/device_features.rs`: request/operation requirement validation, keeping `printer_operations.rs` below 400 lines.
- `crates/pandar-network-plugin/src/gcode/studio_axis.rs`: modern Studio and exact legacy-envelope parsing, keeping parser modules focused.

New focused test modules mirror those boundaries instead of extending the existing oversized suites.

---

### Task 1: Typed Bitmap and Additive Wire Contracts

**Files:**

- Create: `crates/pandar-core/src/device_features.rs`
- Modify: `crates/pandar-core/src/lib.rs`
- Modify: `proto/pandar/agent/v1/agent.proto`
- Modify: `crates/pandar-agent/src/commands/responses.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/reports.rs`
- Modify: `crates/pandar-agent/src/commands/tests.rs`
- Modify: `crates/pandar-hub/src/grpc/commands/conversion.rs`
- Modify: `crates/pandar-hub/src/grpc/inbound.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/printer_snapshots.rs`
- Modify: `crates/pandar-hub/src/routes/tests/printer_events_ws.rs`
- Modify: `crates/pandar-hub/src/routes/tests/plugin/live_status.rs`
- Test: `crates/pandar-core/src/device_features.rs`

**Interfaces:**

- Produces: `BambuDeviceFeatures::from_hex(&str) -> Result<Self, BambuDeviceFeaturesParseError>`, `from_bits(u64)`, `bits() -> u64`, `to_hex() -> String`, and `contains(BambuDeviceFeature) -> bool`.
- Produces: `BambuDeviceFeature::{MqttHoming, MqttAxisControl}` with bit indices 32 and 38, plus `pub const fn bit(self) -> u32` returning the enum discriminant.
- Produces protobuf `PrinterDeviceFeatures`, `PrinterDeviceFeaturesSnapshot`, `DeviceFeature`, `AgentCapability::RequiredDeviceFeatures`, `PrinterSnapshot.device_features`, `AgentEvent.printer_device_features_snapshot`, and `PrinterOperation.required_device_features`.
- Consumed by: every later task.

- [ ] **Step 1: Write failing core tests for the exact bitmap contract**

Add tests alongside the new module with these assertions:

```rust
#[test]
fn parses_formats_and_queries_bambu_fun_bits() {
    let features = BambuDeviceFeatures::from_hex("  4100000000  ").unwrap();
    assert_eq!(features.to_hex(), "4100000000");
    assert!(features.contains(BambuDeviceFeature::MqttHoming));
    assert!(features.contains(BambuDeviceFeature::MqttAxisControl));
}

#[test]
fn preserves_unnamed_and_high_bits() {
    let features = BambuDeviceFeatures::from_hex("8000004100000020").unwrap();
    assert_eq!(features.bits(), 0x8000_0041_0000_0020);
    assert_eq!(features.to_hex(), "8000004100000020");
}

#[test]
fn canonicalizes_zero_and_rejects_non_grammar_inputs() {
    assert_eq!(BambuDeviceFeatures::from_hex("0000").unwrap().to_hex(), "0");
    for value in ["", " ", "-1", "+1", "0x1", "1_0", "GG", "10000000000000000", "\u{00A0}1\u{00A0}"] {
        assert!(BambuDeviceFeatures::from_hex(value).is_err(), "{value}");
    }
}
```

- [ ] **Step 2: Run the RED test**

Run: `cargo test -p pandar-core device_features -- --nocapture`

Expected: compilation fails because `BambuDeviceFeatures` and `BambuDeviceFeature` do not exist.

- [ ] **Step 3: Implement the minimal typed value**

Implement an opaque `u64` newtype. Call `trim_ascii()` (not Unicode `trim()`), validate the result with `1..=16` and `is_ascii_hexdigit()`, then call `u64::from_str_radix`. Unicode whitespace therefore remains and is rejected. Format with `format!("{:X}", self.0)`. Implement string `Serialize`/`Deserialize`, `Display`, `FromStr`, `Default` as zero, and a small error enum whose display names the invalid input class without leaking unrelated data.

Define the named feature enum and bit accessor explicitly so the query below is complete rather than relying on an unstated conversion:

```rust
#[repr(u32)]
pub enum BambuDeviceFeature {
    MqttHoming = 32,
    MqttAxisControl = 38,
}

impl BambuDeviceFeature {
    pub const fn bit(self) -> u32 {
        self as u32
    }
}
```

The named query must use the enum bit index without rebuilding the bitmap:

```rust
pub fn contains(self, feature: BambuDeviceFeature) -> bool {
    self.0 & (1_u64 << feature.bit()) != 0
}
```

- [ ] **Step 4: Add the additive protobuf fields with fixed numbers**

Use these exact wire values:

```proto
enum AgentCapability {
  AGENT_CAPABILITY_UNSPECIFIED = 0;
  AGENT_CAPABILITY_HANDLE_PRINT_ERROR = 1;
  AGENT_CAPABILITY_HANDLE_PRINT_ERROR_SEQUENCE_ZERO_PUBACK_ONLY = 2;
  AGENT_CAPABILITY_REQUIRED_DEVICE_FEATURES = 3;
}

message PrinterDeviceFeatures { fixed64 bambu_fun_bits = 1; }
message PrinterDeviceFeaturesSnapshot {
  string serial = 1;
  PrinterDeviceFeatures device_features = 2;
}

enum DeviceFeature {
  DEVICE_FEATURE_UNSPECIFIED = 0;
  DEVICE_FEATURE_BAMBU_MQTT_HOMING = 32;
  DEVICE_FEATURE_BAMBU_MQTT_AXIS_CONTROL = 38;
}
```

Add `PrinterDeviceFeatures device_features = 13` to `PrinterSnapshot`, event tag 17 to `AgentEvent`, and `repeated DeviceFeature required_device_features = 2` to `PrinterOperation`. Do not renumber any existing field.

- [ ] **Step 5: Resolve the enumerated additive compile fallout**

Update only the eight enumerated compile-owner files mechanically, then run `cargo check --workspace`:

```rust
PrinterSnapshot {
    device_features: None,
    // existing fields unchanged
}

ProtoPrinterOperation {
    serial_number,
    required_device_features: Vec::new(),
    operation,
}
```

Because Task 1 adds an `AgentEvent` oneof variant before Task 3 implements its persistence behavior, add an explicit temporary exhaustive arm in `grpc/inbound.rs`:

```rust
Some(agent_event::Event::PrinterDeviceFeaturesSnapshot(_)) => Ok(()),
```

Task 3 must replace this no-op arm with the named feature-only handler. Do not add feature persistence behavior in Task 1.

Do not add feature behavior in this task.

- [ ] **Step 6: Verify Task 1**

Run:

```powershell
cargo test -p pandar-core device_features -- --nocapture
cargo check --workspace
cargo nextest run -p pandar-core --test module_size
```

Expected: all commands pass and protobuf field numbers remain additive.

---

### Task 2: Agent Feature Observation, Cache, and Feature-Only Events

**Files:**

- Create: `crates/pandar-agent/src/machine/device_features.rs`
- Create: `crates/pandar-agent/src/machine/mqtt/device_features.rs`
- Create: `crates/pandar-agent/src/machine/mqtt/tests/device_features.rs`
- Modify: `crates/pandar-agent/src/machine/mod.rs`
- Modify: `crates/pandar-agent/src/machine/types.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/transport.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/fake.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/snapshot.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/reports.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/tests.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/tests/fixtures.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/tests/snapshot/fixtures.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/tests/snapshot.rs`
- Modify: `crates/pandar-agent/src/machine/tests.rs`
- Modify: `crates/pandar-agent/src/commands/tests.rs`
- Modify: `crates/pandar-agent/src/commands/responses.rs`
- Modify: `crates/pandar-agent/src/commands/refresh.rs`
- Modify: `crates/pandar-agent/src/machine/runtime.rs`
- Modify: `crates/pandar-agent/src/machine/runtime/test_support.rs`
- Modify: `crates/pandar-agent/src/lib.rs`
- Modify: `crates/pandar-agent/src/tests.rs`

**Interfaces:**

- Consumes: Task 1 core and protobuf types.
- Produces: cloneable `DeviceFeatureCache` with async `get`, `update`, and `invalidate` by serial.
- Produces: `device_feature_observation(serial: &str, report: &SnapshotReport) -> anyhow::Result<Option<BambuDeviceFeatures>>` and `probe_device_features(...) -> anyhow::Result<BambuDeviceFeatures>`.
- Produces: full-snapshot optional feature mapping and `PrinterDeviceFeaturesSnapshot` valid/invalidation events.
- Produces: Runtime initialization that advertises capability 3, invalidates/probes before command consumption, and re-invalidates on report reconnect.
- Consumed by: Tasks 3 and 5.

- [ ] **Step 1: Write RED parsing and sibling-preservation tests**

Use real MQTT-shaped bytes through `decode_mqtt_report_payload`. Cover:

```rust
{"print":{"fun":"8000004100000020","gcode_state":"RUNNING","bed_temper":60}}
{"print":{"fun":false,"gcode_state":"RUNNING","bed_temper":60}}
{"print":{"fun":null,"gcode_state":"RUNNING","bed_temper":60}}
{"print":{"fun":"not-hex","gcode_state":"RUNNING","bed_temper":60}}
```

Assert the first produces the exact typed bits. Every later payload produces an invalid-present observation, not `Missing`, while retaining `RUNNING` and `60`. Call `device_feature_observation("SERIAL-1", &report)` directly and assert its returned parse issue formatted with `{err:#}` contains `SERIAL-1`, `print.fun`, and either `expected a hexadecimal string` or the lower parse cause. Add a payload whose string is surrounded by U+00A0 and prove it is rejected rather than Unicode-trimmed.

- [ ] **Step 2: Write RED cache/event tests**

Test these state transitions with one shared cache:

```rust
assert_eq!(cache.get(serial).await, None);
cache.update(serial, high_bits).await;
assert_eq!(cache.get(serial).await, Some(high_bits));
cache.invalidate(serial).await;
assert_eq!(cache.get(serial).await, None);
cache.update(serial, BambuDeviceFeatures::default()).await;
assert_eq!(cache.get(serial).await.unwrap().bits(), 0);
```

Feed a `fun`-only report and assert the emitted event is only `PrinterDeviceFeaturesSnapshot`, not `PrinterSnapshot`; feed a temperature+fun report and assert one full snapshot with `device_features` and no duplicate feature-only event.

- [ ] **Step 3: Write RED probe/startup/reconnect ordering tests**

Before any production changes, add deterministic tests proving:

- a cold probe publishes `RequestPushAll` before it accepts `8000004100000020` and updates the same cache instance returned to Runtime;
- unrelated reports are skipped but a present invalid/null `fun` fails with context;
- timeout leaves the cache unknown;
- session startup emits `Hello`, invalidation, and the exact observation before a queued command is consumed;
- report reconnect and endpoint replacement invalidate the old cache value before accepting a new value.

The reconnect test must distinguish the two existing receive outcomes: an ordinary idle report timeout remains inside the forwarder without invalidating/reprobing, while a non-timeout MQTT poll/connection failure returns to the outer retry, which invalidates and probes before accepting the next value.

Use a paused fake command stream and `FakeMqttTransport.published_commands()` to assert ordering. Do not implement the runtime behavior before these tests exist.

- [ ] **Step 4: Run all Task 2 RED tests**

Run: `cargo test -p pandar-agent device_features -- --nocapture`

Expected: tests fail because parsing, cache, event, and message mapping do not exist.

- [ ] **Step 5: Implement presence-preserving field-scoped observation parsing**

Move the raw `fun` field shape into `machine/mqtt/device_features.rs`:

```rust
#[derive(Default)]
enum FunField {
    #[default]
    Missing,
    String(String),
    Invalid,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PresentFun {
    String(String),
    Invalid(serde::de::IgnoredAny),
}
```

`SnapshotPrint.fun` is `FunField` with `#[serde(default, deserialize_with = "deserialize_fun_field")]`. Serde default is used only when the key is absent. For a present key, deserialize `PresentFun`; strings become `FunField::String`, while `null`, bool, number, array, and object all become `FunField::Invalid`. `device_feature_observation(serial, report)` maps `Missing` to `Ok(None)`, delegates strings to `BambuDeviceFeatures::from_hex`, and maps `Invalid` to a contextual `anyhow` error that names the supplied serial and `print.fun`. Do not fail `SnapshotReport` deserialization for one bad field.

- [ ] **Step 6: Implement the shared cache and full/feature-only mapping**

Use `Arc<tokio::sync::RwLock<HashMap<String, BambuDeviceFeatures>>>`. Add `device_features: Option<BambuDeviceFeatures>` to `MachineSnapshot`. Both full snapshot builders map it to `PrinterDeviceFeatures { bambu_fun_bits }`.

Initialize `device_features: None` in every existing `MachineSnapshot` literal owned by this task, including `commands/tests.rs`, `machine/tests.rs`, and `machine/mqtt/tests/snapshot.rs`, before running Task 2 verification. Do not rely on later compiler-fallout cleanup.

Add event helpers:

```rust
fn feature_event(config: &AgentConfig, serial: String, value: Option<BambuDeviceFeatures>) -> AgentEvent
```

`Some` serializes the exact `fixed64`; `None` is explicit invalidation. Continuous reports update cache only for valid values. Invalid values log with `{error:#}` and leave the prior cache unchanged.

- [ ] **Step 7: Implement bounded `pushall` probing**

Subscribe to the report topic, publish `BambuMqttCommand::RequestPushAll`, and loop until the existing deadline. Ignore unrelated reports without `fun`; return immediately for a valid value; return contextual error for an invalid value; time out with serial and topic context. Never infer from model.

- [ ] **Step 8: Make Runtime session freshness explicit**

Add the new Agent capability in `hello_event`. Store the current Agent event sender in Runtime while a session is active. Before `handle_command_stream_with_gateway` starts:

Update the existing exact `hello_event_has_agent_identity_version_and_exact_capability` expectation in `src/tests.rs` to append `AgentCapability::RequiredDeviceFeatures as i32`; keep the existing capability order unchanged and add no other capability.

1. invalidate every configured serial in the shared cache;
2. enqueue one feature invalidation per serial;
3. probe each serial on its command transport;
4. update cache and enqueue the exact observation when successful;
5. keep invalidation and log the complete cause when unsuccessful;
6. start report forwarders;
7. only then consume Hub commands.

The current `BambuMqttTransport::next_report` `anyhow::Result` conflates ordinary idle timeouts and MQTT poll/connection failures. Preserve the public trait signature and add one private typed timeout marker in `mqtt/transport.rs`, plus crate-private construction/classification helpers. The real transport returns that typed error only when `tokio::time::timeout` expires; its rumqttc poll errors keep their original cause chain. Make `FakeMqttTransport` produce the same typed idle-timeout error and add the smallest deterministic non-timeout receive-failure hook needed by the RED reconnect test.

In `forward_print_reports`, continue the existing loop for the typed idle timeout, but return every non-timeout receive error with serial/topic context to the outer retry. On that report-forwarder retry, invalidate cache and enqueue invalidation before resubscribing/publishing `pushall`. This preserves idle-printer behavior while giving actual poll/connection failures an exact reconnect boundary. Link/refresh update the same cache from the returned full snapshot. A valid zero must overwrite a prior nonzero value.

- [ ] **Step 9: Run the prewritten startup/reconnect tests GREEN**

Rerun the Step 3 cases and confirm the exact event/publish order. No test may open a real socket to a printer.

- [ ] **Step 10: Verify Task 2**

Run:

```powershell
cargo test -p pandar-agent device_features -- --nocapture
cargo test -p pandar-agent runtime -- --nocapture
cargo nextest run -p pandar-agent
cargo nextest run -p pandar-core --test module_size
```

Expected: all Agent tests pass; production modules remain within 400 lines.

---

### Task 3: Hub Persistence and Current-Session Studio Advertisement

**Files:**

- Create: `crates/pandar-hub/migrations/sqlite/20260711000000_bambu_device_features.sql`
- Create: `crates/pandar-hub/migrations/postgres/20260711000000_bambu_device_features.sql`
- Create: `crates/pandar-hub/src/repositories/printers/device_features.rs`
- Create: `crates/pandar-hub/src/grpc/printer_device_features.rs`
- Create: `crates/pandar-hub/src/repositories/tests/printer_device_features.rs`
- Modify: `crates/pandar-core/src/printer.rs`
- Modify: `crates/pandar-core/src/tests.rs`
- Modify: `crates/pandar-hub/src/entities/printers.rs`
- Modify: `crates/pandar-hub/src/repositories/printers.rs`
- Modify: `crates/pandar-hub/src/repositories/mod.rs`
- Modify: `crates/pandar-hub/src/repositories/adapters/printers.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/mod.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/printers.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/postgres.rs`
- Modify: `crates/pandar-hub/src/grpc/inbound.rs`
- Modify: `crates/pandar-hub/src/grpc/printer_snapshots.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/printer_snapshots.rs`
- Modify: `crates/pandar-hub/src/routes/plugin/studio_devices.rs`
- Modify: `crates/pandar-hub/src/routes/tests/plugin.rs`

**Interfaces:**

- Consumes: Task 1 types/messages and Task 2 feature events.
- Produces: nullable `bambu_fun_bits` and `bambu_fun_session_id` persistence.
- Produces: `pub enum DeviceFeatureUpdateOutcome { Updated, StaleOrMissing }` in `repositories/printers/device_features.rs`.
- Produces: `PrinterRepository::update_device_features_if_current(tenant_id, agent_id, session_id, serial, Option<BambuDeviceFeatures>) -> RepositoryResult<DeviceFeatureUpdateOutcome>`.
- Produces: the existing `repositories` public boundary re-exports `DeviceFeatureUpdateOutcome` alongside `PrinterRepository` for Task 4 consumers.
- Produces: exact-session plugin `fun`, otherwise canonical `"0"`.
- Consumed by: Task 4 dispatch gate and Task 6 ABI path.

- [ ] **Step 1: Write RED migration and repository tests**

Use identical migration SQL in both backends:

```sql
ALTER TABLE printers ADD COLUMN bambu_fun_bits TEXT;
ALTER TABLE printers ADD COLUMN bambu_fun_session_id TEXT;
```

Tests must compare the migration files byte-for-byte, verify legacy rows contain two `NULL`s, and run this complete matrix through the public repository boundary:

1. full snapshot create with `8000004100000020` stores canonical bits and observing session;
2. later full snapshot with an absent feature message preserves both last-known bits and prior observation session;
3. present zero overwrites nonzero bits with `"0"` and writes the new current session;
4. present nonzero overwrites zero;
5. bit 63 hydrates into `BambuDeviceFeatures` without signed conversion;
6. feature-only `Some` updates exact bits plus current session without changing full-snapshot fields;
7. feature-only invalidation keeps bits and clears only the session marker;
8. stale session, wrong tenant, wrong Agent owner, and unknown serial are benign no-ops and cannot mutate either feature column.

For cases 6-8, seed non-default status/model/last-seen/nozzle JSON/temperatures/active nozzle/light/state revision and assert every value stays byte/value identical. Put the backend-neutral matrix in a helper used by both the SQLite test and the real PostgreSQL test; do not maintain two weaker assertion lists.

- [ ] **Step 2: Run the RED persistence tests**

Run: `cargo test -p pandar-hub printer_device_features -- --nocapture`

Expected: failure because migrations, entity columns, repository method, and domain fields are absent.

- [ ] **Step 3: Add typed domain and full-snapshot persistence**

Add internal `Option<BambuDeviceFeatures>` and `Option<String>` observation-session fields to `Printer`/`PrinterParts`; keep them out of unrelated serialized public printer responses with explicit serde attributes. Hydration parses stored hex through the core type and adds `failed to rehydrate printer Bambu device features` context.

Full snapshot UPSERT behavior:

```sql
bambu_fun_bits = COALESCE(excluded.bambu_fun_bits, printers.bambu_fun_bits),
bambu_fun_session_id = CASE
  WHEN excluded.bambu_fun_bits IS NULL THEN printers.bambu_fun_session_id
  ELSE excluded.bambu_fun_session_id
END
```

Bind canonical hex plus `token.persisted_id()` only when the protobuf feature message is present.

- [ ] **Step 4: Implement feature-only current-session updates**

Reuse the repository's current-Agent transaction/lock order. `Some(features)` updates exact hex and session id. `None` clears only `bambu_fun_session_id`. Require matching tenant, current Agent session, Agent owner, and serial; stale events are benign no-ops consistent with other inbound current-session events.

Wire `agent_event::Event::PrinterDeviceFeaturesSnapshot` in `grpc/inbound.rs` to the new handler. Define `DeviceFeatureUpdateOutcome` exactly as `Updated | StaleOrMissing`; the repository method returns `RepositoryResult<DeviceFeatureUpdateOutcome>` so tests can distinguish a valid update from a benign no-op without inferring from row contents.

- [ ] **Step 5: Write RED Studio advertisement tests**

Create sessions representing:

- current token + capability 3 + matching observation marker;
- current token without capability 3;
- new current token with old stored marker;
- disconnected Agent;
- matching current token after explicit invalidation.

Only the first response may contain `"fun":"8000004100000020"`; every other response must contain `"fun":"0"`.

- [ ] **Step 6: Implement session-qualified plugin output**

Use `current_token_for_capability(..., AgentCapability::RequiredDeviceFeatures)`. Expose the stored canonical bits only when the returned token's persisted id exactly equals `bambu_fun_session_id`. Otherwise return `"0"`. Never OR, mask, or reconstruct known bits.

- [ ] **Step 7: Verify SQLite and optional PostgreSQL behavior**

Run:

```powershell
cargo test -p pandar-hub printer_device_features -- --nocapture
cargo test -p pandar-hub plugin_printer -- --nocapture
cargo nextest run -p pandar-hub
cargo nextest run -p pandar-core --test module_size
```

If `$env:PANDAR_TEST_POSTGRES_URL` is set, also run:

```powershell
cargo nextest run -p pandar-hub -E 'test(printer_device_features_postgres)' --test-threads 1
```

If it is absent, append the explicit skip to the task report and do not claim PostgreSQL runtime coverage.

---

### Task 4: Required-Feature Semantic Contract and Hub Fail-Closed Dispatch

**Files:**

- Create: `crates/pandar-hub/src/routes/printer_operations/device_features.rs`
- Create: `crates/pandar-hub/src/grpc/commands/device_features.rs`
- Create: `crates/pandar-hub/src/grpc/tests/commands/device_features.rs`
- Modify: `crates/pandar-hub/src/routes/printer_operations.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/operations.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/operations/audit.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/audit/printer_operations.rs`
- Modify: `crates/pandar-hub/src/grpc/commands.rs`
- Modify: `crates/pandar-hub/src/grpc/commands/conversion.rs`
- Modify: `crates/pandar-hub/src/grpc/outbound.rs`
- Modify: `crates/pandar-hub/src/grpc.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/commands.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/commands.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/postgres_commands.rs`
- Modify: `crates/pandar-hub/src/routes/tests/plugin/operations.rs`

**Interfaces:**

- Consumes: Task 3 session-qualified persisted features.
- Produces: typed `required_device_features` on Home/Move request, persisted operation, audit metadata, and protobuf conversion.
- Produces: `dispatch_next_queued_for_session(...) -> Result<SessionQueuedDispatch, Status>`, the only queued outbound path; it validates, fails or marks sent, converts, and sends under one exact-session lease.
- Produces: `SessionQueuedDispatch::{Sent, FailedAndContinue, Empty, SessionEnded, ChannelClosed}`.
- Consumed by: Tasks 5 and 6.

- [ ] **Step 1: Write RED request/persistence validation tests**

Accept only:

```json
{"action":"home","axes":[],"required_device_features":["bambu_mqtt_homing"]}
{"action":"move_axes","movements":[{"axis":"x","delta_mm":10.0}],"required_device_features":["bambu_mqtt_axis_control"]}
```

Reject a homing requirement with selected axes, an axis-control requirement with multiple axes, any delta whose absolute value is not exactly 1 or 10, any feedrate, a requirement on another action, duplicates, or mismatched feature. Explicitly accept `-1`, `+1`, `-10`, and `+10`. Prove old persisted Home/Move JSON without the field deserializes to an empty list.

- [ ] **Step 2: Write RED dispatch-race tests**

Drive the concrete `dispatch_next_queued_for_session` API for all transitions before outbound send:

1. capable current session + matching marker + bit present -> one protobuf command;
2. old/non-capable replacement session -> command failed, no protobuf;
3. capable replacement session but old marker -> failed, no protobuf;
4. matching marker but missing required bit -> failed, no protobuf;
5. disconnect -> failed/no send when the old pump observes close;
6. requirement-free operation -> existing behavior unchanged.

Assert failed commands retain a cause-preserving stable reason and are not marked sent.

Add deterministic pause hooks at `after queued row read`, `after feature validation`, and `before channel send`. While each pause is held, attempt to register a replacement session; assert replacement cannot complete until the old helper either sends under its lease or fails without a send. This proves no unleased gap exists between validation, `mark_sent`, and channel send.

- [ ] **Step 3: Run the RED tests**

Run: `cargo test -p pandar-hub required_device_features -- --nocapture`

Expected: request parsing and outbound dispatch tests fail.

- [ ] **Step 4: Add the typed semantic requirement**

Use one shared Hub enum mapping to the Task 1 protobuf/core values. Add `#[serde(default, skip_serializing_if = "Vec::is_empty")]` so existing payloads remain readable. Put the requirement on Home/Move variants and expose `required_device_features(&self) -> &[BambuDeviceFeature]`.

Conversion fills `PrinterOperation.required_device_features` with protobuf enum integers. Audit metadata includes the list only when non-empty.

- [ ] **Step 5: Replace the queued outbound helper with one session-gated API**

Pass `SessionToken` into `spawn_outbound_pump` and delete its direct use of `next_hub_command_for_agent`. Add this exact shape in `grpc/commands/device_features.rs`:

```rust
pub(super) enum SessionQueuedDispatch {
    Sent,
    FailedAndContinue,
    Empty,
    SessionEnded,
    ChannelClosed,
}

pub(super) async fn dispatch_next_queued_for_session(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    command_sender: &mpsc::Sender<Result<HubCommand, Status>>,
    options: CommandConversionOptions,
) -> Result<SessionQueuedDispatch, Status>;
```

The helper holds `transition_lease_for_session(agent_id, token)` for its entire state machine:

1. verify the token is still current;
2. read one queued row without changing it;
3. deserialize/validate any required features;
4. on gate failure, mark that command failed and return `FailedAndContinue`;
5. convert to `HubCommand` while still queued;
6. mark the row sent;
7. send the converted command on `command_sender` before dropping the lease;
8. return `Sent` or `ChannelClosed`.

`Empty` means no queued row; `SessionEnded` stops the old pump. Conversion failure keeps the row queued and returns `Status`, matching existing artifact error behavior. No separate public helper may mark a required command sent before feature validation.

For non-empty requirements, before marking sent:

- confirm the session is still exact-current;
- confirm capability 3;
- load the operation's owned printer;
- confirm `bambu_fun_session_id == token.persisted_id()`;
- confirm its exact bitmap contains every requirement.

If any check fails, mark that command failed and continue draining without sending protobuf. Do not terminate the Agent stream for this command-level failure. Requirement-free operations pass through the same state machine without feature checks, preserving current behavior while closing the same session-transition gap.

- [ ] **Step 6: Verify Task 4**

Run:

```powershell
cargo test -p pandar-hub required_device_features -- --nocapture
cargo nextest run -p pandar-hub
cargo nextest run -p pandar-core --test module_size
```

Expected: the old-Agent reconnect test proves no protobuf command escapes.

---

### Task 5: Agent Modern and Legacy Printer Payload Selection

**Files:**

- Create: `crates/pandar-agent/src/machine/operations/axis.rs`
- Create: `crates/pandar-agent/src/machine/mqtt/commands/axis.rs`
- Create: `crates/pandar-agent/src/machine/tests/axis_controls.rs`
- Modify: `crates/pandar-agent/src/machine/operations.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/commands.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/commands/payload.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/tests.rs`
- Modify: `crates/pandar-agent/src/machine/mod.rs`
- Modify: `crates/pandar-agent/src/machine/runtime.rs`
- Modify: `crates/pandar-agent/src/commands/operations.rs`
- Modify: `crates/pandar-agent/src/commands/operation_results.rs`
- Modify: `crates/pandar-agent/src/commands/tests.rs`
- Modify: `crates/pandar-agent/src/machine/tests.rs`

**Interfaces:**

- Consumes: Task 2 cache/probe and Task 4 protobuf requirement.
- Produces: typed MQTT `BackToCenter` and `XyzControl { axis, direction, mode }` commands.
- Produces: exact `G28 [axes]` and seven-command legacy movement envelope.
- Produces: Agent-side immediate pre-MQTT required-feature recheck and feature convergence event.

- [ ] **Step 1: Write RED exact-payload tests**

Using one `ConfiguredBambuMachineGateway`, one shared cache, and `FakeMqttTransport`, assert:

- bit 32 + required empty-axis Home -> `{"print":{"command":"back_to_center","sequence_id":"..."}}`;
- bit 38 + required X `+1`, Y `-10`, and Z `+10` -> exact `xyz_ctrl` axis, `dir`, and `mode` with no extra inversion;
- no requirement + `[X]` Home -> `gcode_line` param `G28 X`;
- no requirement + `[Z, X, Y]` Home -> `gcode_line` param `G28 Z X Y`, preserving semantic order;
- no requirement + empty Home without known bit -> `G28`;
- no requirement + legacy X/Y uses feedrate 3000 and Z uses 900 as supplied by Studio parser;
- fallback envelope has exactly seven ordered lines and uses `G1`.

- [ ] **Step 2: Write RED fail-closed Agent tests**

For a required modern operation:

- cached valid bitmap missing the requested bit -> no operation publish, exact bitmap feature event;
- cold cache + probe exact zero -> no operation publish, zero feature event;
- cold cache + valid nonzero missing bit `8000000100000020` -> no operation publish and that exact value event;
- cold cache + invalid/missing/timeout -> no operation publish and invalidation event;
- requirement-free operation under the same conditions -> exact legacy payload.

Also add, before implementation:

- a cold supported probe test asserting the publish order is `pushall` then `back_to_center`/`xyz_ctrl`, never the reverse;
- one shared-cache integration test that ingests a report with `8000004100000020`, dispatches modern Home/Move, ingests exact zero into the same cache, then proves a required modern operation fails with no MQTT publish;
- protobuf parser tests that reject `DEVICE_FEATURE_UNSPECIFIED`, unknown numeric feature values, homing requirement on Move, axis requirement on Home, duplicate requirements, and requirements on Pause/temperature/AMS operations before gateway dispatch.

- [ ] **Step 3: Run the RED tests**

Run: `cargo test -p pandar-agent axis_controls -- --nocapture`

Expected: failures show current Home collapse, three-line movement, and missing modern commands/cache checks.

- [ ] **Step 4: Implement typed modern MQTT commands**

First extract the existing `GcodeLineCommand` type and `gcode_line_payload` builder from the already-399-line `machine/mqtt/commands.rs` into `machine/mqtt/commands/axis.rs`. Add the new typed serde payloads there, not `json!` or `Value`:

```rust
BackToCenter
XyzControl { axis: PrinterAxis, direction: i8, mode: u8 }
```

Use the existing Studio sequence generator. Serialize uppercase axis, numeric `-1|1`, and numeric `0|1`.

`commands.rs` keeps the public `BambuMqttCommand` enum and delegates the three axis-related match arms to the extracted module. Verify its final line count is below 400.

- [ ] **Step 5: Implement exact axis mapping in the extracted module**

Rules:

```text
required homing + bit32 + empty axes -> back_to_center
required axis + bit38 + one axis + abs 1/10 + no F -> xyz_ctrl
required feature not currently observed -> error, no MQTT operation
requirement-free selected-axis home -> G28 plus axes
other requirement-free move -> seven-line Studio envelope
```

Never round a delta, invent a feedrate, collapse `G28 X`, or invert Y/Z.

- [ ] **Step 6: Connect probing and convergence to Runtime's event sender**

If an Agent pre-publish probe changes/invalidates the cache, enqueue the corresponding `PrinterDeviceFeaturesSnapshot` before returning success/failure. A valid nonzero missing-bit result must remain exact. The command failure preserves probe and required-feature context through `{err:#}`.

- [ ] **Step 7: Verify Task 5**

Run:

```powershell
cargo test -p pandar-agent axis_controls -- --nocapture
cargo test -p pandar-agent printer_operation -- --nocapture
cargo nextest run -p pandar-agent
cargo nextest run -p pandar-core --test module_size
```

Expected: exact modern and legacy payload assertions pass.

---

### Task 6: Studio Status, Modern/Legacy Parsing, and Compiled ABI Coverage

**Files:**

- Create: `crates/pandar-network-plugin/src/gcode/studio_axis.rs`
- Modify: `crates/pandar-network-plugin/src/gcode.rs`
- Modify: `crates/pandar-network-plugin/src/gcode/operation.rs`
- Modify: `crates/pandar-network-plugin/src/gcode/studio_json.rs`
- Modify: `crates/pandar-network-plugin/src/studio_status/input.rs`
- Modify: `crates/pandar-network-plugin/src/studio_status/device.rs`
- Modify: `crates/pandar-network-plugin/src/shim.cpp`
- Modify: `crates/pandar-network-plugin/tests/studio_status.rs`
- Modify: `crates/pandar-network-plugin/tests/operation_parser.rs`
- Modify: `crates/pandar-network-plugin/tests/http_boundary/printer_operations.rs`
- Modify: `crates/pandar-network-plugin/tests/studio_abi_probe.rs`
- Modify: `crates/pandar-network-plugin/tests/studio_abi_probe/native_print_error.rs`
- Modify: `crates/pandar-network-plugin/tests/studio_abi_probe/mock_hub.rs`
- Modify: `crates/pandar-network-plugin/tests/studio_abi_probe/mock_hub/operations.rs`
- Modify: `crates/pandar-network-plugin/tests/fixtures/studio_abi_probe.cpp`

**Interfaces:**

- Consumes: Hub request shape from Task 4 and Hub `fun` from Task 3.
- Produces: Rust telemetry field `fun` and typed modern/legacy semantic operation JSON.
- Produces: compiled ABI proof that Studio receives the complete bitmap and both protocol paths submit semantics only.

- [ ] **Step 1: Write RED telemetry tests**

Assert input `{"fun":"8000004100000020", ...}` emits the exact same string. Missing and `null` emit `"fun":"0"` without clearing `gcode_state`, progress, temperature, HMS, or materials.

- [ ] **Step 2: Write RED modern parser tests**

Expected semantic JSON:

```json
{"action":"home","axes":[],"required_device_features":["bambu_mqtt_homing"]}
{"action":"move_axes","movements":[{"axis":"x","delta_mm":1.0}],"required_device_features":["bambu_mqtt_axis_control"]}
```

Cover X/+1/mode0, Y/-1/mode1, Z/+1/mode1. Reject lowercase/`E`, direction `0`/`2`/string, mode `2`/string, and every missing field.

- [ ] **Step 3: Write RED legacy wrapper/envelope tests**

Parse actual JSON `gcode_line` wrappers for `G28`, `G28 X`, and:

```text
M211 S
M211 X1 Y1 Z1
M1002 push_ref_mode
G91
G1 X10.0 F3000
M1002 pop_ref_mode
M211 R
```

Legacy results have no required feature. Reject the envelope with each surrounding line omitted, with lines reordered, with any token altered, and with an extra eighth command.

- [ ] **Step 4: Run the RED plugin tests**

Run:

```powershell
cargo test -p pandar-network-plugin studio_status -- --nocapture
cargo test -p pandar-network-plugin operation_parser -- --nocapture
```

Expected: failures for missing `fun`, modern commands, wrapper parsing, and exact envelope.

- [ ] **Step 5: Implement typed status and remove shim policy**

Add optional `fun` to `PrinterStatus`, serialize `StudioTelemetry.fun` as the input or canonical `"0"`, and remove only the shim's hardcoded `,"fun":""` fragment. Do not add policy or parsing in C++.

- [ ] **Step 6: Implement bounded modern and legacy parsing**

Use exact typed enums for uppercase axis, signed direction, numeric mode, and required feature. `gcode_line.param` re-enters the bounded parser; it cannot recurse into another JSON wrapper. The seven-line matcher validates all fixed commands before calling the existing movement-number parser.

- [ ] **Step 7: Extend the compiled ABI probe**

The mock Hub returns `"fun":"8000004100000020"`. Assert `push_status.print.fun` contains it exactly. Submit via both cloud and local ABI entrypoints:

- `back_to_center`;
- representative `xyz_ctrl`;
- legacy `gcode_line` `G28 X`;
- legacy seven-line movement.

Mock Hub asserts the modern required-feature fields, asserts legacy requests omit them, and asserts no body contains `G28`, `M211`, `xyz_ctrl`, or `back_to_center` raw transport text.

- [ ] **Step 8: Verify Task 6**

Run:

```powershell
cargo nextest run -p pandar-network-plugin
cargo nextest run -p pandar-network-plugin -E 'test(studio_abi_probe)'
cargo nextest run -p pandar-core --test module_size
```

Expected: Rust parser/status tests and compiled C++ ABI probe pass.

---

### Task 7: Final Spec Gate, Documentation, Verification, Commit, and Push

**Files:**

- Modify after final implementation approval: `docs/roadmap.md`
- Modify after final implementation approval: `docs/development.md`
- Modify after final implementation approval: `docs/compatibility/bambu-studio-plugin.md`
- Modify: `.superpowers/sdd/progress.md`

**Interfaces:**

- Consumes: all reviewed implementation tasks.
- Produces: independently approved spec compliance, explicit no-hardware evidence, clean verification, one conventional commit, and pushed `main`.

- [ ] **Step 1: Run the required final implementation reviewers before docs**

Provide the spec, this reviewed plan, baseline SHA `836b1d626f33eebe7fb2ffd1b6456b0ea96348eb`, full working-tree diff, and fresh focused output to an independent reviewer subagent and default-model OpenCode. Require the exact SDD implementation verdict. If either returns REVISE or omits literal approval, fix, rerun focused verification, and rerun both reviewers.

- [ ] **Step 2: Update the three required docs after dual approval**

Document:

- typed `BambuDeviceFeatures` and exact full-bitmap passthrough;
- current capable-session `fun` advertisement and fail-closed required-feature operations;
- bit 32 `back_to_center`, bit 38 `xyz_ctrl`, and exact legacy `G28 X`/seven-line behavior;
- rollout order Hub -> Agent -> plugin and rollback drain rule;
- explicit statement that no live-printer Home or movement was performed.

- [ ] **Step 3: Run fresh full verification**

Run exactly:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run -p pandar-core --test module_size
cargo nextest run --manifest-path 'Cargo.toml' --workspace
```

Then rerun the compiled ABI test explicitly:

```powershell
cargo nextest run -p pandar-network-plugin -E 'test(studio_abi_probe)'
```

If PostgreSQL is configured, run the focused real PostgreSQL test serially. Otherwise record `PANDAR_TEST_POSTGRES_URL not configured; real PostgreSQL device-feature test skipped`.

- [ ] **Step 4: Audit intended scope**

Run:

```powershell
git status --short
git diff --check
git diff --stat
git diff --name-only
```

Confirm every changed path maps to this plan and none of the pre-existing `probe-*` paths are staged.

- [ ] **Step 5: Commit once with Conventional Commits**

Load the `conventional-commits` skill, stage only the reviewed paths, and create:

```text
feat(studio): support feature-aware axis controls
```

- [ ] **Step 6: Push and read back**

Run:

```powershell
git push origin main
git rev-parse HEAD
git rev-parse origin/main
```

Expected: local and remote SHAs match. If push fails, report the local SHA and exact cause without rewriting history.
