# Phase 13 Spec: LAN Discovery, Printer Diagnostics, and Compatibility Gates

## Scope

Phase 13 makes real Bambu printer operation debuggable before a print is dispatched. The implementation must let an operator:

1. Ask a linked agent to discover local Bambu printers over LAN SSDP.
2. Ask a linked agent to diagnose one of its locally configured printers.
3. See structured diagnostic results in the hub/frontend without storing Bambu access codes in the hub.
4. Rely on one centralized compatibility matrix for model-specific feature availability and FTPS runtime policy.

This phase is intentionally not a printer-pairing flow. The hub still does not persist Bambu access codes, and diagnostics for authenticated checks only run against printers already configured on the agent.

## Reference Facts

The implementation should use these behavior facts from `reference/`:

- `reference/bambuddy/backend/app/services/discovery.py` sends SSDP M-SEARCH to multicast `239.255.255.250:2021` with search target `urn:bambulab-com:device:3dprinter:1`.
- Bambuddy discovery parses printer serial from `USN`, printer name from `DevName.bambu.com`, and model from `DevModel.bambu.com`, with fallback model parsing from `NT`.
- Bambu printer MQTT uses TLS on port `8883`, username `bblp`, the LAN access code as password, and report/request topics under `device/{serial}/...`.
- Bambu printer FTPS uses implicit FTPS on port `990`, username `bblp`, the LAN access code as password, and model-specific TLS/profile behavior.
- Bambuddy diagnostic checks separate reachability, MQTT auth/report publishing, FTPS availability, external storage, and developer/LAN-mode evidence.
- BambuStudio reference behavior indicates storage must be present for LAN printing, and device capability flags exist for calibration and other feature gates.

## Non-Goals

- Do not scan arbitrary public networks or perform cloud discovery.
- Do not add a user-facing printer credential storage feature in the hub.
- Do not persist Bambu LAN access codes in command payloads, database rows, logs, frontend state, or environment variables.
- Do not copy unrelated code from reference projects.
- Do not implement virtual printer/proxy behavior from Bambuddy.
- Do not silently fall back to unsupported or uncertain model features.

## Design

### Hub Command Model

The hub remains the tenant-authoritative command dispatcher. Phase 13 adds two command kinds:

- `discover_printers`: agent-local SSDP discovery.
- `diagnose_printer`: agent-local diagnostics for a configured printer.

The existing command table gains a nullable `result_json` column for structured command output. This column is backend-neutral and must be added through both SQLite and PostgreSQL migrations and the SeaORM entity/model mapping.

`CommandRecord` and API command responses include `result_json: Option<String>`. Existing commands leave it `None`.

The gRPC `CommandResult` message gains `result_json`. Agent command handlers set it only for commands with structured output. The hub persists it when a command succeeds or fails so diagnostics can report partial failures with machine-readable checks.

`result_json` must never contain Bambu access codes. Diagnostic details should include host, port, serial number, model, command/check id, and lower-level error context, but the agent must redact the configured access code from any error string before serializing command results or logging diagnostic failures.

Command success semantics:

- Discovery commands use `success = true` when SSDP discovery completes, even if zero printers are found.
- Diagnostic commands use `success = true` when the diagnostic run completes and returns structured checks, even when `overall = "problem"`.
- Diagnostic environment failures such as wrong credentials, no MQTT report, no FTPS listener, missing SD card, full SD card, or upload verification failure are represented in `result_json` checks, not as failed hub commands.
- `success = false` is reserved for command execution failures where the agent could not produce the requested command result, such as invalid command payloads, serialization failures, or unexpected internal errors. In those cases `error` carries the redacted error context and `result_json` may be absent.

Persistence path:

- `entities::commands::Model`, `ActiveModel`, `CommandRecordParts`, and `CommandRecord` gain `result_json`.
- `repositories::commands::rows::command_from_model` maps `result_json`.
- `repositories::commands::transitions::update_status_if_current` gains a `result_json: Option<String>` parameter and sets the column on terminal transitions.
- command transition wrappers expose result-aware APIs, for example `mark_succeeded_with_result` and `mark_failed_with_result`, or equivalent extensions that thread `result_json` into the shared transition function.
- `grpc::commands::handle_result_and_job` passes `CommandResult.result_json` into the repository transition used for succeeded or failed command results.

### Hub HTTP API

Add tenant-scoped routes:

- `POST /api/v1/tenants/{tenant_id}/agents/{agent_id}/discover-printers`
- `POST /api/v1/tenants/{tenant_id}/agents/{agent_id}/diagnose-printer`
- `GET /api/v1/tenants/{tenant_id}/commands/{command_id}`

Authorization:

- Discovery and diagnosis require `Operator`.
- Command detail requires `Viewer` and must enforce tenant ownership.

The command repository gains a tenant-scoped lookup, for example `get_for_tenant(tenant_id, command_id)`, and the command detail HTTP route must use it instead of an unscoped `get(id)`.

Request bodies:

```json
{
  "timeout_seconds": 5
}
```

for discovery. Defaults and bounds:

- default: `5`
- minimum: `1`
- maximum: `15`
- omitted value uses the default
- values outside the inclusive range return HTTP `400`

```json
{
  "serial_number": "..."
}
```

for diagnostics. The hub must not accept or persist access codes here.

Audit actions:

- `agent.discover_printers`
- `agent.diagnose_printer`

### Agent Protocol

Extend `proto/pandar/agent/v1/agent.proto`:

- `HubCommand` adds `DiscoverPrinters` and `DiagnosePrinter`.
- `CommandResult` adds `string result_json = 4`.
- `DiscoverPrinters` carries `timeout_seconds`.
- `DiagnosePrinter` carries `serial_number`.
- `HubCommand.command` adds `DiscoverPrinters discover_printers = 12` and `DiagnosePrinter diagnose_printer = 13`.

Generated protobuf artifacts remain untracked; build scripts must regenerate them before compilation.

### Discovery Result Schema

The agent returns this JSON shape in `CommandResult.result_json` for `discover_printers`:

```json
{
  "type": "printer_discovery",
  "printers": [
    {
      "serial_number": "...",
      "host": "192.168.1.50",
      "name": "Office X1C",
      "model": "X1 Carbon",
      "source": "ssdp"
    }
  ]
}
```

Duplicates are de-duplicated by serial number when present, otherwise by host. Unknown fields should be omitted rather than filled with placeholder text.

`host` is the UDP source socket address of the SSDP response. The implementation may parse `LOCATION` for future metadata, but must not prefer it over the packet source for the returned host in this phase.

### Diagnostic Result Schema

The agent returns this JSON shape in `CommandResult.result_json` for `diagnose_printer`:

```json
{
  "type": "printer_diagnostic",
  "serial_number": "...",
  "host": "192.168.1.50",
  "model": "A1 Mini",
  "overall": "ok|warning|problem",
  "checks": [
    {
      "id": "mqtt_port",
      "status": "ok|warning|problem|skipped",
      "message": "...",
      "details": "..."
    }
  ],
  "compatibility": {
    "normalized_model": "A1_MINI",
    "external_storage": "unsupported",
    "ftps_tls_1_2_cap": false,
    "ftps_clear_data_fallback": true,
    "features": {
      "chamber_temperature": "supported|unsupported|unknown",
      "drying": "supported|unsupported|unknown",
      "dual_nozzle": "unsupported",
      "flow_calibration": "supported|unsupported|unknown",
      "vibration_calibration": "supported|unsupported|unknown",
      "nozzle_offset_calibration": "supported|unsupported|unknown"
    }
  }
}
```

`host`, `model`, and `compatibility` are omitted when the printer is not present in the agent configuration. Configured printers include these fields when the agent has them locally.

Overall status is derived from checks:

- Any `problem` check makes the result `problem`.
- Otherwise any `warning` check makes the result `warning`.
- Otherwise the result is `ok`.
- `skipped` checks do not affect overall status.

### Agent Diagnostics

Diagnostics run only for a serial number present in the agent's configured printer list. If the serial number is absent, return a structured diagnostic with `overall = "problem"` and a `configured_printer` problem check.

For configured printers, diagnostics include these checks:

- `configured_printer`: serial is present in agent config.
- `mqtt_port`: TCP reachability to port `8883`.
- `mqtt_report`: authenticated MQTT subscribe/request/report round trip using the existing access code and report timeout. Wrong access code, wrong serial, stale session behavior, and no MQTT report must be reported with preserved error context.
- `ftps_port`: TCP reachability to port `990`.
- `storage_writable`: non-destructive FTPS upload/delete probe using a small temporary file and the same transfer-mode policy as print dispatch. Missing SD card, full SD card, and upload verification failures must surface as actionable problem messages with lower-level context preserved.
- `compatibility`: model capability and FTPS policy from the centralized compatibility matrix.

Dependent-check rules:

- If `configured_printer` is `problem`, no network checks run. Return only `configured_printer` plus omitted `host`, `model`, and `compatibility`.
- If `mqtt_port` is `problem`, `mqtt_report` is `skipped` with a message that MQTT port reachability failed first.
- If `ftps_port` is `problem`, `storage_writable` is `skipped` with a message that FTPS port reachability failed first.
- If compatibility resolves `external_storage = unsupported`, `storage_writable` is `skipped` with a message that the model does not use removable printer storage for LAN dispatch diagnostics.
- If compatibility resolves `external_storage = supported` or `unknown`, `storage_writable` runs when `ftps_port` is reachable. This lets diagnostics surface missing SD card, full SD card, and upload verification failures for models where external storage may be required.
- If the model is absent in agent config, compatibility still appears with `normalized_model = null` and every model feature set to `unknown`.
- A diagnostic command may fail at the command level only when the command itself cannot be executed or serialized. Expected printer/environment problems should produce a successful command with `overall = "problem"` and structured checks.

The storage probe writes a small temporary file to `Metadata/pandar-diagnostic.tmp` through the existing FTPS upload path and must best-effort delete it. Delete failure should be reported as a warning, not hidden.

### Redaction Contract

Credential redaction is required at every boundary introduced or touched by this phase:

- `diagnose_printer` command payload contains only `serial_number`.
- `result_json` never contains the configured Bambu access code.
- diagnostic `message` and `details` never contain the configured Bambu access code, including error chains from MQTT and FTPS libraries.
- audit event details never contain Bambu access codes.
- frontend state and rendered HTML never contain Bambu access codes.
- diagnostic logs may include lower-level context but must pass through the same access-code redaction before logging.

Tests must inject a distinctive fake access code and assert that it is absent from serialized command payloads, command results, audit rows, and rendered frontend/server-action output touched by diagnostics.

### Central Compatibility Matrix

Add `crates/pandar-agent/src/machine/compatibility.rs` as the single source for model compatibility decisions.

The matrix must provide:

- model alias normalization, including `N7 -> P2S` and `N6 -> X2D`;
- FTPS TLS 1.2 cap behavior currently duplicated in `ftps.rs`;
- FTPS clear-data fallback policy currently implied by A1/A1 Mini handling in `file_transfer.rs`;
- external storage support as a tri-state capability;
- feature tri-states for chamber temperature, drying, dual nozzle, flow calibration, vibration calibration, and nozzle-offset calibration.

Unknown or uncertain model features default to `unknown`, and UI availability must treat `unknown` the same as unavailable. The implementation should only mark a feature `supported` when that behavior is backed by the reference projects or existing live-capture documentation.

Initial matrix:

| Model key | Aliases                     | Evidence                                                                               | FTPS TLS 1.2 cap | Clear-data fallback | External storage | Chamber temp | Drying  | Dual nozzle | Flow calibration | Vibration calibration | Nozzle-offset calibration |
| --------- | --------------------------- | -------------------------------------------------------------------------------------- | ---------------- | ------------------- | ---------------- | ------------ | ------- | ----------- | ---------------- | --------------------- | ------------------------- |
| `A1`      | none                        | Existing Pandar transfer policy and Bambuddy storage diagnostic skip A1-class machines | false            | true                | unsupported      | unknown      | unknown | unsupported | unknown          | unknown               | unknown                   |
| `A1_MINI` | `A1 mini`, `A1 Mini`, `A1M` | Existing Pandar transfer policy and Bambuddy storage diagnostic skip A1-class machines | false            | true                | unsupported      | unknown      | unknown | unsupported | unknown          | unknown               | unknown                   |
| `P2S`     | `N7`                        | Existing Pandar FTPS profile aliases from Bambuddy `ftp_profiles.py`                   | true             | false               | unknown          | unknown      | unknown | unknown     | unknown          | unknown               | unknown                   |
| `X2D`     | `N6`                        | Existing Pandar FTPS profile aliases from Bambuddy `ftp_profiles.py`                   | true             | false               | unknown          | unknown      | unknown | unknown     | unknown          | unknown               | unknown                   |
| `UNKNOWN` | any unmatched/empty model   | No reference-backed model behavior                                                     | false            | false               | unknown          | unknown      | unknown | unknown     | unknown          | unknown               | unknown                   |

This initial table is deliberately conservative. Additional model support requires either a reference-backed row in this table or future live-capture documentation.

Existing FTPS profile and transfer-mode selection must call this module instead of keeping separate model checks.

MQTT print command building must reference this module for calibration capability decisions and must not silently enable model-specific options that are known unsupported or unknown for the configured printer model.

If a print request sets `flow_cali = true` and the configured printer model resolves `flow_calibration` to `unsupported` or `unknown`, the agent rejects the `PrintProjectFile` command before uploading the artifact and returns a redacted command error. The hub enqueue path does not reject this option because the hub does not own the agent-local model compatibility context.

### Frontend

The dashboard adds an operator diagnostics surface:

- show agents for the selected tenant;
- trigger discovery for an agent;
- trigger diagnostics for a configured printer;
- fetch and display the selected command result through the command detail route;
- show compatibility feature gates as unavailable when the result is `unsupported` or `unknown`;
- do not expose access-code inputs or persist Bambu access codes in browser state.

This phase can use refresh-after-command behavior. Live WebSocket updates remain future work.

### Documentation Impact

Update:

- `docs/roadmap.md` with Phase 13 completed work and the next Phase 14/15 status.
- `docs/architecture.md` with the command-result boundary, agent-local credential rule, diagnostics lifecycle, and compatibility matrix ownership.
- Operator-facing documentation in `README.md` or `PRODUCT.md` only if the implementation exposes new commands or environment behavior that operators need to run locally.

### Tests and Verification

Required tests:

- SSDP response parser extracts serial, host, name, and model from representative Bambuddy-style responses.
- Discovery de-duplicates responses by serial/host.
- Diagnostic aggregation derives `ok`, `warning`, and `problem`.
- Wrong/unconfigured serial returns a `configured_printer` problem.
- Fake MQTT/FTPS diagnostics cover wrong access/no report, no FTPS listener, missing/full storage or upload verification failure.
- Redaction tests prove a distinctive fake access code is absent from diagnostic payload/result/audit/frontend surfaces.
- Compatibility matrix covers aliases, FTPS TLS cap, A1/A1 Mini clear-data fallback, unknown feature defaults, and feature JSON serialization.
- Existing FTPS profile and transfer-mode tests prove they use the centralized matrix.
- Hub route tests cover authorization, tenant scoping, audit events, command enqueue, and command detail result persistence.
- gRPC command/result tests cover new command variants and `result_json` persistence.
- Command transition tests prove successful diagnostics with `overall = "problem"` persist as `CommandStatus::Succeeded` with `result_json`, while unexpected agent command failures persist as failed commands with redacted errors.
- Print dispatch tests prove `flow_cali = true` is rejected before upload for unsupported/unknown flow-calibration capability.
- Frontend build validates the diagnostics UI wiring.

Fresh final verification must include:

- `cargo fmt`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
- frontend build when frontend source changes

## Acceptance Criteria

- Operators can enqueue LAN discovery through a tenant-linked agent and inspect structured discovered printer output.
- Operators can enqueue diagnostics for an agent-configured printer and inspect actionable checks before dispatching a print.
- Wrong serial, wrong access/no MQTT report, no FTPS listener, missing SD card, full SD card, and upload verification failure are represented as explicit diagnostic checks.
- Bambu access codes remain agent-local and are not persisted by hub commands or frontend state.
- Compatibility rules are centralized and referenced by FTPS profile selection, FTPS transfer policy, MQTT print option validation, diagnostics, and frontend availability.
- SQLite and PostgreSQL schema/mapping stay in parity.
- No generated protobuf output is committed.

## Rejected Alternatives

- Hub-side LAN discovery: rejected because the hub is not on the operator LAN and does not own Bambu credentials.
- Persisting discovered printers as configured printers: rejected because discovery does not prove credentials or tenant intent.
- Passing transient access codes through hub diagnostic commands: rejected because command payloads are persisted and would violate the agent-local credential boundary.
- Adding a broad subnet scanner in this phase: rejected because SSDP discovery is the requested reference-backed behavior and subnet probing is a separate operational policy decision.
