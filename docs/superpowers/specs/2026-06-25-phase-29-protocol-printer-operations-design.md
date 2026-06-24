# Phase 29 Protocol Printer Operations Design

## Goal

Move operator-facing printer controls from Bambu-specific command strings into protocol-defined Pandar operations so Hub stores and forwards semantic actions, while agents translate those actions into device-specific transport payloads.

## Problem

Phase 27 introduced `printer_control` for pause, resume, stop, and print speed, but the command shape still exposes an action string directly on `PrinterControl`. Upcoming controls such as homing, axis movement, and hotend heating would otherwise tempt Hub or the network plugin to forward raw G-code. That would make Pandar hard to extend to non-Bambu printers.

Customer operations must be represented as Pandar protocol messages. Hub may validate authorization, tenant/printer ownership, model compatibility, and basic value ranges, but Hub must not build or persist Bambu-only G-code for those operations. Bambu-specific MQTT/G-code conversion belongs in `pandar-agent`.

## Scope

In scope:

- Add a new Phase 29 roadmap milestone for protocol-level printer operations.
- Replace the Hub-to-agent `PrinterControl` string action payload with a typed `PrinterOperation` protobuf message.
- Support the existing live controls through the typed operation shape: pause, resume, stop, and set print speed.
- Add typed protocol support for homing, relative axis movement, and hotend target temperature.
- Persist operation payloads in Hub as semantic JSON, not raw G-code.
- Add Hub API support for the new operations through the existing printer controls route.
- Translate typed operations to Bambu MQTT payloads only inside `pandar-agent`.
- Add network plugin parsing for simple G-code control lines into semantic operations and post those operations to Hub. Recognized G-code lines:
  - `G28` and `G28 X/Y/Z` -> home all or selected axes.
  - `G91` followed by one `G0`/`G1` line with `X`/`Y`/`Z` coordinates -> relative axis movement operation.
  - `M104 S<temp>` and `M109 S<temp>` -> set hotend temperature operation.
- Keep unsupported or ambiguous plugin G-code rejected with a stable error instead of forwarding raw G-code.

Out of scope:

- A complete G-code parser.
- Bed temperature, fan, extrusion, AMS, filament load/unload, or arbitrary macro support.
- New frontend controls for every new operation. The existing UI may keep using pause/resume/stop/speed.
- Non-Bambu runtime implementation. The protocol shape must allow it later, but this phase only implements Bambu translation.
- Database schema changes. Existing `commands.payload_json` can store the semantic operation JSON.

## Design

### Protocol

Add `PrinterOperation` as a new message in `proto/pandar/agent/v1/agent.proto` and include it in `HubCommand`.

`PrinterOperation` fields:

- `serial_number`
- oneof operation:
  - `PauseOperation`
  - `ResumeOperation`
  - `StopOperation`
  - `SetPrintSpeedOperation { uint32 speed_mode }`
  - `HomeOperation { repeated Axis axes }`
  - `MoveAxesOperation { repeated AxisMovement movements, uint32 feedrate_mm_per_min }`
  - `SetHotendTemperatureOperation { uint32 temperature_celsius, bool wait }`

`Axis` is a proto3 enum with `AXIS_UNSPECIFIED = 0`, `AXIS_X = 1`, `AXIS_Y = 2`, and `AXIS_Z = 3`. Hub and agent validation reject `AXIS_UNSPECIFIED`. Empty `HomeOperation.axes` means home all axes.

The protocol preserves axis-specific home intent for future non-Bambu devices. The Bambu agent implementation in this phase must not emit axis-specific homing G-code.

Keep the older `PrinterControl` protobuf message only if generated code requires temporary compatibility inside the same branch. The final Hub command path should enqueue and dispatch `printer_operation` for customer controls.

### Hub

Hub introduces `PrinterOperationPayload` in `crates/pandar-hub/src/repositories/commands.rs`. It is owned by Hub because it is a persisted command payload, parallel to the existing `PrinterControlPayload`.

```json
{
  "printer_id": "...",
  "serial_number": "...",
  "operation": {
    "type": "home",
    "axes": ["x", "y", "z"]
  }
}
```

The command kind becomes `printer_operation` for all customer-facing controls. Existing UI/API actions map to semantic operations:

- `pause` -> `{"type":"pause"}`
- `resume` -> `{"type":"resume"}`
- `stop` -> `{"type":"stop"}`
- `set_print_speed` -> `{"type":"set_print_speed","speed_mode":1..4}`
- `home` -> `{"type":"home","axes":["x","y","z"]}` or empty axes for all
- `move_axes` -> `movements` array with axis and delta_mm entries, optional positive integer `feedrate_mm_per_min`
- `set_hotend_temperature` -> `temperature_celsius`, optional `wait`

Hub validation:

- Enforce tenant, printer, and agent ownership using existing boundaries.
- Apply the existing `live_controls_supported` compatibility gate to all `printer_operation` variants in this phase, including home, move axes, and hotend temperature. Unsupported models return the existing repository/API printer-control-unavailable error path.
- Reject invalid speed modes outside `1..=4`.
- Reject empty `move_axes` operations.
- Reject `move_axes` deltas where every supplied axis is exactly `0`.
- Allow negative movement deltas, because relative moves may need either direction.
- Reject any supplied movement delta with absolute value greater than `50.0` mm.
- Reject `feedrate_mm_per_min` when supplied and not in `1..=12000`.
- Reject hotend temperatures outside `0..=300`.
- Reject unknown axes or operation types at public API boundaries.
- Do not generate G-code or Bambu MQTT JSON in Hub.
- Record audit metadata as semantic operation JSON: `action`, optional `axes`, optional `speed_mode`, optional movement fields, optional hotend fields, plus `agent_id` and `serial_number`.

The existing `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/controls` route remains the frontend/API entry point. Its request body keeps the existing top-level `action` field so the current frontend forms do not need broad changes:

```json
{
  "action": "move_axes",
  "movements": [{ "axis": "x", "delta_mm": 10.0 }],
  "feedrate_mm_per_min": 3000
}
```

The route maps this request to `PrinterOperationPayload.operation`, not to Bambu G-code. It does not accept a nested `operation` request body in this phase.

Persisted command payload shape:

```rust
pub struct PrinterOperationPayload {
    pub printer_id: String,
    pub serial_number: String,
    pub operation: PrinterOperationKind,
}

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

#[serde(rename_all = "snake_case")]
pub enum PrinterAxis {
    X,
    Y,
    Z,
}
```

For `MoveAxesOperation`, an unset protobuf axis field means do not move that axis. A movement delta of `0.0` is rejected; at least one movement must be supplied.

This phase intentionally performs a breaking command-kind change from `printer_control` to `printer_operation`. There is no legacy fallback: queued `printer_control` rows from an older deployment are treated as unknown persisted command kinds after deployment. Operators should drain or fail queued control commands before rolling this change out.

### Agent

`pandar-agent` parses `HubCommand::PrinterOperation`, validates the configured printer serial, acknowledges the command, and calls a gateway method with a typed operation enum.

The Bambu gateway maps operations to MQTT payloads:

- pause/resume/stop and print speed keep current MQTT payloads.
- home maps to `print.gcode_line` with bare `G28` for every Bambu home request. Axis-specific home intent is accepted at the protocol boundary but intentionally collapsed to all-axis homing for Bambu safety; `G28 X`, `G28 Y`, and `G28 Z` must never be published by the Bambu MQTT adapter in this phase.
- move axes maps to `print.gcode_line` with `G91`, a `G0` movement line, then `G90`. If `feedrate_mm_per_min` is absent, omit the `F` parameter and let printer firmware use its current/default travel feedrate.
- set hotend temperature maps to `M104 S...` or `M109 S...` based on `wait`.

This is the only device-specific conversion in this phase.

### Network Plugin

Add a small parser for simple control G-code. The parser returns a semantic operation JSON request for Hub. It ignores whitespace and `;` comments but rejects unsupported, extrusion, or absolute-positioning-sensitive input rather than forwarding it.

Movement parsing requires explicit relative mode in the same message: `G91` followed by exactly one `G0` or `G1` movement line. A bare `G0 X10`, a `G90` absolute-mode move, a movement with `E`, or a message with extra commands returns `unsupported_printer_operation`.

Plugin `G28 X/Y/Z` parsing preserves the semantic axes in the Hub request body, but the Bambu agent still collapses it to bare `G28` during MQTT translation.

Add a plugin HTTP helper that posts recognized operations to Hub:

`POST /api/v1/plugin/printers/{printer_id}/operations`

Plugin operation request body:

```json
{
  "action": "home",
  "axes": ["x"]
}
```

The plugin-facing Hub route resolves the authenticated plugin token tenant, verifies the printer, enqueues `printer_operation`, wakes the agent, and returns a compact command response:

```json
{
  "command_id": "...",
  "status": "queued"
}
```

Add Rust FFI function `pandar_plugin_submit_printer_operation(hub_url, token, printer_id, operation_json)` that posts this body. Add C++ shim wiring in `bambu_network_send_message_to_printer`: parse the message as supported operation G-code, call the Rust helper, and return success only when Hub accepts the operation. `bambu_network_start_send_gcode_to_sdcard` remains unsupported because it is a file-transfer print path, not a live operation path.

Stable plugin-local errors:

- `unsupported_printer_operation` for unsupported or ambiguous G-code.
- `invalid_printer_operation` for malformed semantic operation JSON passed to the Rust helper.
- `printer_operation_unavailable` for Hub validation failures where the operation is unsupported for the target printer model or live controls are unavailable.
- Existing `invalid_auth_token`, `plugin_forbidden`, `plugin_token_revoked`, `printer_not_found`, and `hub_unavailable` mapping stays unchanged.

## Documentation Impact

Update these docs in this phase:

- `docs/roadmap.md`: add Phase 29 and update completed/current status for protocol-level operations.
- `docs/development.md`: replace Phase 27 `printer_control` wording with `printer_operation`, document semantic controls and plugin operation submission.
- `docs/architecture.md`: document the Hub semantic-operation boundary and agent-local Bambu translation.
- `docs/compatibility/phase-27-live-printer-controls.md`: add a Phase 29 note that live controls moved from `printer_control` to protocol-defined `printer_operation`, and document Bambu homing safety as bare `G28`.

## Acceptance Criteria

- No Hub code path for customer controls constructs Bambu MQTT JSON or raw G-code.
- Hub command conversion sends `HubCommand::PrinterOperation` for semantic controls.
- Agent tests prove each typed operation is acknowledged and dispatched through the gateway.
- Bambu MQTT tests prove operation-to-payload conversion for pause, speed, home, move axes, and hotend temperature.
- Bambu MQTT tests prove axis-specific home input never publishes `G28 X`, `G28 Y`, or `G28 Z`; it publishes bare `G28`.
- Repository tests prove persisted `printer_operation` payloads are semantic JSON and derive agent/printer ownership correctly.
- Network plugin tests prove recognized simple G-code becomes semantic Hub operation requests and unsupported G-code returns a stable error.
- Documentation and roadmap mention Phase 29 and the protocol boundary.

## Safety And Rollback

This change keeps the database schema stable. Existing print job dispatch and refresh/diagnostic commands remain separate from the new `printer_operation` kind.

Rollout:

- Drain or fail queued/sent `printer_control` rows before deploying this phase. There is no compatibility fallback for old command kinds.
- Deploy Hub and agent together so Hub does not enqueue `printer_operation` rows for agents that cannot decode the new protobuf command.

Rollback:

- Before reverting code, drain or fail queued/sent `printer_operation` rows. A reverted Hub will not understand them.
- After command queues are clear, revert code and redeploy Hub, agent, and plugin artifacts together.

## Verification

Targeted verification:

- `cargo test -p pandar-core`
- `cargo test -p pandar-agent`
- `cargo test -p pandar-hub`
- `cargo test -p pandar-network-plugin`

Final verification:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
