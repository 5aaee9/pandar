# Bambu Studio Native Print Error Design

## Goal

Restore Bambu Studio's native printer-error dialog and its native recovery actions when Studio is connected through Pandar. The first verified case is build-plate mismatch `0x05008051`, but the transport must carry any numeric Bambu `print.print_error` without inferring errors from HMS, `gcode_state`, or pause state.

## Reference Behavior

`reference/BambuStudio` is the sole behavior reference for this change.

- Studio reads `print.print_error` only when it is a JSON number (`DeviceManager.cpp:3020-3023`).
- A nonzero value opens `DeviceErrorDialog`; an explicit zero clears it (`StatusPanel.cpp:3460-3488`). An absent field leaves the current value unchanged.
- Studio reads `print.job_id` into `MachineObject::job_id_` (`DeviceManager.cpp:3115-3121`).
- The 20P action catalog assigns build-plate mismatch the actions Resume, Ignore, and Stop (`resources/hms/hms_action_20P.json:1862-1869`).
- The button handlers pass `std::to_string(m_error_code)` and `job_id_` to `command_hms_resume`, `command_hms_ignore`, or `command_hms_stop` (`DeviceErrorDialog.cpp:535-606`).
- Those methods publish one of the following payloads with QoS 1 (`DeviceManager.cpp:1436-1472`):

```json
{"print":{"command":"resume","err":"83918929","job_id":"<job-id>","param":"reserve","sequence_id":"<sequence>"}}
```

```json
{"print":{"command":"ignore","err":"83918929","job_id":"<job-id>","param":"reserve","sequence_id":"<sequence>"}}
```

```json
{"print":{"command":"stop","err":"83918929","job_id":"<job-id>","param":"reserve","sequence_id":"<sequence>"}}
```

`publish_json`, `send_message_to_printer`, the LAN ABI, and the LAN MQTT session preserve this JSON's semantics and publish it to `device/<serial>/request` with QoS 1 and retain disabled. Pandar must preserve the same fields and types across its remote bridge.

## Considered Approaches

### 1. Infer the dialog from HMS or paused state

Rejected. `print_error` is independent of HMS and many unrelated conditions pause a printer. Inference can display the wrong dialog and cannot recover the exact error code or native actions.

### 2. Map error buttons to existing Resume and Stop operations

Rejected. Existing operations produce `param:""`, discard `err` and `job_id`, generate a different operation shape, and do not support `command:"ignore"`.

### 3. Carry typed native error state and a dedicated error action

Selected. This preserves the native fields without allowing arbitrary raw MQTT JSON through Hub or placing policy in the C++ ABI shim.

## Inbound Printer State

### Agent parsing

The typed printer-report schema will distinguish two existing `print_error` shapes:

- JSON number: current native printer error state.
- Object or string: command/diagnostic error data already retained by Pandar.

Numeric values become `Option<u32>` state. They do not create generic diagnostic events. This means:

- field absent: `None`, so downstream state is not overwritten;
- field present as `0`: `Some(0)`, an explicit Studio clear;
- field present as nonzero: `Some(code)`, a Studio-visible current error.

Object and string values retain the existing diagnostic path and do not masquerade as numeric Studio state. This removes the current stream of generic `print_error` events whose message is merely `"0"` without discarding real structured command errors.

The Agent will also parse the printer's independent `print.job_id` as a string-or-number identifier and normalize it to its decimal/string representation. It remains distinct from `task_id`; the existing protobuf `job_id` currently carries `task_id` and must not be repurposed.

### Agent-to-Hub protocol

Append fields to `PrintJobReport`:

```proto
uint32 print_error = 21;
bool has_print_error = 22;
string printer_job_id = 23;
bool has_printer_job_id = 24;
```

The explicit presence flags preserve partial MQTT report semantics, including explicit zero and empty job identifiers.

### Hub live status

Add nullable live-printer columns in matching SQLite and PostgreSQL migrations:

- `print_error` as an integer capable of holding the complete `u32` range;
- `print_job_id` as text.

Repository patches update each field only when its protocol presence flag is set. The plugin printer API exposes both as optional typed fields. No existing `jobs.print_error` field is reused because that field represents a Pandar job's terminal textual error, not the printer's current numeric state.

## Studio Telemetry

The Rust network-plugin status layer adds optional `print_error: u32` and `job_id: String` fields to the synthesized `print.push_status` telemetry.

- `Some(0)` serializes as the JSON number `0`.
- `Some(nonzero)` serializes as that JSON number.
- `None` omits the field, matching a partial native printer report.
- `job_id` is emitted only when known and remains a JSON string.

The C++ shim only inserts the typed Rust telemetry object into the existing Studio callback envelope. It does not interpret error codes or construct recovery commands.

## Native Error Actions

Introduce a dedicated semantic operation:

```rust
HandlePrintError {
    action: PrintErrorAction, // Resume | Ignore | Stop
    print_error: u32,
    printer_job_id: String,
    sequence_id: u64,
}
```

The network plugin recognizes this operation only when all native markers are present:

- `print.command` is `resume`, `ignore`, or `stop`;
- `print.param` is exactly `"reserve"`;
- `print.err` is a decimal string that parses to a nonzero `u32`;
- `print.job_id` is a string (an empty string remains valid because Studio writes the field even when `job_id_` is empty);
- `print.sequence_id` is a decimal string that parses to `u64`.

Ordinary `resume` and `stop` messages continue using the existing operations. `ignore` is accepted only in the native error-action shape.

The typed REST/repository/protobuf/Agent path preserves the action, error code, printer job identifier, and Studio sequence. The Bambu Agent adapter serializes exactly:

- `command`: lower-case action name;
- `err`: `print_error.to_string()`;
- `job_id`: the supplied string;
- `param`: `"reserve"`;
- `sequence_id`: the supplied Studio sequence converted back to a decimal string.

It publishes to `device/<serial>/request` with the existing Bambu MQTT QoS 1 and retain-disabled behavior. The Agent does not replace the Studio sequence, allowing normal operation-result correlation to use the same identifier Studio generated.

## Validation and Error Handling

All external shapes are decoded with typed Serde or protobuf types.

- Malformed native error actions are rejected by the plugin as `unsupported_printer_operation`; they are not downgraded to ordinary Resume or Stop.
- Hub rejects unspecified action enums and zero error codes.
- Agent rejects an unspecified action enum rather than issuing a printer command.
- Existing lower-level error context remains intact at HTTP, gRPC, repository, and MQTT boundaries.

No raw arbitrary MQTT command is added to the public operation API.

## Testing

Tests will be written before each implementation layer and observed failing for the missing behavior.

1. Agent report tests:
   - absent, zero, and nonzero numeric `print_error` preserve presence semantics;
   - numeric zero creates no generic diagnostic;
   - structured/string print errors retain the existing diagnostic behavior;
   - number/string `job_id` values normalize without replacing `task_id`.
2. Protocol and Hub tests:
   - gRPC presence conversion carries explicit zero and printer job identifiers;
   - live status persists identically in SQLite and PostgreSQL;
   - missing fields do not overwrite prior state;
   - plugin printer API returns typed optional fields.
3. Network-plugin tests:
   - telemetry emits numeric nonzero and zero values and omits unknown values;
   - the parser distinguishes ordinary controls from all three native error actions;
   - malformed `reserve` actions are rejected;
   - the ABI probe observes the typed Hub request and final Studio `push_status` fields.
4. Agent command tests:
   - Resume, Ignore, and Stop payloads match the reference field-for-field;
   - the supplied Studio sequence is retained;
   - topic, QoS, and retain behavior match direct LAN.
5. Full verification:
   - `cargo fmt --all -- --check`;
   - `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
   - `cargo nextest run --manifest-path "Cargo.toml" --workspace`;
   - launch Agent, Hub, and the rebuilt plugin, then confirm on Studio's Device page that the real printer's mismatch dialog appears and each available action produces the reference command shape.

## Scope

This change does not add a Pandar-owned dialog, infer any printer error, change unrelated controls, expose raw MQTT, or implement `mc_print_error_code`. It only restores the native Studio state and action path that Pandar currently drops.
