# Phase 13 Implementation Plan: LAN Discovery, Diagnostics, Compatibility

Spec: `docs/superpowers/specs/2026-06-23-phase-13-discovery-diagnostics-compatibility-design.md`

## Execution Rules

- Stay on `main`; do not create a branch.
- Use fresh implementation subagents per milestone.
- After each milestone, run a spec-compliance review and code-quality review before moving on.
- Do not commit inside milestone subagents; the leader owns final verification, docs, commit, and push.
- Preserve lower-level error context while redacting Bambu access codes from command results, logs, audit metadata, and frontend-rendered data.
- Keep generated protobuf output untracked; compile must regenerate protobuf into `target`.

## Milestone 1: Protocol, Core Command Model, and Hub Persistence

Owner write scope:

- `proto/pandar/agent/v1/agent.proto`
- `crates/pandar-core/src/command.rs`
- `crates/pandar-core/src/tests.rs`
- `crates/pandar-hub/src/entities/commands.rs`
- `crates/pandar-hub/migrations/sqlite/*phase_13*.sql`
- `crates/pandar-hub/migrations/postgres/*phase_13*.sql`
- `crates/pandar-hub/src/repositories/commands.rs`
- `crates/pandar-hub/src/repositories/commands/*.rs`
- `crates/pandar-hub/src/grpc/commands.rs`
- focused tests under `crates/pandar-hub/src/repositories/tests` and `crates/pandar-hub/src/grpc/tests`

Tasks:

1. Add protobuf fields:
   - `CommandResult.result_json = 4`
   - `HubCommand.command.discover_printers = 12`
   - `HubCommand.command.diagnose_printer = 13`
   - new `DiscoverPrinters { uint32 timeout_seconds = 1; }`
   - new `DiagnosePrinter { string serial_number = 1; }`
2. Add nullable `result_json` to both SQLite and PostgreSQL `commands` schema through Phase 13 migrations.
3. Thread `result_json` through SeaORM command entity, `CommandRecordParts`, `CommandRecord`, and row rehydration.
4. Extend command terminal transitions so `result_json` can be persisted on success and failure while existing callers can continue to set `None`.
5. Update `grpc::commands::handle_result_and_job` to persist `CommandResult.result_json` for non-print commands; preserve print-job status behavior.
6. Add command payload types and hub-command mapping for `discover_printers` and `diagnose_printer`.
7. Add `CommandRepository::get_for_tenant`.

Tests:

- core command record construction includes `result_json`.
- SQLite repository tests under `crates/pandar-hub/src/repositories/tests/**` assert `result_json` is persisted on succeeded diagnostics, failed unexpected command results, and tenant-scoped command detail lookup.
- PostgreSQL repository tests extend `crates/pandar-hub/src/repositories/tests/postgres.rs` command coverage when `PANDAR_TEST_POSTGRES_URL` is configured, using the existing optional Postgres harness, and assert the same `result_json` terminal-transition and command-detail behavior as SQLite.
- gRPC tests prove new command variants map to protobuf commands and `result_json` is persisted.
- migration assertions or test setup must exercise both `crates/pandar-hub/migrations/sqlite/*phase_13*.sql` and `crates/pandar-hub/migrations/postgres/*phase_13*.sql`; do not rely on code review alone for PostgreSQL parity.

Review gate:

- Spec reviewer verifies all protocol/persistence acceptance criteria in the spec are implemented.
- Code-quality reviewer checks schema parity, transition idempotence, and generated protobuf output remains untracked.

## Milestone 2: Agent Compatibility Matrix, Discovery, Diagnostics, and Print Gate

Owner write scope:

- `crates/pandar-agent/src/machine/mod.rs`
- `crates/pandar-agent/src/machine/compatibility.rs`
- `crates/pandar-agent/src/machine/discovery.rs`
- `crates/pandar-agent/src/machine/diagnostics.rs`
- `crates/pandar-agent/src/machine/file_transfer.rs`
- `crates/pandar-agent/src/machine/ftps.rs`
- `crates/pandar-agent/src/machine/mqtt.rs`
- `crates/pandar-agent/src/commands.rs`
- focused tests under `crates/pandar-agent/src/machine/**` and `crates/pandar-agent/src/commands/**`

Tasks:

1. Add `compatibility.rs` with:
   - model normalization and aliases `N7 -> P2S`, `N6 -> X2D`, A1 Mini aliases;
   - tri-state capabilities;
   - the initial matrix from the spec;
   - serializable diagnostic compatibility output.
2. Refactor FTPS profile and transfer-mode ordering to use compatibility instead of local model checks.
3. Add agent-side SSDP discovery:
   - parser for Bambuddy-style responses;
   - de-duplication by serial or host;
   - UDP source socket address as returned `host`;
   - timeout input from command, bounded by hub but still safe when called internally.
4. Add diagnostics:
   - structured checks and aggregate status;
   - `configured_printer`, `mqtt_port`, `mqtt_report`, `ftps_port`, `storage_writable`, `compatibility`;
   - dependent skip rules from the spec;
   - redaction helper for access-code-sensitive error strings;
   - storage probe at `Metadata/pandar-diagnostic.tmp` with best-effort delete warning.
5. Extend `BambuMachineGateway` with discovery and diagnostics operations.
6. Ensure the runtime gateway still enables discovery even when no printers are configured, while authenticated diagnostics require a configured serial.
7. Handle new hub commands in `commands.rs`:
   - accept command;
   - return `success = true` with structured `result_json` for completed discovery/diagnostics;
   - return `success = false` only for unexpected execution/serialization failures;
   - redact all access-code occurrences before sending failure events.
8. Reject `PrintProjectFile` before upload when `flow_cali = true` and compatibility says flow calibration is `unsupported` or `unknown`.

Tests:

- SSDP parser extracts serial/name/model/host and ignores unrelated packets.
- discovery de-duplicates by serial and host.
- compatibility aliases, FTPS TLS cap, A1/A1 Mini clear-data fallback, unsupported external storage, unknown defaults, and JSON shape.
- FTPS profile and transfer-mode tests prove central compatibility use.
- diagnostic aggregation and skip rules, including unsupported external storage.
- diagnostic output for configured printers without a model sets `compatibility.normalized_model = null` and every feature to `unknown`.
- fake MQTT/FTPS diagnostics cover wrong access/no report, no FTPS listener, missing/full storage, and upload verification failure.
- redaction test with a distinctive fake access code across diagnostic result, command failure event, serialized command payload, persisted command result, and any diagnostic log string produced by this milestone.
- print dispatch rejects unsupported/unknown `flow_cali` before upload.

Review gate:

- Spec reviewer checks every diagnostic/check/redaction/compatibility behavior from the spec.
- Code-quality reviewer checks async boundaries, error context, test fakes, and avoids speculative abstractions.

## Milestone 3: Hub HTTP API and Frontend Diagnostics Surface

Owner write scope:

- `crates/pandar-hub/src/routes.rs`
- `crates/pandar-hub/src/routes/printers.rs` or a new focused route module if the file would otherwise grow too large
- `crates/pandar-hub/src/routes/tests/**`
- `frontend/app/actions.ts`
- `frontend/app/page.tsx`
- small frontend helper/component files if needed to keep `page.tsx` readable

Tasks:

1. Add hub routes:
   - `POST /api/v1/tenants/{tenant_id}/agents/{agent_id}/discover-printers`
   - `POST /api/v1/tenants/{tenant_id}/agents/{agent_id}/diagnose-printer`
   - `GET /api/v1/tenants/{tenant_id}/commands/{command_id}`
2. Enforce roles:
   - discovery/diagnosis require `Operator`;
   - command detail requires `Viewer`;
   - command detail must use `get_for_tenant`.
3. Add command enqueue methods with audit actions:
   - `agent.discover_printers`
   - `agent.diagnose_printer`
4. Validate discovery timeout:
   - default `5`;
   - min `1`;
   - max `15`;
   - out-of-range returns `400`.
5. Include `result_json` in `CommandResponse`.
6. Add frontend server actions for discovery and diagnostics; redirect with enough query params to select the tenant and command result.
7. Fetch selected command detail on the dashboard and render:
   - discovered printer rows;
   - diagnostic checks;
   - compatibility capabilities, treating `unsupported` and `unknown` as unavailable;
   - no access-code inputs or persisted Bambu credentials.
8. Show tenant agents on the dashboard so discovery can be triggered for a linked agent.

Tests:

- route tests for auth, invalid timeout, tenant scoping, audit action, wake-agent behavior, and command detail result.
- route/frontend redaction tests or assertions prove diagnostic command payloads, audit event metadata rows, server-action redirects/output, and rendered dashboard command data do not contain the distinctive fake access code. This should be structurally true because the hub/frontend never accept access-code input.
- frontend build.
- if practical, focused frontend rendering tests are optional; do not add a new frontend test framework for this phase.

Review gate:

- Spec reviewer checks API/UI acceptance criteria and credential boundary.
- Code-quality reviewer checks route ownership, page complexity, and no card nesting/overly decorative UI churn.

## Milestone 4: Documentation, Final Verification, Commit, Push

Owner: leader.

Tasks:

1. Update `docs/architecture.md` with:
   - command `result_json` boundary;
   - agent-local credential rule;
   - diagnostics lifecycle;
   - compatibility matrix ownership.
2. Update `docs/roadmap.md`:
   - mark Phase 13 completed after implementation review passes;
   - keep Phase 14/15 next items accurate.
3. Update `README.md` or `PRODUCT.md` only if new local operator commands/env behavior need documentation.
4. Run final implementation review against the spec and plan with native reviewer and opencode reviewer.
5. Run fresh verification:
   - `cargo fmt`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo nextest run --manifest-path "Cargo.toml" --workspace`
   - frontend build command from `frontend/package.json` if frontend files changed
6. Inspect `git status --short` and `git diff`.
7. Commit with Lore protocol and push `main`.

Final acceptance checklist:

- Discovery command enqueues, runs on the agent, and returns structured discovered printers.
- Diagnostics command enqueues, runs on configured agent printers, and returns actionable structured checks.
- Expected printer/environment problems are successful commands with `overall = "problem"`.
- Unexpected command execution failures are failed commands with redacted error context.
- Access codes are not present in hub command payload/result/audit, logs touched by this phase, or frontend state/rendering.
- Compatibility rules are centralized and referenced by FTPS profile, FTPS transfer policy, MQTT print option validation, diagnostics, and frontend availability.
- SQLite and PostgreSQL stay in parity.
- No generated protobuf output is committed.
