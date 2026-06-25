# Phase 27: Reference-Backed Live Printer Controls

Status: Draft for SDD reviewer gate
Date: 2026-06-24

## Goal

Add typed live printer controls for pause, resume, stop, and print speed using the existing Hub command lifecycle and agent MQTT transport. The implementation must be backed by the checked-in Bambu reference projects, keep raw command dispatch out of the operator path, and keep command dispatch state separate from physical printer state reported later by MQTT.

## Reference Evidence

The command payloads are already represented by `crates/pandar-agent/src/machine/mqtt.rs` and match `reference/bambuddy/backend/app/services/bambu_mqtt.py`:

| Control     | MQTT JSON                                                                |
| ----------- | ------------------------------------------------------------------------ |
| Pause       | `{"print":{"command":"pause","sequence_id":"0"}}`                        |
| Resume      | `{"print":{"command":"resume","sequence_id":"0"}}`                       |
| Stop        | `{"print":{"command":"stop","sequence_id":"0"}}`                         |
| Print speed | `{"print":{"command":"print_speed","param":"<1..4>","sequence_id":"0"}}` |

`reference/bambuddy/backend/app/services/bambu_mqtt.py` publishes these commands with QoS 1 and limits print speed to modes 1 through 4. Pandar must keep using `BAMBU_MQTT_QOS` for live-control publishes.

`reference/bambuddy/backend/app/api/routes/printers.py` treats pause/resume/stop as printer-control API calls, and marks user stop separately so later printer reports can be interpreted as a user cancel. Pandar Phase 27 will not mutate physical job status at command enqueue or MQTT publish time. A successful control command means the command was dispatched to the printer MQTT topic; physical status remains report-derived through the existing print report reconciliation path.

`reference/BambuStudio` UI text says that after a print job has been sent, cancellation of the send flow does not stop the physical job and the user must stop it from the Device page. Pandar must preserve this dispatch-vs-physical distinction in API results and UI wording.

## Scope

In scope:

- Add typed control actions: `pause`, `resume`, `stop`, and `set_print_speed`.
- Add a Hub API endpoint that enqueues a durable `printer_control` command for one tenant printer.
- Add audit events for successful enqueue attempts with tenant actor, printer target, action, and optional speed mode.
- Extend the Hub-to-agent protobuf command with a typed `PrinterControl` message.
- Extend the agent command handler and `BambuMachineGateway` with a typed control method that publishes the existing reference-backed MQTT payloads.
- Gate controls through a shared compatibility matrix before enqueueing so unknown or unsupported printer models do not send speculative commands.
- Update the dashboard control UI from an unavailable indicator to active controls only when compatibility allows them.
- Add no-network tests for payload shape, authorization, audit, command sequencing, compatibility gating, and status separation.
- Add reference/probe documentation describing the payload evidence and noting that real-printer hardware probing was not run unless hardware becomes available during the phase.

Out of scope:

- Raw MQTT, arbitrary G-code, clear-HMS, skip-object, temperature, fan, camera, or virtual-printer controls.
- Direct Hub-to-printer network connections.
- New database tables or migrations. Existing commands, audit events, command results, printer events, and print reports are sufficient for Phase 27.
- Automatically changing `PrintJob.print_status` when a control command is queued, acknowledged, or dispatched.
- Guessing support for unknown printer models.

## API Design

Add:

`POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/controls`

Request body:

```json
{
  "action": "pause | resume | stop | set_print_speed",
  "speed_mode": 1
}
```

Rules:

- `speed_mode` is required only for `set_print_speed`.
- Valid speed modes are `1`, `2`, `3`, and `4`, matching the reference behavior.
- Unknown JSON fields are rejected.
- The caller must have `Operator` role for the tenant.
- `printer_id` must be a valid UUID owned by the tenant.
- The printer's agent must belong to the same tenant.
- The endpoint returns the existing `CommandResponse` shape.
- On accepted enqueue, the Hub wakes the owning agent.
- Invalid action or speed returns `400`.
- Unknown or unsupported compatibility returns `400` with a stable error code such as `printer_control_unavailable`.
- Viewer-only tokens receive the existing authorization error and do not create commands or audit records.

The endpoint is intentionally per-printer rather than per-agent because these controls target a physical printer. The repository still stores the owning `agent_id` on the command so dispatch uses the existing agent queue.

## Repository And Audit Design

Add a serializable payload:

```rust
pub struct PrinterControlPayload {
    pub printer_id: String,
    pub serial_number: String,
    pub action: PrinterControlAction,
    pub speed_mode: Option<u8>,
}
```

`PrinterControlAction` serializes as snake_case values: `pause`, `resume`, `stop`, `set_print_speed`.

Add `CommandRepository::enqueue_printer_control_with_audit(...)` that:

- Loads and verifies the tenant printer.
- Verifies the printer's agent ownership.
- Checks compatibility before inserting.
- Inserts a `commands` row with `kind = "printer_control"`, `printer_id = Some(printer_id)`, and the JSON payload above.
- Inserts an audit event in the same transaction.

Audit event:

- `action`: `printer.dispatch_control`
- `target_type`: `printer`
- `target_id`: printer id
- `metadata`: `{"agent_id":"...","serial_number":"...","action":"pause"}` plus `speed_mode` for print-speed controls.

The repository must not write to print-job status fields.

## Compatibility Design

Move the existing printer model compatibility policy into a shared core boundary so Hub and agent can use the same model normalization and capability names without making `pandar-hub` depend on `pandar-agent`.

Implementation shape:

- Add `crates/pandar-core/src/compatibility.rs`.
- Move or mirror the existing `Capability`, `CompatibilityFeatures`, `DiagnosticCompatibility`, `compatibility_for_model`, and `normalize_model` definitions there.
- Add `CompatibilityFeatures.live_controls`.
- Export the shared compatibility API from `pandar_core`.
- Update the agent compatibility module to re-export or call the `pandar_core` definitions instead of owning a divergent matrix.
- Use the shared `pandar_core::compatibility::live_controls_supported(model)` helper in Hub repository gating.

Compatibility decisions:

- `A1`, `A1_MINI`, `P2S`, and `X2D` are `Supported` for the Phase 27 typed controls because the reference payloads are generic Bambu `print.command` MQTT commands and these normalized model keys already exist in Pandar's matrix.
- Missing, unknown, or unnormalized model values are `Unknown`.
- Any explicit future `Unsupported` value must block enqueue the same way `Unknown` does.

The Hub is the authoritative compatibility gate because it has the persisted printer model used by the operator UI. The agent must not reject a control only because its local `BambuPrinterEndpoint.model` is missing or stale. Agent validation is limited to configured serial lookup, typed action parsing, and speed-mode validation. This avoids false rejects after the Hub has already accepted a supported persisted printer.

The UI may mirror the shared compatibility logic for rendering, but the API remains authoritative.

## Proto And Agent Design

Extend `proto/pandar/agent/v1/agent.proto`:

- Add `PrinterControl` message with `serial_number`, `action`, and `speed_mode`.
- Add `printer_control` to `HubCommand.command`.

Proto field shape:

```proto
message PrinterControl {
  string serial_number = 1;
  string action = 2;
  uint32 speed_mode = 3;
}
```

`speed_mode = 0` means unset. The agent command handler validates `action` and validates `speed_mode` as `1..=4` only when `action == "set_print_speed"`, avoiding a proto3 enum-default ambiguity.

Agent command behavior:

- Reject ack when the serial is not configured.
- Reject ack when `set_print_speed` lacks a valid mode.
- Reject ack when `action` is not one of the typed Phase 27 actions.
- Emit accepted ack before attempting MQTT publish once the command is structurally valid for the configured printer.
- Publish to `device/{serial}/request` with QoS 1 using existing `BambuMqttCommand` builders.
- Emit command success with structured `result_json`, for example:

```json
{
  "type": "printer_control",
  "serial_number": "...",
  "action": "pause",
  "dispatch": "mqtt_published"
}
```

- Emit command failure with redacted full error context when publish fails.

The gateway method should be typed, for example `control_printer(serial_number, control)`, where `control` maps to the existing MQTT command enum. It must not accept arbitrary JSON.

## Hub Dispatch Design

Extend `crates/pandar-hub/src/grpc/commands.rs` so `kind = "printer_control"` deserializes into the protobuf `PrinterControl` command. Deserialization failures fail the command through the existing invalid-payload path. The command then moves through the existing queued -> sent -> acknowledged -> succeeded/failed lifecycle based on agent events.

No new lifecycle statuses are added.

## Frontend Design

Update `frontend/app/recovery-actions.tsx` and `frontend/app/actions.ts`:

- Replace the static "Pause, resume, and stop are unavailable" text with control forms for the printer associated with each job when one is known and compatible.
- Provide pause, resume, stop, and print-speed controls.
- Render controls disabled/unavailable with concise diagnostic text when:
  - the job's non-null `printer_id` no longer matches a printer in the current dashboard payload,
  - the printer model is unknown or unsupported for live controls.
- Redirect after enqueue with a status such as `printer_control_queued`; error redirects should use the API error code.
- UI copy must say the command was queued or dispatched, not that the printer has physically paused/resumed/stopped.

The UI compatibility check is advisory. The Hub endpoint remains the source of truth.

## Documentation

Update:

- `docs/roadmap.md`: mark Phase 27 progress/completion and next Phase 28 work.
- `docs/development.md` or `docs/architecture.md`: replace outdated text that says pause/resume/stop are not implemented.
- Add `docs/compatibility/phase-27-live-printer-controls.md` with:
  - reference payload table,
  - compatibility policy,
  - status separation note,
  - local no-network evidence,
  - real-printer probe status.

## Tests And Verification

Required tests:

- MQTT payload tests continue to prove pause/resume/stop/print-speed JSON and speed range.
- Agent command tests prove:
  - valid control emits ack then success;
  - invalid serial emits rejected ack without publish;
  - missing or stale local endpoint model does not reject a Hub-accepted command;
  - publish failure emits ack then failed result with redacted full error context;
  - print-speed validates 1 through 4.
- Machine gateway tests prove the configured gateway publishes the expected MQTT command to the correct request topic.
- Hub repository and route tests prove:
  - operator can enqueue a control command;
  - viewer cannot enqueue;
  - invalid speed/action is rejected;
  - unknown/unsupported model is rejected before command/audit insert;
  - shared `pandar_core` compatibility helpers are used for Hub gating;
  - audit event is written with printer target and action metadata;
  - enqueue wakes the owning agent and not a sibling.
- Hub gRPC command tests prove `printer_control` records are serialized into typed protobuf commands.
- Frontend build proves the new forms/actions typecheck.
- A targeted status-separation test or existing print-report test proves control enqueue/success does not mutate physical print status.

Required verification commands after implementation:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path Cargo.toml --workspace
npm --prefix frontend run build
git diff --check
```

## Acceptance Criteria

- Operators can queue pause, resume, stop, and print-speed controls for supported printers from the dashboard.
- The Hub enforces tenant role, printer ownership, compatibility, and speed validation before command enqueue.
- The agent publishes only typed reference-backed MQTT payloads and never exposes raw operator command JSON.
- Commands have durable lifecycle records, structured success/failure results, and audit events.
- Command lifecycle remains separate from physical print status; MQTT reports remain authoritative for actual printer state.
- Unknown or unsupported printer/control combinations remain unavailable in UI and rejected by API without creating commands.
- Documentation records reference evidence, compatibility policy, and local verification.
