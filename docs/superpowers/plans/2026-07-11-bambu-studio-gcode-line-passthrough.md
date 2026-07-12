# Bambu Studio `gcode_line` Passthrough Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preserve unrecognized Bambu Studio `gcode_line.param` strings exactly from the Studio ABI through Hub and Agent to the printer while retaining existing semantic-first mappings.

**Architecture:** Add one typed `GcodeLine { param: String }` operation across the Plugin, Hub persistence, protobuf, and Agent. Hub exposes it only on the authenticated Studio plugin route and gates queued dispatch on a new exact-session Agent capability; Agent publishes the unchanged string in one typed MQTT `gcode_line` payload. Known Home, XYZ, and temperature G-code still maps to the existing semantic operations before the raw fallback.

**Tech Stack:** Rust 2024, serde, axum, SeaORM, tonic/prost, tokio, rumqttc, C++17 ABI probe, cargo-nextest.

## Global Constraints

- Source design: `docs/superpowers/specs/2026-07-11-bambu-studio-gcode-line-passthrough-design.md`.
- Baseline commit: `cb059c6f4799b3691bf3af99699179c1d1f7ae8e`.
- Upstream protocol is singular `print.command = "gcode_line"` with one possibly multiline string `param`; never add a `gcode_lines` array.
- Semantic parsing runs first. Only an unrecognized, typed Studio wrapper falls back to exact-string passthrough.
- Do not accept arbitrary unwrapped G-code as passthrough.
- Do not add G-code content, whitespace, newline, command-name, or line-count restrictions.
- Preserve the existing 64 KiB Hub request-body boundary and HTTP 400 `invalid_printer_control` mapping for every plugin `JsonRejection`.
- Keep the normal tenant operation endpoint unable to submit `gcode_line`.
- Add `AGENT_CAPABILITY_GCODE_LINE = 4`; do not use `BambuDeviceFeatures` or `fun` for this Agent wire capability.
- Do not downgrade or fall back when the current Agent lacks capability 4.
- Hub marks queued work `sent` before gRPC channel send and does not requeue `sent` commands. Preserve that lifecycle.
- `crates/pandar-network-plugin/src/shim.cpp` remains a thin C++ ABI adapter; no parsing or policy there.
- Known JSON shapes use typed serde structs/enums, not manual `serde_json::Value` extraction in production.
- Preserve lower-level error cause/context chains.
- Every touched production Rust module must remain at or below 400 LOC; do not use `include!`.
- SQLite and PostgreSQL behavior remain equivalent; no migration is required.
- Do not modify, delete, or stage pre-existing `crates/pandar-network-plugin/probe-*` directories.
- Do not move or home a live printer during verification.
- No task-level commits. Create one final Conventional Commit only after final SDD review, docs, and fresh verification.

## File Structure

### Wire and Agent

- `proto/pandar/agent/v1/agent.proto`: additive Agent capability 4 and `GcodeLineOperation` oneof tag 26.
- `crates/pandar-agent/src/lib.rs`: advertise capability 4.
- `crates/pandar-agent/src/tests.rs`: exact Hello capability regression.
- `crates/pandar-agent/src/commands/operations.rs`: convert protobuf `GcodeLineOperation` to the machine operation.
- `crates/pandar-agent/src/commands/operation_results.rs`: exhaustive action/result naming.
- `crates/pandar-agent/src/commands/tests.rs`: protobuf-to-machine exact-string boundary test.
- `crates/pandar-agent/src/machine/operations.rs`: machine `GcodeLine { param }` and MQTT command selection.
- `crates/pandar-agent/src/machine/mqtt/commands/axis.rs`: change `GcodeLineCommand` from `lines` to exact `param`.
- `crates/pandar-agent/src/machine/mqtt/tests.rs`: exact serialization and unchanged generated-command regressions.
- `crates/pandar-agent/src/machine/tests.rs`: final MQTT publish boundary.
- `crates/pandar-agent/src/machine/operations/axis.rs`: join existing generated legacy lines before constructing `GcodeLineCommand`.

### Hub

- `crates/pandar-hub/src/repositories/commands/operations.rs`: persisted `GcodeLine { param }`, action name, validation, and no printer-feature requirement.
- `crates/pandar-hub/src/repositories/commands/operations/audit.rs`: action-only audit metadata for G-code.
- `crates/pandar-hub/src/repositories/tests/commands.rs`: SQLite JSON round-trip/validation.
- `crates/pandar-hub/src/repositories/tests/postgres_commands.rs`: PostgreSQL-equivalent round-trip when configured.
- `crates/pandar-hub/src/routes/printer_operations.rs`: add `param: RequestField<String>` and delegate plugin-only conversion.
- `crates/pandar-hub/src/routes/printer_operations/gcode_line.rs`: new focused plugin-only request conversion.
- `crates/pandar-hub/src/routes/tests/plugin/operations.rs`: plugin acceptance, exact persistence, and 64 KiB boundary behavior.
- `crates/pandar-hub/src/routes/tests/printers.rs`: tenant rejection.
- `crates/pandar-hub/src/grpc/commands/agent_capabilities.rs`: new exact-session queued G-code capability gate.
- `crates/pandar-hub/src/grpc/commands/device_features.rs`: invoke the Agent gate under the existing transition lease before required-printer-feature gating.
- `crates/pandar-hub/src/grpc/commands/conversion.rs`: module wiring only after extraction.
- `crates/pandar-hub/src/grpc/commands/conversion/operations.rs`: new exhaustive persisted-operation-to-protobuf mapping, including G-code tag 26.
- `crates/pandar-hub/src/grpc/tests/commands/gcode_line.rs`: capability, conversion, replacement, and no-requeue lifecycle tests.
- `crates/pandar-hub/src/grpc/tests/commands.rs`: test-module wiring and mechanical constructors.

### Network plugin and ABI

- `crates/pandar-network-plugin/src/gcode/operation.rs`: typed `GcodeLine { param }` operation accepted for every string.
- `crates/pandar-network-plugin/src/gcode/studio_json.rs`: semantic-first `gcode_line` fallback.
- `crates/pandar-network-plugin/tests/operation_parser.rs`: exact fallback and malformed/unwrapped rejection tests.
- `crates/pandar-network-plugin/tests/http_boundary/printer_operations.rs`: operation-body validation and exact POST body.
- `crates/pandar-network-plugin/tests/fixtures/studio_abi_probe.cpp`: Cloud/LAN ABI calls using unrecognized G-code with exact whitespace/newlines.
- `crates/pandar-network-plugin/tests/studio_abi_probe.rs`: require the new probe evidence.
- `crates/pandar-network-plugin/tests/studio_abi_probe/mock_hub.rs`: capture expected G-code operations.
- `crates/pandar-network-plugin/tests/studio_abi_probe/mock_hub/operations.rs`: typed exact-body assertions.

---

### Task 1: Add the Wire Contract and Exact Agent MQTT Path

**Files:**
- Modify: `proto/pandar/agent/v1/agent.proto`
- Modify: `crates/pandar-agent/src/lib.rs`
- Modify: `crates/pandar-agent/src/tests.rs`
- Modify: `crates/pandar-agent/src/commands/operations.rs`
- Modify: `crates/pandar-agent/src/commands/operation_results.rs`
- Modify: `crates/pandar-agent/src/commands/tests.rs`
- Modify: `crates/pandar-agent/src/machine/operations.rs`
- Modify: `crates/pandar-agent/src/machine/operations/axis.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/commands/axis.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt/tests.rs`
- Modify: `crates/pandar-agent/src/machine/tests.rs`

**Interfaces:**
- Produces: `AgentCapability::GcodeLine`, protobuf `GcodeLineOperation { param: String }`, and `printer_operation::Operation::GcodeLine` at oneof tag 26.
- Produces: `machine::PrinterOperation::GcodeLine { param: String }`.
- Produces: `mqtt::GcodeLineCommand { param: String }` whose serializer emits `param` unchanged.
- Consumes: no Hub or Plugin behavior from later tasks.

- [ ] **Step 1: Add failing Agent Hello, conversion, payload, and machine-boundary tests**

Add exact tests equivalent to:

```rust
#[test]
fn hello_event_advertises_gcode_line_capability() {
    let hello = hello_event();
    assert!(hello.capabilities.contains(&(AgentCapability::GcodeLine as i32)));
}

#[tokio::test]
async fn printer_operation_gcode_line_reaches_gateway_without_normalization() {
    let param = "M106 P1 S127 \r\n; keep  \n\n";
    let command = printer_operation_command(
        Some(printer_operation::Operation::GcodeLine(GcodeLineOperation {
            param: param.to_owned(),
        })),
    );
    // Dispatch through the real command handler and assert the captured machine operation.
    assert_eq!(captured, MachinePrinterOperation::GcodeLine { param: param.to_owned() });
}

#[test]
fn gcode_line_payload_preserves_exact_param() {
    let param = "M106 P1 S127 \r\n; keep  \n\n";
    let payload = BambuMqttCommand::GcodeLine(GcodeLineCommand {
        param: param.to_owned(),
    })
    .command_payload();
    assert_eq!(payload.payload["print"]["param"], param);
}
```

Retain assertions for current generated Home, seven-line axis movement, and chamber-temperature
payloads so moving the join point cannot alter their bytes.

- [ ] **Step 2: Run the focused tests and prove RED**

Run:

```powershell
cargo nextest run -p pandar-agent -E 'test(~gcode_line) | test(hello_event_has_agent_identity_version_and_exact_capability)'
```

Expected: compilation/test failure because capability 4, protobuf G-code tag 26, internal variants,
and exact `param` field do not exist.

- [ ] **Step 3: Add the additive protobuf definitions**

Make the existing enum and oneof additions exactly:

```proto
enum AgentCapability {
  AGENT_CAPABILITY_UNSPECIFIED = 0;
  AGENT_CAPABILITY_HANDLE_PRINT_ERROR = 1;
  AGENT_CAPABILITY_HANDLE_PRINT_ERROR_SEQUENCE_ZERO_PUBACK_ONLY = 2;
  AGENT_CAPABILITY_REQUIRED_DEVICE_FEATURES = 3;
  AGENT_CAPABILITY_GCODE_LINE = 4;
}

message PrinterOperation {
  string serial_number = 1;
  repeated DeviceFeature required_device_features = 2;
  oneof operation {
    // existing tags 10..25 unchanged
    GcodeLineOperation gcode_line = 26;
  }
}

message GcodeLineOperation {
  string param = 1;
}
```

Do not renumber or reuse any existing tag.

- [ ] **Step 4: Implement the exact Agent conversion and serializer**

Advertise `AgentCapability::GcodeLine`. Add the protobuf conversion arm:

```rust
Some(printer_operation::Operation::GcodeLine(operation)) => {
    Ok(MachinePrinterOperation::GcodeLine {
        param: operation.param,
    })
}
```

Add the machine operation and MQTT selection:

```rust
PrinterOperation::GcodeLine { param } => {
    Ok(BambuMqttCommand::GcodeLine(GcodeLineCommand { param }))
}
```

Change the MQTT type and serializer to:

```rust
pub struct GcodeLineCommand {
    pub param: String,
}

param: command.param.clone(),
```

Existing semantic callers must join their generated lines before construction:

```rust
GcodeLineCommand {
    param: legacy_move_lines(...).join("\n"),
}
```

Do the same for multi-command chamber-temperature output. Single generated commands pass their
existing string directly. Add exhaustive operation action/result names as `"gcode_line"`.

- [ ] **Step 5: Run focused and full Agent verification**

Run:

```powershell
cargo fmt --all -- --check
cargo nextest run -p pandar-agent -E 'test(~gcode_line) | test(hello_event_has_agent_identity_version_and_exact_capability) | test(~axis_controls)'
cargo nextest run -p pandar-agent
cargo clippy -p pandar-agent --all-targets -- -D warnings
cargo nextest run -p pandar-core --test module_size
```

Expected: all commands pass; existing generated G-code payload assertions remain byte-identical; all
touched production modules are at or below 400 LOC.

- [ ] **Step 6: Run the independent Task 1 spec/quality review gate**

Give a reviewer only Task 1's spec/plan excerpt, Task 1 diff, and verification output. Require literal
`VERDICT: APPROVE` for both spec compliance and code quality. If revised, add/adjust failing tests,
fix, rerun Task 1 verification, and re-review.

- [ ] **Step 7: Record the task evidence without committing**

Append focused/full test counts, Clippy/fmt/module-size output, changed paths, and RED→GREEN evidence
to `.superpowers/sdd/progress.md`. Do not stage, commit, or push.

---

### Task 2: Add Plugin-only Hub Persistence and HTTP Boundary Behavior

**Files:**
- Modify: `crates/pandar-hub/src/repositories/commands/operations.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/operations/audit.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/commands.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/postgres_commands.rs`
- Modify: `crates/pandar-hub/src/routes/printer_operations.rs`
- Create: `crates/pandar-hub/src/routes/printer_operations/gcode_line.rs`
- Modify: `crates/pandar-hub/src/routes/tests/plugin/operations.rs`
- Modify: `crates/pandar-hub/src/routes/tests/printers.rs`
- Modify: `crates/pandar-hub/src/grpc/commands/conversion.rs` (minimal exhaustive compile fallout)

**Interfaces:**
- Produces: persisted `PrinterOperationKind::GcodeLine { param }` with action `"gcode_line"`.
- Produces: plugin-only request `{"action":"gcode_line","param":<string>}`.
- Consumes: no gRPC behavior; Task 3 consumes this persisted operation.

- [ ] **Step 1: Add failing repository, plugin-route, tenant-route, and body-limit tests**

Add an exact JSON round trip for `M620 C1 \r\n; keep  \n`. Prove the plugin endpoint returns queued
and persists the exact `param`; the same body on the tenant endpoint returns HTTP 400
`invalid_printer_control`.

Add an audit assertion for the new persisted kind: action is exactly `gcode_line`, while serialized
structured audit metadata contains neither a `param` key nor the raw G-code substring.

Add a table-driven plugin-route rejection test. Missing, null, boolean, number, array, and object
`param`; any extra operation field; and every supplied `required_device_features` value must return
the stable invalid-control error and create no command row. Include `param: ""` as a successful
queued/persisted case so the no-content-policy contract is explicit.

Derive one complete JSON body just below the existing `64 * 1024` router limit and one just above.
The lower request is accepted; the upper returns HTTP 400 `invalid_printer_control` and creates no
command row. Do not add a production `MAX_GCODE_*` constant.

- [ ] **Step 2: Run focused repository/route tests and prove RED**

```powershell
cargo nextest run -p pandar-hub -E 'test(~gcode_line) | test(~plugin_printer_operation)'
```

Expected: compile/test failures because the persisted kind and plugin-only request field are absent.

- [ ] **Step 3: Add the persisted operation and action-only audit mapping**

Add `GcodeLine { param: String }` to `PrinterOperationKind`. Return `"gcode_line"` from `action()`,
return an empty required-printer-feature slice, and accept every string without trimming or content
validation. Audit identifies the action without copying `param` into structured metadata.

- [ ] **Step 4: Add plugin-only HTTP conversion in a focused module**

Add `param: RequestField<String>` to `PrinterOperationRequest`, include `param.is_missing()` in every
existing operation's field-exclusion predicate, and place the `gcode_line` conversion in the new
focused module. `into_plugin_operation` invokes it before delegating all other actions to
`into_tenant_operation`. Tenant conversion has no G-code arm. Missing/null `param`, extra operation
fields, or any required-device-feature field fail.

Because adding `PrinterOperationKind::GcodeLine` makes the existing exhaustive
`proto_printer_operation` match fail to compile, add the final exact mapping arm in
`grpc/commands/conversion.rs` during this task:

```rust
PrinterOperationKind::GcodeLine { param } => {
    printer_operation::Operation::GcodeLine(GcodeLineOperation { param })
}
```

This is compile fallout, not capability delivery: do not add a temporary error arm, Agent gate,
session logic, or conversion tests here. Task 3 owns the conversion tests and mechanically extracts
the already-correct complete mapping into `conversion/operations.rs` before adding its gate.
Because this creates an intermediate ungated dispatch path, the Task 2 working tree is
non-deployable and must not be committed or deployed before Task 3's exact-session capability gate
is complete and reviewed.

- [ ] **Step 5: Run focused Hub HTTP/repository verification**

```powershell
cargo fmt --all -- --check
cargo nextest run -p pandar-hub -E 'test(~gcode_line) | test(~plugin_printer_operation)'
cargo nextest run -p pandar-hub
cargo clippy -p pandar-hub --all-targets -- -D warnings
cargo nextest run -p pandar-core --test module_size
```

If `PANDAR_TEST_POSTGRES_URL` is configured, run the focused PostgreSQL round trip serially. If
unset, explicitly record the skip. The full Hub crate must pass before the Task 2 review package is
created. All touched production modules must stay <=400 LOC.

- [ ] **Step 6: Run the independent Task 2 spec/quality review gate**

Give a fresh reviewer only Task 2's excerpt, diff, and verification. Require literal
`VERDICT: APPROVE`; fix with RED tests and re-review until approved.

- [ ] **Step 7: Record Task 2 evidence without committing**

Append RED→GREEN output, test counts, PostgreSQL result/skip, LOC, and changed paths to the SDD
ledger. Do not stage, commit, or push.

---

### Task 3: Add Hub Protobuf Conversion and Exact-session Agent Capability Gating

**Files:**
- Modify: `crates/pandar-hub/src/grpc/commands.rs`
- Create: `crates/pandar-hub/src/grpc/commands/agent_capabilities.rs`
- Modify: `crates/pandar-hub/src/grpc/commands/device_features.rs`
- Modify: `crates/pandar-hub/src/grpc/commands/conversion.rs`
- Create: `crates/pandar-hub/src/grpc/commands/conversion/operations.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/commands.rs`
- Create: `crates/pandar-hub/src/grpc/tests/commands/gcode_line.rs`

**Interfaces:**
- Consumes: Task 1 protobuf `GcodeLineOperation` and `AgentCapability::GcodeLine`.
- Consumes: Task 2 persisted `PrinterOperationKind::GcodeLine { param }` and its minimal exact mapping
  arm required for Hub compilation.
- Produces: tested/extracted protobuf tag 26 conversion and a capability gate under the transition
  lease.

- [ ] **Step 1: Add protobuf characterization plus failing capability, replacement, and lifecycle tests**

Use deterministic tokens and existing pause points to prove:

- a current capability-4 session receives tag 26 and exact `param`;
- a current incapable session fails the queued row and sends nothing;
- a stale old session returns `SessionEnded` while the row remains exactly `queued`;
- a capable replacement then receives that same still-queued row;
- an already `sent` row is not selected or replayed;
- replacement cannot overtake a dispatch already holding the lease;
- non-G-code and required-printer-feature behavior/error prefixes are unchanged.

- [ ] **Step 2: Run focused gRPC tests and prove RED**

```powershell
cargo nextest run -p pandar-hub -E 'test(~gcode_line) | test(~agent_capability)'
```

Expected: the exact protobuf conversion characterization passes because Task 2 already added the
final mapping arm. Capability-gate and lifecycle tests fail because exact-session Agent capability
gating is absent.

- [ ] **Step 3: Extract and extend Hub-to-protobuf conversion**

Move only `proto_printer_operation` and its small helpers from near-limit `conversion.rs` into
`conversion/operations.rs`. Retain and test Task 2's exact `PrinterOperationKind::GcodeLine` arm
producing `GcodeLineOperation { param }`. The outer required-printer-feature list remains empty.

- [ ] **Step 4: Add the exact-session gate without failing stale work**

Create a helper that returns `None` for non-G-code operations and for every `!current` session. For a
current G-code session, fail only when
`current_token_for_capability(..., AgentCapability::GcodeLine) != Some(token)`. Returning `None` for
stale G-code is mandatory: after both gates, the existing `if !current { SessionEnded }` branch must
leave the row queued for replacement. Do not look up printer `fun`.

Invoke the helper after row decoding, while the existing transition lease is held and before
`mark_sent_and_job`, then run required-printer-feature gating. Refactor the private failure helper to
persist a caller-supplied complete error. Required-feature callers retain
`required device feature gate failed: ...`; G-code uses
`agent capability gate failed: current agent session does not advertise gcode-line capability`.
Closing-session finalization remains unchanged.

- [ ] **Step 5: Run focused and full Hub gRPC verification**

```powershell
cargo fmt --all -- --check
cargo nextest run -p pandar-hub -E 'test(~gcode_line) | test(~agent_capability)'
cargo nextest run -p pandar-hub grpc::tests::lifecycle
cargo nextest run -p pandar-hub
cargo clippy -p pandar-hub --all-targets -- -D warnings
cargo nextest run -p pandar-core --test module_size
```

Expected: stale work remains queued; capable replacement sends; sent work is not replayed; all
touched production modules remain <=400 LOC.

- [ ] **Step 6: Run the independent Task 3 spec/quality review gate**

Give a fresh reviewer only Task 3's excerpt, diff, and verification. Require literal
`VERDICT: APPROVE`; fix with RED tests and re-review.

- [ ] **Step 7: Record Task 3 evidence without committing**

Append RED→GREEN output, focused/full counts, lifecycle evidence, LOC, and changed paths to the SDD
ledger. Do not stage, commit, or push.

---

### Task 4: Add Semantic-first Plugin Fallback and Compiled Cloud/LAN ABI Coverage

**Files:**
- Modify: `crates/pandar-network-plugin/src/gcode/operation.rs`
- Modify: `crates/pandar-network-plugin/src/gcode/studio_json.rs`
- Modify: `crates/pandar-network-plugin/tests/operation_parser.rs`
- Modify: `crates/pandar-network-plugin/tests/http_boundary/printer_operations.rs`
- Modify: `crates/pandar-network-plugin/tests/fixtures/studio_abi_probe.cpp`
- Modify: `crates/pandar-network-plugin/tests/studio_abi_probe.rs`
- Modify: `crates/pandar-network-plugin/tests/studio_abi_probe/mock_hub.rs`
- Modify: `crates/pandar-network-plugin/tests/studio_abi_probe/mock_hub/operations.rs`

**Interfaces:**
- Consumes: Task 2 plugin route body `{"action":"gcode_line","param":String}`.
- Produces: `PrinterOperation::GcodeLine { param }` only for an actual typed Studio wrapper whose
  `gcode_line.param` is not recognized semantically.
- Preserves: existing semantic operation JSON for known Home, XYZ, and temperature input.

- [ ] **Step 1: Add all failing parser, HTTP, and compiled Cloud/LAN ABI tests**

Add table-driven exact cases such as:

```rust
for param in [
    "M106 P1 S127 \n",
    "M620 C1 \r\n; keep trailing  \n\n",
    "",
] {
    let result = operation_json(&studio_gcode_line_message(param));
    assert_operation_json_eq(result, serde_json::json!({
        "action": "gcode_line",
        "param": param,
    }));
}
```

Also prove:

- `G28 X\n`, the exact seven-line axis envelope, and known temperature commands still return their
  existing semantic operation and never `gcode_line`;
- unwrapped `M106 P1 S127` remains unsupported;
- missing, null, boolean, numeric, array, and object `param` remain unsupported;
- the plugin HTTP validator accepts the exact new action and posts it unchanged.

In the same RED change, extend the C++ fixture, Rust probe assertions, and typed mock Hub to require
two new operations before any production fallback exists:

```cpp
send_cloud(agent, "studio-serial-1",
    R"({"print":{"command":"gcode_line","param":"M106 P1 S127 \n","sequence_id":"31009"}})",
    0, 0);

send_printer(agent, "studio-serial-2",
    R"({"print":{"command":"gcode_line","param":"M620 C1 \r\n; keep trailing  \n\n","sequence_id":"31010"}})",
    0, 0);
```

The mock Hub expectations must already assert exact decoded strings and exactly two additional POSTs.
Do not postpone fixture or expectation edits until after production code.

- [ ] **Step 2: Run parser/HTTP and compiled ABI tests and prove RED**

Run:

```powershell
cargo nextest run -p pandar-network-plugin -E 'test(~gcode_line) | test(~operation_parser) | test(~printer_operations)'
cargo nextest run -p pandar-network-plugin -E 'test(studio_abi_probe)'
```

Expected: unknown wrapped G-code still returns `unsupported_printer_operation`; parser/HTTP tests
fail, and the compiled probe proves Cloud creates no operation and/or LAN fails. Record both RED
outputs before editing production code.

- [ ] **Step 3: Implement the typed semantic-first fallback**

Add the enum variant:

```rust
GcodeLine {
    param: String,
},
```

`PrinterOperation::is_valid` returns true for every `GcodeLine` string without trimming or parsing.

Replace the current `gcode_line` branch with a focused method equivalent to:

```rust
fn gcode_line_operation(&self) -> Option<PrinterOperation> {
    let param = self.param.as_string()?;
    Some(super::parse_gcode_operation(param).unwrap_or_else(|| {
        PrinterOperation::GcodeLine {
            param: param.to_owned(),
        }
    }))
}
```

Do not change the outer fallback in `operation_json_from_gcode`; this is what keeps arbitrary
unwrapped strings unsupported. Do not edit `shim.cpp`.

- [ ] **Step 4: Run the unchanged RED test set and make it GREEN**

Rerun both Step 2 commands without changing test expectations. Require both ABI return codes to be
successful, the operation count to increase exactly twice, and exact LF/CRLF/trailing-space/final-
blank-line strings at the mock Hub. Existing modern and legacy semantic ABI assertions stay green.
No policy or parsing enters C++; C++ changes are tests only.

- [ ] **Step 5: Run focused and full Plugin verification**

Run:

```powershell
cargo fmt --all -- --check
cargo nextest run -p pandar-network-plugin -E 'test(~gcode_line) | test(~operation_parser) | test(~printer_operations)'
cargo nextest run -p pandar-network-plugin -E 'test(studio_abi_probe)'
cargo nextest run -p pandar-network-plugin
cargo clippy -p pandar-network-plugin --all-targets -- -D warnings
cargo nextest run -p pandar-core --test module_size
```

Expected: exact parser, HTTP, Cloud ABI, LAN ABI, and full plugin suites pass; `shim.cpp` contains no
new business logic; all production modules are at or below 400 LOC.

- [ ] **Step 6: Run the independent Task 4 spec/quality review gate**

Give a fresh reviewer only Task 4's excerpt, diff, ABI RED→GREEN evidence, and verification output.
Require literal `VERDICT: APPROVE`; fix and re-review until approved.

- [ ] **Step 7: Record the task evidence without committing**

Append RED→GREEN output, compiled ABI evidence, test counts, module LOC, and changed paths to
`.superpowers/sdd/progress.md`. Do not stage, commit, or push.

---

### Task 5: Final Reviews, Documentation, Fresh Verification, Commit, and Push

**Files:**
- Modify after final implementation approval: `docs/compatibility/bambu-studio-plugin.md`
- Modify after final implementation approval: `docs/development.md`
- Modify after final implementation approval: `docs/roadmap.md`
- Modify: `.superpowers/sdd/progress.md` (ignored process ledger)

**Interfaces:**
- Consumes: all implemented tasks and their review reports.
- Produces: reviewed documentation, one verified Conventional Commit, and an exact remote SHA
  readback.

- [ ] **Step 1: Run the mandatory dual implementation review before docs**

Give both an independent Codex reviewer and default-model OpenCode only the approved spec, approved
plan, baseline SHA, full working-tree diff, and verification evidence. Require exactly:

```text
VERDICT: APPROVE | REVISE
SPEC_COVERAGE:
- [implemented requirement or missing requirement]
BLOCKERS:
- [blocking gap or None]
REQUIRED_CHANGES:
- [change or None]
```

If either reviewer returns anything other than literal `VERDICT: APPROVE`, fix with failing tests,
rerun relevant verification, and rerun both reviewers from scratch.

- [ ] **Step 2: Update docs only after both implementation reviewers approve**

Document:

- semantic-first recognized G-code behavior;
- exact-string fallback for other Studio `gcode_line.param` values;
- plugin-only authorization and Agent capability 4;
- existing 64 KiB request boundary and stable HTTP 400 mapping;
- queued→sent/no-Hub-requeue lifecycle;
- rollback warning for queued new-operation payloads;
- no claim of live-printer movement testing.

- [ ] **Step 3: Re-run both final reviewers on the complete implementation-and-docs diff**

After documentation changes, give both reviewers the complete diff from baseline, not a docs-only
slice. Require the same exact final implementation verdict format and literal `VERDICT: APPROVE`
from both. Any change made after this gate requires both reviewers to run again before commit.

- [ ] **Step 4: Run fresh completion verification**

Run from the repository root after docs are complete:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run -p pandar-core --test module_size
cargo nextest run --manifest-path 'Cargo.toml' --workspace
cargo nextest run -p pandar-network-plugin -E 'test(studio_abi_probe)'
git diff --check
```

If `PANDAR_TEST_POSTGRES_URL` is set, run the focused real PostgreSQL G-code persistence test. If it
is unset, report the skip explicitly.

Audit production Rust LOC and verify `shim.cpp` remains thin. Do not run a live printer movement or
Homing command.

- [ ] **Step 5: Audit the final scope and stage explicit paths**

Run:

```powershell
git status --short
git diff --stat
git diff --name-only
git diff --check
```

Stage only spec, plan, implementation, tests, and docs. Explicitly exclude every pre-existing
`crates/pandar-network-plugin/probe-*` path. Verify `git diff --cached --name-only` contains no probe
directory and `git diff --cached --check` passes.

- [ ] **Step 6: Create one Conventional Commit and push `main`**

Use exactly:

```powershell
git commit -m "feat(studio): pass through gcode line commands"
git push origin main
```

Do not force-push.

- [ ] **Step 7: Verify remote readback and close the ledger**

Run:

```powershell
$head = (git rev-parse HEAD).Trim()
$tracking = (git rev-parse origin/main).Trim()
$remote = ((git ls-remote origin refs/heads/main).Trim() -split '\s+')[0]
if ($head -ne $tracking -or $head -ne $remote) { throw 'remote readback mismatch' }
git status --short
```

Expected: all three SHAs match. Only the pre-existing untracked `probe-*` directories remain. Record
the full commit SHA, push, and readback in `.superpowers/sdd/progress.md`.
