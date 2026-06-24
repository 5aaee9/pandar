# Phase 29 Protocol Printer Operations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace customer-facing printer controls with protocol-defined `printer_operation` commands so Hub stores semantic operations and Bambu-specific G-code/MQTT translation happens only in `pandar-agent`.

**Architecture:** Add a typed protobuf `PrinterOperation` command, persist semantic operation JSON in Hub, route existing `/controls` and new plugin operation requests into the same repository enqueue path, then translate typed operations inside agent-owned Bambu gateway code. Network plugin parses only a small supported G-code subset into semantic operation requests and rejects unsupported input.

**Tech Stack:** Rust workspace, tonic/prost protobuf generation, Axum routes, SeaORM-backed repositories, tokio tests, reqwest-based network plugin HTTP helpers, C++ ABI shim.

---

## File Map

- Modify `proto/pandar/agent/v1/agent.proto`: add `PrinterOperation`, operation oneof messages, and `Axis`.
- Create `crates/pandar-hub/src/repositories/commands/operations.rs`: semantic operation payload types and validation helpers.
- Modify `crates/pandar-hub/src/repositories/commands.rs`: export operation types and enqueue method.
- Modify `crates/pandar-hub/src/repositories/commands/audit.rs`: enqueue audited `printer_operation` records and semantic audit metadata.
- Modify `crates/pandar-hub/src/grpc/commands.rs`: convert persisted `printer_operation` payloads to protobuf.
- Create `crates/pandar-hub/src/routes/printer_operations.rs`: shared route request parsing for tenant and plugin operation endpoints.
- Modify `crates/pandar-hub/src/routes/printers.rs`: keep `/controls` top-level `action` API and map it to semantic operation payloads.
- Modify `crates/pandar-hub/src/routes/plugin.rs` and `crates/pandar-hub/src/routes.rs`: add plugin operation endpoint.
- Modify Hub tests under `crates/pandar-hub/src/repositories/tests/commands.rs`, `crates/pandar-hub/src/repositories/tests/postgres_commands.rs`, `crates/pandar-hub/src/grpc/tests/commands.rs`, `crates/pandar-hub/src/grpc/tests/print_jobs.rs`, and route plugin/printer tests.
- Create `crates/pandar-agent/src/machine/operations.rs`: typed operation enum and Bambu MQTT dispatch.
- Modify `crates/pandar-agent/src/machine/mod.rs`: expose `PrinterOperation` gateway method and delegate configured dispatch.
- Create `crates/pandar-agent/src/commands/operations.rs`: protobuf-to-agent operation parsing and command event emission.
- Modify `crates/pandar-agent/src/commands.rs`: route `HubCommand::PrinterOperation` to the new module.
- Modify agent tests under `crates/pandar-agent/src/commands/tests.rs` and `crates/pandar-agent/src/machine/tests.rs`.
- Modify `crates/pandar-network-plugin/src/lib.rs`: add G-code parser, semantic operation HTTP helper, FFI export, and error mapping.
- Modify `crates/pandar-network-plugin/src/shim.cpp`: call the Rust operation helper from `bambu_network_send_message_to_printer`.
- Modify network plugin tests under `crates/pandar-network-plugin/tests/http_boundary.rs`, `tests/support/mod.rs`, and `tests/fixtures/studio_abi_probe.cpp` if needed.
- Update `docs/roadmap.md`, `docs/development.md`, `docs/architecture.md`, and `docs/compatibility/phase-27-live-printer-controls.md`.

---

### Task 1: Protobuf Operation Contract

**Files:**
- Modify: `proto/pandar/agent/v1/agent.proto`
- Modify: generated Rust users after build through later tasks

- [ ] **Step 1: Add protobuf messages**

In `proto/pandar/agent/v1/agent.proto`, replace `PrinterControl printer_control = 14;` in `HubCommand` with:

```proto
PrinterOperation printer_operation = 14;
```

Add:

```proto
enum Axis {
  AXIS_UNSPECIFIED = 0;
  AXIS_X = 1;
  AXIS_Y = 2;
  AXIS_Z = 3;
}

message PrinterOperation {
  string serial_number = 1;
  oneof operation {
    PauseOperation pause = 10;
    ResumeOperation resume = 11;
    StopOperation stop = 12;
    SetPrintSpeedOperation set_print_speed = 13;
    HomeOperation home = 14;
    MoveAxesOperation move_axes = 15;
    SetHotendTemperatureOperation set_hotend_temperature = 16;
  }
}

message PauseOperation {}
message ResumeOperation {}
message StopOperation {}

message SetPrintSpeedOperation {
  uint32 speed_mode = 1;
}

message HomeOperation {
  repeated Axis axes = 1;
}

message AxisMovement {
  Axis axis = 1;
  double delta_mm = 2;
}

message MoveAxesOperation {
  repeated AxisMovement movements = 1;
  uint32 feedrate_mm_per_min = 2;
}

message SetHotendTemperatureOperation {
  uint32 temperature_celsius = 1;
  bool wait = 2;
}
```

Remove the old `PrinterControl` message.

- [ ] **Step 2: Regenerate/check generated users through compilation**

Run:

```bash
cargo check -p pandar-hub
```

Expected: FAIL with compile errors in Hub and agent code that still imports or matches `PrinterControl`. Those failures define the remaining migration surface for Tasks 2-4.

### Task 2: Hub Semantic Operation Payloads And Repository Enqueue

**Files:**
- Create: `crates/pandar-hub/src/repositories/commands/operations.rs`
- Modify: `crates/pandar-hub/src/repositories/commands.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/audit.rs`
- Modify: `crates/pandar-hub/src/repositories/mod.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/commands.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/postgres_commands.rs`

- [ ] **Step 1: Write failing repository tests**

In `crates/pandar-hub/src/repositories/tests/commands.rs`, replace/add printer-control tests for `printer_operation`:

```rust
#[tokio::test]
async fn command_enqueue_printer_operation_derives_agent_persists_semantic_payload_and_audits() {
    // enqueue PrinterOperationRequest::Home { axes: vec![PrinterAxis::X] }
    // assert command.kind == "printer_operation"
    // assert payload_json contains operation.type == "home" and axes == ["x"]
    // assert payload_json does not contain "G28" or "gcode"
}

#[tokio::test]
async fn command_enqueue_printer_operation_rejects_invalid_move() {
    // empty move, all-zero move, >50mm delta, and invalid feedrate all return InvalidPrinterControl
}

#[tokio::test]
async fn command_enqueue_printer_operation_rejects_unknown_model_before_insert() {
    // unknown model returns PrinterControlUnavailable and command count remains 0
}
```

Update the PostgreSQL mirror tests in `crates/pandar-hub/src/repositories/tests/postgres_commands.rs` so the configured PostgreSQL path calls `enqueue_printer_operation_with_audit`, asserts `command.kind == "printer_operation"`, and keeps the same unavailable-model behavior. Do not leave PostgreSQL tests asserting `printer_control`.

- [ ] **Step 2: Run focused failing tests**

Run:

```bash
cargo test -p pandar-hub command_enqueue_printer_operation
```

Expected: FAIL because operation types/enqueue are missing.

- [ ] **Step 3: Implement operation payload module**

Create `crates/pandar-hub/src/repositories/commands/operations.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrinterOperationPayload {
    pub printer_id: String,
    pub serial_number: String,
    pub operation: PrinterOperationKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PrinterOperationKind {
    Pause,
    Resume,
    Stop,
    SetPrintSpeed { speed_mode: u8 },
    Home { axes: Vec<PrinterAxis> },
    MoveAxes {
        movements: Vec<PrinterAxisMovement>,
        feedrate_mm_per_min: Option<u32>,
    },
    SetHotendTemperature { temperature_celsius: u16, wait: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrinterAxis {
    X,
    Y,
    Z,
}

impl PrinterAxis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
        }
    }
}

impl PrinterOperationKind {
    pub fn action(&self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Stop => "stop",
            Self::SetPrintSpeed { .. } => "set_print_speed",
            Self::Home { .. } => "home",
            Self::MoveAxes { .. } => "move_axes",
            Self::SetHotendTemperature { .. } => "set_hotend_temperature",
        }
    }
}
```

Add `validate_printer_operation(operation: &PrinterOperationKind) -> RepositoryResult<()>` in the same file:

- `SetPrintSpeed.speed_mode` must be `1..=4`.
- `Home.axes` may be empty or contain only axis enum values.
- `MoveAxes` must supply at least one movement, every supplied movement must be non-zero, each supplied absolute delta must be `<= 50.0`, and supplied feedrate must be `1..=12000`.
- `SetHotendTemperature.temperature_celsius` must be `0..=300`.

Use `RepositoryError::InvalidPrinterControl` for validation failures to preserve the existing stable API error.

- [ ] **Step 4: Wire repository exports**

In `crates/pandar-hub/src/repositories/commands.rs`:

```rust
mod operations;
pub use operations::{
    PrinterAxis, PrinterOperationKind, PrinterOperationPayload, validate_printer_operation,
};
```

Remove `PrinterControlAction` and `PrinterControlPayload`.

In `crates/pandar-hub/src/repositories/mod.rs`, re-export the new types instead of the old ones.

- [ ] **Step 5: Implement audited enqueue**

In `crates/pandar-hub/src/repositories/commands/audit.rs`, replace `enqueue_printer_control_with_audit` with `enqueue_printer_operation_with_audit` taking `PrinterOperationKind`.

Required behavior:

- Call `validate_printer_operation`.
- Resolve printer by tenant and verify owning agent.
- Apply `pandar_core::compatibility::live_controls_supported(printer.model.as_deref())` to every operation.
- Persist kind `"printer_operation"` and semantic `PrinterOperationPayload`.
- Audit action remains `"printer.dispatch_control"` but metadata must be flat semantic JSON, matching the existing control audit style:

```rust
serde_json::json!({
    "agent_id": printer.agent_id.to_string(),
    "serial_number": printer.serial_number,
    "action": operation.action(),
    // include exactly the variant fields that apply:
    // "axes": ["x", "z"] for home when axes are supplied,
    // "speed_mode": 4 for set_print_speed,
    // movement fields for move_axes,
    // "temperature_celsius" and "wait" for set_hotend_temperature.
})
```

- [ ] **Step 6: Run repository tests**

Run:

```bash
cargo test -p pandar-hub command_enqueue_printer_operation
```

Expected: PASS.

### Task 3: Hub gRPC Conversion And HTTP Routes

**Files:**
- Modify: `crates/pandar-hub/src/grpc/commands.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/commands.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/print_jobs.rs`
- Modify: `crates/pandar-hub/src/routes/printers.rs`
- Modify: `crates/pandar-hub/src/routes/plugin.rs`
- Modify: `crates/pandar-hub/src/routes.rs`
- Modify: route tests under `crates/pandar-hub/src/routes/tests/`

- [ ] **Step 1: Write failing gRPC conversion tests**

In `crates/pandar-hub/src/grpc/tests/commands.rs`, add tests that build `CommandRecord` values with `kind: "printer_operation"` and semantic payload JSON:

```rust
#[test]
fn grpc_hub_command_from_record_maps_printer_operation_speed() {
    let command = command_record_with_payload(
        "printer_operation",
        r#"{"printer_id":"printer-1","serial_number":"SERIAL123","operation":{"type":"set_print_speed","speed_mode":4}}"#,
    );

    let hub_command = hub_command_from_record(command).unwrap();

    let Some(hub_command::Command::PrinterOperation(operation)) = hub_command.command else {
        panic!("expected printer operation");
    };
    let Some(printer_operation::Operation::SetPrintSpeed(speed)) = operation.operation else {
        panic!("expected set print speed operation");
    };
    assert_eq!(operation.serial_number, "SERIAL123");
    assert_eq!(speed.speed_mode, 4);
}

#[test]
fn grpc_hub_command_from_record_maps_printer_operation_home_axes() {
    let command = command_record_with_payload(
        "printer_operation",
        r#"{"printer_id":"printer-1","serial_number":"SERIAL123","operation":{"type":"home","axes":["x","z"]}}"#,
    );

    let hub_command = hub_command_from_record(command).unwrap();

    let Some(hub_command::Command::PrinterOperation(operation)) = hub_command.command else {
        panic!("expected printer operation");
    };
    let Some(printer_operation::Operation::Home(home)) = operation.operation else {
        panic!("expected home operation");
    };
    assert_eq!(home.axes, vec![Axis::AxisX as i32, Axis::AxisZ as i32]);
}

#[test]
fn grpc_hub_command_from_record_rejects_invalid_printer_operation_payload() {
    let command = command_record_with_payload(
        "printer_operation",
        r#"{"printer_id":"printer-1","serial_number":"SERIAL123","operation":{"type":"move_axes"}}"#,
    );

    let status = hub_command_from_record(command).unwrap_err();

    assert_eq!(status.message(), "invalid printer operation command payload");
}
```

If `command_record_with_payload` does not already exist, add it as a local test helper next to the existing command-record helpers.

- [ ] **Step 2: Run focused failing tests**

Run:

```bash
cargo test -p pandar-hub grpc_hub_command_from_record_maps_printer_operation
```

Expected: FAIL until conversion is implemented.

- [ ] **Step 3: Complete gRPC conversion**

Map `PrinterOperationPayload.operation` to generated protobuf `printer_operation::Operation` variants. Ensure `PrinterAxis::X/Y/Z` become numeric `Axis::AxisX/Y/Z` values and never `AxisUnspecified`.

Unknown/invalid payload JSON returns `Status::internal("invalid printer operation command payload")` and logs the full error chain.

Update `crates/pandar-hub/src/grpc/tests/print_jobs.rs::printer_control_success_does_not_mutate_physical_print_status` to enqueue `printer_operation` stop and assert its result JSON uses `"type":"printer_operation"`. Keep the behavioral assertion that control command success does not mutate physical print status.

- [ ] **Step 4: Add `/controls` request parsing tests**

Add route tests proving:

- Existing body `{"action":"pause"}` enqueues `printer_operation`.
- Existing body `{"action":"set_print_speed","speed_mode":4}` enqueues semantic speed operation.
- New body `{"action":"home","axes":["x","z"]}` enqueues semantic home operation with axes and no G-code.
- Invalid movement returns `400 invalid_printer_control`.

- [ ] **Step 5: Implement route request shape**

Create a shared route helper module or shared private functions usable by both `routes/printers.rs` and `routes/plugin.rs`; do not duplicate divergent parsing logic. A simple option is `crates/pandar-hub/src/routes/printer_operations.rs` with `pub(super)` request type and conversion helper, imported by both route modules.

In that shared route helper, define:

```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PrinterOperationRequest {
    action: String,
    speed_mode: Option<u8>,
    axes: Option<Vec<PrinterAxis>>,
    movements: Vec<PrinterAxisMovement>,
    feedrate_mm_per_min: Option<u32>,
    temperature_celsius: Option<u16>,
    wait: Option<bool>,
}
```

Convert it to `PrinterOperationKind`:

- `pause`, `resume`, `stop` reject extra operation-specific fields.
- `set_print_speed` requires `speed_mode`.
- `home` defaults missing `axes` to empty.
- `move_axes` uses the supplied `movements` array.
- `set_hotend_temperature` requires `temperature_celsius` and defaults missing `wait` to `false`.
- Unknown `action` strings return `400 invalid_printer_control`.
- Unknown axis strings return `400 invalid_printer_control`.

Call `enqueue_printer_operation_with_audit`.

- [ ] **Step 6: Add plugin operation route**

In `crates/pandar-hub/src/routes.rs`, add:

```rust
.route(
    "/api/v1/plugin/printers/{printer_id}/operations",
    post(plugin::create_printer_operation),
)
```

In `routes/plugin.rs`, implement `create_printer_operation` using `authorize_plugin_studio`, `Path(printer_id)`, and the same `PrinterOperationRequest` conversion helper. Return:

```rust
#[derive(Debug, Serialize)]
struct PluginOperationResponse {
    command_id: String,
    status: String,
}
```

Wake the command's agent after enqueue.

- [ ] **Step 7: Map stable API errors**

Keep existing `RepositoryError::PrinterControlUnavailable` mapping for the tenant API as `printer_control_unavailable` for API stability. In the plugin operation route, map `RepositoryError::PrinterControlUnavailable` to body `{"error":"printer_operation_unavailable"}`.

In `crates/pandar-network-plugin/src/lib.rs`, add `printer_operation_unavailable` to `is_stable_hub_error` so plugin clients receive the stable Hub validation error instead of `invalid_response`.

- [ ] **Step 8: Run Hub tests**

Run:

```bash
cargo test -p pandar-hub printer_operation
cargo test -p pandar-hub plugin
```

Expected: PASS.

### Task 4: Agent Command Handling And Bambu Dispatch

**Files:**
- Create: `crates/pandar-agent/src/machine/operations.rs`
- Modify: `crates/pandar-agent/src/machine/mod.rs`
- Create: `crates/pandar-agent/src/commands/operations.rs`
- Modify: `crates/pandar-agent/src/commands.rs`
- Modify: `crates/pandar-agent/src/commands/tests.rs`
- Modify: `crates/pandar-agent/src/machine/tests.rs`

- [ ] **Step 1: Write failing agent command tests**

Add tests proving:

- `HubCommand::PrinterOperation(Pause)` emits ack then success with result JSON `{ "type": "printer_operation", "action": "pause", ... }`.
- Invalid protobuf `AxisUnspecified` in home emits rejected ack.
- Agent validates configured printer serial before dispatch.

- [ ] **Step 2: Write failing Bambu dispatch tests**

In `crates/pandar-agent/src/machine/tests.rs`, add tests proving:

- `PrinterOperation::Home { axes: vec![Axis::Z] }` publishes `{"print":{"command":"gcode_line","param":"G28",...}}`, not `G28 Z`.
- Move with x/y and feedrate publishes a `gcode_line` param containing `G91`, `G0 X... Y... F...`, and `G90`.
- Move without feedrate omits `F`.
- Hotend wait false publishes `M104 S...`; wait true publishes `M109 S...`.

- [ ] **Step 3: Implement operation enum**

Create `crates/pandar-agent/src/machine/operations.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PrinterOperation {
    Pause,
    Resume,
    Stop,
    SetPrintSpeed(u8),
    Home { axes: Vec<Axis> },
    MoveAxes {
        movements: Vec<AxisMovement>,
        feedrate_mm_per_min: Option<f64>,
    },
    SetHotendTemperature { temperature_celsius: u16, wait: bool },
}
```

Add `dispatch_printer_operation(endpoint, mqtt, operation)` that publishes to `device/{serial}/request`.

For Bambu home, ignore `axes` and publish bare `G28`.

- [ ] **Step 4: Wire gateway method**

In `machine/mod.rs`:

- `pub mod operations;`
- `pub use operations::{Axis, PrinterOperation};`
- Replace `control_printer` with `operate_printer`.
- Update `NoopMachineGateway` and `ConfiguredBambuMachineGateway` implementations.

- [ ] **Step 5: Implement command parser module**

Create `crates/pandar-agent/src/commands/operations.rs` with:

- `emit_printer_operation_events`
- protobuf `PrinterOperation` -> machine `PrinterOperation` parsing
- result JSON builder

Validation:

- Unknown oneof is rejected ack.
- `AxisUnspecified` is rejected ack.
- Speed must be `1..=4`.
- Move/hotend ranges can trust Hub for normal operation but still reject malformed protobuf at the agent boundary when values are missing or invalid.

Update `commands.rs` to route `hub_command::Command::PrinterOperation`.

- [ ] **Step 6: Run agent tests**

Run:

```bash
cargo test -p pandar-agent printer_operation
cargo test -p pandar-agent configured_printer_operation
```

Expected: PASS.

### Task 5: Network Plugin G-code Operation Parser And HTTP Helper

**Files:**
- Modify: `crates/pandar-network-plugin/src/lib.rs`
- Modify: `crates/pandar-network-plugin/src/shim.cpp`
- Modify: `crates/pandar-network-plugin/tests/http_boundary.rs`
- Modify: `crates/pandar-network-plugin/tests/support/mod.rs`
- Modify: `crates/pandar-network-plugin/tests/fixtures/studio_abi_probe.cpp` if the probe expects direct message failure.

- [ ] **Step 1: Write failing parser and HTTP tests**

Add Rust tests proving:

- `operation_json_from_gcode("G28")` returns `{"action":"home","axes":[]}`.
- `operation_json_from_gcode("G28 Z")` returns `{"action":"home","axes":["z"]}`.
- `operation_json_from_gcode("G91\nG1 X10 F3000\nG90")` returns `{"action":"move_axes","movements":[{"axis":"x","delta_mm":10.0}],"feedrate_mm_per_min":3000}`.
- `operation_json_from_gcode("M104 S215")` returns `{"action":"set_hotend_temperature","temperature_celsius":215,"wait":false}`.
- `operation_json_from_gcode("M109 S215")` returns wait true.
- Bare `G1 X10`, movement with `E`, and unsupported commands return `unsupported_printer_operation` from the parser before network.
- `pandar_plugin_submit_printer_operation` posts a supplied semantic operation JSON body unchanged to `/api/v1/plugin/printers/{printer_id}/operations`.
- Malformed semantic operation JSON passed to `pandar_plugin_submit_printer_operation` returns `{"error":"invalid_printer_operation"}` before network.

- [ ] **Step 2: Add HTTP helper**

Add FFI export:

```rust
#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_submit_printer_operation(
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    token_ptr: *const u8,
    token_len: usize,
    printer_id_ptr: *const u8,
    printer_id_len: usize,
    operation_json_ptr: *const u8,
    operation_json_len: usize,
) -> PluginHttpResult
```

This function reads and validates inputs, parses `operation_json` as JSON object, then posts that semantic body to:

`/api/v1/plugin/printers/{printer_id}/operations`

Add `RequestKind::PrinterOperation` and error redaction:

- Hub stable `printer_operation_unavailable` passes through.
- Malformed operation JSON returns `invalid_printer_operation`.

- [ ] **Step 3: Implement parser**

Implement a small parser in Rust without adding dependencies:

- Strip `;` comments.
- Split non-empty lines.
- Accept one-line `G28` with optional axes.
- Accept one-line `M104/M109 S<num>`.
- Accept exactly two lines for movement: first `G91`, second `G0/G1` with at least one X/Y/Z, optional F, no E, no extra commands.
- Reject everything else.

Expose parser FFI for the C++ shim:

```rust
#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_operation_json_from_gcode(
    gcode_ptr: *const u8,
    gcode_len: usize,
) -> PluginHttpResult
```

On success, return HTTP-like status `0`, code `200`, and the operation JSON body. On unsupported input, return status `1`, code `400`, and `{"error":"unsupported_printer_operation"}`.

- [ ] **Step 4: Wire C++ shim**

In `src/shim.cpp`:

- Declare `pandar_plugin_submit_printer_operation`.
- Add `rust_submit_printer_operation`.
- Change `bambu_network_send_message_to_printer(void* agent, std::string dev_id, std::string message, int, int)` to:
  - validate agent handle,
  - call `pandar_plugin_operation_json_from_gcode` with `message`,
  - return `BAMBU_NETWORK_ERR_CONNECT_FAILED` with `last_error` containing `unsupported_printer_operation` when parsing fails,
  - call `pandar_plugin_submit_printer_operation` with `dev_id` and the semantic operation JSON,
  - set `last_error` from failed response body,
  - return `BAMBU_NETWORK_SUCCESS` on success and `BAMBU_NETWORK_ERR_CONNECT_FAILED` on failure.

Keep `bambu_network_start_send_gcode_to_sdcard` unsupported.

- [ ] **Step 5: Run plugin tests**

Run:

```bash
cargo test -p pandar-network-plugin printer_operation
cargo test -p pandar-network-plugin http_boundary
```

Expected: PASS.

### Task 6: Documentation Updates

**Files:**
- Modify: `docs/roadmap.md`
- Modify: `docs/development.md`
- Modify: `docs/architecture.md`
- Modify: `docs/compatibility/phase-27-live-printer-controls.md`

- [ ] **Step 1: Update roadmap**

Add Phase 29 to `docs/roadmap.md` and note:

- `printer_operation` replaces customer-facing `printer_control`.
- Hub persists semantic operations and never raw G-code for controls.
- Bambu agent translates operation to MQTT/G-code.
- Network plugin parses limited G-code into semantic operations.
- Bambu homing always publishes bare `G28`.

- [ ] **Step 2: Update development docs**

In `docs/development.md`, replace Phase 27 wording that says `printer_control` commands with `printer_operation` and document the expanded operation set plus plugin route.

- [ ] **Step 3: Update architecture docs**

In `docs/architecture.md`, add a short paragraph under machine command/plugin sections:

```text
Customer controls are protocol-defined printer operations. Hub stores and forwards semantic operations only; device-specific G-code/MQTT translation is agent-local.
```

- [ ] **Step 4: Update compatibility note**

In `docs/compatibility/phase-27-live-printer-controls.md`, add a Phase 29 note explaining:

- command kind moved to `printer_operation`,
- existing pause/resume/stop/speed semantics remain,
- home/move/hotend are protocol-level operations,
- Bambu home ignores requested axes and publishes bare `G28` for safety.

### Task 7: Final Verification And Cleanup

**Files:**
- All modified files.

- [ ] **Step 1: Format**

Run:

```bash
cargo fmt --all -- --check
```

If it fails, run `cargo fmt --all`, then rerun `cargo fmt --all -- --check`.

- [ ] **Step 2: Focused tests**

Run:

```bash
cargo test -p pandar-hub printer_operation
cargo test -p pandar-agent printer_operation
cargo test -p pandar-network-plugin printer_operation
```

- [ ] **Step 3: Workspace verification**

Run:

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

- [ ] **Step 4: Diff review**

Run:

```bash
git status --short
git diff --stat
git diff --check
```

Confirm only intended protocol, Hub, agent, plugin, docs, spec, and plan files changed.
