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
- The default runtime file-transfer adapter still returns `Bambu FTPS runtime is not implemented in this phase`; deployments must share hub spool and agent artifact roots for the filesystem artifact reader, but live printer upload awaits the next runtime implementation phase.

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

Phase 4 replaces the placeholder landing page with a small operational dashboard. It fetches hub summary counts, tenant list, and the first tenant's printer inventory from `APP_API_URL` using uncached server-side HTTP requests. It renders empty states for no tenants and no reported printers. Phase 5 adds job history plus an HTTP-only dispatch form that posts base64 artifacts and print flags through the Rust hub API. The frontend does not consume the printer WebSocket yet; live subscription is left for a later phase after stronger auth and tenant selection are in place.

## Data Model Draft

- `tenants`: tenant identity and display metadata.
- `users`: tenant users and role assignments.
- `agents`: reverse-connection identity, tenant binding, last seen time, version.
- `printers`: tenant, agent, serial number, name, model, network endpoint metadata, active flag.
- `printer_credentials`: agent-visible encrypted access code material or agent-local credential references.
- `printer_state_snapshots`: latest normalized state and raw state pointer for diagnostics.
- `jobs`: user-requested print jobs and dispatch metadata.
- `job_artifacts`: uploaded 3MF/G-code metadata and storage location.
- `commands`: durable hub-to-agent command ledger.
- `machine_events`: normalized printer and agent events for audit/debug history.

Credentials should not be sent to frontend clients. Prefer keeping printer access codes agent-local when possible; if hub storage is required, encrypt at rest and scope access by tenant and agent.

## Protocol Boundaries

Hub-agent gRPC should carry normalized commands and events, not raw MQTT as the default API. Raw MQTT capture can exist as a diagnostics feature gated by authorization.

Agent-machine MQTT should be encapsulated behind a trait such as `MachineControlTransport`.

Agent-machine file transfer should be encapsulated behind a trait such as `MachineFileTransfer`.

Hub persistence should be encapsulated behind repositories that are tested against SQLite and PostgreSQL.

## Open Questions

- Whether Pandar will support Bambu cloud account integration or LAN-only operation first.
- Whether printer access codes are stored in hub, agent-local config, or both.
- Whether the file channel should expose the term SFTP, FTPS, or a protocol-neutral "file transfer" surface.
- Which printer families are required for the first compatibility target.
- Whether virtual-printer/proxy behavior from `bambuddy` is in scope for the first release.
