# Bambu Studio Native Print Error Design

## Goal

Restore Bambu Studio's native printer-error dialog and its native recovery actions when Studio is connected through Pandar. The first verified case is build-plate mismatch `0x05008051`, but the transport must carry any numeric Bambu `print.print_error` without inferring errors from HMS, `gcode_state`, or pause state.

## Reference Behavior

`reference/BambuStudio` is normative for Studio UI, payload, and ABI behavior. `reference/bambuddy` is corroborating evidence only for the direct-LAN MQTT transport.

- Studio reads `print.print_error` only when it is a JSON number (`DeviceManager.cpp:3020-3023`).
- A changed positive value (`print_error > 0 && print_error != last_error`) opens `DeviceErrorDialog`; any value `<= 0` clears it (`StatusPanel.cpp:3460-3488`). An absent field leaves the current value unchanged.
- Studio reads `print.job_id` into `MachineObject::job_id_` (`DeviceManager.cpp:3115-3121`).
- The 20P action catalog assigns build-plate mismatch the actions Resume, Ignore, and Stop (`resources/hms/hms_action_20P.json:1862-1869`).
- The button handlers pass `std::to_string(m_error_code)` and `job_id_` to `command_hms_resume`, `command_hms_ignore`, or `command_hms_stop` (`DeviceErrorDialog.cpp:535-606`).
- Resume and Ignore first run `check_resume_condition()` and emit nothing when `jobState_ > 1`; Stop has no such guard (`DeviceManager.cpp:1436-1472,1593-1600`). When emitted on the normal FDM path, those methods serialize one of these payloads and call the selected Cloud/LAN ABI with QoS 1 and flag `0`:

```json
{"print":{"command":"resume","err":"83918929","job_id":"<job-id>","param":"reserve","sequence_id":"<sequence>"}}
```

```json
{"print":{"command":"ignore","err":"83918929","job_id":"<job-id>","param":"reserve","sequence_id":"<sequence>"}}
```

```json
{"print":{"command":"stop","err":"83918929","job_id":"<job-id>","param":"reserve","sequence_id":"<sequence>"}}
```

`reference/bambuddy` corroborates the direct-LAN topic `device/<serial>/request` and QoS 1 (`mqtt_bridge.py:729-746`, `bambu_mqtt.py:4096-4107`). Its Paho call omits `retain`, which uses Paho's default `false`; Pandar must set retain disabled explicitly and preserve the same fields and types across its remote bridge.

## Considered Approaches

### 1. Infer the dialog from HMS or paused state

Rejected. `print_error` is independent of HMS and many unrelated conditions pause a printer. Inference can display the wrong dialog and cannot recover the exact error code or native actions.

### 2. Map error buttons to existing Resume and Stop operations

Rejected. Existing operations produce `param:""`, discard `err` and `job_id`, generate a different operation shape, and do not support `command:"ignore"`.

### 3. Carry typed native error state and a dedicated error action

Selected. This preserves the native fields without allowing arbitrary raw MQTT JSON through Hub or placing policy in the C++ ABI shim.

## Inbound Printer State

### Agent parsing

The typed printer-report schema will distinguish the existing `print_error` shapes without allowing one invalid field to reject the complete `print` report:

- JSON number: possible current native printer error state.
- Object or string: command/diagnostic error data already retained by Pandar.
- Any other JSON type: invalid for this field and ignored locally.

The boundary type stores a JSON number separately from object/string diagnostics and applies Studio's observable `get<int>()` behavior for values in the defined signed-32-bit domain:

- finite numbers in `i32::MIN..=i32::MAX` are truncated toward zero to an `i32`;
- a converted value `<= 0` is normalized to state `0`, matching Studio's clear behavior;
- a converted value in `1..=i32::MAX` becomes that current error code;
- a numeric value outside the signed-32-bit domain produces no patch rather than reproducing C++'s implementation-defined out-of-range cast;
- a nonnumeric field produces no numeric state patch;
- no invalid field rejects progress, HMS, materials, or any other field in the report.

Valid numeric values do not create generic diagnostic events. This means:

- field absent: `None`, so downstream state is not overwritten;
- field present as `0`: `Some(0)`, an explicit Studio clear;
- field present as nonzero: `Some(code)`, a Studio-visible current error.

Object and string values retain the existing diagnostic path and do not masquerade as numeric Studio state. This removes the current stream of generic `print_error` events whose message is merely `"0"` without discarding real structured command errors.

The Agent will also parse the printer's independent `print.job_id` using Studio's `DevJsonValParser::get_longlong_val` semantics whenever the field is present:

- a JSON string, including an empty or nonnumeric string, is preserved byte-for-byte;
- a finite JSON number in the signed-64-bit domain is truncated toward zero and becomes its canonical decimal string;
- a number outside the signed-64-bit domain or any non-string/nonnumeric JSON type becomes an explicit empty-string patch, matching the helper's empty return value;
- field absence produces no patch, which is distinct from a present field that converts to an empty string;
- conversion never rejects or removes other fields from the same printer report.

The real MQTT byte boundary preserves the exact numeric lexeme for this one known field before the ordinary `serde_json::Value` parse can round it through `f64`. A typed borrowed `RawValue` view recognizes only numeric `print.job_id`, performs decimal/exponent truncation and signed-64 range validation without floating point, and substitutes the canonical string (or explicit empty string) before the existing typed report pipeline runs. Strings and all nonnumeric shapes retain their existing typed deserialization behavior. Enabling serde_json `arbitrary_precision` workspace-wide is rejected because its private number-map representation changes the existing untagged report schema.

It remains distinct from `task_id`; the existing protobuf `job_id` currently carries `task_id` and must not be repurposed.

### Agent-to-Hub protocol

Append fields to `PrintJobReport`:

```proto
uint32 print_error = 21;
bool has_print_error = 22;
string printer_job_id = 23;
bool has_printer_job_id = 24;
```

The explicit presence flags preserve partial MQTT report semantics, including explicit zero and empty job identifiers.

Append an explicit Agent capability to `AgentHello`:

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
```

The new Agent advertises `AGENT_CAPABILITY_HANDLE_PRINT_ERROR`. An old Agent omits field 4 and is therefore unsupported. Hub stores the advertised set on the live `AgentSession`; version-string comparison is not used as a capability proxy.

### Hub live status

Add a new, later-numbered migration in both backend directories rather than editing an already-applied migration. The migration adds nullable live-printer columns:

- SQLite `print_error INTEGER` and PostgreSQL `print_error INTEGER`, both covering the native Studio state domain `0..=i32::MAX`;
- `print_job_id` as text.

Repository patches update each field only when its protocol presence flag is set. The plugin printer API exposes both as optional typed fields. No existing `jobs.print_error` field is reused because that field represents a Pandar job's terminal textual error, not the printer's current numeric state.

The Hub gRPC boundary also enforces the storage/Studio domain: `has_print_error=true` patches only values `0..=i32::MAX`; a larger protobuf `uint32` produces no error-state patch while the rest of that authenticated report is still applied. An explicit zero remains a valid clear. `has_printer_job_id=true` preserves the string exactly, including empty/whitespace-only values, and never passes through trimming.

## Studio Telemetry

The Rust network-plugin status layer adds optional `print_error: u32` and `job_id: String` fields to the synthesized `print.push_status` telemetry.

- `Some(0)` serializes as the JSON number `0`.
- `Some(nonzero)` serializes as that JSON number.
- `None` omits the field, matching a partial native printer report.
- `job_id` is emitted only when known and remains a JSON string.

The C++ shim only inserts the typed Rust telemetry object into the existing Studio callback envelope. It does not interpret error codes or construct recovery commands.

The callback tunnel is explicit, matching Studio's two inbound paths: cloud subscription/status and cloud status-request responses invoke `on_message`, while local-printer status/version responses invoke `on_local_message`. `connect_printer` reports LAN connection success only through `on_local_connect`; it never invokes the cloud-only `on_printer_connected`, never overwrites the account-selected machine, and does not synthesize a status message itself. Studio's LAN-connect callback issues the initial `get_version` and `pushall` requests. Callback availability never selects or changes the tunnel because Studio normally registers both message callbacks; a missing callback means that tunnel has no recipient and must not fall back to the other one. This matches Studio's separate cloud and LAN connection callbacks (`GUI_App.cpp:2129-2239`), cloud `get_user_machine(...)/parse_json("cloud", ...)`, and LAN `get_my_machine(...)/parse_json("lan", ...)` paths (`GUI_App.cpp:2241-2316`, `DeviceCore/DevManager.cpp:475-496`).

Both Studio ABIs classify only typed top-level `info.command == "get_version"` and `pushing.command == "pushall"` as transport/status requests before printer-operation parsing. Rust owns this exact classification behind a flat FFI; `shim.cpp` adapts only its numeric kind and returned sequence string. Arbitrary substrings in unrelated commands or native `job_id` values never select a status branch. Each request has one response family: `get_version` emits only `info.get_version`, while `pushall` refreshes and emits only `print.push_status`. The local ABI answers them successfully through `on_local_message` with zero Hub operation requests, just as the cloud ABI answers through `on_message`; this is required by Sync AMS Filament, which issues both requests (`FilamentManagerVM.cpp:541-568`, `DeviceManager.cpp:1274-1317`). The plugin tracks the single active local device independently of account-selected/cloud-subscribed devices, replaces it on the next `connect_printer`, clears it on `disconnect_printer`, refreshes the Hub snapshot once per Pandar heartbeat, and emits that snapshot to every cloud subscription through the cloud callback and to the active local device through the local callback. Cache discovery/refresh updates the printer connection and telemetry maps but never adds discovered serials to `cloud_subscribed_devices`; Studio's explicit subscribe/unsubscribe calls and the existing account-selection flow own that set. Therefore an explicitly unsubscribed active-local serial remains local-only across successful refreshes. A serial deliberately present in both sets receives one emission on each explicit Studio tunnel. BambuStudio establishes the independent callback paths and its 30-second disconnect timeout, but not the proprietary plugin's heartbeat cadence; Pandar retains its existing two-second synthetic cadence.

### Module layout and size boundaries

New logic is placed in focused modules so touched Rust files remain below 400 LOC:

- Agent report-to-protobuf construction moves from the near-limit `machine/mqtt/reports.rs` into `machine/mqtt/reports/protocol.rs`; tolerant boundary decoding remains in `reports/schema.rs`.
- Agent raw MQTT byte adaptation for exact numeric `print.job_id` lexemes lives in `machine/mqtt/report_payload.rs`; the ordinary typed report pipeline remains unchanged after that one-field boundary substitution.
- The native error MQTT builder lives in `machine/mqtt/commands/print_error.rs`; near-limit `commands.rs` only declares and dispatches the new variant.
- Hub live registration, claims, and exact-session cleanup live in `sessions/live_commands.rs`; `sessions.rs` retains registry/session basics.
- Hub ack/result event handling moves from near-limit `grpc/inbound.rs` into `grpc/inbound/commands.rs`, where it uses the claim API.
- Hub operation audit metadata moves from near-limit `repositories/commands/operations.rs` into `repositories/commands/operations/audit.rs`.
- Hub ownership-checked persistence lives in `repositories/commands/audit/printer_operations.rs`; its deterministic reassignment pause is isolated in a test-only child module.
- Plugin live-operation orchestration is implemented behind helpers in `routes/printer_operations/live.rs`, so near-limit `routes/plugin.rs` remains a thin authenticated handler.
- Network-plugin status-request classification lives in the typed Rust `studio_status/request.rs` module and is exposed through a flat FFI result.
- Network-plugin parser outcomes and policy remain in the existing Rust `gcode` modules; the FFI export in near-limit `lib.rs` is a short adapter and does not grow past 400 LOC.

`shim.cpp` is already a pre-existing oversized ABI translation unit. Its changes are limited to parser-result adaptation, explicit tunnel/callback ownership, active-local ABI state, and shared routing of the pre-existing status requests; no new printer policy, status JSON construction, HTTP behavior, or JSON request recognition is added there. Choosing the callback and routing the numeric Rust classifier result before operation parsing are ABI responsibilities; exact recognition of `get_version`/`pushall` remains entirely in typed Rust. Splitting unrelated legacy ABI entrypoints would materially expand this bug fix and is outside scope.

## Native Error Actions

Introduce a dedicated semantic operation. `error_action` is intentionally distinct from the existing outer Serde discriminator `action`:

```rust
HandlePrintError {
    error_action: PrintErrorAction, // Resume | Ignore | Stop
    print_error: u32,
    printer_job_id: String,
    sequence_id: u64,
}
```

### Cross-layer contract

The network plugin sends exactly this REST body:

```json
{
  "action": "handle_print_error",
  "error_action": "resume",
  "print_error": 83918929,
  "printer_job_id": "<job-id>",
  "sequence_id": 20042
}
```

`error_action` accepts only `"resume"`, `"ignore"`, or `"stop"`. This operation is accepted only by `POST /api/v1/plugin/printers/{printer_id}/operations`, authenticated by the existing exact `[PluginStudio]` token scope. The shared tenant control route rejects `action:"handle_print_error"`; this is a Studio protocol recovery action, not a general dashboard control.

Unlike ordinary durable queued controls, `handle_print_error` is dispatched through the existing live-session command pattern because it is a response to a currently displayed printer condition:

1. Hub resolves the printer's Agent and reads the current `SessionToken` only when that live session advertises `AGENT_CAPABILITY_HANDLE_PRINT_ERROR`.
2. Hub passes the resolved Agent as `expected_agent_id` into persistence. SQLite opens an `IMMEDIATE` transaction; PostgreSQL locks the tenant/printer/expected-Agent row. Ownership revalidation, command insert, and audit insert share that transaction. If the same serial was reassigned before this linearization point, Hub returns `printer_operation_unavailable`, persists no command or audit, and sends to neither Agent.
3. After persistence succeeds, Hub builds the protobuf command from the same typed payload with a dedicated live-printer-operation builder and calls a capability-aware extension of `SessionRegistry::try_dispatch_live_command` with the same tenant, Agent, `SessionToken`, command ID, and required capability.
4. Under the session registry lock, that method atomically verifies tenant, Agent, exact token, and capability, records the pending live command, and performs the existing nonblocking channel send. A replacement registration cannot interleave between validation and send.
5. If the token became stale, the capability is absent, or the channel is unavailable, no old/new stream receives the command. Hub transitions the persisted command to failed with the full internal cause and returns `printer_operation_unavailable` to the plugin route.

An offline or incapable Agent is rejected before command creation. A replacement race may leave a failed command for audit, but never sends oneof tag 25 to the wrong session. A printer-reassignment race either commits the command while the expected Agent still owns the row or rejects before persistence; a stale Agent snapshot can never be inserted after the new owner is authoritative.

The A→B reassignment window is exercised with a `cfg(test)`-only ownership pause immediately before the transaction begins; production builds contain neither the pause registry nor the wait. Each installed pause receives a monotonic generation and duplicate installation for the same printer is rejected before replacing the current entry. The public test wait is bounded to five seconds, while unit tests can inject a shorter timeout. Its matching-generation `Drop` cleanup removes only its own entry, so timeout, cancellation/task abort, and normal scope exit cannot leak a pause or delete a newer generation. Three focused regressions prove unreachable-wait timeout cleanup, aborted-wait cleanup, and duplicate-install rejection without poisoning the shared mutex.

### Current session-cleanup delta

The current Hub has only a link-printer-specific subset of the required cleanup behavior. The implementation must modify or add each path explicitly:

| Session path | Current behavior | Required behavior |
| --- | --- | --- |
| Inbound stream closes | `grpc/inbound.rs::fail_pending_live_commands_on_close` drains pending commands only after `remove_if_current`, with hard-coded `printer link` wording. | Generalize it into the transition-serialized shared helper and use the native-operation close reason. |
| Forced local/cluster close | `SessionRegistry::close_local_agent` removes the session, sends close, returns `()`, and discards the pending map; the cluster `AgentClose` handler calls the same method. | Return the exact removed `AgentSession` and make both call sites clean it. |
| Stale-session expiry | `SessionRegistry::expire_stale` returns exact removed sessions after marking Agents offline, but its caller currently counts and discards them. | The runtime caller cleans every returned session before reporting the count. |
| Replacement registration | `SessionRegistry::register` returns the exact previous session, but gRPC setup ignores the return value. | Clean that previous session before starting the replacement pumps. |

All four paths are implementation work for this change; none may rely on the old inbound helper's link-printer-only wording or behavior.

Live command acknowledgement/result handling is linearized against replacement and removal rather than relying on the existing non-atomic `while_current` before/after checks. This claim path replaces the current `while_current` handling for every command present in `pending_live_commands`, including the existing link-printer live flow; link-printer access-code redaction metadata remains on the pending entry. A `NotPending` event continues through the ordinary durable-command `while_current` path only after typed persisted-command inspection proves it is not live-only. Unclaimed `link_printer` and `PrinterOperationKind::HandlePrintError` events are ignored until exact-session cleanup terminalizes them, so a reconnect cannot complete a detached session's command. Each `AgentSession` owns a `live_command_transition` async mutex in addition to `pending_live_commands`. `SessionRegistry::claim_current_live_command`:

1. acquires the session-registry lock;
2. verifies tenant, Agent, exact `SessionToken`, and that the command ID is in that session's pending-live map;
3. acquires an owned permit from that exact session's `live_command_transition` while the registry lock is still held;
4. rechecks the pending entry and returns a claim containing the permit, pending map, command ID, and any link-printer access-code metadata.

The inbound handler holds the claim across the repository acknowledgement/result transition and never calls back into `SessionRegistry` while holding it. An accepted acknowledgement leaves the command pending for its result; a rejected acknowledgement or any terminal result removes the pending entry before releasing the claim. A `NotCurrent` claim result is ignored. A current command with no pending-live entry continues through the ordinary durable-command path. Therefore a live event that obtains the claim first is completed before removal cleanup can run, while a replacement/removal that obtains the registry lock first makes the old event `NotCurrent`; there is no window in which the old stream can update a command after replacement cleanup has failed it.

The governing lock order is registry → current session's transition permit → pending-map mutex. Code holding a transition permit must never acquire the registry lock. Replacement/removal detaches the exact session under the registry lock, releases that lock, and only then acquires the detached session's transition permit for cleanup. This invariant prevents a registry/transition lock cycle and makes the linearization point inspectable in code review.

Session replacement cleanup is explicit. `SessionRegistry::register` already returns the exact replaced `AgentSession`; the gRPC connection setup consumes that returned session, acquires its `live_command_transition`, drains only its still-pending live commands, and marks each drained command failed with `agent session replaced before printer operation completed` before starting the replacement pumps. It never removes or mutates the newly registered session. A claimed terminal result that linearized before replacement may complete and remove itself; an accepted acknowledgement that linearized before replacement remains pending and is then failed by replacement cleanup. Late ack/result events whose old token lost the race are ignored.

Every other session-removal path consumes and cleans the exact removed session through the same transition-serialized helper:

- normal inbound close uses token-scoped `remove_if_current` and fails remaining commands with `agent connection closed before printer operation completed`;
- `close_local_agent` returns the removed `AgentSession`, and both the local close call and cluster close-message handler fail its remaining commands with `agent session closed before printer operation completed`;
- stale-session expiry returns each exact removed session and fails its remaining commands with `agent session expired before printer operation completed`.

Every `AppState` also owns a stable UUID instance generation. Cluster `AgentClose` messages carry the source instance ID. The publishing instance has already detached and cleaned its exact target session, so its runtime ignores delayed same-source delivery; a sibling instance still detaches and cleans the session that is current when the cross-source message is handled. This prevents local S2 from being removed when it reconnects while S1 cleanup is blocked without weakening cross-replica close delivery.

The existing stale unowned recovery uses a backend-neutral SeaORM `Kind == "link_printer"` column filter. It is extended to also select sent/acknowledged `Kind == "printer_operation"` candidates, deserialize those candidate payloads to `PrinterOperationPayload` in Rust, and retain only `operation.type:"handle_print_error"`. It excludes command IDs still owned by a local session and, after the existing live-command timeout, marks an unowned record failed with `live printer operation owner unavailable before completion`. This preserves SQLite/PostgreSQL parity without backend-specific JSON predicates, covers Hub process loss between live send and normal session cleanup, and never reclaims the operation through the durable queued pump.

`SessionRegistry` and the pending-owner exclusion set are process-local, matching the existing live link-printer path. The plugin route succeeds only on the Hub process that owns the Agent stream; a non-owning replica returns `printer_operation_unavailable`, and another replica cannot distinguish that owner's pending IDs during stale recovery. This change therefore requires one active Hub process for native print-error actions. Cross-replica live-command forwarding and ownership-aware stale recovery are outside this change; session affinity alone is not sufficient.

The repository persists the operation in the existing command payload shape:

```json
{
  "printer_id": "<pandar-printer-id>",
  "serial_number": "<serial>",
  "operation": {
    "type": "handle_print_error",
    "error_action": "resume",
    "print_error": 83918929,
    "printer_job_id": "<job-id>",
    "sequence_id": 20042
  }
}
```

The audit event uses the existing flat metadata convention:

```json
{
  "agent_id": "<agent-id>",
  "serial_number": "<serial>",
  "action": "handle_print_error",
  "error_action": "resume",
  "print_error": 83918929,
  "printer_job_id": "<job-id>",
  "sequence_id": 20042
}
```

Append this protobuf contract without renumbering existing fields:

```proto
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

Add `HandlePrintErrorOperation handle_print_error = 25;` to `PrinterOperation.operation`. Hub REST rejects missing/unknown `error_action`; Agent rejects `PRINT_ERROR_ACTION_UNSPECIFIED` and unknown protobuf enum values. The ordinary durable outbound pump never selects this live-dispatched operation.

The ordinary queued `enqueue_printer_operation_with_audit` repository entry point explicitly rejects `PrinterOperationKind::HandlePrintError`; only `create_printer_operation_sent_with_audit` accepts it. This makes the live-only status invariant enforceable below the route layer instead of relying only on request conversion.

The dedicated `live_printer_operation_hub_command` maps the typed `HandlePrintError` payload after the sent record is committed. The record-based durable `hub_command_from_record` `"printer_operation"` arm explicitly returns `failed_precondition` if it ever deserializes `PrinterOperationKind::HandlePrintError`, matching the existing live-only link-printer guard. This keeps protobuf match exhaustiveness explicit while ensuring the ordinary pump can never convert the live-only operation.

### Studio parser decision table

The typed Studio parser classifies a message before mapping it:

| Studio `print` message | Classification | Result |
| --- | --- | --- |
| `command:"ignore"` with any fields | Native candidate | Require the complete valid native shape; otherwise reject. |
| `command:"resume"` or `"stop"` with `err` present | Native candidate | Require the complete valid native shape; otherwise reject. |
| `command:"resume"` or `"stop"` with any present `param` other than the empty string | Native candidate | Require the complete valid native shape; only the exact string `"reserve"` can pass validation. |
| `command:"resume"` or `"stop"` without `err` and with `param` absent or exactly `""` | Ordinary control | Preserve the existing ordinary Resume/Stop mapping; `job_id` or `sequence_id` alone does not make it native because Studio's normal Stop may include a job ID. |
| Other recognized Studio controls | Ordinary control | Preserve existing mapping. |
| Everything else | Unsupported | Do not create a Hub request. |

A missing or non-string `print.command` is unsupported because no native candidate can be identified. “Partial native candidate” means a command-known Resume/Ignore/Stop candidate from the rows above with one of its remaining required native fields missing or invalid. In particular, `job_id` or `sequence_id` alone—even when present with a wrong type—does not turn an otherwise ordinary Resume/Stop into a native candidate.

A native candidate is valid only when all native markers have the exact direct-Studio types:

- `print.command` is `resume`, `ignore`, or `stop`;
- `print.param` is exactly `"reserve"`;
- `print.err` is a decimal string that parses to `1..=i32::MAX`, the positive range Studio's `int m_error_code` can produce;
- `print.job_id` is a string (an empty string remains valid because Studio writes the field even when `job_id_` is empty);
- `print.sequence_id` is a nonnegative decimal string parseable as `u64`. Studio produces a canonical signed-`int` decimal string, which is a subset of this accepted transport domain; Pandar deliberately widens the parsed value and emits its canonical decimal representation unchanged in numeric meaning.

Partial candidates never fall back to an ordinary Resume or Stop. `ignore` is accepted only in the complete native error-action shape. The Rust parser returns distinct outcomes for valid operation, unsupported noncandidate, and invalid native candidate so the C++ ABI shim does not implement this policy.

### Cloud and local ABI behavior

| Parser outcome | `bambu_network_send_message` | `bambu_network_send_message_to_printer` |
| --- | --- | --- |
| Valid operation | Submit exactly one Hub request; propagate submit success/failure. | Submit exactly one Hub request; propagate submit success/failure. |
| Unsupported noncandidate | Preserve current cloud behavior: return `BAMBU_NETWORK_SUCCESS`, do not change `last_error`, and make no Hub request. | Preserve current local behavior: set `last_error` to `{"error":"unsupported_printer_operation"}`, return `BAMBU_NETWORK_ERR_INVALID_RESULT`, and make no Hub request. |
| Invalid native candidate | Set `last_error` to `{"error":"unsupported_printer_operation"}`, return `BAMBU_NETWORK_ERR_INVALID_RESULT`, and make no Hub request. | Set the same `last_error`, return the same error, and make no Hub request. |

The shim only branches on the typed Rust parser outcome and adapts it to the ABI return code. Candidate recognition, validation, and operation construction remain in Rust.

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
- The plugin and Hub reject zero, negative, or out-of-range error codes; `print_error` must be `1..=i32::MAX` for an action even though its wire/storage type is unsigned.
- The Hub plugin route requires the exact five-field REST shape above and rejects missing, extra, or cross-operation fields as `invalid_printer_control`.
- The shared tenant route does not expose this operation.
- Agent rejects an unspecified action enum rather than issuing a printer command.
- The plugin's `post_json` request path wraps the `reqwest` failure with the context `POST plugin printer operation request` and writes exactly one diagnostic line to process stderr through a Rust-owned writer boundary: `pandar network plugin request failed: {error:#}`. Production passes a locked stderr writer; tests pass a byte buffer to the same formatter/writer and assert the operation context and lower-level connection cause are both present.
- The diagnostic must not include the bearer token, request JSON, response body, access code, or HTTP headers. The ABI receives only the stable, redacted `{"error":"hub_unavailable"}` body.
- New HTTP, gRPC, repository, and MQTT failure paths retain or log their complete lower-level cause chain before converting to stable external errors. Tests assert the stable external error separately from the internal chained diagnostic.

No raw arbitrary MQTT command is added to the public operation API.

## Testing

Tests will be written before each implementation layer and observed failing for the missing behavior.

1. Agent report tests:
   - absent, zero, and nonzero numeric `print_error` preserve presence semantics;
   - `i32::MAX` is retained; fractions truncate toward zero; nonpositive converted values clear to zero; out-of-signed-32-range values are ignored without losing progress, HMS, or materials in the same report;
   - numeric zero creates no generic diagnostic;
   - structured/string print errors retain the existing diagnostic behavior;
   - empty/nonnumeric strings are preserved for `job_id`, signed-64-bit-domain numbers truncate and normalize to decimal strings, and a present out-of-range/other value explicitly clears to `""` without replacing `task_id` or losing other report fields.
2. Protocol and Hub tests:
   - gRPC presence conversion carries explicit zero and printer job identifiers;
   - live status persists identically in SQLite and in a configured PostgreSQL database for absent, zero, nonzero, empty job ID, and `i32::MAX` values;
   - missing or invalid fields do not overwrite prior state;
   - the plugin printer API returns typed optional fields;
   - plugin route to persisted payload/audit to protobuf is covered end-to-end for all three error actions;
   - the tenant control route rejects `handle_print_error`.
   - plugin live dispatch rejects offline/old Agent sessions without the capability;
   - capable-to-incapable and incapable-to-capable replacement races prove a stale `SessionToken` cannot send, claim, or fail a command for the current stream, while a post-persistence race produces only a failed audit command and emits no oneof tag 25 to either wrong stream.
   - acknowledgement/result claims and replacement are tested in both lock orders: a claimed terminal result finishes before cleanup, while replacement that wins first fails the exact old pending command and causes the late event to be ignored;
   - replacement after an accepted acknowledgement but before result fails the still-pending old command, preserves the new session, and ignores stale late results;
   - normal close, forced local/cluster close, stale-session expiry, and stale unowned-command recovery all terminalize only the exact removed/unowned live commands and never mutate the current replacement session.
   - same-serial Agent A→B reassignment between route resolution and persistence returns `printer_operation_unavailable`, persists no command/audit, and emits no oneof tag 25; SQLite `IMMEDIATE` and real PostgreSQL row-lock paths are both exercised;
   - the deterministic reassignment pause has dedicated timeout, abort, and duplicate-install regressions proving generation-safe cleanup and subsequent reinstallability;
   - close/expiry followed by reconnect proves a current session's `NotPending` ack/result cannot complete a detached live-only command, while ordinary durable fallback remains unchanged;
   - a real control-plane consumer proves delayed same-source `AgentClose` delivery preserves the replacement session, while sibling-instance close remains effective.
3. Network-plugin tests:
   - telemetry emits numeric nonzero and zero values and omits unknown values;
   - the parser distinguishes ordinary controls from all three native error actions;
   - every partial candidate combination from the decision table is rejected without falling back or issuing a Hub request;
   - cloud/local tests assert the exact stable parser error body at the Rust FFI boundary, then assert the ABI return code, observable error/connection state, and Hub request count for valid, unsupported, and invalid-native outcomes; no production-only `last_error` inspection ABI is added for tests;
   - with both callbacks registered, cloud status/version emissions reach only `on_message`; `connect_printer` reports only `on_local_connect` success and does not invoke `on_printer_connected` or a message callback; local `get_version`/`pushall` and local heartbeat status reach only `on_local_message`; neither tunnel falls back to the other callback and status requests create zero Hub operations;
   - `get_version` emits only an `info.get_version` response and `pushall` emits only `print.push_status` on each tunnel;
   - lookalike commands such as `not_get_version`/`not_pushall` follow the unsupported Cloud/LAN contracts, while native action job IDs containing both substrings still produce the six exact Hub POSTs;
   - distinct cloud/local serials prove independent heartbeat targets after explicitly excluding the local serial from cloud subscriptions, and a successful refresh proves it is not re-added; a second local connect replaces the first without changing account selection; `disconnect_printer` stops only the local target while cloud heartbeats continue; a separate same-serial case proves one explicit emission per tunnel;
   - the Studio C++ ABI probe is mandatory on Windows and Unix: compiler discovery must use the configured `CXX` or the platform toolchain discovery available to the crate build, missing/failed probe compilation is a test failure rather than a successful skip, and MSVC compilation uses the same `/MD` and `_ITERATOR_DEBUG_LEVEL=0` STL runtime ABI as the shim and Studio;
   - the ABI probe observes the exact typed Hub request and final Studio `push_status` field types;
   - the HTTP error-path test observes a stable external body and a complete internal cause chain.
4. Agent command tests:
   - Resume, Ignore, and Stop payloads match the reference field-for-field;
   - the supplied Studio sequence is retained;
   - topic, QoS, and retain behavior match direct LAN.
5. Full verification:
   - `cargo fmt --all -- --check`;
   - `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
   - `cargo nextest run --manifest-path "Cargo.toml" --workspace`;
   - require `PANDAR_TEST_POSTGRES_URL` for the targeted PostgreSQL live-status and operation-contract test invocation; a skip does not satisfy backend verification;
   - launch Agent, Hub, and the rebuilt plugin, then confirm on Studio's Device page that a printer already reporting a genuine mismatch displays the native dialog. Do not induce a fault or click Resume/Ignore/Stop without explicit operator approval; exact action payloads are verified automatically against a fake MQTT transport.

## Deployment, Rollback, and Documentation

This is an additive protocol/database change. The explicit Agent capability and token-bound live dispatch prevent the new operation from being dispatched to an old, offline, downgraded, stale, or partially rolled-out Agent.

Deployment order:

1. Deploy the new Agent first. An old Hub ignores its unknown additive report/capability fields.
2. Apply the new nullable migration and deploy the new Hub. Hub now records the live capability and rejects plugin live dispatch when it is absent.
3. Deploy the new network plugin. Even if an Agent was missed or later downgraded, the token-and-capability-bound live dispatch prevents oneof tag 25 from reaching it.

Expected mixed-version behavior:

- old Agent + new Hub: existing functions work, the empty capability set prevents `handle_print_error` live dispatch, and no numeric error state is reported;
- new Agent + old Hub: unknown additive report fields are ignored and existing operations continue;
- old plugin + new Hub/Agent: existing behavior continues without the restored dialog/action path;
- new plugin + old Hub is unsupported and prevented by the deployment order because the old route rejects the new REST action.

Rollback order is plugin, Hub, then Agent. Before rolling Hub or Agent back, verify no sent/pending `printer_operation` command payload contains `operation.type:"handle_print_error"`; wait for its ack/result, exact session-removal cleanup, or stale unowned-command recovery so every such command is terminal. This operation never remains queued for the durable outbound pump. Nullable columns are left in place during binary rollback because older binaries ignore them; destructive column removal is not part of operational rollback.

After implementation, update `docs/roadmap.md` with the completed native error-state/action bridge, the SQLite/PostgreSQL evidence, and any remaining real-printer action probe that was intentionally not clicked for safety.

## Scope

This change does not add a Pandar-owned dialog, infer any printer error, change unrelated controls, expose raw MQTT, or implement `mc_print_error_code`. It only restores the native Studio state and action path that Pandar currently drops.
