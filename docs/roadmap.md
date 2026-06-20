# Pandar Roadmap

## Reference Findings

- `reference/bambuddy` provides the clearest direct implementation reference for Bambu MQTT, file transfer, printer state normalization, and printer connection management.
- `reference/BambuStudio` provides the higher-level product and protocol boundaries: print host upload jobs, network-agent discovery/message APIs, and local print/send-to-SD-card entry points.
- The machine command channel should use MQTT over TLS with `device/{serial}/report` and `device/{serial}/request` topics.
- The machine file channel should start from the reference's implicit FTPS behavior on port 990, even though the high-level product brief says SFTP. Keep Pandar's public boundary protocol-neutral until implementation confirms final naming.
- Print dispatch should be modeled as upload artifact, verify artifact, send MQTT `project_file`, then reconcile state from reports.

## Completed

- Created the initial Rust workspace with `pandar-core`, `pandar-hub`, `pandar-agent`, and `pandar-app`.
- Added a repository-backed Axum hub with health, summary, tenant create/list, and tenant-scoped agent create/list endpoints.
- Added a minimal agent CLI boundary and a Bambu machine gateway trait for future SFTP/MQTT work.
- Added the first gRPC protocol contract under `proto/pandar/agent/v1/agent.proto`.
- Added a minimal Next.js frontend skeleton using `APP_API_URL`.
- Added `docs/architecture.md` with the target component split and reference-derived machine communication notes.
- Added Phase 1 SQLx persistence for SQLite and PostgreSQL with migrations, repository tests, SQLite durability coverage, and optional PostgreSQL tests behind `PANDAR_TEST_POSTGRES_URL`.
- Pushed the Phase 1 foundation to `main` at commit `1b02636`.
- Added Phase 2 generated gRPC protocol plumbing through build scripts so protobuf Rust output stays under Cargo `target`.
- Added the hub reverse gRPC service, live session registry, command ledger transitions, HTTP+gRPC startup, and the agent reverse client.
- Added SQLite-backed gRPC tests for session lifecycle, command dispatch, acknowledgement, result handling, stale stream protection, and replacement sessions.

## Phase 1: Foundation

- Completed canonical tenant and agent domain IDs/records in `pandar-core`.
- Completed hub repository layer and removed in-memory tenant/agent vectors from HTTP state.
- Completed SQLite and PostgreSQL migrations for Phase 1 tenants, users, agents, printers, and commands.
- Completed repository test harnesses for SQLite by default and optional PostgreSQL via `PANDAR_TEST_POSTGRES_URL`.
- Completed Phase 1 hub HTTP/API wiring against repositories, including startup migration from `PANDAR_DATABASE_URL`.

## Phase 2: Agent Reverse Connection

Goal: establish the durable reverse-control channel between locally deployed agents and `pandar-hub`.

- Expand `proto/pandar/agent/v1/agent.proto` for:
  - agent hello
  - heartbeat
  - printer snapshot
  - hub command dispatch
  - agent command acknowledgement
  - command result
- Completed tonic build/runtime dependencies in the hub and agent crate boundaries that own gRPC.
- Completed hub-side gRPC service for reverse agent sessions.
- Completed hub-side agent session registry with tenant/agent identity, connected status, heartbeat updates, stale-session protection, and replacement-session shutdown.
- Completed persisted agent version, last-seen, and status updates through the existing repository/database boundary.
- Completed `pandar-agent` outbound connection to `pandar-hub` with hello, heartbeat, refresh-printers ack/result, and reconnect/backoff.
- Add tenant binding or registration token placeholder flow sufficient for local development without introducing full auth yet.
- Completed local-development tenant/agent binding through explicit `PANDAR_TENANT_ID` and `PANDAR_AGENT_ID` values.
- Completed integration tests for:
  - agent hello registers a live session
  - heartbeat updates last-seen state
  - disconnected or timed-out agents become unavailable
  - hub command dispatch reaches the connected agent stream
  - command acknowledgement/result updates the command ledger

Exit criteria:

- A local `pandar-agent` can connect outward to a local `pandar-hub`.
- Hub can distinguish offline, connecting, and online agent state from persisted metadata plus live sessions.
- Hub can enqueue a command and receive an acknowledgement/result over the reverse stream.
- No Bambu machine network sockets are opened in Phase 2.

## Phase 3: Bambu Machine Transport

- Implement agent-side MQTT transport based on the reference facts:
  - TLS port 8883.
  - username `bblp`, password access code.
  - subscribe `device/{serial}/report`.
  - publish `device/{serial}/request`.
  - QoS 1 for publishes.
- Implement state refresh via `pushing.pushall`.
- Implement basic commands: pause, resume, stop, print speed, raw diagnostics command.
- Implement machine file transfer abstraction based on the reference FTPS behavior:
  - implicit TLS port 990.
  - username `bblp`, password access code.
  - upload, download, list, delete.
  - protected data mode first, model-specific fallback where needed.
- Add targeted tests for command JSON construction, topic naming, and file-transfer mode selection.

## Phase 4: Printer Inventory And State

- Add hub APIs for registering printers under an agent and tenant.
- Add agent-local printer config loading and hub assignment sync.
- Normalize MQTT reports into stable printer state events.
- Persist latest printer state in hub and broadcast state changes over WebSocket.
- Add frontend printer inventory and live state views.

## Phase 5: Print Dispatch

- Model job artifacts in hub.
- Send print requests from hub to agent through the command ledger.
- Upload artifact from agent to printer file storage.
- Verify uploaded artifact before print command.
- Publish MQTT `project_file` with plate path, calibration flags, AMS mapping, and unique task identity.
- Reconcile print start, progress, completion, failure, and cancellation from MQTT reports.
- Add frontend job dispatch and job history views.

## Phase 6: Multi-Tenant Product Hardening

- Add authentication and tenant-scoped authorization.
- Add user roles for tenant admin, operator, and viewer.
- Add audit events for printer commands and agent actions.
- Add credential handling policy for printer access codes.
- Add WebSocket authorization and tenant filtering.
- Add Docker Compose examples for SQLite and PostgreSQL deployments.

## Phase 7: Compatibility Expansion

- Build a printer model compatibility matrix from the references and live captures.
- Add AMS and external-spool mapping support.
- Add model-specific feature gates for chamber temperature, drying, dual nozzle, and calibration commands.
- Add diagnostics for wrong serial number, wrong access code, stale MQTT sessions, missing SD card, and file-transfer failures.
- Decide whether virtual-printer/proxy behavior from `reference/bambuddy` is in scope.

## Immediate Next

- Start Phase 3 by implementing the agent-side Bambu MQTT transport boundary and tests for topic naming, TLS port, credentials, QoS, and `pushall`.
- Implement the machine file-transfer abstraction from the reference FTPS behavior without exposing protocol-specific details through hub APIs.
- Add hub command variants for the first real printer controls after the transport boundary is tested locally.
