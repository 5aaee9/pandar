# Bambu Studio Native Print Error Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preserve Bambu's numeric `print.print_error` and printer `job_id` through Agent, Hub, and the network plugin, then dispatch Studio's native Resume/Ignore/Stop error actions with the exact direct-LAN payload and session safety.

**Architecture:** Extend the typed additive protobuf and nullable live-status schema first, then add a dedicated typed `HandlePrintError` operation that is persisted as sent and delivered only to the exact capable Agent session. Keep Studio candidate recognition and validation in Rust, keep `shim.cpp` as an ABI adapter, and preserve ordinary durable controls unchanged.

**Tech Stack:** Rust 2024, Tokio, tonic/prost, serde/serde_json, SeaORM/sqlx, SQLite, PostgreSQL, axum, reqwest, rumqttc, C++ Bambu Studio network-plugin ABI, cargo-nextest.

## Global Constraints

- `reference/BambuStudio` is the sole behavior reference for the Studio state and native error-action contract; do not infer `print_error` from HMS, pause state, or `gcode_state`.
- Preserve additive wire numbers exactly: `PrintJobReport` fields 21-24, `AgentHello.capabilities` field 4, and `PrinterOperation.handle_print_error` oneof tag 25.
- Preserve the direct-Studio MQTT field names and types exactly: lower-case `command`, decimal-string `err`, string `job_id`, `param:"reserve"`, and the supplied decimal-string `sequence_id`.
- Publish through the existing `device/<serial>/request` path with QoS 1 and retain disabled.
- `handle_print_error` is plugin-only, requires exact `[PluginStudio]` scope, is never exposed by the tenant control route, and is never selected by the durable queued outbound pump.
- Bind live delivery to the exact `SessionToken` and `AGENT_CAPABILITY_HANDLE_PRINT_ERROR`; version-string checks are forbidden.
- Maintain nullable/presence semantics: absent state does not overwrite, `Some(0)` explicitly clears Studio state, and an empty printer job ID is a real patch.
- Use typed serde/protobuf structures for all known JSON and wire shapes; do not manually mine `serde_json::Value` for known fields.
- Support SQLite and PostgreSQL with equivalent new migrations and real backend tests; a skipped PostgreSQL test is not evidence.
- Preserve full lower-level error context at runtime boundaries and redact credentials, bearer tokens, access codes, request bodies, response bodies, and headers.
- Keep `crates/pandar-network-plugin/src/shim.cpp` limited to ABI adaptation; all parser policy, operation construction, and HTTP diagnostics remain in Rust.
- Do not edit already-applied migrations, add raw MQTT controls, add a Pandar-owned dialog, or implement `mc_print_error_code`.
- Keep modified Rust modules below 400 LOC by extracting focused modules where the touched file would cross that threshold; do not use `include!`.
- Do not commit after individual tasks. The SDD hard gate requires one intentional Conventional Commit only after final independent implementation approval, documentation updates, and fresh verification.
- Do not click Resume, Ignore, or Stop on a real printer without explicit operator permission; automated fake MQTT transport tests prove the action payloads.

---

## File Structure

- `proto/pandar/agent/v1/agent.proto` owns the additive report fields, Agent capability, print-error action enum/message, and oneof tag.
- `crates/pandar-agent/src/machine/mqtt/reports/schema.rs` owns tolerant boundary decoding for numeric error state and printer `job_id`; `reports/protocol.rs` maps progress into the Agent protobuf report.
- `crates/pandar-agent/src/machine/mqtt/commands.rs` and `commands/payload.rs` own the exact Bambu MQTT request shape; `machine/operations.rs` owns transport dispatch.
- `crates/pandar-hub/src/repositories/printers/live_status.rs` owns live status patch/presence behavior; backend migrations own the nullable columns.
- `crates/pandar-hub/src/repositories/commands/operations.rs` owns the semantic operation and validation; `operations/audit.rs` owns flat audit metadata; `commands/audit/printer_operations.rs` owns queued/sent printer-operation audit transactions, with `commands/audit.rs` as the parent/re-export boundary.
- `crates/pandar-hub/src/sessions/live_commands.rs` owns capability/token-bound registration/send, transition claims, and exact-session pending cleanup, keeping `sessions.rs` below 400 LOC.
- `crates/pandar-hub/src/routes/printer_operations.rs` owns distinct tenant/plugin request conversion; `routes/plugin.rs` only orchestrates plugin authorization, persistence, conversion, and dispatch.
- `crates/pandar-network-plugin/src/gcode/studio_json.rs` owns the three-way parser decision; `gcode/operation.rs` owns the REST operation type.
- `crates/pandar-network-plugin/src/http.rs` owns the contextual, redacted one-line stderr diagnostic through an injectable Rust writer.
- `crates/pandar-network-plugin/src/shim.cpp` maps Rust parser outcomes and owns only ABI-level tunnel/callback routing, pre-operation status-request dispatch, and active-local connection state; it adds no printer policy, status JSON construction, or HTTP behavior.

### Task 1: Additive Protocol, Agent Capability, and Tolerant Printer-State Parsing

**Files:**

- Modify: `crates/pandar-agent/Cargo.toml`
- Modify: `proto/pandar/agent/v1/agent.proto`
- Modify: `crates/pandar-agent/src/machine/mqtt.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/commands.rs`
- Create: `crates/pandar-agent/src/machine/mqtt/report_payload.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/reports/schema.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/reports.rs`
- Create: `crates/pandar-agent/src/machine/mqtt/reports/protocol.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/tests.rs`
- Create: `crates/pandar-agent/src/machine/mqtt/tests/print_error.rs`
- Modify: `crates/pandar-agent/src/lib.rs`
- Modify initializers found by: `rg -n "AgentHello \{|PrintJobReport \{" crates`

**Interfaces:**

- Produces generated `AgentCapability::{Unspecified, HandlePrintError}` and an Agent hello containing `capabilities: vec![AgentCapability::HandlePrintError as i32]`.
- Adds `print_error: Option<u32>` and `printer_job_id: Option<String>` to the existing `PrintReportProgress` fields without changing the other fields.
- Produces protobuf presence pairs `print_error/has_print_error` and `printer_job_id/has_printer_job_id` without changing existing `PrintJobReport.job_id` task-ID semantics.
- Later tasks consume the generated `PrintErrorAction`, `HandlePrintErrorOperation`, and `printer_operation::Operation::HandlePrintError` added here.

- [ ] **Step 1: Write failing report-boundary tests for the complete state matrix**

Add `mod print_error;` to `crates/pandar-agent/src/machine/mqtt/tests.rs`. In the new file use the existing `endpoint()`, `print_report_from_report`, and `print_job_report_event` helpers. The core table and presence assertion are:

```rust
use super::*;

#[test]
fn numeric_print_error_matches_studio_int_state_semantics() {
    let cases = [
        (serde_json::json!(0), Some(0)),
        (serde_json::json!(-3), Some(0)),
        (serde_json::json!(12.9), Some(12)),
        (serde_json::json!(i32::MAX), Some(i32::MAX as u32)),
        (serde_json::json!(2147483648_u64), None),
    ];
    for (value, expected) in cases {
        let progress = print_report_from_report(
            &endpoint(),
            &serde_json::json!({"print": {"print_error": value, "mc_percent": 37}}),
        );
        assert_eq!(progress.print_error, expected);
        assert_eq!(progress.percent, Some(37));
    }
}

#[test]
fn zero_print_error_is_state_not_a_generic_diagnostic() {
    let progress = print_report_from_report(
        &endpoint(),
        &serde_json::json!({"print": {"print_error": 0}}),
    );
    assert_eq!(progress.print_error, Some(0));
    assert!(progress.diagnostics.is_empty());
}

#[test]
fn printer_job_id_preserves_presence_and_studio_conversion() {
    let cases = [
        (serde_json::json!(""), Some(String::new())),
        (serde_json::json!("not-a-number"), Some("not-a-number".to_owned())),
        (serde_json::json!(42.9), Some("42".to_owned())),
        (serde_json::json!(9223372036854775808_u64), Some(String::new())),
        (serde_json::json!({"bad": true}), Some(String::new())),
    ];
    for (value, expected) in cases {
        let progress = print_report_from_report(
            &endpoint(),
            &serde_json::json!({"print": {"task_id": "task-7", "job_id": value}}),
        );
        assert_eq!(progress.job_id.as_deref(), Some("task-7"));
        assert_eq!(progress.printer_job_id, expected);
    }
}
```

Also cover: field absence yields `None`; a present null `job_id` yields `Some("")`; string/object `print_error` retains one generic diagnostic; booleans/null do not patch numeric state; a malformed error plus valid HMS/material/progress fields does not drop those fields; `print_job_report_event` sets both presence flags for explicit zero/empty string and leaves them false when absent.

- [ ] **Step 2: Run the new Agent tests and record the expected failure**

Run: `cargo nextest run -p pandar-agent print_error`

Expected: compile failure because `PrintReportProgress` and generated `PrintJobReport` do not yet have the new fields, or assertions fail because numeric zero is still emitted as a diagnostic.

- [ ] **Step 3: Append the exact protobuf contract**

Add the following definitions without renumbering any existing field:

```proto
enum AgentCapability {
  AGENT_CAPABILITY_UNSPECIFIED = 0;
  AGENT_CAPABILITY_HANDLE_PRINT_ERROR = 1;
}

message AgentHello {
  string name = 1;
  string version = 2;
  string credential = 3;
  repeated AgentCapability capabilities = 4;
}

message PrintJobReport {
  // Existing fields 1 through 20 remain unchanged.
  uint32 print_error = 21;
  bool has_print_error = 22;
  string printer_job_id = 23;
  bool has_printer_job_id = 24;
}

enum PrintErrorAction {
  PRINT_ERROR_ACTION_UNSPECIFIED = 0;
  PRINT_ERROR_ACTION_RESUME = 1;
  PRINT_ERROR_ACTION_IGNORE = 2;
  PRINT_ERROR_ACTION_STOP = 3;
}

message HandlePrintErrorOperation {
  PrintErrorAction error_action = 1;
  uint32 print_error = 2;
  string printer_job_id = 3;
  uint64 sequence_id = 4;
}
```

Append `HandlePrintErrorOperation handle_print_error = 25;` to `PrinterOperation.operation`. Add `capabilities: Vec::new()` and `..Default::default()` only where required by generated-struct initializers; the production Agent hello alone advertises `HandlePrintError`.

- [ ] **Step 4: Implement typed tolerant parsing and report presence**

Replace the single `DiagnosticValue` boundary for `print_error` with a typed untagged shape that separates JSON numbers from diagnostic object/string values. Add `#[serde(default, deserialize_with = "deserialize_printer_job_id")]` on the independent `job_id: Option<String>` field: absence uses the default `None`, while the custom deserializer is invoked for every present value (including null) and returns `Some(studio_printer_job_id(&value))`. Implement these conversion contracts:

```rust
fn studio_print_error(number: &serde_json::Number) -> Option<u32> {
    if let Some(value) = number.as_i64() {
        return i32::try_from(value).ok().map(|value| value.max(0) as u32);
    }
    if let Some(value) = number.as_u64() {
        return i32::try_from(value).ok().map(|value| value as u32);
    }
    let value = number.as_f64()?;
    (value.is_finite() && value >= i32::MIN as f64 && value <= i32::MAX as f64)
        .then(|| value.trunc().max(0.0) as u32)
}

fn studio_printer_job_id(value: &ReportJson) -> String {
    match value {
        ReportJson::String(value) => value.clone(),
        ReportJson::Number(number) if number.as_i64().is_some() => {
            number.as_i64().expect("checked above").to_string()
        }
        ReportJson::Number(number) if number.as_u64().is_some() => i64::try_from(
            number.as_u64().expect("checked above"),
        )
        .map(|value| value.to_string())
        .unwrap_or_default(),
        ReportJson::Number(number) => number
            .as_f64()
            .filter(|value| value.is_finite())
            .filter(|value| *value >= -9_223_372_036_854_775_808.0)
            .filter(|value| *value < 9_223_372_036_854_775_808.0)
            .map(|value| (value.trunc() as i64).to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn deserialize_printer_job_id<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = ReportJson::deserialize(deserializer)?;
    Ok(Some(studio_printer_job_id(&value)))
}
```

Do not run numeric `print_error` through `DiagnosticValue::message`. Map object/string diagnostics through the existing payload path. Move `print_job_report_event` and its protobuf field construction into `reports/protocol.rs`, re-export it through the parent, populate the new values plus `has_*` before unwrapping the options, and keep `progress.job_id` sourced only from `task_id`. This extraction keeps `reports.rs` below 400 LOC.

The real MQTT transport must not depend on the `as_f64` branch above for raw numeric `job_id`, because the initial ordinary-precision `Value` parse has already lost the boundary lexeme. Enable serde_json's additive `raw_value` API only for `pandar-agent`, add a focused typed byte-boundary helper that borrows numeric `print.job_id`, performs decimal/exponent signed-64 truncation exactly, and substitutes only that field's canonical string or empty string before the existing `Value`/typed-report pipeline. Add raw-byte RED/GREEN cases for i64 min/max, one beyond both sides, positive/negative boundary fractions, exponent forms, a nonrepresentable huge exponent, strings, invalid shapes, absence, and preservation of another progress field. Do not enable `arbitrary_precision` because its private number-map form changes existing untagged deserialization.

- [ ] **Step 5: Advertise the exact capability and update all generated initializers**

The production hello must contain:

```rust
AgentHello {
    name: config.agent_name.clone(),
    version: config.agent_version.clone(),
    credential: config.agent_credential.clone(),
    capabilities: vec![AgentCapability::HandlePrintError as i32],
}
```

Add a unit assertion in `crates/pandar-agent/src/tests.rs` that the hello has exactly that one capability. Old-Hub compatibility remains protobuf's unknown-field behavior; no version fallback is added.

- [ ] **Step 6: Run focused tests and formatting**

Run: `cargo fmt --all -- --check`

Run: `cargo nextest run -p pandar-agent print_error`

Run: `cargo nextest run -p pandar-agent hello`

Expected: all selected tests pass, explicit zero has `has_print_error == true`, explicit empty printer job ID has `has_printer_job_id == true`, and numeric zero produces no generic diagnostic.

### Task 2: Persist and Expose Live Numeric Error State on Both Database Backends

**Files:**

- Create: `crates/pandar-hub/migrations/sqlite/20260709010000_printer_print_error.sql`
- Create: `crates/pandar-hub/migrations/postgres/20260709010000_printer_print_error.sql`
- Modify: `crates/pandar-hub/src/entities/printers.rs`
- Modify: `crates/pandar-hub/src/repositories/printers/live_status.rs`
- Modify: `crates/pandar-hub/src/repositories/jobs/print_reports.rs`
- Modify: `crates/pandar-hub/src/grpc/print_reports.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/print_reports/live_status.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/printer_live_status.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/postgres.rs`
- Modify: `crates/pandar-hub/src/routes/plugin/studio_devices.rs`
- Modify: `crates/pandar-hub/src/routes/tests/plugin/live_status.rs`

**Interfaces:**

- Consumes Task 1's `PrintJobReport.print_error/has_print_error` and `printer_job_id/has_printer_job_id`.
- Adds `print_error: Option<u32>` and `printer_job_id: Option<String>` to the existing `PrinterLiveStatus` and `PrinterLiveStatusPatch` fields, where each patch `Option` means “patch present”.
- Produces plugin API fields `print_error: Option<u32>` and `job_id: Option<String>` for Task 5.

- [ ] **Step 1: Extend existing SQLite/PostgreSQL shared live-status tests first**

In `exercise_printer_live_status`, seed nonzero and empty-string patches and assert them, send an absent patch and assert preservation, then send explicit zero/new job ID and assert replacement:

```rust
assert_eq!(persisted.live_status.print_error, Some(i32::MAX as u32));
assert_eq!(persisted.live_status.printer_job_id.as_deref(), Some(""));

assert_eq!(preserved.live_status.print_error, Some(i32::MAX as u32));
assert_eq!(preserved.live_status.printer_job_id.as_deref(), Some(""));

assert_eq!(cleared.live_status.print_error, Some(0));
assert_eq!(cleared.live_status.printer_job_id.as_deref(), Some("studio-job-2"));
```

Update the gRPC test with a report containing `print_error: 0`, `has_print_error: true`, `printer_job_id: ""`, `has_printer_job_id: true`; assert that a later report with both flags false preserves both fields. Then seed `print_error: 83_918_929` and send this boundary case:

```rust
handle_print_report(
    &state,
    tenant_id,
    agent_id,
    PrintJobReport {
        serial: "serial".to_owned(),
        print_error: u32::MAX,
        has_print_error: true,
        percent: 73,
        has_percent: true,
        observed_at: "2026-07-09T10:03:00Z".to_owned(),
        ..Default::default()
    },
)
.await
.unwrap();
```

Assert the persisted error remains `Some(83_918_929)` while progress changes to `Some(73)`. This is the failing boundary test that proves an authenticated out-of-domain protobuf value produces no error patch and does not discard other report fields. Update plugin-route JSON assertions to require numeric `"print_error": 83918929` and string `"job_id":"studio-job"`.

- [ ] **Step 2: Run focused Hub tests and record the expected failure**

Run: `cargo nextest run -p pandar-hub printer_live_status`

Run: `cargo nextest run -p pandar-hub grpc_print_job_report_preserves`

Run: `cargo nextest run -p pandar-hub plugin_printer_list_returns_current_external_print_and_hms_snapshot`

Expected: compile failures for missing live-status fields and entity columns.

- [ ] **Step 3: Add equivalent additive migrations and entity fields**

SQLite migration:

```sql
ALTER TABLE printers ADD COLUMN print_error INTEGER;
ALTER TABLE printers ADD COLUMN print_job_id TEXT;
```

PostgreSQL migration:

```sql
ALTER TABLE printers ADD COLUMN print_error INTEGER;
ALTER TABLE printers ADD COLUMN print_job_id TEXT;
```

Add `pub print_error: Option<i32>` and `pub print_job_id: Option<String>` to the SeaORM entity. Rehydrate with `model.print_error.map(u32::try_from).transpose().context("failed to read printer print error")?`; write each present value with `i32::try_from(value).context("failed to persist printer print error")?` so an invalid internal value retains a cause chain instead of silently wrapping.

- [ ] **Step 4: Thread presence-aware patches through gRPC and repositories**

Add these exact fields to both `ApplyPrintReport` and `PrinterLiveStatusPatch`:

```rust
pub print_error: Option<u32>,
pub printer_job_id: Option<String>,
```

Build them only from the protocol flags:

```rust
print_error: report
    .has_print_error
    .then_some(report.print_error)
    .filter(|value| *value <= i32::MAX as u32),
printer_job_id: report
    .has_printer_job_id
    .then_some(report.printer_job_id),
```

The `print_error` filter makes an out-of-domain protobuf value produce no patch without rejecting the rest of the report. Do not trim `printer_job_id`: an explicit empty string is meaningful. In the active model, use `NotSet` on absent patches and `Set(Some(value))` on present patches. Add the two optional fields to `PluginPrinterResponse`, mapping `printer_job_id` to the Studio-facing JSON key `job_id`.

- [ ] **Step 5: Prove both database backends, including real PostgreSQL**

Run: `cargo nextest run -p pandar-hub printer_live_status`

Run with a configured real test database: `if (-not $env:PANDAR_TEST_POSTGRES_URL) { throw 'PANDAR_TEST_POSTGRES_URL is required' }; cargo nextest run -p pandar-hub -E 'test(postgres_print_reports_merge_printer_live_status_without_a_job_when_configured)'`

Expected: both SQLite and PostgreSQL assertions pass for absent, zero, nonzero, empty job ID, and `i32::MAX`. If the named PostgreSQL wrapper uses a different current test name, locate it with `cargo nextest list -p pandar-hub | Select-String 'postgres.*live_status'` and run that exact non-skipped test.

- [ ] **Step 6: Run plugin API tests and formatting**

Run: `cargo fmt --all -- --check`

Run: `cargo nextest run -p pandar-hub plugin --test-threads=1`

Expected: plugin device JSON has optional numeric `print_error` and optional string `job_id`; unknown fields are omitted and existing live status remains unchanged.

### Task 3: Serialize and Execute Exact Native Error Actions in the Agent

**Files:**

- Modify: `crates/pandar-agent/src/machine/mqtt/commands.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/commands/payload.rs`
- Create: `crates/pandar-agent/src/machine/mqtt/commands/print_error.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/transport.rs`
- Modify: `crates/pandar-agent/src/machine/operations.rs`
- Modify: `crates/pandar-agent/src/commands/operations.rs`
- Modify: `crates/pandar-agent/src/commands/operation_results.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/tests/print_error.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/tests.rs`
- Modify: `crates/pandar-agent/src/commands/tests.rs`
- Create: `crates/pandar-agent/src/commands/tests/print_error.rs`
- Modify: `crates/pandar-agent/src/machine/tests.rs`
- Create: `crates/pandar-agent/src/machine/tests/print_error.rs`

**Interfaces:**

- Consumes Task 1's generated `PrintErrorAction` and `HandlePrintErrorOperation`.
- Produces Agent domain `PrintErrorAction` and `PrinterOperation::HandlePrintError { error_action, print_error, printer_job_id, sequence_id }`.
- Produces `BambuMqttCommand::HandlePrintError(HandlePrintErrorCommand)` whose `BambuMqttCommandPayload.sequence_id` is the supplied Studio sequence.

- [ ] **Step 1: Write failing field-for-field MQTT payload tests**

Extend the small `mqtt/tests/print_error.rs` module and table-test all three actions:

```rust
use super::*;

#[test]
fn native_print_error_actions_match_studio_payloads() {
    for (action, command) in [
        (PrintErrorAction::Resume, "resume"),
        (PrintErrorAction::Ignore, "ignore"),
        (PrintErrorAction::Stop, "stop"),
    ] {
        let payload = BambuMqttCommand::HandlePrintError(HandlePrintErrorCommand {
            error_action: action,
            print_error: 83_918_929,
            printer_job_id: "job-7".to_owned(),
            sequence_id: 20_042,
        })
        .command_payload();
        assert_eq!(payload.sequence_id.as_deref(), Some("20042"));
        assert_eq!(payload.payload, serde_json::json!({
            "print": {
                "command": command,
                "err": "83918929",
                "job_id": "job-7",
                "param": "reserve",
                "sequence_id": "20042"
            }
        }));
    }
}
```

Extend the fake MQTT dispatch test to assert topic `device/01S00EXAMPLE/request`, `qos == BAMBU_MQTT_QOS`, and `BAMBU_MQTT_RETAIN == false`; make `mqtt/transport.rs` use that constant in its rumqttc publish call. Add command-conversion tests in `commands/tests/print_error.rs` that unspecified and unknown protobuf enum values fail without invoking `BambuMachineGateway::operate_printer`, and machine dispatch tests in `machine/tests/print_error.rs` for the exact published request and result correlation.

- [ ] **Step 2: Run focused Agent command tests and record the expected failure**

Run: `cargo nextest run -p pandar-agent native_print_error`

Expected: compile failure because the Agent domain/MQTT variants do not exist.

- [ ] **Step 3: Add the typed Agent domain and protobuf conversion**

Use an exhaustive enum rather than strings:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintErrorAction {
    Resume,
    Ignore,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlePrintErrorCommand {
    pub error_action: PrintErrorAction,
    pub print_error: u32,
    pub printer_job_id: String,
    pub sequence_id: u64,
}
```

Map generated enum values with `PrintErrorAction::try_from(operation.error_action)` and return an `anyhow` error for `Unspecified` or unknown values. Validate `print_error` with `(1..=i32::MAX as u32).contains(&print_error)` before constructing the machine operation.

- [ ] **Step 4: Serialize the exact Bambu shape and preserve correlation**

Add a typed payload struct in `commands/payload.rs`:

```rust
#[derive(Serialize)]
pub(super) struct PrintErrorCommand<'a> {
    pub(super) command: &'static str,
    pub(super) err: String,
    pub(super) job_id: &'a str,
    pub(super) param: &'static str,
    pub(super) sequence_id: String,
}
```

Put the new builder and action-to-command mapping in `commands/print_error.rs` because `commands.rs` is already near 400 LOC. Set both JSON `sequence_id` and `BambuMqttCommandPayload.sequence_id` from the caller-supplied `u64`; never call `next_studio_sequence_id()` for this variant. Route it through the existing subscribe/publish/result-correlation path so QoS, topic, and retain behavior remain shared.

- [ ] **Step 5: Run the focused tests and all Agent tests**

Run: `cargo fmt --all -- --check`

Run: `cargo nextest run -p pandar-agent native_print_error`

Run: `cargo nextest run -p pandar-agent`

Expected: all three exact payloads, transport properties, supplied sequence correlation, and invalid-enum rejection pass.

### Task 4: Add the Plugin-Only Semantic Operation and Capability/Token-Bound Hub Dispatch

**Files:**

- Modify: `crates/pandar-hub/src/repositories/commands/operations.rs`
- Create: `crates/pandar-hub/src/repositories/commands/operations/audit.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/audit.rs`
- Create: `crates/pandar-hub/src/repositories/commands/audit/printer_operations.rs`
- Create: `crates/pandar-hub/src/repositories/commands/audit/printer_operations/ownership_pause.rs` (`cfg(test)` only)
- Modify: `crates/pandar-hub/src/repositories/commands/ownership.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/transitions.rs`
- Modify re-exports in: `crates/pandar-hub/src/repositories/commands.rs`
- Modify: `crates/pandar-hub/src/routes/printer_operations.rs`
- Create: `crates/pandar-hub/src/routes/printer_operations/live.rs`
- Modify: `crates/pandar-hub/src/routes/plugin.rs`
- Modify: `crates/pandar-hub/src/grpc/commands/conversion.rs`
- Modify: `crates/pandar-hub/src/grpc/commands.rs`
- Modify: `crates/pandar-hub/src/sessions.rs`
- Create: `crates/pandar-hub/src/sessions/live_commands.rs`
- Modify: `crates/pandar-hub/src/grpc.rs`
- Modify: `crates/pandar-hub/src/grpc/inbound.rs`
- Create: `crates/pandar-hub/src/grpc/inbound/commands.rs`
- Modify: `crates/pandar-hub/src/cluster.rs`
- Modify: `crates/pandar-hub/src/cluster/tests.rs`
- Modify: `crates/pandar-hub/src/lib.rs`
- Modify: `crates/pandar-hub/src/runtime.rs`
- Modify: `crates/pandar-hub/src/runtime/tests.rs`
- Create: `crates/pandar-hub/src/runtime/tests/print_error.rs`
- Create: `crates/pandar-hub/src/runtime/tests/control_plane_close.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/commands.rs`
- Create: `crates/pandar-hub/src/repositories/tests/commands/print_error.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/postgres_commands.rs`
- Create: `crates/pandar-hub/src/repositories/tests/postgres_commands/print_error.rs`
- Modify: `crates/pandar-hub/src/routes/tests/plugin.rs`
- Create: `crates/pandar-hub/src/routes/tests/plugin/operations.rs`
- Modify: `crates/pandar-hub/src/routes/tests/printer_commands.rs`
- Create: `crates/pandar-hub/src/routes/tests/printer_commands/print_error.rs`
- Modify: `crates/pandar-hub/src/sessions/tests.rs`
- Create: `crates/pandar-hub/src/sessions/tests/print_error.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/commands.rs`
- Create: `crates/pandar-hub/src/grpc/tests/commands/print_error.rs`
- Create: `crates/pandar-hub/src/grpc/tests/commands/print_error/reconnect.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/lifecycle.rs`
- Modify initializers found by: `rg -n "AgentSession \{" crates/pandar-hub/src`

**Interfaces:**

- Produces repository `PrintErrorAction` and `PrinterOperationKind::HandlePrintError { error_action, print_error, printer_job_id, sequence_id }`.
- Produces `PrinterOperationRequest::into_plugin_operation() -> Result<PluginPrinterOperation, ApiError>` where `PluginPrinterOperation::{Queued, Live}` separates the route behaviors; existing `into_operation()` remains tenant-safe and rejects the new action.
- Produces `CommandRepository::create_printer_operation_sent_with_audit(tenant_id: TenantId, printer_id: &str, expected_agent_id: AgentId, operation: PrinterOperationKind, actor: AuditActor) -> RepositoryResult<CommandRecord>`.
- Produces `SessionRegistry::current_token_for_capability(tenant_id: TenantId, agent_id: AgentId, capability: AgentCapability) -> Option<SessionToken>` and the fully specified `try_dispatch_live_command_with_capability` method in Step 6; existing unqualified live dispatch remains for link-printer.
- Produces `LiveCommandClaimOutcome::{Claim, NotCurrent, NotPending}` and a transition permit that serializes live ack/result updates against exact-session cleanup.
- Produces reusable `fail_pending_live_commands(state, tenant_id, agent_id, session, reason)` in `sessions/live_commands.rs` for replacement, current close, forced close, and expiry.
- Produces `live_printer_operation_hub_command(command_id: CommandId, serial_number: String, operation: PrinterOperationKind) -> HubCommand` for the committed sent/live path; the record-based durable converter rejects `HandlePrintError` with `failed_precondition`.
- Extends stale unowned-live-command recovery to typed `HandlePrintError` records.

- [ ] **Step 1: Write failing request, persistence, protobuf, and route tests**

Add exact-body tests for `resume`, `ignore`, and `stop`:

```json
{
  "action": "handle_print_error",
  "error_action": "resume",
  "print_error": 83918929,
  "printer_job_id": "job-7",
  "sequence_id": 20042
}
```

For each action assert: HTTP success only through the plugin route; command status is `sent`; payload `operation.type` is `handle_print_error`; audit metadata is exactly flat and includes the five action fields plus existing agent/serial fields; emitted protobuf has oneof tag 25 and exact values. Add negative cases for zero, `i32::MAX + 1`, missing/unknown `error_action`, missing/extra/cross-operation fields, and the tenant route returning `invalid_printer_control`.

Exercise the same sent payload/audit contract in focused `repositories/tests/commands/print_error.rs` and `repositories/tests/postgres_commands/print_error.rs` modules for SQLite and a real PostgreSQL repository wrapper. Their oversized parent test files only register `mod print_error;`. Assert no `handle_print_error` record is created with `queued` status.

Add runtime lifecycle cases in `runtime/tests/print_error.rs`; the oversized parent only registers the child module. These tests must exercise the actual runtime callers for forced local/cluster close, stale expiry, and stale unowned-command recovery rather than testing only registry helpers.

- [ ] **Step 2: Write failing session and replacement-race tests**

Construct sessions with explicit capability sets and assert:

```rust
assert!(registry
    .current_token_for_capability(tenant_id, agent_id, AgentCapability::HandlePrintError)
    .await
    .is_none());
```

Put the new registry races in `sessions/tests/print_error.rs` and the new gRPC command cases in `grpc/tests/commands/print_error.rs`; oversized parent files only register their child module. Cover offline, incapable, capable-to-incapable replacement, incapable-to-capable replacement, and a stale token after command persistence. Exercise both claim/replacement lock orders: a terminal result claim that wins first completes and removes itself before cleanup, while replacement that wins first drains/fails the old pending command and makes the event `NotCurrent`. Exercise accepted ack followed by replacement, forced local/cluster close, normal close, stale expiry, and stale unowned-command recovery. Assert each path terminalizes only the exact old/removed/unowned command, preserves the current replacement session, and emits no oneof tag 25 to a wrong stream. Retain and extend the existing link-printer tests so access-code redaction, accepted ack, terminal result, duplicate/late result, forced close, and replacement now pass through the same claim path without regression.

- [ ] **Step 3: Run focused Hub tests and record the expected failure**

Run: `cargo nextest run -p pandar-hub -E 'test(handle_print_error)'`

Run: `cargo nextest run -p pandar-hub -E 'test(sessions::tests) | test(grpc::tests::lifecycle)'`

Expected: compile failures for the new operation, capability set, sent persistence method, and capability-aware dispatch methods.

- [ ] **Step 4: Add the exact semantic operation, validation, and flat audit metadata**

Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrintErrorAction { Resume, Ignore, Stop }

HandlePrintError {
    error_action: PrintErrorAction,
    print_error: u32,
    printer_job_id: String,
    sequence_id: u64,
},
```

Return action string `"handle_print_error"`; validate `1..=i32::MAX as u32`; and move the existing audit metadata types/functions into `operations/audit.rs`, adding `OperationAuditFields::PrintError` there with `error_action`, `print_error`, `printer_job_id`, and `sequence_id`. Re-export `operation_audit_metadata` through `operations.rs`. Keep the serialized persisted discriminator `"type":"handle_print_error"` and keep `operations.rs` below 400 LOC.

Split the existing queued printer-operation audit transaction from near-limit `commands/audit.rs` into `commands/audit/printer_operations.rs`, leaving the public wrapper/re-export stable. Implement `create_printer_operation_sent_with_audit` beside it by following `create_link_printer_sent_with_audit`, but require the route's `expected_agent_id`: SQLite begins an `IMMEDIATE` transaction, PostgreSQL selects the tenant/printer/expected-Agent row `FOR UPDATE`, and ownership revalidation, typed `kind:"printer_operation"` sent insert, and audit insert share the same transaction. Ownership mismatch returns `PrinterControlUnavailable` with zero command/audit rows. Make the ordinary queued `enqueue_printer_operation_with_audit` reject `HandlePrintError` with `RepositoryError::InvalidPrinterControl`; add repository plus deterministic SQLite/real-PostgreSQL A→B reassignment tests proving no stale Agent receives tag 25. Keep both parent modules below 400 LOC.

Put the deterministic pre-transaction reassignment hook in the `cfg(test)`-only `commands/audit/printer_operations/ownership_pause.rs`; production must not compile or call it. `install(printer_id)` assigns a monotonic generation, rejects a duplicate entry without poisoning the shared mutex, and returns a guard whose matching-generation `Drop` removes only its own key. `wait_until_reached()` has a five-second bound and delegates to a shorter test-injectable timeout. Add the three named unit regressions `unreachable_pause_times_out_and_removes_only_its_key`, `aborting_pause_wait_removes_only_its_key`, and `duplicate_pause_install_is_rejected`; each cleanup case must allow a subsequent install and must never remove a newer generation.

- [ ] **Step 5: Separate plugin and tenant request conversion**

Add optional request fields with `deny_unknown_fields`, then require the exact native shape only in `into_plugin_operation`:

```rust
pub(super) enum PluginPrinterOperation {
    Queued(PrinterOperationKind),
    Live(PrinterOperationKind),
}
```

`into_operation()` must contain no `handle_print_error` match and its ordinary-field guards must require every native field to be absent. `into_plugin_operation()` returns `Live` only when all four native fields are present, no ordinary-control field is present, and repository validation accepts the positive signed-32 error range; otherwise it returns `invalid_printer_control`. All existing actions return `Queued(self.into_operation()?)`.

- [ ] **Step 6: Add capability-aware session storage and atomic live dispatch**

Store `HashSet<AgentCapability>` and `live_command_transition: Arc<tokio::sync::Mutex<()>>` on `AgentSession`, populating capabilities from `hello.capabilities` with `filter_map(|value| AgentCapability::try_from(value).ok())`. In `sessions/live_commands.rs`, retain the registry lock across exact tenant/agent/token/capability validation, pending registration, and `try_send`:

```rust
pub async fn try_dispatch_live_command_with_capability(
    &self,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    capability: AgentCapability,
    command_id: CommandId,
    command: HubCommand,
) -> Result<(), LiveDispatchError>
```

On stale token or missing capability return `NotCurrent` before registering/sending. On closed/full channel remove only the just-registered pending command. Preserve the existing link-printer method without imposing a capability.

Add `claim_current_live_command(tenant_id, agent_id, token, command_id) -> LiveCommandClaimOutcome`. While holding the registry lock it verifies the exact session and pending ID, then acquires an owned permit from that session's `live_command_transition` and rechecks the entry. The returned claim owns the permit and a clone of the pending map, so the inbound handler can hold it across the repository transition and remove a rejected/terminal entry without re-entering the registry. Accepted ack keeps the entry. This replaces `while_current` for every entry in `pending_live_commands`, including link-printer; `NotCurrent` is ignored and only `NotPending` uses the existing durable-command path.

Use these concrete ownership types:

```rust
pub enum LiveCommandClaimOutcome {
    Claim(LiveCommandClaim),
    NotCurrent,
    NotPending,
}

pub struct LiveCommandClaim {
    command_id: CommandId,
    pending: PendingLiveCommands,
    access_code: Option<String>,
    _transition: tokio::sync::OwnedMutexGuard<()>,
}

impl LiveCommandClaim {
    pub fn access_code(&self) -> Option<&str> {
        self.access_code.as_deref()
    }

    pub fn remove_pending(&self) {
        self.pending
            .lock()
            .expect("pending live commands mutex should not be poisoned")
            .remove(&self.command_id);
    }
}
```

The method returns `NotCurrent` for tenant/token mismatch before waiting. For a current session, it acquires `session.live_command_transition.clone().lock_owned().await` while the registry guard is held, then rechecks `pending_live_commands` and returns `NotPending` if the entry was removed by an earlier serialized event. Enforce one lock order everywhere: registry → current-session transition → pending map; code holding a transition claim never re-enters the registry, and detached-session cleanup releases the registry before taking that session's transition.

Move the current ack/result branches and their link/live result helpers into `grpc/inbound/commands.rs`. Have them call this method before `while_current`: `Claim` holds the permit through `handle_ack_and_job`/`handle_result_for_command` and performs the pending-map update directly; `NotCurrent` returns `Ok(())`; `NotPending` first loads the authenticated Agent's persisted command and uses typed `PrinterOperationPayload` inspection. It ignores unclaimed `link_printer` and `HandlePrintError` commands and executes the existing ordinary path only for durable command kinds. The claim-held path must not call any `SessionRegistry` method before dropping the claim. Add deterministic close/expiry→reconnect ack/result races plus an ordinary durable fallback regression. `inbound.rs` only delegates these two event variants, keeping it below 400 LOC.

- [ ] **Step 7: Convert, persist, dispatch, and fail atomically at the route boundary**

Map the repository enum exhaustively to generated `PrintErrorAction` and `HandlePrintErrorOperation`. In the plugin handler:

1. authorize exact PluginStudio scope;
2. resolve the printer/Agent and current token only when the capability is present;
3. create the command directly in sent state with audit;
4. build its typed `HubCommand` from the same typed operation with `live_printer_operation_hub_command` only after persistence succeeds;
5. call `try_dispatch_live_command_with_capability` with the captured token;
6. on dispatch error, `mark_failed` with the full internal reason and return stable `printer_operation_unavailable`.

Put the plugin live orchestration behind a focused helper in `routes/printer_operations.rs`; the authenticated `routes/plugin.rs` handler delegates to it so that file stays below 400 LOC. Do not call `wake_agent` for the live variant. Keep ordinary variants on the existing queued persistence plus wake path. Have the dedicated live builder call the exhaustive `proto_printer_operation` mapping. In the record-based durable `"printer_operation"` arm, detect a deserialized `HandlePrintError`, log the command ID with the full conversion context, and return `Status::failed_precondition("print error operation requires live dispatch")` before building a command. The queued repository entry point also rejects this variant.

Re-export `live_printer_operation_hub_command` through `grpc/commands.rs`; the route must not reach into the private `conversion` module.

- [ ] **Step 8: Consume replaced sessions before starting replacement pumps**

Move pending claim/drain behavior to `sessions/live_commands.rs`:

```rust
pub(crate) async fn fail_pending_live_commands(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    session: AgentSession,
    reason: &'static str,
)
```

Immediately consume the `Option<AgentSession>` returned by `SessionRegistry::register` and call it with `agent session replaced before printer operation completed` before spawning inbound/outbound pumps. Current-stream close calls the same function only after token-scoped `remove_if_current`, with `agent connection closed before printer operation completed`. Log `format!("{err:#}")` on failure. Never look the old session up by agent ID after replacement and never remove the new session.

Change `close_local_agent` to return the exact removed session, and make both `AppState::close_agent` and the cluster close-message handler pass it to the helper with `agent session closed before printer operation completed`. Move the near-limit `AppState::close_agent` implementation into `sessions/live_commands.rs` while keeping its public API stable so `lib.rs` stays below 400 LOC. Give each constructed `AppState` a stable UUID instance ID (preserved by `Clone`, distinct for sibling instances) and include it as `source_instance_id` on `AgentClose`; the runtime ignores same-source delivery because the source already detached its exact target, but still detaches the current exact session for cross-source messages. Add a real-control-plane regression holding S1 cleanup while S2 registers. Make stale expiry push only the `AgentSession` actually returned by `remove_if_current` (not a pre-removal clone), then drain each exact removed session with `agent session expired before printer operation completed`. Each cleanup acquires the removed session's `live_command_transition` before draining, so an in-flight claim either completes first or loses before it can write.

Rename/extend the existing stale link-printer cleanup to include sent/acknowledged `printer_operation` candidates older than the live-command timeout. Fetch backend-neutral candidates, deserialize `PrinterOperationPayload`, select only `HandlePrintError`, exclude IDs still present in local pending maps, and conditionally fail them with `live printer operation owner unavailable before completion`. Add SQLite and real PostgreSQL tests. Do not write backend-specific JSON SQL.

Document and test the process-local behavior: `SessionRegistry` and the stale-recovery owner set do not coordinate across Hub replicas, so a non-owning process returns `printer_operation_unavailable` and could misclassify another replica's pending command. Native print-error actions therefore require one active Hub process; session affinity alone is insufficient.

- [ ] **Step 9: Run focused Hub tests, including PostgreSQL**

Run: `cargo fmt --all -- --check`

Run: `cargo nextest run -p pandar-hub -E 'test(handle_print_error)'`

Run: `cargo nextest run -p pandar-hub -E 'test(sessions::tests) | test(grpc::tests::lifecycle)'`

Run with the configured real database: `if (-not $env:PANDAR_TEST_POSTGRES_URL) { throw 'PANDAR_TEST_POSTGRES_URL is required' }; cargo nextest run -p pandar-hub -j 1 -E 'test(postgres_handle_print_error_when_configured)'`

Expected: exact request/payload/audit/protobuf tests pass; tenant route rejection passes; all stale/replacement/capability races pass; the real PostgreSQL test executes rather than skips.

### Task 5: Emit Native Error State in Synthesized Studio Telemetry

**Files:**

- Modify: `crates/pandar-network-plugin/Cargo.toml`
- Modify: `crates/pandar-network-plugin/src/studio_status.rs`
- Modify: `crates/pandar-network-plugin/src/studio_status/input.rs`
- Modify: `crates/pandar-network-plugin/src/studio_status/device.rs`
- Modify: `crates/pandar-network-plugin/src/studio_status/list.rs`
- Create: `crates/pandar-network-plugin/src/studio_status/request.rs`
- Modify: `crates/pandar-network-plugin/src/shim.cpp`
- Modify: `crates/pandar-network-plugin/tests/studio_status.rs`
- Create: `crates/pandar-network-plugin/tests/status_request.rs`
- Modify: `crates/pandar-network-plugin/tests/fixtures/studio_abi_probe.cpp`
- Modify: `crates/pandar-network-plugin/tests/studio_abi_probe.rs`
- Create: `crates/pandar-network-plugin/tests/studio_abi_probe/native_print_error.rs`

**Interfaces:**

- Consumes Task 2's plugin JSON `print_error: Option<u32>` and `job_id: Option<String>`.
- Produces optional typed `print_error` and `job_id` fields in the Rust-generated `print.push_status` telemetry fragment; `shim.cpp` continues to insert that fragment without interpretation.
- Produces explicit cloud `on_message` versus LAN `on_local_message` status routing, native `on_local_connect` connection signaling, separate local `get_version`/`pushall` responses, and an active-local heartbeat target without changing printer-operation policy.

- [ ] **Step 1: Write failing telemetry and ABI type assertions**

Add three serde-json assertions:

```rust
assert_eq!(telemetry["print_error"], serde_json::json!(83_918_929));
assert_eq!(telemetry["job_id"], serde_json::json!("job-7"));
assert_eq!(cleared["print_error"], serde_json::json!(0));
assert!(unknown.get("print_error").is_none());
assert!(unknown.get("job_id").is_none());
```

Register `mod native_print_error;` in the oversized Rust ABI harness and put new Rust assertions in `tests/studio_abi_probe/native_print_error.rs`; the parent only exposes the existing harness helpers. In the C++ fixture, register both message callbacks before exercising either transport and maintain separate cloud/local push-status and version counters. Inspect each callback's final `print.push_status` object and require `print_error` to be a JSON number and `job_id` to be a JSON string, including a separate explicit-zero refresh.

Assert cloud subscribe/heartbeat and cloud `get_version`/`pushall` increment only cloud message counters. Assert `connect_printer` invokes `on_local_connect(ConnectStatusOk, ...)` only: it must not invoke cloud `on_printer_connected`, either message callback, or change `get_user_selected_machine`. Then assert local `get_version`, local `pushall`, and a local heartbeat increment only local message counters; the local status contains current progress/HMS/materials as well as the new fields. On each tunnel, `get_version` emits only `info.get_version` and `pushall` emits only `print.push_status`. Both local status requests return success and create zero Hub operation POSTs. With the message callback for one tunnel absent, assert its messages are not delivered to the other callback.

Use a distinct cloud subscription serial and active-local serial to prove independent heartbeat targets. Explicitly unsubscribe the active-local serial, force a successful cache refresh, and assert it is not re-added to the cloud target set. Connect a second local serial and assert it replaces the first without changing account selection. After `disconnect_printer`, wait beyond one heartbeat interval and assert the local counter remains fixed while the cloud counter advances. Keep a separate same-serial case by deliberately subscribing that active-local serial and assert one emission reaches each explicit tunnel rather than being deduplicated or routed by callback presence.

Make the C++ fixture compilation mandatory on Windows and Unix. Add `cc` as a dev dependency and, for the MSVC target, use `cc::windows_registry::find_tool(std::env::consts::ARCH, "cl.exe")` plus the returned `Tool::to_command()` so the same MSVC discovery and environment used by the build script is available even when `cl.exe` is not on `PATH`; honor an explicit `CXX` first. Compile the MSVC fixture with `/MD` and `/D_ITERATOR_DEBUG_LEVEL=0`, matching `build.rs` and Studio's STL runtime ABI across the DLL boundary. Select MSVC versus GNU flag families from `cfg!(target_env = "msvc")`, not from the compiler filename, so `clang-cl` is not misclassified. On Windows GNU and Unix, retain `CXX`/standard compiler lookup but panic if none works; add `-pthread` on Unix for the fixture's thread usage. Change `compile_probe` and `run_probe` to non-optional results and remove every `let Some(...) else { return; }` skip. Unsupported platforms may omit the test with `cfg`, but a supported-platform discovery or compilation failure is a hard test failure.

- [ ] **Step 2: Run focused plugin tests and record the expected failure**

Run: `cargo nextest run -p pandar-network-plugin studio_status`

Run: `cargo nextest run -p pandar-network-plugin studio_abi_probe`

Expected: telemetry assertions fail because the new fields are absent; callback assertions also expose the current cloud-first direct-connect routing, the incorrect cloud `on_printer_connected` and immediate status emissions during LAN connect, coupled `get_version` plus push-status responses, missing local `get_version`/`pushall` handling, missing local heartbeat target/lifecycle, account-selection mutation, and the current false-success ABI probe skip when MSVC is not on `PATH`.

- [ ] **Step 3: Add optional typed fields and explicit ABI tunnel routing**

Add to `PrinterStatus`:

```rust
#[serde(default)]
pub(super) print_error: Option<u32>,
#[serde(default)]
pub(super) job_id: Option<String>,
```

Add to `StudioTelemetry`:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
print_error: Option<u32>,
#[serde(skip_serializing_if = "Option::is_none")]
job_id: Option<String>,
```

Clone/map the fields directly. Do not default unknown error state to zero and do not put any error-code policy in C++.

Add `_print_error: Option<u32>` and `_job_id: Option<String>` to the typed `studio_status/list.rs` Hub-response validator as well. This ensures a wrong Hub field type fails refresh validation at the boundary instead of causing `printer_telemetry_fragment` to default the complete printer state.

In `shim.cpp`, add a small ABI-only `MessageTunnel::{Cloud, Local}` discriminator. Make the status emitter capture only the callback for that tunnel, with no fallback. Store a single `active_local_device` independently from account selection; set/replace it in `connect_printer`, clear it in `disconnect_printer`, and include it beside cloud subscriptions in heartbeat snapshots. `connect_printer` invokes only `on_local_connect(ConnectStatusOk, ...)`; remove its cloud `on_printer_connected` call, do not mutate `selected_machine`, and do not emit an immediate status message. Remove the implicit `cloud_subscribed_devices.insert(discovered_devices...)` mutation from `remember_printer_connections`: refresh updates connection/model/telemetry caches only, while existing explicit subscribe/unsubscribe and account-selection entrypoints own the cloud target set. Refresh the Hub cache once per existing Pandar heartbeat, then emit each cloud subscription through `on_message` and the active local device through `on_local_message`; if a serial is deliberately in both sets, emit once on each explicit tunnel.

Extract the existing cloud `get_version`/`pushall` special cases into one shared ABI status-request helper used before `StudioOperationParse` by both send entrypoints. Put exact typed top-level classification (`info.command == "get_version"`, `pushing.command == "pushall"`) behind a flat Rust FFI returning only a numeric kind and typed sequence string; `shim.cpp` must not use substring searches or parse command JSON. Preserve cloud-only initialization/connection-notification bookkeeping. A `get_version` request returns success after emitting only the version report; a `pushall` request refreshes the cache, returns success, and emits only the status report. For the local tunnel, route both response families only through `on_local_message` and make zero Hub operation requests. Add Rust and compiled Cloud/LAN collision tests for lookalike commands and native job IDs containing both substrings. Reuse the existing Rust telemetry fragment and existing version/status builders; do not add error-code interpretation or command construction in C++.

- [ ] **Step 4: Run telemetry and ABI tests**

Run: `cargo fmt --all -- --check`

Run: `cargo nextest run -p pandar-network-plugin studio_status`

Run: `cargo nextest run -p pandar-network-plugin studio_abi_probe`

Expected: nonzero and zero remain JSON numbers, unknown fields are omitted, job ID remains a JSON string, existing progress/HMS/material telemetry remains live on both tunnels, LAN connect uses only `on_local_connect`, local Sync AMS refresh requests do not enter operation parsing, `get_version` and `pushall` emit only their own response families through the explicit tunnel, successful refresh does not turn a local-only serial into a cloud target, a second local connect replaces the heartbeat target without changing account selection, disconnect clears only the local heartbeat target, and the mandatory ABI-compatible compiled C++ probe actually executes.

### Task 6: Parse Native Studio Actions, Preserve ABI Differences, and Log Full Redacted HTTP Causes

**Files:**

- Modify: `crates/pandar-network-plugin/src/gcode/operation.rs`
- Modify: `crates/pandar-network-plugin/src/gcode/studio_json.rs`
- Modify: `crates/pandar-network-plugin/src/gcode.rs`
- Modify: `crates/pandar-network-plugin/src/lib.rs`
- Modify: `crates/pandar-network-plugin/src/http.rs`
- Create: `crates/pandar-network-plugin/src/http/tests.rs`
- Modify: `crates/pandar-network-plugin/src/shim.cpp`
- Create: `crates/pandar-network-plugin/tests/native_print_error.rs`
- Modify: `crates/pandar-network-plugin/tests/http_boundary.rs`
- Create: focused child modules under `crates/pandar-network-plugin/tests/http_boundary/`
- Modify: `crates/pandar-network-plugin/tests/fixtures/studio_abi_probe.cpp`
- Modify: `crates/pandar-network-plugin/tests/studio_abi_probe/native_print_error.rs`

**Interfaces:**

- Extends plugin `PrinterOperation` with REST variant `HandlePrintError { error_action, print_error, printer_job_id, sequence_id }` and lower-case `PrintErrorAction`.
- Produces `StudioOperationParse::{Operation(PrinterOperation), Unsupported, InvalidNativeCandidate}` entirely in Rust.
- Exposes those three outcomes across the flat C ABI using stable integer statuses documented beside the export; C++ only adapts them to the two Studio entrypoint contracts.
- Preserves Task 5's explicit status-request handling before operation parsing in both ABIs.
- Produces `post_json_with_writer<W: std::io::Write>(url: &str, token: Option<&str>, body: impl Serialize, kind: RequestKind, writer: &mut W) -> PluginHttpResult` for deterministic diagnostics; production `post_json` passes locked stderr.

- [ ] **Step 1: Write the complete parser decision-table tests before implementation**

Use data-driven cases for:

```rust
let valid = [
    ("resume", "resume"),
    ("ignore", "ignore"),
    ("stop", "stop"),
];
for (command, error_action) in valid {
    let input = format!(r#"{{"print":{{"command":"{command}","err":"83918929","job_id":"","param":"reserve","sequence_id":"20042"}}}}"#);
    assert_eq!(parse(&input), serde_json::json!({
        "action":"handle_print_error",
        "error_action":error_action,
        "print_error":83918929,
        "printer_job_id":"",
        "sequence_id":20042
    }));
}
```

Add ordinary cases for Resume/Stop with no `err` and absent/empty-string `param`, including `job_id` or `sequence_id` alone with valid, null, or wrong-typed values. A missing or non-string `command` is unsupported because it cannot identify a native candidate. For a command-known native candidate, add invalid-native cases for each remaining missing required field (`param`, `err`, `job_id`, `sequence_id`); wrong field types; `param` absent/empty/not `reserve`; `err` zero/negative/nondecimal/greater than `i32::MAX`; sequence negative/nondecimal/greater than `u64::MAX`; and every `ignore` shape except the complete one. Assert invalid candidates never become ordinary controls and unsupported unrelated commands remain distinct.

- [ ] **Step 2: Add failing cloud/local ABI and HTTP diagnostic tests**

At the flat Rust parser FFI boundary, assert statuses 1 and 2 both return the exact stable `{"error":"unsupported_printer_operation"}` body. Then extend the compiled ABI probe table to assert exact return code, observable `is_server_connected`/error-state transition, and Hub request count. Do not add a production ABI solely to inspect the shim's private `last_error` string:

`get_version` and `pushall` are transport/status requests handled before `StudioOperationParse`; they are not unsupported printer operations in either ABI and retain Task 5's zero-POST success behavior.

| Outcome        | Cloud send                                                  | Local send                                                  |
| -------------- | ----------------------------------------------------------- | ----------------------------------------------------------- |
| Valid          | submit once; propagate result                               | submit once; propagate result                               |
| Unsupported    | success; last error unchanged; zero POSTs                   | invalid result; `unsupported_printer_operation`; zero POSTs |
| Invalid native | invalid result; `unsupported_printer_operation`; zero POSTs | invalid result; same error; zero POSTs                      |

In the internal `http/tests.rs`, call the writer-injected helper against a refused local connection and assert:

```rust
assert_eq!(external_body, r#"{"error":"hub_unavailable"}"#);
let diagnostic = String::from_utf8(buffer).unwrap();
assert_eq!(diagnostic.lines().count(), 1);
assert!(diagnostic.contains("POST plugin printer operation request"));
let lower = diagnostic.to_ascii_lowercase();
assert!(
    lower.contains("connection refused")
        || lower.contains("actively refused")
        || lower.contains("os error 10061")
);
assert!(!diagnostic.contains("Bearer"));
assert!(!diagnostic.contains("secret-token"));
```

- [ ] **Step 3: Run focused plugin tests and record the expected failure**

Run: `cargo nextest run -p pandar-network-plugin --test native_print_error`

Run: `cargo nextest run -p pandar-network-plugin http_boundary`

Run: `cargo nextest run -p pandar-network-plugin studio_abi_probe`

Expected: parser tests show native Resume/Stop downgrading or Ignore unsupported; ABI outcome distinction and HTTP cause-chain assertions fail.

- [ ] **Step 4: Implement the typed three-way parser and exact REST operation**

Model the direct fields as typed string/number/wrong-type values so presence and wrong types survive deserialization:

```rust
#[derive(Deserialize)]
struct StudioPrint {
    command: String,
    #[serde(default, deserialize_with = "deserialize_studio_field_presence")]
    param: StudioFieldPresence,
    #[serde(default, deserialize_with = "deserialize_studio_field_presence")]
    err: StudioFieldPresence,
    #[serde(default, deserialize_with = "deserialize_studio_field_presence")]
    job_id: StudioFieldPresence,
    #[serde(default, deserialize_with = "deserialize_studio_field_presence")]
    sequence_id: StudioFieldPresence,
    // Existing ordinary typed fields remain.
}

#[derive(Default)]
enum StudioFieldPresence {
    #[default]
    Absent,
    Present(StudioField),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StudioField {
    String(String),
    Unsigned(u64),
    Signed(i64),
    Float(f64),
    Invalid(serde::de::IgnoredAny),
}

fn deserialize_studio_field_presence<'de, D>(
    deserializer: D,
) -> Result<StudioFieldPresence, D::Error>
where
    D: serde::Deserializer<'de>,
{
    StudioField::deserialize(deserializer).map(StudioFieldPresence::Present)
}

pub(crate) enum StudioOperationParse {
    Operation(PrinterOperation),
    Unsupported,
    InvalidNativeCandidate,
}
```

The non-`Option` `StudioFieldPresence` is essential: Serde would map a present JSON null to `None` before an `Option` inner deserializer ran. Here absence uses `Default::default`, while every present value—including null/object/array—invokes the custom deserializer and becomes `Present`. Its ordinary `as_u64` conversion accepts the same string/unsigned forms as before. Classify before ordinary mapping: any Ignore, Resume/Stop with present `err`, or Resume/Stop with any present `param` other than exact `Present(StudioField::String(""))` is a native candidate. Validate the complete exact string shape, parse positive error within signed-32 range and a nonnegative decimal `u64` sequence, then construct `HandlePrintError`. Never fall back after candidate classification. Starting from a complete command-known candidate, replace each of `param`, `err`, `job_id`, and `sequence_id` with null in turn and prove it is invalid with zero Hub requests. Separately prove that null `job_id` or `sequence_id` alone does not create a candidate for an otherwise ordinary Resume/Stop.

- [ ] **Step 5: Export parser outcomes and adapt the two operation ABIs without moving policy to C++**

Use stable Rust export statuses such as:

```rust
const PARSE_OPERATION: i32 = 0;
const PARSE_UNSUPPORTED: i32 = 1;
const PARSE_INVALID_NATIVE: i32 = 2;
```

Return serialized operation only for status 0 and stable `unsupported_printer_operation` for both non-operation statuses. In `shim.cpp`, branch only on those numeric outcomes: cloud status 1 preserves current silent success; cloud status 2 sets `last_error` and returns `BAMBU_NETWORK_ERR_INVALID_RESULT`; local statuses 1/2 both set the same error and return invalid result. Both error outcomes make zero Hub submissions. If `lib.rs` would exceed 400 LOC, keep the constants/result conversion in the existing focused `gcode` module and leave the export as a short adapter.

- [ ] **Step 6: Preserve and emit the HTTP cause chain through Rust-owned stderr**

Wrap only the send failure after the request has been built:

```rust
let response = request
    .send()
    .await
    .map_err(reqwest::Error::without_url)
    .context("POST plugin printer operation request");
```

On error, call one formatter/writer boundary. The lower-level wording is platform-dependent; tests accept the portable connection-refused wording as well as Windows' “actively refused”/OS error 10061 while still requiring an actual nested cause:

```rust
fn write_network_error(mut writer: impl std::io::Write, error: &anyhow::Error) {
    let _ = writeln!(
        writer,
        "pandar network plugin request failed: {error:#}"
    );
}
```

For `RequestKind::PrinterOperation`, remove the URL from `reqwest::Error` before attaching the exact context `POST plugin printer operation request`; other callers of the shared POST helper keep a request-kind-appropriate context. Production obtains `std::io::stderr().lock()` and passes it to the same function used by the byte-buffer test. Do not format the URL, token, request body, response body, headers, or access code into the error. Return only the existing stable `hub_unavailable` ABI body. Keep `http.rs` tests in `http/tests.rs` via `#[cfg(test)] mod tests;` so the production module stays below 400 LOC. Because `tests/http_boundary.rs` is already over 600 LOC, split it into focused child modules before adding or changing Task 6 boundary tests; keep every touched Rust module at or below 400 LOC without `include!`.

- [ ] **Step 7: Run all focused plugin tests and inspect the C++ shim diff**

Run: `cargo fmt --all -- --check`

Run: `cargo nextest run -p pandar-network-plugin --test native_print_error`

Run: `cargo test -p pandar-network-plugin --lib http::tests::printer_operation_network_failure_logs_complete_redacted_chain -- --exact`

Run: `cargo nextest run -p pandar-network-plugin studio_abi_probe`

Run: `git diff --check -- crates/pandar-network-plugin/src/shim.cpp`

Inspect: `git diff -- crates/pandar-network-plugin/src/shim.cpp` and reject any new printer policy, status JSON construction, HTTP behavior, or command-shape parsing in C++.

Expected: parser matrix, distinct cloud/local outcomes, exact request count, Task 5's explicit cloud/local status routing, stable redacted body, and one-line full cause chain all pass; the shim changes remain limited to ABI routing/state and Rust parser-result adaptation.

### Task 7: Independent Final Gate, Documentation, Fresh Verification, Commit, Push, and Safe Smoke Test

**Files:**

- Modify only after both final implementation reviewers approve: `docs/roadmap.md`
- Modify only after both final implementation reviewers approve: `docs/development.md`
- Verify: `docs/superpowers/specs/2026-07-09-studio-native-print-error-design.md`
- Verify: `docs/superpowers/plans/2026-07-09-studio-native-print-error.md`

**Interfaces:**

- Consumes all prior task outputs and the SDD final-review verdicts.
- Produces documentation evidence, one clean Conventional Commit, and a pushed current branch.

- [ ] **Step 1: Run task-level integration tests before requesting final review**

Run:

```powershell
cargo fmt --all -- --check
cargo nextest run -p pandar-agent print_error
cargo nextest run -p pandar-agent native_print_error
cargo nextest run -p pandar-hub -E 'test(handle_print_error)'
cargo nextest run -p pandar-hub printer_live_status
cargo nextest run -p pandar-hub -E 'test(sessions::tests) | test(grpc::tests::lifecycle)'
cargo nextest run -p pandar-network-plugin --test native_print_error
cargo test -p pandar-network-plugin --lib http::tests::printer_operation_network_failure_logs_complete_redacted_chain -- --exact
cargo nextest run -p pandar-network-plugin studio_status
cargo nextest run -p pandar-network-plugin studio_abi_probe
```

Expected: every selected test passes. Capture exact output for both required final reviewers.

- [ ] **Step 2: Pass the SDD final implementation gate**

Give the independent Codex reviewer and default opencode reviewer the reviewed spec, this reviewed plan, base/head diff, and Step 1 output. Require exact `VERDICT: APPROVE`. If either revises, fix only the cited spec gap, rerun relevant tests, then rerun both reviews until both approve.

- [ ] **Step 3: Update the roadmap only after final approval**

Add a concise completed entry to `docs/roadmap.md` recording: numeric `print_error` and printer `job_id` presence bridge; native Resume/Ignore/Stop exact-payload path; capability/token-bound live dispatch and replacement cleanup; SQLite and real PostgreSQL evidence. Record the additive rollout order Agent → Hub/migration → plugin, and the rollback order plugin → Hub → Agent with the requirement that every sent/pending `operation.type:"handle_print_error"` command is terminal before Hub/Agent rollback; nullable columns remain in place. Record the remaining real-printer action click as intentionally unperformed unless the operator explicitly authorized it.

In `docs/development.md`, extend the existing live-operation deployment notes with the exact restriction: native Studio print-error actions use process-local Agent session and pending-owner state; a request received by a non-owning replica returns `printer_operation_unavailable`; another replica's stale cleanup cannot identify the owning pending set; therefore this action path requires one active Hub process, and session affinity alone is insufficient. Mention cross-replica live forwarding/ownership-aware cleanup as unsupported rather than implying NATS solves it.

- [ ] **Step 4: Run fresh repository-wide verification**

Use `superpowers:verification-before-completion`, then run:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
if (-not $env:PANDAR_TEST_POSTGRES_URL) { throw 'PANDAR_TEST_POSTGRES_URL is required' }
$postgresTestUrl = $env:PANDAR_TEST_POSTGRES_URL
Remove-Item Env:PANDAR_TEST_POSTGRES_URL
try {
    cargo nextest run --manifest-path "Cargo.toml" --workspace
} finally {
    $env:PANDAR_TEST_POSTGRES_URL = $postgresTestUrl
}
cargo nextest run --manifest-path "Cargo.toml" -p pandar-hub -j 1 postgres --success-output immediate
git diff --check
```

Expected: fmt, clippy, the full workspace with opt-in PostgreSQL tests disabled, every real PostgreSQL test in one serial invocation, and diff check all pass. All PostgreSQL wrappers share and truncate the same dedicated database, so none may run concurrently. Do not report success if PostgreSQL is skipped or credentials are unavailable; report that exact external blocker.

- [ ] **Step 5: Inspect scope and create one Conventional Commit**

Run:

```powershell
git status --short
git diff --stat
git diff -- docs/superpowers/specs/2026-07-09-studio-native-print-error-design.md docs/superpowers/plans/2026-07-09-studio-native-print-error.md docs/roadmap.md docs/development.md proto crates/pandar-agent crates/pandar-hub crates/pandar-network-plugin
```

Exclude every pre-existing untracked `crates/pandar-network-plugin/probe-config-*` and `probe-lan-only/` directory. Stage only reviewed files and commit:

```powershell
git add docs/superpowers/specs/2026-07-09-studio-native-print-error-design.md docs/superpowers/plans/2026-07-09-studio-native-print-error.md docs/roadmap.md docs/development.md proto/pandar/agent/v1/agent.proto crates/pandar-agent crates/pandar-hub crates/pandar-network-plugin/Cargo.toml crates/pandar-network-plugin/src crates/pandar-network-plugin/tests
git commit -m "fix(studio): restore native print error handling"
```

Expected: one commit containing only the reviewed feature, spec, plan, roadmap, and `docs/development.md` operational-limit changes; probe directories remain untracked.

- [ ] **Step 6: Push the current branch and report exact evidence**

Run: `git push`

If there is no upstream and the current branch name is unambiguous, run `git push --set-upstream origin $(git branch --show-current)`. Report the commit SHA, remote/branch, full verification commands, PostgreSQL execution evidence, and final reviewer verdicts. If push fails, retain the local commit and report the exact push error.

- [ ] **Step 7: Launch the configured local stack and classify hardware observation as non-gating**

Build the plugin, then start each configured process in its own inherited-environment terminal/session:

```powershell
cargo build -p pandar-network-plugin --release
cargo run -p pandar-hub
cargo run -p pandar-agent
npm run dev --workspace pandar-web
```

Expected automated observations: `Invoke-WebRequest http://127.0.0.1:8080/healthz` returns HTTP 200; `Invoke-WebRequest http://127.0.0.1:8080/readyz` returns HTTP 200; `Invoke-WebRequest http://127.0.0.1:3000` returns an HTTP response; Agent logs a successful reverse gRPC connection using the already configured identity and printer environment. If required deployment credentials/configuration are absent, report that external prerequisite rather than inventing values; this does not invalidate automated protocol tests.

The real-printer Studio observation is a non-gating external follow-up: on Studio's Device page, observe whether a printer already and genuinely reporting the mismatch shows the native dialog and whether Printing Progress/AMS remain functional. Do not induce a fault and do not click any recovery action without explicit operator permission. The fake MQTT transport and ABI probe remain the gating evidence for action payloads.
