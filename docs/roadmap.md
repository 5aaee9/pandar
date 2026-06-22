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
- Added Phase 3 agent-side Bambu MQTT models, payload builders, fake/runtime transport boundary, refresh gateway, and `RefreshPrinters` snapshot/result sequencing.
- Added Phase 3 agent-local `PANDAR_PRINTERS` parsing with startup validation and no-network empty config behavior.
- Added Phase 3 machine file-transfer boundary with FTPS-derived constants, request shapes, protected/clear mode policy, success-only cache behavior, and fake no-network tests.
- Added Phase 4 hub printer inventory persistence, tenant-scoped printer HTTP APIs, refresh-printers command dispatch endpoint, future-only printer WebSocket events, and the read-only frontend operations dashboard.
- Added Phase 5 hub print artifacts/jobs persistence, tenant-scoped print job HTTP APIs, print command gRPC dispatch, command/job status coupling, agent artifact-root handling, frontend job history, and HTTP-only print dispatch form.

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

- Completed agent-side MQTT transport boundary based on the reference facts:
  - TLS port 8883.
  - Bambu LAN self-signed certificate policy isolated to the agent MQTT adapter.
  - username `bblp`, password access code.
  - subscribe `device/{serial}/report`.
  - publish `device/{serial}/request`.
  - QoS 1 for publishes.
- Completed state refresh via `pushing.pushall` through the `RefreshPrinters` gateway path.
- Completed basic command payload builders: pause, resume, stop, print speed, raw diagnostics command, and reserved `project_file` shape.
- Completed machine file transfer abstraction based on the reference FTPS behavior:
  - implicit TLS port 990.
  - username `bblp`, password access code.
  - upload, download, list, delete.
  - protected data mode first, model-specific fallback where needed.
- Completed targeted tests for command JSON construction, topic naming, fake MQTT refresh, printer config parsing, command event sequencing, and file-transfer mode selection/fallback.

## Phase 4: Printer Inventory And State

- Completed hub persistence for latest tenant-scoped printer state reported by agents.
- Completed tenant-scoped printer list/detail HTTP APIs.
- Completed refresh-printers HTTP command dispatch through the command ledger.
- Completed future-only tenant WebSocket broadcasts for printer snapshots; historical state is loaded through HTTP.
- Completed frontend summary, tenant, and printer inventory dashboard using uncached server-side HTTP reads from `APP_API_URL`.
- Deferred frontend WebSocket consumption until authentication and tenant selection are stronger.

## Phase 5: Print Dispatch

- Completed `JobArtifact` and `Job` core domain models and protobuf `PrintProjectFile` command payload.
- Completed SQLite and PostgreSQL migrations for `job_artifacts` and `jobs`.
- Completed hub artifact spool storage with `PANDAR_SPOOL_DIR`, `PANDAR_MAX_ARTIFACT_BYTES`, filename sanitization, and scoped cleanup on repository failure.
- Completed tenant-scoped print job HTTP APIs:
  - `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs`
  - `GET /api/v1/tenants/{tenant_id}/jobs`
  - `GET /api/v1/tenants/{tenant_id}/jobs/{job_id}`
- Completed atomic print job creation: artifact metadata, linked command, and job row commit together.
- Completed print command dispatch over the existing agent reverse gRPC stream, including printer id, Bambu serial number, artifact metadata, and print options.
- Completed command/job lifecycle coupling for print jobs through repository-level SQLite/PostgreSQL transactions.
- Completed agent `PANDAR_ARTIFACT_ROOT` handling, safe relative artifact path resolution, missing-artifact failure reporting, and unknown-serial rejection before artifact I/O.
- Completed configured agent gateway composition for uploading a project artifact through `MachineFileTransfer`, then publishing MQTT `project_file` with job identity and print flags; fake tests verify upload-before-publish and no-publish-on-upload-failure behavior without live Bambu sockets.
- Completed frontend print job history, per-printer dispatch API visibility, and an HTTP-only dispatch form that posts base64 artifacts through the hub API.
- Deferred real printer file-transfer runtime upload and upload verification; the default Phase 5 runtime adapter returns an explicit unavailable error after serial selection until the FTPS implementation is completed.
- Deferred printer-report reconciliation for physical print progress/completion/failure to the next machine-runtime phase.

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

- Implement real agent-side FTPS upload and upload verification behind the existing file-transfer boundary.
- Wire the real runtime FTPS adapter into the configured gateway path that already fake-tests MQTT `project_file` publishing.
- Reconcile printer MQTT reports into print job progress, terminal success, and terminal failure state.
- Add authenticated browser-side job creation and tenant/user authorization around the existing HTTP form.
