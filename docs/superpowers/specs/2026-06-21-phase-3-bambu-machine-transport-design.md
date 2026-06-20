# Phase 3 Bambu Machine Transport Design

## Goal

Phase 3 introduces the agent-side Bambu machine transport in reviewable milestones. It builds the MQTT command/report boundary and the machine file-transfer boundary from the local reference projects, then wires those boundaries into the agent command executor without requiring a real printer in tests.

## Scope

This phase is split into four milestones:

1. **MQTT model and payload builders**: typed printer endpoint, topics, QoS policy, and JSON command builders.
2. **MQTT client adapter**: a runtime abstraction that can publish commands and subscribe to reports, with a test double for command execution.
3. **Machine file-transfer model and adapter boundary**: typed FTPS-derived configuration, mode selection, path operations, and a test double.
4. **Agent command integration and docs**: route Phase 2 hub commands through the new transport boundary where possible, update docs and roadmap, and keep tests hermetic.

Each milestone must leave the workspace compiling and tested. Real printer integration is optional only behind explicit configuration and must not run during tests.

## Agent Printer Configuration

Phase 3 introduces an explicit agent-local printer configuration string:

```bash
PANDAR_PRINTERS='[{"host":"192.0.2.10","serial":"01S00EXAMPLE","access_code":"12345678","model":"A1 Mini","name":"garage-a1"}]'
```

The JSON value is an array. Each entry maps to:

- `host`: LAN address or DNS name used by MQTT and file transfer.
- `serial`: Bambu printer serial used to derive MQTT topics.
- `access_code`: Bambu LAN access code used as MQTT/FTPS password.
- `model`: optional model string used for FTPS fallback policy.
- `name`: optional local display name used in snapshots.

An empty or absent `PANDAR_PRINTERS` value means no configured printers. That mode must not open machine network sockets and `RefreshPrinters` should ack and succeed without emitting snapshots. Invalid JSON or missing `host`, `serial`, or `access_code` is a process-boundary configuration error and must preserve parse/validation context.

## Reference-Derived Requirements

From `reference/bambuddy/backend/app/services/bambu_mqtt.py`:

- MQTT report topic is `device/{serial}/report`.
- MQTT request topic is `device/{serial}/request`.
- MQTT publishes must use QoS `1`.
- Printer auth uses username `bblp` and access code as password.
- The initial full-state request payload is `{"pushing":{"command":"pushall"}}`.
- Basic print controls are JSON under `print.command`:
  - pause: `{"print":{"command":"pause","sequence_id":"0"}}`
  - resume: `{"print":{"command":"resume","sequence_id":"0"}}`
  - stop: `{"print":{"command":"stop","sequence_id":"0"}}`
  - print speed: `{"print":{"command":"print_speed","param":"<1..4>","sequence_id":"0"}}`
- Print dispatch uses `print.command = "project_file"`, `url = "ftp://{filename}"`, `file`, `param = "Metadata/plate_{plate_id}.gcode"`, calibration flags, and per-submission identity fields. Full job dispatch may remain a later phase, but the builder must reserve a type-safe shape for it.

From `reference/bambuddy/backend/app/services/bambu_ftp.py`:

- Machine file transfer uses implicit FTPS on port `990`.
- Login uses username `bblp` and access code as password.
- Default data mode is protected data (`PROT P`).
- A1/A1 Mini may need clear data mode (`PROT C`) fallback while keeping the control channel encrypted.
- Upload should be represented as manual `STOR` chunk transfer with a default chunk size of 64 KiB.
- File operations are list, download, upload, and delete.
- Working transfer mode should be cacheable per printer endpoint.

## Architecture

### Agent Machine Module

Create an agent-owned module tree under `crates/pandar-agent/src/machine/`:

- `mod.rs`: public exports and shared endpoint/credential types.
- `mqtt.rs`: topic functions, QoS constant, command enum, payload builders, and transport trait.
- `file_transfer.rs`: transfer configuration, mode enum, file operation trait, and mode-cache type.

This keeps printer protocol concerns out of `pandar-core` until a stable cross-crate model is needed. The hub continues to see normalized agent events and hub commands over gRPC.

### MQTT Boundary

The MQTT boundary must expose:

- `BambuPrinterEndpoint { host, serial, access_code, model }`.
- `BambuMqttTopics { report, request }` derived from serial.
- `BAMBU_MQTT_PORT = 8883`.
- `BAMBU_MQTT_USERNAME = "bblp"`.
- `BAMBU_MQTT_QOS = 1`.
- `BambuMqttCommand` variants for `RequestPushAll`, `PausePrint`, `ResumePrint`, `StopPrint`, `SetPrintSpeed(PrintSpeed)`, `RawJson(serde_json::Value)`, and a reserved `ProjectFile(ProjectFileCommand)`.
- `BambuMqttTransport` trait with publish semantics that accept topic, payload bytes/string, and QoS.
- Report-side methods on the same trait or a paired trait:
  - subscribe to `device/{serial}/report`;
  - receive one decoded `serde_json::Value` report with a caller-provided timeout;
  - preserve timeout and decode errors as distinct contexts.

Milestone 1 may implement only pure builders and tests. Milestone 2 may add a concrete runtime adapter after dependency review.

### File-Transfer Boundary

The file-transfer boundary must expose:

- `BAMBU_FILE_TRANSFER_PORT = 990`.
- `BAMBU_FILE_TRANSFER_USERNAME = "bblp"`.
- `BAMBU_FILE_TRANSFER_CHUNK_SIZE = 64 * 1024`.
- `TransferProtectionMode::{ProtectedData, ClearData}`.
- A1/A1 Mini model detection for initial fallback policy.
- A per-printer mode cache keyed by host or stable endpoint key.
- `MachineFileTransfer` trait with list, download, upload, and delete operations.

The first milestone for this boundary may be pure configuration and behavior tests. A concrete FTPS runtime adapter can be implemented only after selecting a Rust crate that supports implicit FTPS and the needed data-channel behavior.

### Agent Integration

Phase 2 currently handles `RefreshPrinters` by immediately acking and succeeding. Phase 3 should route refresh through a `BambuMachineGateway` implementation when configured. If no configured printers exist, refresh must still ack and succeed, but it must not claim an "empty result" in `CommandResult`; the current proto has no result payload. Empty refresh means no `PrinterSnapshot` events are emitted.

The gateway boundary must be async and small:

```rust
pub struct MachineSnapshot {
    pub serial: String,
    pub name: String,
    pub state: String,
}

#[async_trait]
pub trait BambuMachineGateway {
    async fn refresh_printers(&self) -> anyhow::Result<Vec<MachineSnapshot>>;
}
```

`RefreshPrinters` event sequencing must be:

1. Send accepted `CommandAck`.
2. Call `BambuMachineGateway::refresh_printers`.
3. For each returned `MachineSnapshot`, send a `PrinterSnapshot` agent event using `tenant_id` and `agent_id` from `AgentConfig`; `HubCommand` currently carries only `command_id`.
4. Send successful `CommandResult` if refresh succeeded, or failed `CommandResult` with preserved error context if refresh failed.

Acceptance tests must cover no configured printers, one snapshot emission, multiple snapshot emission, and failed refresh producing an error result after the accepted ack.

For configured printers, `refresh_printers` must:

1. Subscribe to the report topic for each configured printer.
2. Publish `RequestPushAll` to the request topic with QoS `1`.
3. Wait for one report per printer using a bounded timeout supplied by the gateway configuration.
4. Convert each report into `MachineSnapshot` using:
   - `serial` from configuration, not from untrusted payload;
   - `name` from printer config when present, otherwise serial;
   - `state` from the first string found at `print.gcode_state`, `print.state`, or `state`, otherwise `"unknown"`.
5. Return an error if any configured printer times out or returns invalid JSON. The error must identify the printer serial and preserve the transport/decode cause chain.

No Phase 3 test may open real MQTT or FTPS sockets. Runtime adapters must be tested through fakes or local mock transports.

## Milestone Acceptance Criteria

### M1: MQTT Model And Payload Builders

- Topic builders produce `device/{serial}/report` and `device/{serial}/request`.
- MQTT constants match port `8883`, username `bblp`, and QoS `1`.
- `RequestPushAll`, pause, resume, stop, and speed payloads match the reference JSON shape.
- `SetPrintSpeed` accepts only modes 1 through 4 through a typed enum or constructor.
- Raw JSON command preserves caller-provided JSON and still publishes with QoS `1`.
- Tests compare `serde_json::Value`, not ad hoc string fragments.

### M2: MQTT Runtime Adapter Boundary

- A `BambuMqttTransport` trait exists and can be used without a concrete printer.
- A test double records publish topic, payload, and QoS.
- The test double can return decoded report payloads or timeout errors for refresh tests.
- Command executor publishes to the request topic with QoS `1`, subscribes to the report topic, and maps report JSON into `MachineSnapshot`.
- Any concrete MQTT dependency is justified in the plan before adoption and isolated to the agent crate.
- Errors preserve lower-level context with `anyhow::Context` and `{err:#}` style logging when logged.

### M3: Machine File-Transfer Boundary

- FTPS-derived constants and auth defaults match the reference.
- A1/A1 Mini model detection selects `ClearData` fallback only after protected mode fails or when explicitly forced.
- Protected-mode failure context is preserved when fallback is attempted. If fallback also fails, the returned error must include both the protected-mode failure and clear-data failure contexts.
- Mode cache stores and returns the working mode per endpoint.
- Mode cache is updated only after a mode succeeds; failed protected or clear attempts must not poison the cache.
- Upload behavior exposes a 64 KiB chunk-size constant and a `STOR`-shaped operation through the trait boundary.
- Tests cover protected-first behavior, A1/A1 Mini fallback, forced clear mode, successful-mode caching, failed-mode non-caching, and operation request shapes without network sockets.

### M4: Agent Integration, Docs, And Roadmap

- `BambuMachineGateway` is moved from the existing label-only type to a small async boundary that can refresh printer state through MQTT transport abstractions.
- Phase 2 `RefreshPrinters` handling delegates to the gateway boundary and returns ack/result based on gateway outcome.
- The default local-development gateway remains non-networked unless printer configuration is explicitly supplied.
- `AgentConfig` exposes `PANDAR_PRINTERS`; README documents the JSON schema.
- Invalid printer configuration fails at startup with source context rather than being ignored.
- `README.md`, `docs/architecture.md`, and `docs/roadmap.md` document Phase 3 milestones, MQTT/FTPS defaults, and the no-real-network test policy.
- Full workspace verification passes.

## Out Of Scope

- Real printer discovery.
- Credential storage in hub.
- Frontend UI.
- Full print dispatch orchestration across upload plus MQTT `project_file`.
- Live Bambu compatibility matrix.
- Docker deployment changes.
- Tests that require a physical printer or real LAN credentials.

## Safety And Rollback

- The implementation must be additive and agent-scoped unless a shared command type is required.
- No code may publish to a real printer during unit tests.
- Real-network runtime paths must require explicit host, serial, and access code configuration.
- FTPS fallback must preserve original protected-mode failure context, report fallback failure context, and cache only successful modes.
- If concrete MQTT or FTPS crate integration proves risky, keep the milestone at trait plus mock adapter and document the dependency decision for the next milestone.

## Verification

Required targeted verification:

- `cargo test -p pandar-agent machine`
- `cargo clippy -p pandar-agent --all-targets -- -D warnings`
- `cargo fmt --check -p pandar-agent`

Required final verification:

- `cargo fmt`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
- `git diff --check`

## Documentation Impact

- `README.md` must describe any new agent configuration keys introduced by Phase 3.
- `docs/architecture.md` must describe the agent-side MQTT and file-transfer boundaries.
- `docs/roadmap.md` must mark completed Phase 3 milestones and move Immediate Next to the next incomplete milestone.
