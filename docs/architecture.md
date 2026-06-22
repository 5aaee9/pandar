# Pandar Architecture

Pandar is a self-hosted Bambu Studio cloud alternative. The system is split into a public hub, local agents, a shared core crate, and a Next.js product UI.

## System Shape

```text
Client -(HTTP / WebSocket)-> pandar-hub
pandar-agent -(gRPC stream)-> pandar-hub
pandar-agent -(MQTT + file transfer)-> Bambu machines
```

`pandar-hub` owns tenants, users, authorization, agent registration, printer inventory, durable command state, and user-facing HTTP/WebSocket APIs. It must treat SQLite and PostgreSQL as first-class database backends.

`pandar-agent` runs on a user's local network. It owns LAN printer discovery, printer credentials, MQTT sessions, file upload/download, and local machine command execution. The agent connects outward to the hub over gRPC so deployments do not need inbound access to the user's LAN.

`pandar-core` owns shared domain types and wire contracts used by hub and agent. Protocol-specific printer code should stay out of core unless it is a stable, shared data model.

`frontend` is the product UI. It should talk only to `pandar-hub`, never directly to agents or printers.

End-user authentication can be delegated to Clerk or Logto, but tenant authorization remains inside Pandar. The hub verifies identity-provider JWTs, maps provider subjects to local users, and checks Rust-managed user-to-tenant membership plus tenant roles for every tenant-scoped operation.

## Reference Scan

The Bambu machine implementation should be derived from `reference/BambuStudio` and `reference/bambuddy`, without copying unrelated application code.

### MQTT Machine Channel

Evidence from `reference/bambuddy/backend/app/services/bambu_mqtt.py`:

- Bambu printers use MQTT over TLS on port `8883`.
- Authentication uses username `bblp` and the printer access code as password.
- Status reports arrive on `device/{serial_number}/report`.
- Commands are published to `device/{serial_number}/request`.
- Publish calls should use QoS `1`; the reference notes QoS `0` can be ignored while printers are busy.
- The client requests state with `{"pushing": {"command": "pushall"}}`.
- Basic print controls are JSON commands under `print.command`, including `project_file`, `stop`, `pause`, `resume`, and `print_speed`.

Evidence from `reference/BambuStudio/src/slic3r/Utils/NetworkAgent.hpp`:

- Bambu Studio has a network boundary for discovery, subscribe/unsubscribe, generic message send, printer connect/disconnect, and local print/start-to-SD-card operations.
- Pandar should mirror this as an agent-side capability boundary rather than leaking MQTT implementation details into hub APIs.

### File Transfer Channel

Evidence from `reference/bambuddy/backend/app/services/bambu_ftp.py`:

- The reference implementation uses implicit FTP over TLS on port `990`.
- Login uses username `bblp` and the printer access code.
- Some models need protected data mode (`PROT P`), while A1/A1 Mini paths may need fallback to clear data mode (`PROT C`) with the control channel still encrypted.
- Uploads use manual `STOR` transfer chunks instead of `storbinary()` for A1 compatibility.
- Upload completion should wait for the printer's transfer response or verify server-side size before issuing the print command.
- Downloads and uploads cache the working mode per printer IP.
- Model profiles are needed for firmware-specific transport quirks such as TLS 1.2 caps on affected printers.

The user-facing architecture currently names this channel "SFTP / MQTT". The reference projects show Bambu-compatible file transfer behavior as implicit FTPS. Pandar should keep the public abstraction as "machine file transfer" until the exact supported protocol set is implemented and tested.

### Print Dispatch Flow

Evidence from `reference/bambuddy/backend/app/services/bambu_mqtt.py` and `reference/BambuStudio/src/slic3r/Utils/PrintHost.hpp`:

- Bambu Studio models print host upload as a `PrintHostUpload` with source path, upload path, and post-upload action.
- The send dialog enqueues a print host upload job, then optionally starts printing.
- Bambuddy starts a print by first uploading a file to the printer, then publishing a `project_file` MQTT command with `url: ftp://{filename}`, `file`, plate gcode path, calibration flags, AMS mapping, and unique task identity fields.

Pandar should preserve this split:

1. Upload artifact to printer file storage.
2. Confirm or verify file availability.
3. Send MQTT print command.
4. Track state transitions from MQTT reports.
5. Persist dispatch identity so reconnects and hub restarts can reconcile job state.

### Print Report Reconciliation

Evidence from `reference/bambuddy/backend/app/services/bambu_mqtt.py`:

- `project_file` commands should carry unique `project_id`, `task_id`, and `subtask_id` values so repeated prints are not mistaken for stale continuations by firmware or observers.
- Printer reports expose physical state through fields such as `print.gcode_state`, `print.mc_percent`, `print.mc_remaining_time`, `print.layer_num`, `print.total_layer_num`, `print.subtask_id`, `print.gcode_file`, and `print.subtask_name`.
- Terminal state should be inferred from transitions:
  - `RUNNING` marks a physical print in progress.
  - `FINISH` marks a successful physical completion.
  - `FAILED` marks a failed print, including some pre-print setup failures.
  - `IDLE` immediately after `RUNNING` marks an abort/cancel path.
- `print_error` and HMS-style fields are separate diagnostic channels and should be normalized into machine events rather than flattened into a display-only string.

Phase 5 deliberately stops at dispatch success. Later phases must keep command dispatch state and physical print state separate so a successful MQTT publish is never treated as a completed print.

### Discovery And Compatibility

Evidence from `reference/bambuddy/backend/app/services/discovery.py`:

- Bambu LAN discovery uses SSDP multicast `239.255.255.250:2021`.
- The search target is `urn:bambulab-com:device:3dprinter:1`.
- Discovery and compatibility must remain agent-local because they depend on the user's LAN, local credentials, printer firmware, and model-specific transport behavior.

### External Identity Providers

Evidence from Clerk and Logto documentation:

- Clerk session tokens can be verified by backends with a public key or JWKS, expected signing algorithm, token expiration/not-before checks, and optional authorized-party checks for trusted frontend origins.
- Logto access tokens for APIs are JWTs validated through JWKS, issuer, audience/API resource, expiration, and scope or organization-context checks.
- Both providers supply authentication identity. Pandar should not use provider organizations as the tenant authorization source unless a future phase explicitly defines a synchronization model.

Pandar's contract:

1. Verify the bearer token cryptographically and validate issuer/audience/time claims.
2. Extract a stable provider subject and provider identifier.
3. Resolve that identity to a local Pandar user.
4. Authorize tenant access through Pandar-managed `users`/membership/role records.
5. Preserve Phase 6 tenant API tokens as service credentials for automation and non-browser clients.

Phase 10 implements this contract in `pandar-hub` with one configured external identity profile per hub process. The hub parses `PANDAR_EXTERNAL_AUTH_PROVIDER`, issuer, JWKS URL, optional audience, RS-family algorithm allow-list, optional Clerk-style authorized parties, optional Logto-style required scopes, and clock leeway at startup. Partial external-auth configuration is a startup error.

Tenant route authentication checks bearer credentials in this order:

1. Existing Phase 6 API token lookup.
2. External JWT verification against cached JWKS when configured.
3. Local `{tenant_id, provider, subject}` lookup in `user_identities`.
4. Local tenant role check from the linked Pandar user.

JWT verification failures return `401 invalid_auth_token`. A cryptographically valid identity-provider token without a tenant-local identity link returns `403 tenant_forbidden`. Insufficient Pandar tenant role still returns `403 role_forbidden`.

## Target Components

### pandar-hub

- HTTP API for tenants, users, agents, printers, jobs, and printer commands.
- WebSocket API for live printer state, job progress, agent status, and notifications.
- gRPC server for reverse agent sessions.
- Backend-neutral persistence layer for SQLite and PostgreSQL.
- Command ledger: durable records for requested commands, dispatch status, agent acknowledgement, printer acknowledgement, timeout, and failure cause.
- Tenant boundary: every user, agent, printer, job, and command is tenant-scoped.

Phase 1 currently implements the hub HTTP foundation with repository-backed persistence:

- `GET /healthz` returns process health.
- `GET /api/v1/summary` returns tenant, agent, printer, and command counts.
- `POST /api/v1/tenants` and `GET /api/v1/tenants` create and list persisted tenants.
- `POST /api/v1/tenants/{tenant_id}/agents` and `GET /api/v1/tenants/{tenant_id}/agents` create and list persisted tenant-scoped agents.
- Duplicate tenant slugs and duplicate agent names map to stable conflict errors; malformed tenant IDs and missing tenants map to stable client errors.

`pandar-hub` reads `PANDAR_DATABASE_URL`, defaults to `sqlite://pandar.db`, connects through the backend-neutral `Database` boundary, and runs SQLx migrations before serving. SQLite and PostgreSQL use separate migration directories with equivalent Phase 1 tables and repository behavior.

Phase 2 adds the reverse gRPC control plane:

- The hub starts its HTTP listener from `PANDAR_HUB_BIND` and its gRPC listener from `PANDAR_HUB_GRPC_BIND`.
- `pandar-agent` connects outward to the hub through `AgentControl/ReverseConnect`.
- The first agent event must be `AgentHello` with tenant ID, agent ID, name, and version.
- The hub validates tenant/agent ownership, marks the persisted agent online, and registers a live session token.
- Heartbeats update both live session metadata and persisted agent last-seen/version fields.
- Hub commands are first written to the durable command ledger, then dispatched to the active session and marked sent.
- Agent acknowledgement and result events update the command ledger through token-scoped session handling so stale replaced streams cannot mutate current state.
- Replacing a live session closes the previous response stream while preserving the newer session.

The session registry is intentionally in-memory and only represents currently connected agents. The command ledger is durable and remains the source of truth for queued, sent, acknowledged, succeeded, and failed commands across hub restarts.

Phase 4 adds tenant-scoped printer inventory and state:

- Agent `PrinterSnapshot` events are accepted only from the current live session token, persisted as latest printer state, and ignored from stale replaced streams.
- The printer repository stores latest name, serial, optional model, normalized status, owning agent, and last-seen time behind the backend-neutral SQLite/PostgreSQL boundary.
- `GET /api/v1/tenants/{tenant_id}/printers` and `GET /api/v1/tenants/{tenant_id}/printers/{printer_id}` expose tenant-scoped inventory.
- `POST /api/v1/tenants/{tenant_id}/agents/{agent_id}/refresh-printers` writes a durable refresh command before dispatching it to the live agent session.
- `GET /api/v1/tenants/{tenant_id}/printer-events` broadcasts future printer snapshot updates over an in-memory tenant WebSocket channel. It does not replay historical state; HTTP listing remains the initial-state source.

Phase 5 adds tenant-scoped print dispatch while preserving the durable command ledger:

- `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs` accepts a base64 project artifact plus print options, writes the artifact into the hub spool, then creates artifact metadata, a linked print command, and a job row in one backend-neutral SQLite/PostgreSQL transaction.
- `GET /api/v1/tenants/{tenant_id}/jobs` and `GET /api/v1/tenants/{tenant_id}/jobs/{job_id}` expose tenant-scoped job history and command status.
- `PANDAR_SPOOL_DIR` controls the hub artifact root and defaults to `pandar-spool`; `PANDAR_MAX_ARTIFACT_BYTES` controls decoded artifact size and defaults to `10485760`.
- The hub sends `PrintProjectFile` over the existing reverse gRPC stream with job id, artifact id, printer id, Bambu serial number, artifact metadata, plate id, AMS, flow calibration, and timelapse flags.
- Print job creation is not exposed as a standalone command enqueue path. Print commands are created only with their linked job and artifact metadata.
- Command acknowledgement/result events update both command and job state through repository-level transactions, so print job status cannot drift from its durable command status.
- `succeeded` means dispatch work completed at the agent boundary. It does not mean the printer physically finished the print; MQTT report reconciliation remains a later phase.

Phase 9 adds physical print reconciliation while preserving that Phase 5 dispatch contract:

- The agent emits `PrintJobReport` events over the existing reverse gRPC stream. Reports include printer serial, optional job/artifact/subtask ids, active file names, `gcode_state`, progress percent, remaining minutes, layer counters, diagnostics, and an agent-observed RFC3339 timestamp.
- The hub accepts report events only from the current live session token, rejects blank serials and invalid timestamps, trims optional strings, drops out-of-range transient metrics, and ignores stale replaced streams.
- `jobs.status` and `command.status` remain dispatch lifecycle fields. Physical state is stored under `jobs.print_status`, `printer_state`, `progress_percent`, `remaining_time_minutes`, layer fields, active file, monotonic last progress/layer fields, `print_error`, and print lifecycle timestamps.
- `GET /api/v1/tenants/{tenant_id}/jobs` and `GET /api/v1/tenants/{tenant_id}/jobs/{job_id}` include a nested `print` object with physical status, progress, layer, remaining-time, active-file, terminal error, and timestamps.
- Reconciliation matches reports by exact job id first, artifact/subtask id second, then a same-printer active-file fallback for non-terminal jobs created in the last 24 hours. Ambiguous fallback does not update a job.
- `machine_events` stores normalized `print_progress`, `print_terminal`, `print_error`, and `hms` diagnostics with replay-stable tenant-scoped event keys. Replayed terminal reports dedupe instead of creating duplicate completion/failure events.
- `/api/v1/tenants/{tenant_id}/printer-events` now broadcasts future `job_progress` events with the same job response shape after a print report changes a job or inserts job-scoped machine events. It still does not provide durable replay; HTTP job list/detail remains the initial-state source.
- SQLite and PostgreSQL migrations add equivalent job print-lifecycle columns, metric constraints, `machine_events`, and indexes. Repository hydration uses checked integer conversion for progress fields.

Phase 10 adds external identity authentication while preserving API-token automation:

- `user_identities` links provider subjects such as Clerk or Logto user ids to existing tenant-scoped Pandar users.
- The hub verifies RS256/RS384/RS512 JWTs through configured JWKS, issuer, optional audience, expiration, optional not-before, optional authorized-party, and optional scope rules.
- HTTP tenant routes and `/printer-events` WebSocket authorization share the same API-token-first then external-JWT flow.
- The verifier caches JWKS and refreshes when a token references an unknown `kid`.
- Route tests use local RSA/JWKS fixtures and do not contact Clerk, Logto, or external JWKS endpoints.

Phase 11 adds explicit provisioning and bootstrap boundaries:

- `PANDAR_BOOTSTRAP_TOKEN` is the only credential accepted by cross-tenant endpoints: `GET /api/v1/summary`, `GET /api/v1/tenants`, `POST /api/v1/tenants`, and `POST /api/v1/bootstrap/tenant-admin`.
- `POST /api/v1/bootstrap/tenant-admin` creates a tenant, tenant admin user, initial API token, and bootstrap audit events in one SQLite/PostgreSQL transaction. The plaintext token is returned once and only its hash is stored.
- Tenant admins can list/create users, update local user roles, link Clerk/Logto provider subjects, create/list/revoke API tokens, and create agent pairing bundles through tenant-scoped APIs.
- API-token revocation sets `api_tokens.revoked_at`; revoked tokens are excluded from bearer authentication.
- Provisioning actions are represented in `audit_events` with actions such as `tenant.bootstrap`, `tenant.create`, `user.create`, `user.role_update`, `user_identity.link`, `api_token.create`, `api_token.revoke`, and `agent.pairing_bundle`.
- Agent pairing bundles return `PANDAR_TENANT_ID`, `PANDAR_AGENT_ID`, and `PANDAR_AGENT_NAME` for deployment. The future token-rotation protocol will add short-lived pairing secrets and authenticated gRPC agent credential rotation.

Bootstrap a fresh tenant:

```bash
curl -sS -X POST "$PANDAR_API/api/v1/bootstrap/tenant-admin" \
  -H "Authorization: Bearer $PANDAR_BOOTSTRAP_TOKEN" \
  -H "content-type: application/json" \
  -d '{"tenant_slug":"acme","tenant_display_name":"Acme","admin_email":"admin@example.com","admin_display_name":"Admin","api_token_name":"bootstrap-admin"}'
```

Tenant-admin provisioning examples:

```bash
curl -sS -X POST "$PANDAR_API/api/v1/tenants/$TENANT_ID/users" \
  -H "Authorization: Bearer $TENANT_ADMIN_TOKEN" \
  -H "content-type: application/json" \
  -d '{"email":"operator@example.com","display_name":"Operator","role":"operator"}'

curl -sS -X POST "$PANDAR_API/api/v1/tenants/$TENANT_ID/users/$USER_ID/identities" \
  -H "Authorization: Bearer $TENANT_ADMIN_TOKEN" \
  -H "content-type: application/json" \
  -d '{"provider":"logto","subject":"user_123"}'

curl -sS -X POST "$PANDAR_API/api/v1/tenants/$TENANT_ID/users/$USER_ID/api-tokens" \
  -H "Authorization: Bearer $TENANT_ADMIN_TOKEN" \
  -H "content-type: application/json" \
  -d '{"name":"automation"}'

curl -sS -X DELETE "$PANDAR_API/api/v1/tenants/$TENANT_ID/api-tokens/$TOKEN_ID" \
  -H "Authorization: Bearer $TENANT_ADMIN_TOKEN"
```

Agent pairing bundle example:

```bash
curl -sS -X POST "$PANDAR_API/api/v1/tenants/$TENANT_ID/agent-pairings" \
  -H "Authorization: Bearer $TENANT_ADMIN_TOKEN" \
  -H "content-type: application/json" \
  -d '{"name":"workshop-agent"}'
```

Planned hub phases after Phase 7:

- Phase 8 keeps hub behavior mostly unchanged while the agent gains real FTPS upload; hub command/job status still records dispatch success or failure.
- Phase 9 added physical print reconciliation, persistent progress fields, normalized machine events, and tenant WebSocket job progress broadcasts.
- Phase 10 added Clerk/Logto-compatible JWT verification, provider-subject-to-local-user mapping, and Pandar-owned tenant membership checks for HTTP and WebSocket auth.
- Phase 11 added first-user/bootstrap, tenant user/token management, explicit bootstrap boundaries, provisioning audit events, and identity linking flows.
- Phase 12 completed the staged repository layer migration to SeaORM while preserving SQLite/PostgreSQL behavior and existing SQLx schema migrations.
- Phase 13 added structured command `result_json` persistence, tenant-scoped discovery/diagnostic command APIs, and command detail reads for frontend diagnostics. `result_json` is for structured agent output such as discovery rows and diagnostic checks; it must not contain Bambu access codes.

Phase 12 persistence boundary:

- Hub repositories use SeaORM 2.0 hand-written entities and SeaORM transactions for persistent behavior.
- SQLx remains for database connection setup and migration execution.
- Raw SQL repository business logic is limited to `crates/pandar-hub/src/repositories/adapters/printers.rs`, which preserves atomic printer snapshot upsert on `(tenant_id, serial_number)` with SQLite/PostgreSQL `ON CONFLICT` semantics and parity tests.
- SQLx in repository tests remains for backend setup and corruption fixtures.

### pandar-agent

- gRPC client that keeps a long-lived reverse session to `pandar-hub`.
- Local printer registry loaded from hub assignment plus local discovery.
- MQTT transport module for printer reports and command publish.
- Machine file transfer module for upload, download, list, delete, and mode probing.
- Command executor that maps hub commands to printer operations.
- State normalizer that converts raw printer reports into stable Pandar events.
- Reconnect manager with backoff and stale-session detection.

Phase 3 adds the agent-side machine transport boundary:

- `PANDAR_PRINTERS` is an agent-local JSON array of `{host, serial, access_code, model?, name?}`. Empty config keeps the gateway non-networked. Invalid config fails before the reconnect loop starts.
- MQTT uses TLS port `8883`, username `bblp`, access code password, report topic `device/{serial}/report`, request topic `device/{serial}/request`, and QoS `1`.
- Bambu LAN printers present printer-local/self-signed TLS certificates on MQTT. The runtime MQTT adapter uses a Bambu-specific rustls verifier that accepts the printer certificate while keeping TLS encryption and handshake signature verification. This policy is isolated to agent-to-printer MQTT and does not apply to hub-facing HTTP/gRPC TLS.
- `RefreshPrinters` sends an accepted command ack, publishes `{"pushing":{"command":"pushall"}}`, waits for one report with a bounded timeout, emits normalized `PrinterSnapshot` events, then emits a success or failed command result.
- MQTT report normalization uses serial/name from config and reads state from `print.gcode_state`, `print.state`, or root `state`, falling back to `unknown`.
- The MQTT runtime adapter is isolated in `pandar-agent`; tests use fake transports and do not open live broker connections.
- Machine file transfer is modeled as a protocol-neutral boundary derived from the reference FTPS behavior: implicit TLS port `990`, username `bblp`, 64 KiB upload chunks, list/download/upload/delete requests, protected data mode first, and A1/A1 Mini clear-data fallback with success-only mode caching.
- Phase 3 does not change hub persistence. The hub still receives normalized agent events over the existing gRPC stream.

Phase 4 carries configured printer model values into normalized snapshots. `RefreshPrinters` remains the explicit snapshot path: empty printer config stays no-network, configured printers publish `pushall`, one report is normalized, and the hub persists the latest state plus broadcasts a tenant event.

Phase 5 adds the `PrintProjectFile` command executor:

- `PANDAR_ARTIFACT_ROOT` controls where the agent reads hub-spooled artifacts and defaults to the current directory.
- The agent validates the requested Bambu serial against configured printers before resolving or reading artifact paths.
- Artifact storage paths must be relative paths below `PANDAR_ARTIFACT_ROOT`; absolute paths, `..`, and prefix escapes are rejected.
- The configured machine gateway composes machine file upload and MQTT `project_file` publish in order. It uploads the artifact filename through the file-transfer boundary, then publishes to `device/{serial}/request` with QoS `1`, `ftp://{filename}`, `Metadata/plate_{plate_id}.gcode`, job/subtask ids, and print flags.
- Unit tests use fake file-transfer and MQTT transports to prove upload-before-publish behavior and no-publish-on-upload-failure behavior without opening real Bambu sockets.
- Configured runtime agents use the Bambu FTPS adapter for machine file upload. The adapter uses implicit FTPS on port `990`, the Bambu LAN TLS policy for printer-local/self-signed certificates, protected/clear data mode selection, and server-side size verification before MQTT `project_file` publish. Tests use fake transfer transports and do not open live Bambu sockets.

Agent phase status after Phase 7:

- Phase 8 added the real file-transfer runtime with implicit FTPS on port `990`, post-upload size verification, model/profile transport policy, and actionable upload diagnostics.
- Phase 9 converts continuous MQTT reports into normalized job progress and terminal print events that can be correlated to hub jobs. Configured printers use a separate report MQTT client id from command transport so long-running subscriptions do not collide with command sessions.
- Phase 13 adds LAN discovery, credential validation, printer diagnostics, and centralized compatibility rules for feature availability and transport policy. Discovery uses agent-local SSDP and can run without configured printers. Diagnostics run only for agent-configured serials, keep access codes agent-local, and return structured checks for MQTT reachability/report flow, FTPS reachability/storage probe, configured-printer status, and compatibility. Expected printer or environment problems are represented as successful command results with `overall = "problem"` instead of failed hub commands.
- Phase 14 promotes AMS, external-spool, tray-change, and filament usage data into stable Pandar models.

### pandar-core

- IDs and domain records: tenant, user, agent, printer, job, command.
- Shared event and command enums for hub-agent gRPC.
- Normalized printer state models.
- Error types that preserve lower-level causes.

### frontend

- Tenant-scoped dashboard.
- Agent status and pairing screens.
- Printer inventory and live state.
- Job dispatch and command controls.
- Operational settings for database-independent hub behavior.

Phase 4 replaces the placeholder landing page with a small operational dashboard. It fetches hub summary counts, tenant list, and the first tenant's printer inventory from `APP_API_URL` using uncached server-side HTTP requests. It renders empty states for no tenants and no reported printers. Phase 5 adds job history plus an HTTP-only dispatch form that posts base64 artifacts and print flags through the Rust hub API. Phase 9 displays dispatch status separately from physical print status, percent/layer progress, remaining time, and terminal print reason from the HTTP `job.print` shape. Phase 10 centralizes frontend bearer forwarding: request cookie `APP_AUTH_COOKIE_NAME` defaulting to `pandar_auth_token`, then `APP_AUTH_BEARER_TOKEN`, then `APP_API_TOKEN`. Phase 11 keeps configured tenant dashboards on tenant-scoped APIs when `APP_TENANT_ID` is set, so ordinary tenant tokens do not need bootstrap authority. Phase 13 exposes linked agents, discovery commands, diagnostic commands, and selected command details. It renders discovery rows, diagnostic checks, and compatibility capability availability from hub command `result_json`; it does not accept or display Bambu access codes. The frontend still does not consume the printer WebSocket; live subscription is left for Phase 15.

Planned frontend phases after Phase 7:

- Phase 9 exposes job progress and terminal print failure/success state from HTTP job history; hub live `job_progress` events are available for Phase 15 consumption.
- Phase 10 forwards Clerk or Logto identity-provider bearer tokens to the Rust API through server-side cookie/static-token helpers. Provider SDK sign-in UI remains out of scope.
- Phase 11 adds provisioning, identity linking, and tenant token/user management screens.
- Phase 13 exposes discovery and compatibility diagnostics.
- Phase 15 consumes authenticated WebSocket events for day-to-day monitoring and notifications.

## Data Model Draft

- `tenants`: tenant identity and display metadata.
- `users`: tenant users and role assignments.
- `user_identities`: external identity links such as `{provider, subject, user_id}` for Clerk/Logto users.
- `tenant_memberships`: local user-to-tenant role assignments owned by Pandar.
- `agents`: reverse-connection identity, tenant binding, last seen time, version.
- `printers`: tenant, agent, serial number, name, model, network endpoint metadata, active flag.
- `printer_credentials`: agent-visible encrypted access code material or agent-local credential references.
- `printer_state_snapshots`: latest normalized state and raw state pointer for diagnostics.
- `jobs`: user-requested print jobs and dispatch metadata.
- `job_artifacts`: uploaded 3MF/G-code metadata and storage location.
- `commands`: durable hub-to-agent command ledger.
- `machine_events`: normalized printer and agent events for audit/debug history.

Credentials should not be sent to frontend clients. Prefer keeping printer access codes agent-local when possible; if hub storage is required, encrypt at rest and scope access by tenant and agent.

Identity-provider tokens should not define tenant access by themselves. The frontend may obtain tokens from Clerk or Logto, but Rust must validate the token and then authorize through Pandar-managed local user and tenant membership records.

## Protocol Boundaries

Hub-agent gRPC should carry normalized commands and events, not raw MQTT as the default API. Raw MQTT capture can exist as a diagnostics feature gated by authorization.

Agent-machine MQTT should be encapsulated behind a trait such as `MachineControlTransport`.

Agent-machine file transfer should be encapsulated behind a trait such as `MachineFileTransfer`.

Model-specific printer behavior should be encapsulated in the agent compatibility matrix. FTPS TLS/profile decisions, clear-data fallback, print option gates, diagnostics, and frontend availability should all consume the same conservative capability output. Unknown capability means unavailable in user-facing controls unless a future reference-backed phase upgrades it.

Hub persistence should be encapsulated behind repositories that are tested against SQLite and PostgreSQL.

## Open Questions

- Whether Pandar will support Bambu cloud account integration or LAN-only operation first.
- Whether printer access codes are stored in hub, agent-local config, or both.
- Whether the file channel should expose the term SFTP, FTPS, or a protocol-neutral "file transfer" surface.
- Which printer families are required for the first compatibility target.
- Whether virtual-printer/proxy behavior from `bambuddy` is in scope for the first release.
- Whether SeaORM's migration system should replace SQLx migrations after repository migration is complete, or whether SQLx migrations should remain the schema authority.
