# Pandar Architecture

Pandar is a self-hosted Bambu Studio cloud alternative. The system is split into a public hub, local agents, a shared core crate, and a Next.js product UI.

## System Shape

```text
Client -(HTTP / WebSocket)-> pandar-web -(HTTP / WebSocket)-> pandar-hub
pandar-auth -(JWT / JWKS)-> pandar-hub
pandar-agent -(gRPC stream)-> pandar-hub
pandar-agent -(MQTT + file transfer)-> Bambu machines
Bambu Studio -(network plugin ABI)-> pandar-network-plugin -(HTTP / WebSocket)-> pandar-hub
```

`pandar-hub` owns tenants, users, authorization, agent registration, printer inventory, durable command state, and user-facing HTTP/WebSocket APIs. It must treat SQLite and PostgreSQL as first-class database backends.

`pandar-agent` runs on a user's local network. It owns LAN printer discovery, printer credentials, MQTT sessions, file upload/download, and local machine command execution. The agent connects outward to the hub over gRPC so deployments do not need inbound access to the user's LAN.

`pandar-core` owns shared domain types and wire contracts used by hub and agent. Protocol-specific printer code should stay out of core unless it is a stable, shared data model.

`frontend` is the product UI. It should talk only to `pandar-hub`, never directly to agents or printers. When configured for the self-hosted Better Auth issuer, it receives a compact JWT through `/auth/betterauth/callback`, stores it as the HTTP-only bearer cookie, and keeps `better-auth` runtime dependencies out of the dashboard bundle.

`pandar-auth` is the optional self-hosted Better Auth issuer app. It owns email magic-link sign-in, optional post-login passkey binding, its own SQLite database, and JWT/JWKS issuance. The hub does not read Better Auth tables; it only verifies emitted JWTs through the existing external-auth contract.

`pandar-network-plugin` is a Hub-backed Bambu Studio dynamic-library adapter. It builds one reviewed
ABI series at a time from `studio-abi-profiles.json`. The catalog currently covers `02.06.00`,
`02.06.01`, `02.07.00`, `02.07.01`, `02.08.00`, and `02.08.01`; a four-component Studio version
selects its artifact by the first three components. Each series is released as a separate archive because the C++
signatures and layouts change between series. The plugin connects only to `pandar-hub`; it must not
connect directly to `pandar-agent` or Bambu machines, and local machine access remains the agent's
responsibility.

End-user authentication can be delegated to Clerk, Logto, or Better Auth, but tenant authorization remains inside Pandar. The hub verifies identity-provider JWTs, maps provider subjects to local tenant users, and checks Rust-managed user-to-tenant membership plus tenant roles for every tenant-scoped operation. Pandar user rows are tenant-local projections of external accounts; the primary product path does not make Pandar an account manager.

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
- Basic Bambu print controls are JSON commands under `print.command`, including `project_file`, `stop`, `pause`, `resume`, and `print_speed`; Pandar keeps these as agent-side adapter details behind semantic Hub operations.

Evidence from `reference/BambuStudio/src/slic3r/Utils/NetworkAgent.hpp`:

- Bambu Studio has a network boundary for discovery, subscribe/unsubscribe, generic message send, printer connect/disconnect, and local print/start-to-SD-card operations.
- Pandar should mirror this as an agent-side capability boundary rather than leaking MQTT implementation details into hub APIs.

### File Transfer Channel

Evidence from `reference/bambuddy/backend/app/services/bambu_ftp.py`:

- The reference implementation uses implicit FTP over TLS on port `990`.
- Login uses username `bblp` and the printer access code.
- The reference can fall back to clear data mode (`PROT C`) for A1/A1 Mini. Pandar intentionally does not adopt that behavior: every FTPS data channel requires protected mode (`PROT P`) and fails closed if protection cannot be established.
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

### Web Print Monitor And Build-Plate Recovery

The Web client is a presentation and reconciliation boundary. It consumes enriched printer snapshots socket-first, uses REST for authoritative baselines and bounded repairs, and fails closed by clearing the enriched view and recovery controls when that state is unavailable.

Hub owns recovery policy and occurrence identity. Under the locked printer row it validates the error generation and marker, current task/state, printer model and native action catalog, and exact Agent session/capability. Web and Bambu Studio plugin recovery share the same printer-level single-flight so their native operations cannot overlap.

Agent owns only the Bambu transport primitive. Web recovery sequence ID `0` is recovery-only and runs over a fresh clean MQTT connection; a matching QoS1 PUBACK confirms delivery to the broker/printer transport, not application acceptance or physical recovery. Subsequent printer telemetry remains the only recovery confirmation.

Native recovery ownership and pending dispatch remain process-local, so this path supports one active Hub only. NATS event fanout or HTTP/gRPC session affinity does not create cross-process ownership, dispatch forwarding, or cleanup and therefore does not remove that restriction.

### Discovery And Compatibility

Evidence from `reference/bambuddy/backend/app/services/discovery.py`:

- Bambu LAN discovery uses SSDP multicast `239.255.255.250:2021`.
- The search target is `urn:bambulab-com:device:3dprinter:1`.
- When multicast is unavailable, the reference subnet scanner identifies local candidates and sends the same SSDP request directly to each printer host.
- Pandar sends discovery to multicast and to hosts on operational, non-point-to-point private IPv4 interfaces. Broad interface networks are limited to the `/22` containing the agent address, and public networks are never scanned.
- Discovery and compatibility must remain agent-local because they depend on the user's LAN, local credentials, printer firmware, and model-specific transport behavior.

### External Identity Providers

Evidence from Clerk, Logto, and Better Auth documentation:

- Clerk session tokens can be verified by backends with a public key or JWKS, expected signing algorithm, token expiration/not-before checks, and optional authorized-party checks for trusted frontend origins.
- Logto access tokens for APIs are JWTs validated through JWKS, issuer, audience/API resource, expiration, and scope or organization-context checks.
- Better Auth's JWT plugin exposes JWT/JWKS material. Better Auth 1.6.25 uses the JWA algorithm value `RS256` for RSA key generation, matching Pandar's `PANDAR_EXTERNAL_AUTH_ALGORITHMS=RS256` verifier setting.
- Both providers supply authentication identity. Pandar should not use provider organizations as the tenant authorization source unless a future phase explicitly defines a synchronization model.

Pandar's contract:

1. Verify the bearer token cryptographically and validate issuer/audience/time claims.
2. Extract a stable provider subject and provider identifier.
3. Resolve that identity to a local Pandar user.
4. Authorize tenant access through Pandar-managed `users`/membership/role records.
5. Preserve tenant-owned tokens as service credentials for automation and non-browser clients.

Phase 10 implements this contract in `pandar-hub` with one configured external identity profile per hub process. The hub parses `PANDAR_EXTERNAL_AUTH_PROVIDER`, issuer, JWKS URL, required audience, RS-family algorithm allow-list, optional Clerk-style authorized parties, optional Logto-style required scopes, and clock leeway at startup. Partial external-auth configuration is a startup error.

The self-hosted Better Auth issuer is packaged as a sibling Next.js app named `pandar-auth`. It configures Better Auth's magic-link, passkey, and JWT plugins, exposes JWKS at `/api/auth/jwks`, exposes session JWT retrieval at `/api/auth/token`, and redirects successful sign-ins back to `pandar-web`'s callback with a `token` query parameter. The dashboard callback validates the Better Auth issuer/audience shape, sets the HTTP-only bearer cookie, and redirects to `/` by default. A Studio login may instead carry a versioned base64url `return_to`; both apps decode it fail-closed and accept only the same-origin `/plugin-sign-in` path and query before the dashboard callback redirects there. The return intent never contains the JWT. Every callback response uses `cache-control: no-store` and `referrer-policy: no-referrer`. Callback request logs and browser history can contain a bearer JWT and should be treated as sensitive. Email delivery is selected at deployment time through Resend or SMTP. `BETTER_AUTH_SECRET` signs Better Auth sessions and, by default, encrypts the persisted JWKS private key in the issuer database; rotating it without re-encrypting or clearing JWKS rows breaks issuer signing until the key material is repaired.

Tenant route authentication checks bearer credentials in this order:

1. Tenant-token lookup.
2. External JWT verification against cached JWKS when configured.
3. Local `{tenant_id, provider, subject}` lookup in `user_identities`.
4. Local tenant role check from the linked Pandar user.

JWT verification failures return `401 invalid_auth_token`. A cryptographically valid identity-provider token without a tenant-local identity link returns `403 tenant_forbidden`. Insufficient Pandar tenant role still returns `403 role_forbidden`.

### Bambu Studio Network Plugin ABI

Evidence from `reference/open-bamboo-networking` and `reference/BambuStudio`:

- Bambu Studio loads a dynamic network plugin and resolves exported `bambu_network_*` functions from `NetworkAgent.cpp`.
- The plugin ABI includes lifecycle, user login, server/cloud state, printer messaging, print dispatch, preset/settings, tracking, and `ft_*` file-transfer symbols.
- `reference/open-bamboo-networking/tests/probe_plugin.cpp` is a useful compatibility probe for required exports.
- The login dialog builds its target URL from `agent->get_bambulab_host() + "/sign-in"` and opens that page inside a Studio WebView.
- Login pages can send Studio script messages:
  - `user_login` with inline login info;
  - `user_ticket_login` with a ticket that Studio exchanges through plugin ABI;
  - `get_localhost_url`, which starts Studio's localhost HTTP server and returns `http://localhost:13618`;
  - `thirdparty_login` or `new_webpage`, which ask Studio to open a URL in the default browser.
- Studio's localhost login handler expects `ticket` and `redirect_url`, calls `agent->get_my_token(ticket)`, then `agent->get_my_profile(access_token)`, constructs login JSON, and calls `agent->change_user(login_info)`.
- Studio keeps sidebars and WebViews in sync by calling `build_login_cmd` / `build_login_info` and posting `studio_userlogin` or `studio_useroffline` envelopes.

Pandar's plugin design:

1. `pandar-network-plugin` starts a process-local loopback HTTP server and returns that server as the plugin host/sign-in URL.
2. The loopback server serves an embedded minimal sign-in/configuration page from `frontend/plugin-local/dist` through `rust-embed`.
3. The local page asks for the Pandar Web URL, discovers the Hub URL from that deployment's public `/.well-known/pandar` document, and links to the configured frontend `/plugin-sign-in` page.
4. The Pandar frontend completes Clerk, Logto, or Better Auth authentication and asks the hub to create a short-lived, one-use plugin login ticket for a selected tenant. Better Auth preserves the selected tenant and Studio localhost callback through its magic-link, optional passkey, and dashboard callback hops with the validated opaque return intent described above.
5. The page uses Studio's `get_localhost_url` flow and redirects the browser to Studio's local callback with `ticket` and `redirect_url`.
6. Studio calls the plugin's token/profile ABI. The plugin exchanges the ticket with `pandar-hub` for a tenant-owned plugin credential and returns Bambu-shaped token/profile JSON to Studio.
7. `change_user` stores enough session state for Studio login UI and future hub API calls.

Personal preset synchronization is a direct Studio plugin → Hub flow; `pandar-agent` is not involved. The Hub derives the owner from the live Operator user attached to the exact `plugin:studio` token, so no-auth sessions and tenant-wide token identities cannot access this surface. Process, Filament, and Printer presets use a complete owner catalogue plus owner-scoped full/create/replace/delete routes. SQLite and PostgreSQL persist the same schema behind `PersonalPresetRepository`, including a durable owner clock that supplies strictly increasing Studio timestamps. The plugin builds list results in temporary Rust state, publishes only a complete generation-current catalogue, drains it once into Studio's name-keyed map, and clears it on failure, cancellation, logout, account/configuration replacement, or destroy.

Rust owns typed account/persisted-login policy, session selection, Cloud subscriptions, virtual-local
generations, heartbeat scheduling and delivery eligibility, status projection, and the one-pass
firmware -> status -> semantic-operation -> unsupported message classifier. C++ is limited to ABI/STL
adaptation, callback invocation, and the synchronization those callbacks require. A status call
returns success only after an eligible current callback delivery; an ineligible or missing listener
returns `-2`.

Studio's LAN-shaped connect/message entrypoints form a Hub-backed virtual/local ABI proxy. The
authorized `dev_id` is the only target authority. Passed host/IP, username, password, and SSL values
are ignored and scrubbed, no direct printer socket is opened, and status/messages still flow through
Hub and Agent. Direct discovery, bind/unbind, certificate handling, MQTT, FTPS, and `ft_*` remain
unsupported in the plugin.

Studio identity and print ownership stay separated across boundaries. The plugin resolves Studio's
`dev_id` only to an authorized Hub printer and sends the Hub printer id internally; it never treats a
Studio device id, host, or credential as direct machine authority. Hub assigns and persists the
stable Studio submission/task ids, job/command state, plate index, and available subtask/slice
metadata in one backend-neutral transaction. Plugin callbacks describe admission, upload, durable
creation, dispatch, and the observed terminal Hub job only; HTTP acceptance is never physical print
start. Agent alone owns artifact transfer, Bambu MQTT command translation, and printer observations.

Deploy this alignment in the order equivalent SQLite/PostgreSQL migrations, Agent, Hub, then the
network plugin and same-target BambuSource companion. Before binary rollback, stop the plugin from
creating new Studio jobs or operations and drain or explicitly fail nonterminal Studio-owned commands
and jobs. Roll Hub back only after that ownership is terminal, then roll Agent back; leave additive
database columns in place. Replacing or rolling back the Studio files requires Studio to be stopped
and both original libraries to be restored together.

The plugin credential is tenant-owned and revocable. It is not a user-owned API token and does not receive `agent:register`.

## Target Components

### pandar-hub

- HTTP API for tenants, users, agents, printers, jobs, and printer commands.
- WebSocket API for live printer state and job progress. Agent status streams and hub-originated notification streams are future work.
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

Runtime printer linking is the exception to durable command replay: the Hub stores only a redacted `link_printer` command record and sends the Bambu access code directly to the currently connected local agent stream. If the local stream is unavailable, the request fails with `agent_not_connected`; in-flight live-only commands are failed by session/stale cleanup instead of replayed after restart.

Hub replicas use an explicit control-plane split:

- SQLite deployments run as a lightweight single Hub process with the in-process control plane and no broker.
- PostgreSQL deployments can run either one Hub process with the in-process control plane or multiple Hub replicas with NATS enabled through `PANDAR_CONTROL_PLANE=nats`, `PANDAR_NATS_URL`, and optional `PANDAR_NATS_SUBJECT`.
- NATS carries only internal Hub wake, close, and live-event fanout messages. Tenants, browsers, and `pandar-agent` continue to authenticate through Hub HTTP/WebSocket/gRPC APIs, and `pandar-agent` keeps the existing reverse gRPC connection.
- PostgreSQL remains the shared fact source for tenant, agent, command, job, printer, material, audit, plugin-ticket, tenant-token, and WebSocket-ticket state.
- Multi-Hub PostgreSQL plus NATS deployments should use the S3-compatible artifact storage backend. The filesystem backend is the SQLite/single-Hub default and requires an explicit shared-filesystem readiness override before it is accepted with PostgreSQL plus NATS.

Phase 4 adds tenant-scoped printer inventory and state:

- Agent `PrinterSnapshot` events are accepted only from the current live session token, persisted as latest printer state, and ignored from stale replaced streams.
- The printer repository stores latest name, serial, optional model, normalized status, owning agent, and last-seen time behind the backend-neutral SQLite/PostgreSQL boundary.
- `GET /api/v1/tenants/{tenant_id}/printers` and `GET /api/v1/tenants/{tenant_id}/printers/{printer_id}` expose tenant-scoped inventory.
- `POST /api/v1/tenants/{tenant_id}/agents/{agent_id}/refresh-printers` writes a durable refresh command before dispatching it to the live agent session.
- `GET /api/v1/tenants/{tenant_id}/printer-events` broadcasts future printer snapshot updates over an in-memory tenant WebSocket channel. It does not replay historical state; HTTP listing remains the initial-state source.

Phase 5 adds tenant-scoped print dispatch while preserving the durable command ledger:

- `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs` accepts multipart project artifact upload plus print options, writes the artifact through the configured artifact storage backend, then creates artifact metadata, a linked print command, and a job row in one backend-neutral SQLite/PostgreSQL transaction.
- `GET /api/v1/tenants/{tenant_id}/jobs` and `GET /api/v1/tenants/{tenant_id}/jobs/{job_id}` expose tenant-scoped job history and command status.
- `PANDAR_ARTIFACT_STORAGE=filesystem|s3` selects artifact byte storage. The filesystem backend uses `PANDAR_SPOOL_DIR`, defaults to `pandar-spool`, and is intended for SQLite/single-Hub deployments. The S3-compatible backend uses the `PANDAR_ARTIFACT_S3_*` settings and is intended for scaled PostgreSQL plus NATS deployments. `PANDAR_MAX_ARTIFACT_BYTES` controls upload size and defaults to `10485760`.
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

- `user_identities` links provider subjects such as Clerk, Logto, or Better Auth user ids to tenant-scoped Pandar users.
- The hub verifies RS256/RS384/RS512 JWTs through configured JWKS, issuer, required audience, expiration, optional not-before, optional authorized-party, and optional scope rules.
- HTTP tenant routes and `/printer-events` WebSocket authorization share the same API-token-first then external-JWT flow.
- The verifier caches JWKS and refreshes when a token references an unknown `kid`.
- Route tests use local RSA/JWKS fixtures and do not contact Clerk, Logto, or external JWKS endpoints.

Phase 31 adds external self-service tenant onboarding:

- `GET /api/v1/me` verifies an external JWT and returns profile claims plus tenant memberships without creating database rows.
- `POST /api/v1/onboarding/tenants` creates a new tenant when self-create is enabled and creates the caller's tenant-local admin projection atomically.
- Tenant admins create hash-stored join links with role, optional verified-email restriction, expiry, max uses, one-time plaintext token response, and audit records.
- `POST /api/v1/join-links/accept` verifies the caller's external account, checks the join link, creates the tenant-local user projection and identity link, assigns the join-link role, and preserves an existing member's current role without consuming a use.
- `pandar-web` can be configured for Clerk, Logto, or Better Auth provider metadata while the hub remains the authorization source.
- Phase 32 removed manual user creation and identity linking. Join links and external tenant self-create are the supported membership-entry paths; user/identity reads and tenant-local role updates remain available.

Phase 11 adds explicit provisioning and bootstrap boundaries:

- `PANDAR_BOOTSTRAP_TOKEN` is the only credential accepted by cross-tenant endpoints: `GET /api/v1/summary`, `GET /api/v1/tenants`, `POST /api/v1/tenants`, and `POST /api/v1/bootstrap/tenant-admin`.
- `POST /api/v1/bootstrap/tenant-admin` creates a tenant, tenant admin user, initial tenant token, and bootstrap audit events in one SQLite/PostgreSQL transaction. The plaintext token is returned once and only its hash is stored.
- Tenant admins can list users and linked identities, update local user roles, create/list/revoke tenant tokens, manage join links, and create agent pairing bundles through tenant-scoped APIs.
- Tenant-token revocation sets `tenant_tokens.revoked_at`; revoked tokens are excluded from bearer authentication.
- Provisioning actions are represented in `audit_events` with actions such as `tenant.bootstrap`, `tenant.create`, `user.create`, `user.role_update`, `tenant_token.create`, `tenant_token.revoke`, and `agent.pairing_bundle`; historical `user_identity.link` events remain readable.
- Agent pairing bundles return `PANDAR_TENANT_ID`, `PANDAR_AGENT_ID`, `PANDAR_AGENT_NAME`, and `PANDAR_AGENT_CREDENTIAL` for deployment.

Phase 16 replaced the Phase 6/11 user-owned API token model with tenant-owned tokens:

- `tenant_tokens` belong directly to a tenant, not to a user. User records remain for human identity, local roles, and audit actors.
- Token authorization comes from token `scopes`, not from the creator's current or historical user role.
- Empty `scopes` means read-only tenant API access, equivalent to viewer behavior.
- `["*"]` means all tenant-scoped API and agent-registration capabilities.
- `["agent:register"]` means the token can register or rotate agents but cannot read or mutate ordinary tenant API resources.
- `created_by_user_id` remains nullable audit metadata. Bootstrap or system-created tokens may have no creating user.
- Existing user-scoped API tokens no longer authenticate after the tenant-token migration.

Phase 17-20 add the product and operational surfaces over these foundations:

- Tenant admins can inspect users and identity links, manage roles, join links, tenant tokens, agent pairings, and recent audit events from the frontend. Admin API failures render a compact unavailable state.
- Operators can manually refresh printers, retry dispatch, reprint, duplicate jobs, and queue typed live printer controls while the UI keeps dispatch lifecycle wording separate from physical print state.
- `/readyz` and `/metrics` expose deployment readiness and Prometheus metrics with redaction and hashed tenant labels.
- `pandar cleanup` performs retention cleanup for terminal jobs, commands, machine events, audit rows, expired/used plugin tickets, revoked/expired tenant tokens, and unreferenced artifacts.
- Artifact upload UX shows selected file, size, multipart upload state, configured max size, and stable backend error codes. The hub still treats slicer files as opaque artifacts.

Phase 14 adds material-state persistence and reporting:

- The agent sends normalized material patch JSON on `PrintJobReport.printer_materials_json`. Empty strings are treated as no material update so older agents remain compatible.
- The hub owns tenant-scoped material state in `printer_material_snapshots`. It merges normalized AMS units, external spools, active tray evidence, and observed timestamps behind the backend-neutral repository boundary.
- Print job creation accepts optional `ams_mapping` and `ams_mapping2` arrays, persists omitted fields as `NULL`, persists present empty arrays as `[]`, and dispatches valid JSON to agents through `PrintProjectFile`.
- Terminal physical print reports derive `job_filament_usages` from persisted mappings plus the latest printer material snapshot. Mapping2 takes precedence per slot; external spool identity is canonicalized to `(external_id = "254", tray_id)`.
- HTTP printer responses expose response-safe `materials` summaries. HTTP job responses expose persisted mapping JSON plus derived filament usage rows. Corrupt persisted mapping JSON is a repository error with parse context, not a partially rendered response.
- Phase 14 deliberately does not add Spoolman, inventory purchasing, spool weight tracking, catalog sync, or any external material-inventory system. It establishes Pandar's internal material state first.

Phase 15 adds browser-safe live runtime UX:

- `POST /api/v1/tenants/{tenant_id}/printer-events/tickets` issues short-lived WebSocket tickets after normal tenant viewer authorization. Tickets are tenant-scoped, one-use, expire after 60 seconds, and are stored hashed in SQLite/PostgreSQL so horizontally scaled Hub replicas can validate tickets issued by a sibling replica.
- `GET /api/v1/tenants/{tenant_id}/printer-events` accepts either an `Authorization` bearer credential or a `ticket` query parameter. Header auth remains the non-browser path; ticket auth is for browser WebSocket clients that cannot set custom upgrade headers.
- The Next.js ticket route `POST /api/tenants/{tenantId}/printer-events/ticket` calls the hub server-side using the existing frontend credential precedence. The browser receives auth metadata and the opaque ticket only; it never receives `APP_API_TOKEN`, `APP_AUTH_BEARER_TOKEN`, or HttpOnly cookie token values.
- Fronting proxies and access logs should redact the `ticket` query parameter.
- The frontend runtime dashboard consumes authenticated `printer_snapshot` and `job_progress` events, merges them into the initial HTTP state, reconnects after 1s, 2s, 5s, and 10s, and shows the live channel as unavailable after 3 failures while retries continue.
- Notifications cover WebSocket subscription failure/disconnect plus future live transitions: printer offline, dispatch/job failure or error, physical print failed, and physical print completed. Historical replay and cancellation transitions do not notify.
- The dashboard now exposes live status, printer inventory, job history with artifact/material/progress details, operational notifications, and tenant setting/action references for agent pairing, tenant tokens, and diagnostics without rendering token values.

Bootstrap a fresh tenant:

```bash
curl -sS -X POST "$PANDAR_API/api/v1/bootstrap/tenant-admin" \
  -H "Authorization: Bearer $PANDAR_BOOTSTRAP_TOKEN" \
  -H "content-type: application/json" \
  -d '{"tenant_slug":"acme","tenant_display_name":"Acme","admin_email":"admin@example.com","admin_display_name":"Admin","api_token_name":"bootstrap-admin"}'
```

Tenant-admin membership inspection and role update examples:

```bash
curl -sS "$PANDAR_API/api/v1/tenants/$TENANT_ID/users" \
  -H "Authorization: Bearer $TENANT_ADMIN_TOKEN"

curl -sS -X PATCH "$PANDAR_API/api/v1/tenants/$TENANT_ID/users/$USER_ID/role" \
  -H "Authorization: Bearer $TENANT_ADMIN_TOKEN" \
  -H "content-type: application/json" \
  -d '{"role":"operator"}'
```

Agent pairing bundle example:

```bash
curl -sS -X POST "$PANDAR_API/api/v1/tenants/$TENANT_ID/agent-pairings" \
  -H "Authorization: Bearer $TENANT_ADMIN_TOKEN" \
  -H "content-type: application/json" \
  -d '{"name":"workshop-agent"}'
```

Hub phase status after Phase 7:

- Phase 8 keeps hub behavior mostly unchanged while the agent gains real FTPS upload; hub command/job status still records dispatch success or failure.
- Phase 9 added physical print reconciliation, persistent progress fields, normalized machine events, and tenant WebSocket job progress broadcasts.
- Phase 10 added Clerk/Logto-compatible JWT verification, provider-subject-to-local-user mapping, and Pandar-owned tenant membership checks for HTTP and WebSocket auth.
- Phase 11 added first-user/bootstrap, tenant user/token management, explicit bootstrap boundaries, provisioning audit events, and identity linking flows.
- Phase 12 completed the staged repository layer migration to SeaORM while preserving SQLite/PostgreSQL behavior and existing SQLx schema migrations.
- Phase 13 added structured command `result_json` persistence, tenant-scoped discovery/diagnostic command APIs, and command detail reads for frontend diagnostics. `result_json` is for structured agent output such as discovery rows and diagnostic checks; it must not contain Bambu access codes.
- Phase 14 added normalized AMS/external-spool material snapshots, persisted print mappings, derived filament usage rows, HTTP material responses, and dashboard material summaries. Spoolman-style external inventory remains out of scope.
- Phase 15 added one-use browser WebSocket tickets, live frontend printer/job event consumption, reconnect status, transition notifications, and token-safe tenant operation references.

Phase 16-21 hub status:

- Phase 16 added tenant-owned scoped tokens, agent enrollment credentials, gRPC agent credential authentication, token rotation/revocation, plugin login tickets, and plugin-scoped tenant credentials.
- Phase 17 exposed tenant users, identity links, scoped tenant tokens, agent pairings, and audit events in the product UI.
- Phase 18 added retry/reprint/duplicate recovery APIs and UI controls. Phase 27 adds typed, compatibility-gated pause/resume/stop/print-speed dispatch commands without treating dispatch success as a physical printer-state change.
- Phase 19 added readiness checks, Prometheus metrics, redaction coverage, retention cleanup, and cleanup CLI behavior.
- Phase 20 improved artifact upload UX and duplicate/reprint flows without adding slicer-file parsing.
- Phase 21 added the `pandar-network-plugin` ABI shim crate and export-list verification.

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
- The 30-second MQTT protocol keep-alive, 10-second idle report receive timeout, 15-second Agent-to-Hub heartbeat, and fixed 60-second full-status request are independent. Each report-forwarding session sends the existing `pushall` request immediately and then every 60 seconds with `MissedTickBehavior::Delay`; its retry-owned timer is discarded and recreated after a forwarding failure. A scheduled publish alone emits no event and never proves printer presence: only a qualifying real MQTT report can reach the existing Hub presence paths.
- The runtime agent applies one Bambu-specific TLS verifier to MQTT, FTPS, and BRTC. It requires the certificate CN to equal the configured serial, verifies X.509 v3 chains against Studio's bundled Bambu printer CA set without requiring a DNS/IP SAN, and keeps TLS handshake signature verification through rustls' AWS-LC algorithms. Because rustls-webpki rejects X.509 v1 leaves, the verifier narrowly validates those leaves itself: the leaf must be directly signed by one of the bundled trusted Bambu CAs with RSA/SHA-256, use matching inner and outer signature algorithms, match the configured serial, and be within its validity period. Leaf-only certificates whose issuer is absent from the bundle still require an operator-configured per-serial SHA-256 leaf pin through `PANDAR_BAMBU_CERTIFICATE_SHA256_PINS`; pin matching and certificate validity are fail-closed. This policy is isolated to agent-to-printer LAN transports and does not apply to hub-facing HTTP/gRPC TLS.
- Some printers, including X2D, use the MQTT TLS certificate common name as the `device/{id}/...` topic identity while exposing a different inventory serial. The runtime MQTT transport resolves and caches that certificate identity before its first subscribe or publish, rewrites only MQTT topics, and keeps the inventory serial unchanged in Hub snapshots and commands. Background report forwarding retries transient identity-probe or subscribe failures.
- `RefreshPrinters` sends an accepted command ack, publishes `{"info":{"command":"get_version","sequence_id":"90002"}}`, discovers the model from the `ota` module `product_name`, then publishes `{"pushing":{"command":"pushall"}}`, waits for one state report with a bounded timeout, emits normalized `PrinterSnapshot` events, then emits a success or failed command result.
- MQTT report normalization uses serial/name from config and reads state from `print.gcode_state`, `print.state`, or root `state`, falling back to `unknown`. Refresh snapshot model values come from MQTT `get_version`; discovery failure fails the refresh instead of falling back to configured `model`.
- The MQTT runtime adapter is isolated in `pandar-agent`; tests use fake transports and do not open live broker connections.
- Machine file transfer is modeled as a protocol-neutral boundary derived from the reference FTPS behavior: implicit TLS port `990`, username `bblp`, 64 KiB upload chunks, list/download/upload/delete requests, and protected data mode (`PROT P`) for every model. Failure never downgrades the data channel to `PROT C`.
- Phase 3 does not change hub persistence. The hub still receives normalized agent events over the existing gRPC stream.

Phase 4 carries refreshed printer snapshots into hub inventory. `RefreshPrinters` remains the explicit snapshot path: empty printer config stays no-network, configured printers discover model with `info.get_version`, publish `pushall`, normalize one state report, and the hub persists the latest state plus broadcasts a tenant event.

Phase 5 adds the `PrintProjectFile` command executor:

- `PrintProjectFile` carries `artifact_download_path`; agents download artifact bytes from Hub HTTP with their agent credential. `PANDAR_HUB_API_URL` configures that HTTP base URL when the gRPC URL is not HTTP(S).
- `PANDAR_ARTIFACT_ROOT` controls the legacy local artifact reader for commands that do not include `artifact_download_path` and defaults to the current directory.
- The agent validates the requested Bambu serial against configured printers before resolving or reading artifact paths.
- Artifact storage paths must be relative paths below `PANDAR_ARTIFACT_ROOT`; absolute paths, `..`, and prefix escapes are rejected.
- The configured machine gateway composes machine file upload and MQTT `project_file` publish in order. It uploads the artifact filename through the file-transfer boundary, then publishes to `device/{serial}/request` with QoS `1`, `ftp://{filename}`, `Metadata/plate_{plate_id}.gcode`, job/subtask ids, and print flags.
- Unit tests use fake file-transfer and MQTT transports to prove upload-before-publish behavior and no-publish-on-upload-failure behavior without opening real Bambu sockets.
- Configured runtime agents use the Bambu FTPS adapter for machine file upload. The adapter uses implicit FTPS on port `990`, the Bambu LAN TLS policy for printer-local/self-signed certificates, protected data mode only, and server-side size verification before MQTT `project_file` publish. Tests use fake transfer transports and do not open live Bambu sockets.

Agent phase status after Phase 7:

- Phase 8 added the real file-transfer runtime with implicit FTPS on port `990`, post-upload size verification, model/profile transport policy, and actionable upload diagnostics.
- Phase 9 converts continuous MQTT reports into normalized job progress and terminal print events that can be correlated to hub jobs. Configured printers use a separate report MQTT client id from command transport so long-running subscriptions do not collide with command sessions.
- Phase 13 adds LAN discovery, credential validation, printer diagnostics, and centralized compatibility rules for feature availability and transport policy. Discovery uses agent-local multicast SSDP plus bounded private-subnet unicast SSDP and can run without configured printers. Diagnostics run only for agent-configured serials, keep access codes agent-local, and return structured checks for MQTT reachability/report flow, FTPS reachability/storage probe, configured-printer status, and compatibility. Expected printer or environment problems are represented as successful command results with `overall = "problem"` instead of failed hub commands.
- Phase 14 promotes AMS, external-spool, tray-change, and filament usage data into stable Pandar models. The agent remains responsible for Bambu MQTT shape normalization, credential-key filtering, `tray_exist_bits` cleanup, `vir_slot`/`vt_tray` external-spool handling, active-tray derivation, and Bambu MQTT `ams_mapping_2` spelling. It does not persist material inventory locally.

Agent phase status after Phase 15:

- Phase 16 replaced manual `PANDAR_TENANT_ID`/`PANDAR_AGENT_ID` trust with authenticated enrollment credentials issued by tenant tokens carrying `agent:register` or `*` scope.
- Phase 18 made dispatch retry/reprint/duplicate explicit through hub recovery APIs. Phase 29 routes pause/resume/stop/print-speed/home/move/hotend through typed `printer_operation` commands gated by Hub compatibility checks; these commands are dispatch lifecycle records, not direct physical status mutations. Hub stores semantic operations only, while `pandar-agent` performs Bambu-specific MQTT/G-code translation.
- Phase 19 exposes hub-side readiness evidence and redacts Bambu access codes, agent credentials, plugin tickets, WebSocket tickets, bearer tokens, and artifact paths.

### pandar-network-plugin

- Dynamic-library crate implementing separately built binaries for the six reviewed ABI series from
  `02.06.00` through `02.08.01`. The shared ABI-series catalog drives build selection, release
  matrices, installer matching, Windows hook version gates, and ABI/release-smoke expectations.
- File Transfer, direct discovery/bind/certificate handling, and direct printer sockets keep their
  exact ABI shape but remain explicit non-success paths.
- Connects only to `pandar-hub` through HTTP/WebSocket APIs.
- Uses Bambu Studio's existing login WebView, local callback, token/profile ABI, and `change_user` flow. The plugin host is a minimal loopback webserver that serves the embedded local sign-in/config page and keeps the hub URL selectable before token exchange.
- Presents Hub-backed printer state and Hub-backed authorized/paginated task history to Bambu Studio;
  `get_user_tasks` no longer returns a synthetic empty success.
- Submits print actions to the hub; the hub dispatches through authenticated agents.
- Does not store Bambu access codes, open printer MQTT/FTPS/SFTP sockets, or call agents directly.
- Rust owns account, session/selection/subscription, heartbeat, status, and message-classification
  policy. The C++ shim owns only ABI/STL adaptation, callback invocation, and required synchronization.
- Development no-auth issuance is an exactly-one-tenant transaction. SQLite uses an immediate write
  transaction and PostgreSQL takes the corresponding tenant-table lock before counting at most two
  ordered tenants. Zero or multiple tenants return stable non-credential outcomes; only one tenant can
  atomically create the scoped `plugin:studio` token and its redacted create audit.
- Startup no-auth recovery is a serialized Rust state machine, not C++ heartbeat policy. It retries
  only a connection failure proven to occur before HTTP delivery, with a bounded generation/account-
  epoch key and one in-flight attempt. Hub/account/configuration/token changes, logout, and destroy
  fence captured work; post-response admission prevents a stale credential from becoming current.
- Login, pending-revocation, direct-revocation, and completed-revocation mutations are serialized both
  in-process and across Studio processes by the config-directory
  `.pandar-plugin-account.lock`. Atomic replacement/removal is followed by parent-directory
  confirmation. `Confirmed` is the only outcome that may admit a login candidate or authorize a
  pending/direct `DELETE`; `ChangedUnconfirmed` means the namespace change was published but its
  directory durability could not be confirmed and therefore fails closed. An ordinary persistence
  error leaves the current canonical namespace unchanged, but is not a claim that a rollback would
  survive a crash before its directory metadata reaches stable storage.
- Requested logout and passive account loss share a linearizable transition coordinator. Passive loss
  clears local state without revoking the Hub token, but a concurrent requested logout upgrades the
  same transition before finalization. Requested logout first seeks a `Confirmed` entry in
  `pandar-plugin-pending-revocations.json`; if that cannot be established, it must establish a
  `Confirmed` `pandar-plugin-direct-revocation.json` intent before calling the tenant-scoped
  `DELETE /api/v1/plugin/session`. A passive transition upgraded after clearing runtime state retains
  the complete account snapshot and restores it on an ordinary pre-DELETE preparation failure;
  changed-unconfirmed intent fails closed without issuing DELETE. Staged or direct remote failure
  remains replayable, `401`/`410` is idempotent success, callbacks run outside account locks, and one
  Hub token can produce at most one redacted revoke audit on either SQLite or PostgreSQL.
- Successful revocation first records `{hub_url, token_sha256}` in the unbounded
  `pandar-plugin-completed-revocations.json` ledger. Loading or storing a matching login consults that
  ledger, pending state, and direct intent so a stale Studio process cannot resurrect a revoked token.
  Direct completion removes any duplicate pending entry on a best-effort, idempotent basis; a cleanup
  failure cannot turn an already successful Hub DELETE into a failed revocation. The completed ledger
  has no automatic compaction because its entries are stale-write tombstones; operators may clear it
  only after all Studio processes using the directory are stopped and every corresponding Hub plugin
  session is revoked, invalid, or expired.
- Printer-list and task list/plate/subtask authorization recovery share the current account snapshot.
  A no-auth `401`/`410` joins one credential rotation and retries the original request exactly once;
  stale account/configuration responses fail closed, while authenticated credentials never fall back
  to development no-auth. Firmware catalog/version/send requests likewise carry one frozen request
  snapshot and claimed firmware generation through admission, HTTP, post-check, and callback delivery;
  the C++ shim never rereads live generation or owns a `last_error` side channel.
- `pandar-bambu-source` is a separate companion required by the pinned Studio startup gate. It
  exports the sentinel plus the exact 21-entry local-media ABI and accepts only a random one-use
  loopback MJPEG relay URL from the network plugin. The relay keeps the Hub bearer internal; printer
  host/access-code credentials remain Agent-local. Only normalized A1, A1 Mini, P1S, and A2L printers
  with an online current capable Agent are eligible. The companion has no cloud/TUTK/Agora,
  recording, discovery, MQTT, FTPS, or direct printer transport behavior.
- Phase 23 automated probes and native release-smoke remain separate from real Studio evidence. Exact
  Studio and hardware claims are gated by the compatibility manifest.

### pandar-core

- IDs and domain records: tenant, user, agent, printer, job, command.
- Shared event and command enums for hub-agent gRPC.
- Normalized printer state models.
- Error types that preserve lower-level causes.

### frontend

The dashboard (Devices, Jobs, Agents, Users, Settings) runs as server-component route pages under
`frontend/app/(dashboard)/` with per-route `loading.tsx` skeletons and a shared client shell
(`DashboardShellProvider`/`DashboardShellLayout`). Server pages fetch shared data through
`dashboard-data.tsx`, and all hub traffic crosses the server-side proxy (`hub-proxy.ts` and the
`frontend/app/api/tenants/[tenantId]/` routes); browser code never calls the hub directly. Bearer
forwarding follows the credential precedence: request cookie `APP_AUTH_COOKIE_NAME` defaulting to
`pandar_auth_token`, then `APP_AUTH_BEARER_TOKEN`, then `APP_API_TOKEN`. When `APP_TENANT_ID`
binds the deployment to one tenant, dashboard reads stay on tenant-scoped APIs.

Current product surfaces:

- The selected tenant resolves from the `pandar.tenant` cookie with first-effective-tenant fallback; the sidebar switcher writes the cookie and re-renders the current view, server actions receive their target tenant through hidden form fields, and navigation hrefs stay tenant-free.
- Live updates consume ticket-authenticated tenant printer-event WebSockets socket-first; REST supplies authoritative baselines, and enriched print/build-plate state repairs on a serialized cadence.
- Printer controls declare typed control intents in `frontend/app/printer-controls.tsx`. Mutations return typed action states rendered as in-place pending feedback and localized toasts instead of status redirects. Dashboard health/attention derives from shared `dashboard-attention.ts` helpers feeding the devices/jobs/agents pages.
- Devices cards expose cooling-fan telemetry and presets, print-speed modes, HMS messages with localized Bambu detail links, AMS drying control, H2C nozzle-rack state and operations, and a camera viewer with fullscreen and native picture-in-picture that lives in the persistent layout so streams survive navigation.
- Settings provides audited workspace rename, token management, agent enrollment, audit panels, and scrollspy section navigation. User locale/theme preferences persist through zustand under the `pandar.settings` key.
- Tenant administration covers users, roles, identity links, scoped tenant tokens, agent pairing/enrollment, and audit events without rendering token values.

`frontend/auth` is the separate self-hosted Better Auth issuer app with its own UI primitives.
`mobile/android` is the Jetpack Compose Android client speaking the same Hub HTTP/WebSocket APIs
(see `docs/android.md`).

Remaining frontend limitations:

- Live printer controls are typed, compatibility-gated dispatch actions. Physical control behavior must still be proven by the hardware evidence gates tracked in `docs/roadmap.md`.
- Production proxies still need request body limits aligned with `PANDAR_MAX_ARTIFACT_BYTES`.

## Data Model Draft

- `tenants`: tenant identity and display metadata.
- `users`: tenant users and role assignments.
- `user_identities`: external identity links such as `{provider, subject, user_id}` for Clerk/Logto users.
- `tenant_memberships`: local user-to-tenant role assignments owned by Pandar.
- `tenant_tokens`: tenant-owned bearer credentials with hashed token values, scopes, nullable creator audit metadata, last-used timestamps, expiry, and revocation state. These replace user-owned API tokens after Phase 16.
- `plugin_login_tickets`: short-lived, one-use hub-issued tickets for Bambu Studio network plugin login. These are exchanged through the plugin ABI for tenant-owned plugin credentials and should expire quickly.
- plugin-scoped `tenant_tokens`: revocable `["plugin:studio"]` credentials created from login tickets for Bambu Studio plugin access.
- `agents`: reverse-connection identity, tenant binding, last seen time, version.
- `printers`: tenant, agent, serial number, name, model, network endpoint metadata, active flag.
- `printer_credentials`: agent-visible encrypted access code material or agent-local credential references.
- `printer_state_snapshots`: latest normalized state and raw state pointer for diagnostics.
- `jobs`: user-requested print jobs and dispatch metadata.
- `job_artifacts`: uploaded 3MF/G-code metadata and storage location.
- `printer_material_snapshots`: latest tenant-scoped normalized AMS/external-spool material state per printer.
- `job_filament_usages`: derived print-time mapping rows for terminal physical jobs.
- `commands`: durable hub-to-agent command ledger.
- `machine_events`: normalized printer and agent events for audit/debug history.

Credentials should not be sent to frontend clients. Prefer keeping printer access codes agent-local when possible; if hub storage is required, encrypt at rest and scope access by tenant and agent.

Identity-provider tokens should not define tenant access by themselves. The frontend may obtain tokens from Clerk or Logto, but Rust must validate the token and then authorize through Pandar-managed local user and tenant membership records.

## Protocol Boundaries

Hub-agent gRPC should carry normalized commands and events, not raw MQTT as the default API. Raw MQTT capture can exist as a diagnostics feature gated by authorization.

Agent-machine MQTT should be encapsulated behind a trait such as `MachineControlTransport`.

Agent-machine file transfer should be encapsulated behind a trait such as `MachineFileTransfer`.

Bambu Studio plugin traffic should be encapsulated in `pandar-network-plugin` and terminate at `pandar-hub`. The plugin is an adapter for Studio's ABI and UI expectations, not a second agent runtime.

Model-specific printer behavior should be encapsulated in the agent compatibility matrix. FTPS TLS/profile decisions, print option gates, diagnostics, and frontend availability should all consume the same conservative capability output; protected FTPS data channels remain a global security policy rather than a model capability. Unknown capability means unavailable in user-facing controls unless a future reference-backed phase upgrades it.

Hub persistence should be encapsulated behind repositories that are tested against SQLite and PostgreSQL.

Material-state semantics should stay split by boundary: raw Bambu report parsing in `pandar-agent`, tenant-scoped merge/persistence and usage derivation in `pandar-hub`, and response-safe summaries in `frontend`. External inventory systems such as Spoolman require a future explicit integration phase.

## Open Questions

The previously open questions are resolved in the implementation: machine file transfer is protected implicit FTPS behind the protocol-neutral `MachineFileTransfer` boundary; access codes stay agent-local by default with optional encrypted Hub storage (`PANDAR_PRINTER_ACCESS_CODE_KEY`); the first compatibility targets are catalogued per Studio ABI series in `docs/compatibility/`; SeaORM repositories run over SQLx-managed migrations; Bambu cloud integration is limited to the Hub-backed Studio virtual-local proxy with cloud settings and AMS cloud sync explicitly unsupported; and virtual-printer/proxy behavior remains out of scope pending a roadmap decision. Active next steps are tracked in `docs/roadmap.md`.
