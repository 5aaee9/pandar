# Pandar Roadmap

## Reference Findings

- `reference/bambuddy` provides the clearest direct implementation reference for Bambu MQTT, file transfer, printer state normalization, and printer connection management.
- `reference/BambuStudio` provides the higher-level product and protocol boundaries: print host upload jobs, network-agent discovery/message APIs, and local print/send-to-SD-card entry points.
- The machine command channel should use MQTT over TLS with `device/{serial}/report` and `device/{serial}/request` topics.
- The machine file channel should start from the reference's implicit FTPS behavior on port 990, even though the high-level product brief says SFTP. Keep Pandar's public boundary protocol-neutral until implementation confirms final naming.
- Print dispatch should be modeled as upload artifact, verify artifact, send MQTT `project_file`, then reconcile state from reports.
- `reference/bambuddy/backend/app/services/bambu_ftp.py` adds details needed for the real runtime: implicit FTPS, username `bblp`, manual 64 KiB `STOR` chunks, protected-data mode first, clear-data fallback for A1-family behavior, post-upload `226`/`SIZE` verification, and model profiles such as TLS 1.2 caps for affected firmware.
- `reference/bambuddy/backend/app/services/bambu_mqtt.py` shows that physical job state must be reconciled from `gcode_state`, `mc_percent`, remaining time, layer counts, `subtask_id`, `print_error`, and HMS-style errors instead of treating MQTT publish success as print completion.
- `reference/bambuddy/backend/app/services/discovery.py` shows Bambu LAN discovery through SSDP multicast `239.255.255.250:2021` with search target `urn:bambulab-com:device:3dprinter:1`.
- Clerk and Logto both support backend API protection through JWT verification against provider JWKS plus issuer, audience, expiration, and optional authorized-party/scope checks. Pandar should treat the identity provider as authentication only; Rust remains the source of truth for user-to-tenant membership and tenant role authorization.

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
- Added Phase 6 tenant API token authentication, tenant role authorization, audit events, WebSocket auth, frontend server-side token forwarding, and SQLite/PostgreSQL Docker Compose examples.
- Added Phase 7 staged SeaORM 2.0 migration groundwork with SQLx 0.9 alignment, a shared SeaORM connection accessor, a hand-written `tenants` entity, and SeaORM-backed tenant repository operations.
- Added Phase 9 print report reconciliation with agent MQTT `PrintJobReport` forwarding, hub-side physical print lifecycle persistence, normalized machine events, tenant `job_progress` WebSocket broadcasts, nested `job.print` HTTP responses, and frontend job progress display.

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

- Completed API token authentication for tenant-scoped HTTP and WebSocket APIs.
- Completed tenant role authorization:
  - `tenant_admin` can create agents and perform operator/viewer actions.
  - `operator` can create jobs, refresh printers, and perform viewer actions.
  - `viewer` can read tenant resources and subscribe to printer events.
- Completed SQLite and PostgreSQL migrations for `api_tokens` and `audit_events`.
- Completed backend-neutral auth and audit repositories with SQLite default tests and optional PostgreSQL coverage via `PANDAR_TEST_POSTGRES_URL`.
- Completed audit events for successful agent creation, refresh-printers commands, and print job creation.
- Completed WebSocket authorization and tenant filtering through the same bearer-token boundary as HTTP.
- Completed frontend server-side `APP_API_TOKEN` forwarding and optional `APP_TENANT_ID` tenant binding for tenant printer/job reads and print job creation.
- Completed credential policy documentation: Bambu printer access codes remain agent-local in `PANDAR_PRINTERS` and must not be stored in the hub database or frontend env.
- Completed Docker Compose examples for SQLite and PostgreSQL deployments.

## Phase 7: SeaORM Migration

- Completed the first staged SeaORM 2.0 migration by adding SeaORM `2.0.0-rc.41` behind the existing SQLx pool boundary.
- Completed workspace SQLx `0.9.0` alignment required by SeaORM 2.0.
- Completed hand-written SeaORM entity coverage for `tenants`.
- Completed `TenantRepository` create/list/count migration to SeaORM while preserving the existing repository API and SQLite/PostgreSQL behavior.
- Deferred auth, audit, agents, printers, commands, jobs, and SeaORM migration-system adoption to later phases.

## Phase 8: Real Machine File Transfer Runtime

Goal: replace the Phase 5 unavailable runtime adapter with real agent-side Bambu-compatible file transfer while keeping the public boundary protocol-neutral.

- Completed implicit FTPS on port `990` behind the existing `MachineFileTransfer` trait.
- Completed the Bambu LAN TLS policy for printer-local/self-signed certificates.
- Completed protected/clear data mode selection with success-only mode caching.
- Completed server-side upload size verification before publishing MQTT `project_file`.
- Completed configured gateway wiring so runtime agents use the FTPS adapter for machine file upload.
- Kept tests fake by default with adapter-level coverage for mode policy, verification decisions, and error mapping without requiring live printer sockets.

Exit criteria:

- A configured agent can upload a project artifact to a Bambu printer through the runtime adapter.
- The configured print gateway still publishes MQTT only after upload verification succeeds.
- Upload failures preserve enough context to distinguish auth failure, no FTPS listener, missing SD card/path failure, quota/full card, timeout, TLS/profile mismatch, and partial upload.

## Phase 9: Print Report Reconciliation

Goal: make hub job state represent physical printer progress instead of only dispatch success.

- Completed agent MQTT report normalization beyond the snapshot path to emit `PrintJobReport` events while connected.
- Completed correlation to Pandar jobs using exact job id, artifact/subtask id, and deterministic active-file fallback.
- Completed persistence for printer state, percent, remaining time, current layer, total layers, active file, last valid progress, last valid layer, terminal errors, and normalized `machine_events` in both SQLite and PostgreSQL migrations.
- Completed `gcode_state` transition mapping:
  - `RUNNING` means physical print started or resumed.
  - `FINISH` means completed.
  - `FAILED` means failed, including pre-print failures from preparation states.
  - `IDLE` after `RUNNING` means cancelled or aborted.
- Completed `print_error` and HMS-style structured machine event capture with replay-stable dedupe keys.
- Completed tenant WebSocket `job_progress` broadcasts and nested HTTP `job.print` response fields.
- Completed frontend job history display for dispatch state, physical print state, progress, layers, remaining time, and terminal reason.
- Kept dispatch lifecycle and physical print lifecycle separate in naming and persistence so command success cannot be confused with print completion.

Exit criteria:

- A print job can move from queued/dispatching into running/completed/failed/cancelled from MQTT reports without changing dispatch status semantics.
- Hub restarts and agent reconnects can continue reconciling from latest reports without duplicating terminal events or regressing terminal physical status.
- Frontend users can see physical progress and terminal failure/success reasons for tenant jobs from HTTP job history. Browser live WebSocket consumption remains Phase 15; the authenticated hub `job_progress` WebSocket event already exists and is tested.

## Phase 10: External Identity Authentication

Goal: let users sign in with Clerk or Logto while keeping Pandar's tenant membership and role model in Rust.

- Add a provider-neutral OIDC/JWT verifier in `pandar-hub` for HTTP and WebSocket bearer tokens.
- Support Clerk and Logto through configuration, not provider-specific authorization logic:
  - issuer URL
  - JWKS URL or discoverable JWKS
  - expected audience/API resource
  - accepted algorithms
  - optional authorized parties/origins for Clerk-style session tokens
  - optional scope checks for Logto API-resource tokens
- Validate token signature, `iss`, `aud`, `exp`, `nbf` where present, token type where needed, and provider subject.
- Map verified `{provider, subject}` identities to local Pandar users.
- Manage user-to-tenant membership and tenant role assignments in Pandar tables; do not trust Clerk organizations or Logto organizations as the tenant authorization source.
- Keep tenant API tokens from Phase 6 for service/agent/admin automation, but make browser user flows use external identity tokens.
- Add frontend auth integration points so the UI can obtain a Clerk/Logto token and forward it to the Rust API as `Authorization: Bearer`.
- Add tests with local JWKS fixtures for valid token, unknown key, bad issuer, bad audience, expired token, missing membership, and insufficient tenant role.

Exit criteria:

- A signed-in Clerk or Logto user can call tenant-scoped APIs only when Rust has a matching local user and tenant membership.
- A valid identity-provider token without Pandar tenant membership is authenticated but not authorized.
- Tenant role decisions are fully enforced by Pandar repositories and are independent of provider-side organization membership.
- Existing API-token auth remains available for non-browser automation.

## Phase 11: Provisioning And Admin Boundaries

Goal: remove development-only tenant/token ergonomics before production multi-tenant exposure.

- Add first-user/bootstrap flow for creating the initial tenant admin and API token without direct repository fixtures.
- Add user invite/linking flows that bind a verified Clerk/Logto subject to a local Pandar user.
- Add tenant-scoped user and token management APIs for tenant admins.
- Add explicit global admin or bootstrap authorization for cross-tenant summary/listing endpoints.
- Audit provisioning, token creation/revocation, role changes, and agent pairing actions.
- Define agent pairing/token rotation flow that does not require copying persistent database IDs into agent env by hand.

Exit criteria:

- A fresh deployment can be bootstrapped through documented APIs/CLI without test fixtures.
- Tenant users cannot list or summarize other tenants unless they hold the explicit global/bootstrap authority.
- All provisioning actions are represented in audit events.

## Phase 12: Complete SeaORM Repository Migration

Goal: finish the staged SeaORM 2.0 migration without changing external repository behavior.

- Migrate auth, identity, membership, and audit repositories first because Phase 10 and Phase 11 expand those surfaces.
- Migrate agents/printers next, preserving live-session and printer snapshot semantics.
- Migrate command/job/artifact repositories last because they have the broadest transaction coupling.
- Keep SQLx migrations as the schema source until there is a separate, explicit decision to adopt SeaORM migrations.
- Maintain SQLite and PostgreSQL parity tests for every migrated repository.

Exit criteria:

- All persistent repository operations use SeaORM query/entity APIs or an explicitly documented backend-specific adapter.
- SQLite and PostgreSQL test coverage remains green for repository behavior and transaction coupling.
- No mixed SQLx/SeaORM behavior drift remains outside migration/bootstrap plumbing.

## Phase 13: Discovery, Diagnostics, And Compatibility Matrix

Goal: make real printer operation debuggable across Bambu printer families.

- Add agent-side LAN discovery from the reference SSDP behavior on multicast `239.255.255.250:2021`.
- Add diagnostics for wrong serial number, wrong access code, stale MQTT sessions, no MQTT report, no FTPS listener, missing SD card, full SD card, and upload verification failure.
- Build a compatibility matrix from reference behavior and live captures.
- Add model-specific feature gates for chamber temperature, drying, dual nozzle, flow/vibration/nozzle-offset calibration, and firmware-specific FTPS behavior.
- Keep unsupported or uncertain features visibly unavailable instead of adding silent fallbacks.

Exit criteria:

- Operators can discover local printers, validate credentials, and see actionable diagnostics before dispatching a print.
- Compatibility rules are centralized and referenced by MQTT command building, FTPS runtime policy, and UI availability.

## Phase 14: AMS, Filament, And Spool Operations

Goal: promote AMS/external-spool data from raw report details into first-class tenant-visible state.

- Normalize AMS units, tray IDs, external spool identifiers, active tray, tray changes, filament type/color/material fields, and remaining estimates from MQTT reports.
- Preserve `ams_mapping` and `ams_mapping2` semantics used by `project_file` commands.
- Add job-level filament usage records once Phase 9 supplies reliable start/completion boundaries.
- Evaluate optional Spoolman-style external inventory integration only after Pandar's internal state model is stable.

Exit criteria:

- The printer view exposes current AMS/external-spool state without raw MQTT payload knowledge.
- Print dispatch can show and persist the mapping used for each job.
- Filament usage can be derived from completed or failed jobs with clear uncertainty boundaries.

## Phase 15: Product Runtime UX And Notifications

Goal: turn the operational skeleton into a usable day-to-day cloud replacement surface.

- Consume authenticated printer/job WebSocket events in the frontend.
- Add focused operator notifications for agent disconnect, printer offline, upload failure, print failure, and print completion.
- Improve job detail/history views around dispatch status, physical progress, artifact metadata, and machine diagnostics.
- Add tenant settings for agent pairing, token management, and printer compatibility details.

Exit criteria:

- Common print monitoring workflows can be performed without refreshing the page.
- Notification and job detail surfaces explain whether a failure happened in hub dispatch, agent upload, MQTT publish, or physical printing.

## Optional Later: Virtual Printer And Proxy

- Decide whether virtual-printer/proxy behavior from `reference/bambuddy` is in scope.
- If accepted, isolate it as a separate local-agent feature because it changes LAN behavior, port ownership, MQTT/FTPS proxying, and discovery semantics.

## Immediate Next

- Start Phase 10 before browser-facing multi-tenant installs so Clerk/Logto users are authenticated by Rust and authorized through local tenant memberships.
- Do Phase 11 before exposing broader multi-tenant administration.
- Continue SeaORM repository migration as Phase 12 after identity/provisioning/auth/audit boundaries are stable.
