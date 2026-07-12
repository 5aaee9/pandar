# Bambu Studio `gcode_line` Passthrough Design

## Context

Bambu Studio sends interactive legacy G-code through one MQTT-style command:

```json
{"print":{"command":"gcode_line","param":"<string>","sequence_id":"<string>"}}
```

`param` is one string. It can contain multiple LF-delimited commands, trailing spaces, comments,
blank lines, and a final newline. Bambu Studio does not send a `gcode_lines` array. The current
network plugin recognizes a bounded set of Home, XYZ, and temperature forms and converts them to
typed semantic operations. Other valid Studio commands, such as `M106 P1 S127 \n` and
`M620 C1 \n`, are rejected before reaching Hub.

This follow-up extends the feature-aware axis-control work delivered in commit `cb059c6f`.

## Goal

Support every well-formed Studio `print.command = "gcode_line"` wrapper without adding a G-code
command whitelist or interpreting unrecognized G-code. Keep the existing semantic mappings when a
known command can be represented losslessly by an existing operation. Otherwise, carry the decoded
`param` string unchanged through Plugin, Hub, Agent, and the printer MQTT payload.

Exact passthrough applies to `param`, not to Studio wrapper metadata. Agent continues to generate
the printer-facing `sequence_id` as it does for all current semantic operations. Studio `user_id`,
QoS, flag, and original sequence ID are not added to the operation contract.

## Non-goals

- Do not add a `gcode_lines` JSON array or send one MQTT message per line.
- Do not add generic raw MQTT or raw Bambu JSON passthrough.
- Do not change modern `back_to_center` or `xyz_ctrl` feature selection.
- Do not change existing semantic Home, movement, or temperature behavior.
- Do not expose raw G-code through the normal tenant printer-operation endpoint in this change.
- Do not add G-code content, line-count, whitespace, newline, or command-name restrictions.
- Do not move or home a live printer as part of automated verification.

## Considered Approaches

### 1. Typed exact-string operation — selected

Add a `gcode_line` operation with one `param: String` field. Preserve that string across JSON,
durable command storage, protobuf, Agent conversion, and typed MQTT serialization.

This is the smallest boundary that matches Bambu Studio and does not broaden access to arbitrary
MQTT commands.

### 2. Split into `Vec<String>` — rejected

Splitting and joining lines loses information such as final newlines, CRLF versus LF, empty final
lines, and trailing whitespace unless a second line-ending model is introduced. That complexity is
unnecessary because the printer protocol already accepts one string.

### 3. Pass the complete Studio JSON envelope — rejected

Raw envelope passthrough would preserve unrelated metadata but would also create a generic Bambu
command tunnel, bypass typed serde contracts, and expand the plugin's authority beyond this request.

## Data Flow

### Network plugin

For a typed Studio wrapper whose command is exactly `gcode_line` and whose `param` is a JSON string:

1. Run the existing bounded semantic G-code parser.
2. If it recognizes the command, return the existing semantic operation unchanged.
3. Otherwise return `{"action":"gcode_line","param":<exact decoded string>}`.

Only the typed Studio wrapper receives this fallback. A plain string passed to
`pandar_plugin_operation_json_from_gcode` continues to require an existing semantic parse; arbitrary
unwrapped input remains unsupported. A missing, null, boolean, number, object, or array `param`
remains `unsupported_printer_operation`.

The Rust operation enum gains `GcodeLine { param: String }`. Its validation accepts every string,
including an empty string, because this feature deliberately does not impose a content policy. The
C++ shim remains a thin ABI adapter and receives no parsing or policy logic.

### Hub HTTP and persistence

The plugin printer-operation endpoint accepts `action = "gcode_line"` with exactly one `param`
field and no required-device-feature field. The normal tenant printer-operation endpoint rejects
this action, keeping the authorization expansion limited to authenticated Studio plugin traffic.

The existing Hub default HTTP request-body limit remains 64 KiB. This is an existing transport
boundary, not a new G-code policy. The complete JSON request, including the escaped `param`, must fit
that limit. The plugin handler currently maps every `JsonRejection`, including an exceeded body
limit, to HTTP 400 `invalid_printer_control`; oversized requests retain that stable route behavior
and never enqueue a command. No smaller G-code-specific limit or new size-specific error is
introduced. The resulting MQTT payload also remains
subject to the existing 256 KiB Bambu MQTT packet limit; every request admitted by the 64 KiB HTTP
boundary fits beneath it after the small typed envelope is added.

Hub persists the typed operation in the existing durable `printer_operation` command payload. No
database migration is required. It intentionally uses the same first-dispatch lifecycle as other
queued Studio printer operations. While the command remains `queued`, it is not bound to the Studio
or Agent session that accepted the HTTP request, so a capable replacement session may claim it. Hub
changes the row to `sent` before writing the protobuf command to the gRPC channel. A disconnect,
closed channel, missing ACK, or missing result after that transition does not requeue or redispatch
the command. Therefore Hub itself does not replay a `sent` G-code operation to a replacement
session. Lower printer transports such as MQTT QoS 1 retain their own delivery semantics, but this
feature adds no Hub retry or exactly-once guarantee.

A stale closing session cannot dispatch a still-queued operation after its replacement becomes
current. This change does not create a second live-only command path or alter the existing
queued-to-sent transition ordering.

Audit metadata records the operation action but does not duplicate the raw G-code string into
structured audit metadata; the durable command payload remains the source of the exact value.

### Hub/Agent protocol

Add an unused `PrinterOperation.oneof` tag:

```proto
GcodeLineOperation gcode_line = 26;
```

with:

```proto
message GcodeLineOperation {
  string param = 1;
}
```

Add `AGENT_CAPABILITY_GCODE_LINE = 4`. Updated Agent advertises it. Hub sends a queued `gcode_line`
operation only while the exact current Agent session advertises this capability. An older Agent is
not given an operation it cannot decode; Hub marks that command failed with the preserved capability
cause. There is no downgrade or alternate command.

Capability validation and enqueue-to-stream dispatch occur while holding the existing exact-session
transition lease used by queued required-feature commands. Replacement waits for a dispatch that
already owns that lease; once replacement wins the lease, the old session cannot send. The
G-code-line capability gate belongs in a new focused queued-command capability module rather than in
the printer-feature bitmap module.

The capability is an Agent wire-compatibility assertion. It is not a printer feature and must not be
mixed into `BambuDeviceFeatures` or Studio `fun`.

### Agent and printer MQTT

Agent converts the protobuf operation to `PrinterOperation::GcodeLine { param }` without parsing or
normalizing it. The typed `GcodeLineCommand` stores the exact `param` string, and its serde payload
builder emits:

```json
{"print":{"command":"gcode_line","param":"<exact string>","sequence_id":"<new Studio-style ID>"}}
```

Existing semantic operations that synthesize G-code construct their canonical parameter string
before creating `GcodeLineCommand`; their current printer-facing output remains unchanged.

## Module Boundaries

Several affected modules are already near the 400-line production limit. Implementation must not
append large branches to them:

- Put plugin-only request conversion for `gcode_line` in a new
  `routes/printer_operations/gcode_line.rs` module; keep shared request-field wiring minimal in
  `printer_operations.rs`.
- Put Hub queued Agent-capability selection and exact-session dispatch helpers in a new
  `grpc/commands/agent_capabilities.rs` module, parallel to but separate from printer
  `device_features.rs`.
- Put `PrinterOperationKind::GcodeLine` validation/audit mapping in the existing small operation
  submodules, extracting a focused helper if any touched production file would exceed 400 lines.
- Put Hub-to-protobuf G-code-line construction in a new `grpc/commands/conversion/operations.rs`
  submodule if adding the exhaustive arm would take `conversion.rs` over 400 lines.
- The existing Agent typed MQTT implementation remains in `machine/mqtt/commands/axis.rs`; converting
  `GcodeLineCommand` from `lines: Vec<String>` to `param: String` reduces normalization logic there.

The implementation plan must list the post-change LOC check for every touched production Rust file.

## Error Behavior

- Malformed Studio wrappers return the existing stable `unsupported_printer_operation` error and do
  not contact Hub.
- Invalid plugin operation JSON returns the existing `invalid_printer_operation` error.
- Missing Agent wire capability fails the queued command; it is never silently downgraded.
- Requests exceeding the existing 64 KiB Hub body limit retain the plugin route's existing HTTP 400
  `invalid_printer_control` mapping for `JsonRejection`; no command is persisted or sent.
- MQTT subscription, publish, and result-report errors retain their complete cause chains.
- Printer-reported errors use the existing operation result path.

## Tests

### Plugin

- Parser tests prove known Home/XYZ/temperature G-code still becomes existing semantic operations.
- Parser tests prove unrecognized `gcode_line.param` becomes `GcodeLine` with exact LF, CRLF,
  whitespace, comments, blank lines, and final newline preservation.
- Parser tests prove arbitrary unwrapped G-code and non-string `param` values remain unsupported.
- The compiled ABI probe sends one Cloud and one LAN unrecognized `gcode_line` command and proves
  both reach the mock Hub with exact `param` values.

### Hub

- Plugin route tests prove exact persistence and a queued response.
- Tenant route tests prove `gcode_line` remains unavailable there.
- HTTP boundary tests prove a request below the existing 64 KiB body limit is accepted and one above
  it returns HTTP 400 `invalid_printer_control` without persistence. The test derives bodies around
  the router limit rather than inventing a separate `param` limit.
- Repository JSON round-trip and audit tests cover the new exhaustive enum variant.
- gRPC tests prove exact protobuf conversion, current-session capability delivery, old-session
  rejection, replacement-session behavior, and that a stale closing session cannot dispatch after
  replacement becomes current.
- Command lifecycle tests record the existing first-dispatch behavior: a replacement may receive an
  operation that is still `queued`, while an operation already marked `sent` is not automatically
  requeued or delivered again after disconnect.
- SQLite coverage is mandatory. Real PostgreSQL coverage runs when `PANDAR_TEST_POSTGRES_URL` is
  configured and is explicitly reported as skipped otherwise; no schema change is involved.

### Agent

- Hello tests prove the exact new capability advertisement.
- Command conversion tests preserve `param` exactly.
- Typed MQTT payload tests preserve `param` exactly.
- Machine boundary tests prove the final published printer payload contains one `gcode_line` command
  with the exact input string.

### Completion

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- focused Plugin, Hub, Agent, compiled ABI, and module-size tests
- `cargo nextest run --manifest-path Cargo.toml --workspace`

## Documentation

Update the Bambu Studio compatibility document, development protocol notes, and roadmap to state
that recognized G-code remains semantic-first while other Studio `gcode_line.param` strings use the
typed exact-string path.

## Rollback

The change requires no migration. Revert the delivery commit to remove it. Before rolling Hub or
Agent back independently, drain or explicitly fail queued `gcode_line` operations because an older
binary cannot deserialize the new persisted/protobuf operation. Existing semantic operations and
device-feature state remain unaffected.

## Acceptance Criteria

1. Bambu Studio Cloud and LAN calls with any string `gcode_line.param` whose complete plugin HTTP
   request fits the existing 64 KiB body limit return through the normal plugin operation path
   instead of being rejected solely because the G-code is unknown.
2. Known semantic commands keep their existing operation type and modern/legacy feature behavior.
3. Unknown wrapped G-code reaches the printer in one `gcode_line` MQTT command with byte-for-byte
   equal UTF-8 string content after JSON decoding, including whitespace and line endings.
4. No `gcode_lines` array, raw MQTT tunnel, C++ policy, or G-code whitelist is introduced.
5. Old Agents do not receive the new protobuf operation and no fallback is attempted.
6. Replacement-session behavior follows the documented command lifecycle: still-queued work may
   move to the capable replacement, while `sent` work is not replayed by Hub.
7. Required focused tests, ABI tests, formatting, strict Clippy, module-size guard, and workspace
   nextest pass, with any unavailable real PostgreSQL verification reported honestly.
