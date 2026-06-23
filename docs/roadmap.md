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
- `reference/open-bamboo-networking` documents and implements the Bambu Studio network plugin ABI surface, including the `bambu_network_*` and `ft_*` dynamic-library exports that a compatible replacement must provide.
- `reference/BambuStudio` drives login through the network plugin ABI: Studio opens `agent->get_bambulab_host() + "/sign-in"` in a WebView, accepts page messages such as `user_login`, `user_ticket_login`, `get_localhost_url`, and `thirdparty_login`, starts its own localhost HTTP server on port `13618`, then calls plugin token/profile ABI methods before applying `change_user(login_info)`.

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
- Added Phase 10 external identity authentication with local `user_identities`, Clerk/Logto-compatible JWT verification through configured JWKS, API-token-first tenant route auth, local tenant role enforcement, local JWKS route tests, and frontend bearer forwarding from request cookies/static tokens.
- Added Phase 11 provisioning/admin boundaries with bootstrap-only cross-tenant APIs, atomic tenant-admin bootstrap, tenant-admin user/token/identity management, API-token revocation, provisioning audit events, agent pairing bundles, and tenant-bound frontend reads.
- Added Phase 12 full SeaORM repository migration coverage for auth, audit, agents, printers, commands, jobs, print reports, machine events, and documented the remaining atomic printer snapshot SQLx adapter.
- Added Phase 13 LAN discovery, printer diagnostics, structured command result persistence, conservative compatibility matrix ownership, hub diagnostic APIs, and frontend diagnostic result rendering.
- Added Phase 14 AMS/external-spool material normalization, tenant-scoped material snapshots, print mapping persistence/dispatch, terminal filament usage derivation, HTTP material responses, and dashboard material summaries.
- Added Phase 15 browser-safe WebSocket tickets, live runtime dashboard event consumption, reconnect status, transition notifications, and token-safe tenant operation references.
- Added Phase 16 tenant-owned token repository/routes, scoped tenant-token bearer authorization, retired user API-token routes, and bootstrap tenant-token issuance.
- Added Nix flake packaging for `pandar-hub`, `pandar-agent`, `pandar-cli`, `pandar-network-plugin`, `pandar-web`, checks, formatter, and development shell; `pandar-cli` installs the unified `pandar hub` / `pandar agent` Rust entrypoint while the frontend remains `pandar-web`.
- Split Nix packaging into a flake-parts root module and `nix/pandar.nix` so package, check, formatter, and dev shell logic stays outside the top-level flake.
- Added a NixOS module exposed as `nixosModules.default` / `nixosModules.pandar` to run `pandar-hub` and `pandar-web` with configurable bind addresses, packages, URLs, and environment.
- Extended the NixOS module with an optional `pandar-agent` systemd service, including hub gRPC URL, identity, credential, printers, artifact root, environment file, and package overrides.
- Generated `services.pandar` NixOS option documentation under `docs/deployment/nixos/options.md` and linked it from the README.
- Added GitHub Actions CI to run `nix flake check --show-trace` on pushes to `main` and pull requests.
- Added Mic92/hestia-backed GitHub Actions caching for Nix flake checks, with a scheduled cache GC workflow.

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
- Completed browser WebSocket consumption later in Phase 15 after authentication and tenant selection were stronger.

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
- Frontend users can see physical progress and terminal failure/success reasons for tenant jobs from HTTP job history. Browser live WebSocket consumption is completed in Phase 15; the authenticated hub `job_progress` WebSocket event already exists and is tested.

## Phase 10: External Identity Authentication

Goal: let users sign in with Clerk or Logto while keeping Pandar's tenant membership and role model in Rust.

- Completed equivalent SQLite and PostgreSQL `user_identities` migrations for external provider subject links.
- Completed repository methods for linking and resolving external identities to existing tenant-scoped Pandar users.
- Completed a provider-neutral OIDC/JWT verifier in `pandar-hub` for HTTP and WebSocket bearer tokens.
- Completed Clerk and Logto support through configuration, not provider-specific authorization logic:
  - issuer URL
  - JWKS URL
  - expected audience/API resource
  - accepted algorithms
  - optional authorized parties/origins for Clerk-style session tokens
  - optional scope checks for Logto API-resource tokens
- Completed token validation for signature, `iss`, `aud`, `exp`, optional `nbf`, allowed algorithms, `kid`, optional `azp`, optional scopes, and provider subject.
- Completed API-token-first route authentication so Phase 6 service/automation tokens remain valid when external identity auth is configured.
- Completed local Pandar tenant role enforcement for linked external identities; provider organizations are not trusted as tenant authorization.
- Completed frontend auth integration points so server components/actions forward request-cookie bearer tokens, `APP_AUTH_BEARER_TOKEN`, or `APP_API_TOKEN` to the Rust API.
- Completed tests with local JWKS fixtures for valid token, unknown key, bad issuer, bad audience, expired token, missing membership, insufficient tenant role, print job authorization, and WebSocket authorization.

Exit criteria:

- A signed-in Clerk or Logto user can call tenant-scoped APIs only when Rust has a matching local user and tenant membership.
- A valid identity-provider token without Pandar tenant membership is authenticated but not authorized.
- Tenant role decisions are fully enforced by Pandar repositories and are independent of provider-side organization membership.
- Existing API-token auth remains available for non-browser automation.

## Phase 11: Provisioning And Admin Boundaries

Goal: remove development-only tenant/token ergonomics before production multi-tenant exposure.

- Completed bootstrap-token protection for cross-tenant summary, tenant listing, and tenant creation endpoints using `PANDAR_BOOTSTRAP_TOKEN`.
- Completed first-user/bootstrap flow for creating a tenant, tenant admin, initial API token, and bootstrap audit events in one SQLite/PostgreSQL transaction.
- Completed user invite/linking APIs that bind a verified Clerk/Logto subject to a local Pandar user.
- Completed tenant-scoped user and token management APIs for tenant admins, including role updates and API-token revocation.
- Completed explicit bootstrap authorization for cross-tenant summary/listing endpoints.
- Completed audit coverage for provisioning, token creation/revocation, role changes, and agent pairing actions.
- Completed agent pairing bundle flow that avoids hand-copying persistent database IDs from separate responses, and documented the future token-rotation protocol.
- Completed frontend tenant-bound dashboard reads so `APP_TENANT_ID` deployments do not require cross-tenant bootstrap authority for normal tenant views.

Exit criteria:

- Completed: a fresh deployment can be bootstrapped through documented APIs without test fixtures.
- Completed: tenant users cannot list or summarize other tenants unless they hold the explicit bootstrap authority.
- Completed: provisioning actions are represented in audit events.

## Phase 12: Complete SeaORM Repository Migration

Goal: finish the staged SeaORM 2.0 migration without changing external repository behavior.

- Implemented auth, identity, membership, and audit repository migration.
- Implemented agents/printers migration while preserving live-session and printer snapshot semantics.
- Implemented command/job/artifact repository migration and transaction coupling.
- Completed SQLx escape-hatch audit: repository raw SQL is limited to `crates/pandar-hub/src/repositories/adapters/printers.rs`.
- Kept SQLx migrations as the schema source until there is a separate, explicit decision to adopt SeaORM migrations.
- Maintained SQLite and PostgreSQL parity tests for migrated repository behavior, including the printer snapshot adapter.
- Completed final SDD implementation review and full verification.

Exit criteria:

- Completed: all persistent repository operations use SeaORM query/entity APIs or the explicitly documented printer snapshot upsert adapter.
- Completed: SQLite and PostgreSQL test coverage covers repository behavior and transaction coupling.
- Completed: no mixed SQLx/SeaORM behavior drift remains outside connection/migration plumbing, tests, and the documented adapter.

## Phase 13: Discovery, Diagnostics, And Compatibility Matrix

Goal: make real printer operation debuggable across Bambu printer families.

- Completed agent-side LAN discovery from the reference SSDP behavior on multicast `239.255.255.250:2021`.
- Completed structured diagnostics for configured-printer validation, MQTT reachability/report flow, FTPS reachability, storage write probe, and compatibility.
- Completed command `result_json` persistence and tenant-scoped command detail reads for structured discovery/diagnostic output.
- Completed hub APIs for discovery and diagnostics with operator authorization, tenant scoping, audit events, and wake-agent dispatch.
- Completed a centralized conservative compatibility matrix for model aliases, FTPS TLS/profile policy, clear-data fallback, external storage, and feature availability.
- Completed print-time rejection for unsupported or unknown flow calibration before artifact upload.
- Completed frontend linked-agent controls and command result rendering for discovery rows, diagnostic checks, and compatibility availability without Bambu access-code inputs.

Exit criteria:

- Completed: operators can discover local printers, validate configured credentials indirectly, and see actionable diagnostics before dispatching a print.
- Completed: expected printer or environment problems are successful diagnostic command results with `overall = "problem"` instead of failed hub commands.
- Completed: compatibility rules are centralized and referenced by print command building, FTPS runtime policy, diagnostics, and UI availability.
- Completed: Bambu access codes remain agent-local and are not accepted by hub diagnostic APIs or frontend forms.

## Phase 14: AMS, Filament, And Spool Operations

Goal: promote AMS/external-spool data from raw report details into first-class tenant-visible state.

- Completed agent-side normalization for AMS units, tray IDs, external spool identifiers, active tray, filament type/color/material fields, remaining estimates, credential filtering, and Bambu mapping payloads.
- Completed SQLite/PostgreSQL migrations, SeaORM entities, and repositories for tenant-scoped material snapshots plus derived job filament usage rows.
- Completed preservation of `ams_mapping` and `ams_mapping2` semantics used by `project_file` commands, including strict API shape validation, null-vs-empty persistence, external spool canonicalization, and dispatch to agents.
- Completed terminal job filament usage derivation from persisted mappings and the latest material snapshot with clear `mapped_no_quantity` uncertainty boundaries.
- Completed printer/job HTTP response shapes and frontend dashboard rendering for material summaries and job material rows.
- Kept Spoolman-style external inventory, spool weight tracking, catalog sync, and purchasing out of scope until Pandar's internal state model is stable.

Exit criteria:

- Completed: the printer view exposes current AMS/external-spool state without raw MQTT payload knowledge.
- Completed: print dispatch can persist and show the mapping used for each job.
- Completed: filament usage can be derived from completed or failed jobs with clear uncertainty boundaries.

## Phase 15: Product Runtime UX And Notifications

Goal: turn the operational skeleton into a usable day-to-day cloud replacement surface.

- Completed hub-issued WebSocket tickets:
  - `POST /api/v1/tenants/{tenant_id}/printer-events/tickets` issues tenant-scoped viewer tickets.
  - Tickets are one-use, expire after 60 seconds, and are stored hashed in SQLite/PostgreSQL so sibling Hub replicas can consume them.
  - `GET /api/v1/tenants/{tenant_id}/printer-events` accepts either `Authorization` bearer auth or `ticket` query auth.
- Completed browser-safe ticket bridging through `POST /api/tenants/{tenantId}/printer-events/ticket`; browser code receives auth metadata and opaque tickets only, not `APP_API_TOKEN`, `APP_AUTH_BEARER_TOKEN`, or HttpOnly cookie token values.
- Completed authenticated frontend consumption of `printer_snapshot` and `job_progress` events with live state merging and reconnect delays of 1s, 2s, 5s, and 10s. The UI marks the channel unavailable after 3 failures while continuing retries.
- Completed focused operator notifications for live connection loss and future live transitions:
  - WebSocket subscription failure or disconnect
  - printer offline
  - dispatch/job failure or error
  - physical print failed
  - physical print completed
- Excluded cancellation notifications and historical replay notifications.
- Completed job history/detail improvements for dispatch status, physical progress, artifact details, material details, and command references.
- Completed tenant operation references for agent pairing, API token management, and diagnostics without rendering token values.

Exit criteria:

- Completed: common print monitoring workflows can be performed without refreshing the page.
- Completed: notification and job detail surfaces distinguish hub dispatch/job errors from physical print failures and completions.

## Phase 16: Tenant Tokens And Agent Enrollment

Goal: replace the user-scoped API token model with tenant-owned tokens that can authorize API access and agent registration while preserving the outbound reverse-connection model.

- Completed a new `tenant_tokens` model owned directly by `tenant_id`, not by `user_id`.
- Completed replacing user-scoped API tokens for bearer API authentication; user records remain for human identity and role management, not token ownership.
- Completed hash-only tenant token storage with plaintext token values returned once on creation and rotation.
- Completed `scopes` as the sole token capability source:
  - empty `scopes` means read-only tenant access, equivalent to viewer behavior;
  - `["*"]` means all tenant-scoped API and agent-registration capabilities;
  - `["agent:register"]` means the token can register or rotate agents but cannot read or mutate ordinary tenant API resources.
- Completed nullable `created_by_user_id` audit metadata. Token authorization does not inherit the creating user's role, and later user role changes do not change token capability.
- Completed tenant-token creation, listing, revocation, rotation, and last-used tracking APIs for tenant admins.
- Use tenant tokens with `agent:register` or `*` scope to issue or rotate agent enrollment credentials.
- Require `pandar-agent` to authenticate the reverse gRPC stream with a tenant-scoped agent credential instead of only `PANDAR_TENANT_ID` and `PANDAR_AGENT_ID`.
- Persist only hashed agent credentials in the hub and return plaintext agent credentials once.
- Bind gRPC command, heartbeat, snapshot, print report, and command result updates to the authenticated agent identity.
- Preserve stale-session protection and replacement-session behavior from Phase 2.
- Updated deployment docs so API automation and agent pairing use tenant tokens instead of user-owned API tokens or long-lived bootstrap credentials.

Exit criteria:

- Existing user-scoped API tokens are no longer accepted for bearer API authentication after the migration.
- A tenant can own multiple active tenant tokens with independent scopes, revocation, rotation, and audit metadata.
- Empty-scope tenant tokens can read tenant resources but cannot mutate them.
- `*` tenant tokens can perform all tenant-scoped operations.
- `agent:register` tenant tokens can register or rotate agents but cannot access ordinary tenant API resources.
- A fresh agent can be enrolled through tenant-token-authorized pairing and connect without manual database identifiers.
- Revoked or rotated tenant tokens and agent credentials cannot open or mutate protected sessions.
- Existing command dispatch and printer/job report tests prove authenticated agent identity is enforced.

## Phase 17: Tenant Admin Product UI

Goal: turn the existing provisioning APIs into a usable tenant-admin surface without moving authorization decisions out of Rust.

- Completed frontend tenant administration for users, roles, external identity links, tenant tokens, agent pairing, linked agents, and recent audit events.
- Kept Clerk/Logto as authentication providers only; tenant membership and roles remain Pandar-owned data.
- Completed copy-once handling by discarding plaintext token/pairing responses in browser UI state and avoiding local storage, cookies, and Zustand for secrets.
- Completed compact unavailable rendering when admin resources cannot be read by the current auth context.
- Kept bootstrap-only cross-tenant APIs separate from ordinary tenant-admin UI.

Exit criteria:

- A tenant admin can onboard an operator or viewer, link a Clerk/Logto subject, issue/revoke scoped tenant tokens, and pair an agent from the product UI.
- The UI never displays stored secret values after creation.
- Viewer/operator roles cannot access tenant-admin screens or mutations.

## Phase 18: Command Controls And Recovery UX

Goal: make day-to-day printer operations recoverable from the UI when dispatch or machine state changes unexpectedly.

- Completed tenant-authorized refresh, retry dispatch, reprint, and duplicate-and-print controls.
- Kept pause, resume, and stop unavailable because the underlying live printer control path is not implemented yet.
- Show command state transitions and latest structured result details inline with the affected printer or job.
- Added safe retry affordances for failed dispatch/upload/MQTT operations without creating duplicate physical prints accidentally.
- Kept raw Bambu commands behind diagnostics/admin boundaries; normal operators use typed controls.

Exit criteria:

- Operators can recover common failed or stuck jobs without leaving the dashboard.
- Retrying a failed dispatch is explicit and does not confuse command success with physical print completion.
- Command controls preserve audit events and role authorization.

## Phase 19: Operational Reliability And Observability

Goal: make Pandar easier to operate in long-running self-hosted deployments.

- Completed `/readyz` checks for database, artifact spool access, gRPC bind configuration, and external-auth JWKS readiness.
- Completed `/metrics` Prometheus output for agent sessions, command lifecycle counts, WebSocket tickets/subscriptions, job outcomes, printer report ingestion, and readiness gauges.
- Completed redaction coverage for bearer tokens, WebSocket tickets, plugin tickets, Bambu access codes, artifact paths, and agent credentials.
- Completed cleanup CLI and retention behavior for terminal jobs/commands, unreferenced artifacts, old machine/audit events, expired/used plugin tickets, and revoked/expired tenant tokens.
- Added backup/restore guidance for SQLite and PostgreSQL deployments.

Exit criteria:

- Operators can distinguish app, database, agent, and printer failures from health/metrics/log evidence.
- Sensitive credentials remain redacted in logs and metrics.
- Self-hosted deployments have documented cleanup and backup paths.

## Phase 20: Artifact And Slicer Workflow Polish

Goal: make print submission closer to a practical Bambu Studio cloud replacement while keeping slicer concerns out of the hub core.

- Completed artifact upload UX with selected filename/size, browser-side base64 conversion state, displayed max size, and stable backend error-code labels.
- Preserved artifact metadata for operator inspection while keeping slicer files opaque to the hub.
- Completed job duplication and reprint flows that reuse existing artifacts safely.
- Kept backend APIs authoritative for validation.
- Deferred slicer metadata parsing to a future reference-backed parser phase.

Exit criteria:

- Operators can upload, inspect, duplicate, and reprint project artifacts through the UI.
- Material mapping remains explicit and validated.
- The hub still treats slicer files as artifacts unless a future phase adds a reference-backed parser.

## Phase 21: Bambu Studio Network Plugin

Goal: add `crates/pandar-network-plugin` as a Bambu Studio network plugin ABI dynamic-library replacement that connects Bambu Studio to `pandar-hub`.

- Completed `crates/pandar-network-plugin` as a Rust `cdylib` crate with a checked-in C++ ABI shim and export-list test.
- Used `reference/open-bamboo-networking` as the ABI/symbol compatibility reference and `reference/BambuStudio` as the caller-behavior reference.
- Targeted a minimal ABI-compatible shim first, not a full Bambu cloud clone.
- Exported the required `bambu_network_*` and `ft_*` symbols for Bambu Studio loading.
- Kept direct LAN/printer paths as stable no-op/unsupported behavior; the plugin does not connect directly to `pandar-agent` or Bambu machines.
- Implemented login scaffolding around Bambu Studio's existing flow:
  - `bambu_network_get_bambulab_host` returns a Pandar frontend URL that serves a Studio-compatible sign-in entry page.
  - The sign-in page lets the user enter or confirm the Pandar URL when needed, then redirects to the configured Pandar frontend authentication flow.
  - The frontend authenticates with Clerk or Logto, selects a tenant through Pandar-managed membership, creates a short-lived one-use plugin login ticket, and returns it through Studio's expected local callback path.
  - The web page uses Bambu Studio's `get_localhost_url` message when available, then sends the browser to Studio's localhost HTTP server with `ticket` and `redirect_url`.
  - Studio calls the plugin's `get_my_token(ticket)` and `get_my_profile(token)` ABI methods; the plugin exchanges the ticket with `pandar-hub` and returns Bambu-shaped token/profile JSON that lets Studio call `change_user(login_info)`.
- Represented the resulting plugin credential as a tenant-owned `["plugin:studio"]` token issued by the hub, not as a user-owned API token.
- Kept Bambu printer access codes and LAN addresses out of the plugin. Those remain agent-local.
- Added a symbol export test from the local ABI symbol file so missing exports fail before runtime Studio loading.
- Documented Linux, Windows, and macOS replacement paths with packaging/signing explicitly optional.

Exit criteria:

- Bambu Studio can load the Pandar dynamic library through the network plugin path without missing-symbol failures.
- Clicking login in Bambu Studio opens the Pandar sign-in flow, completes Clerk/Logto authentication through the frontend, and returns a tenant-scoped plugin credential through Studio's existing localhost ticket flow.
- The plugin authenticates only to `pandar-hub` and can display user/login state in Bambu Studio through the expected `studio_userlogin`/`studio_useroffline` message shapes.
- No plugin code opens MQTT, FTPS, SFTP, or direct agent sockets.
- Tenant-token revocation or plugin-session revocation prevents further hub access from the plugin.

## Phase 22: Hub Horizontal Scaling Control Plane

Goal: support lightweight single-process Hub deployments and horizontally scaled Hub replicas without changing agent authentication or the reverse gRPC model.

- Completed an explicit Hub control-plane boundary:
  - SQLite and PostgreSQL default to an in-process control plane for single Hub processes.
  - PostgreSQL can use NATS with `PANDAR_CONTROL_PLANE=nats`, `PANDAR_NATS_URL`, and optional `PANDAR_NATS_SUBJECT`.
  - SQLite rejects NATS because it is intentionally scoped to lightweight single-process deployments.
- Completed control messages for agent wake, agent close, and tenant-scoped printer events.
- Kept `pandar-agent` on the existing Hub-authenticated reverse gRPC connection. Agents, browsers, and tenants do not connect to NATS.
- Moved browser WebSocket tickets into SQLite/PostgreSQL-backed one-use storage so browser ticket validation works across Hub replicas.
- Preserved PostgreSQL as the shared fact source for durable tenant, command, job, printer, and ticket state.
- Added cross-instance tests for agent wake/close, WebSocket ticket consumption, and printer event fanout.
- Updated PostgreSQL Docker Compose with an optional NATS profile and documented the deployment split.
- Extended the NixOS module so scaled Hub deployments can use either the local NixOS NATS service or an externally managed NATS URL.

Exit criteria:

- SQLite + no broker remains the lightweight single-machine deployment path.
- PostgreSQL + NATS can fan out Hub control messages across replicas while preserving tenant-token authorization at Hub boundaries.
- Print artifacts still use `PANDAR_SPOOL_DIR`; scaled print-job creation requires shared spool storage until a later object-storage backend exists.

## Optional Later: Virtual Printer And Proxy

- Decide whether virtual-printer/proxy behavior from `reference/bambuddy` is in scope.
- If accepted, isolate it as a separate local-agent feature because it changes LAN behavior, port ownership, MQTT/FTPS proxying, and discovery semantics.

## Immediate Next

- Run real Bambu Studio compatibility testing for `pandar-network-plugin` on Linux, Windows, and macOS.
- Harden Phase 21 plugin hub HTTP behavior with live Studio smoke tests, richer error reporting, and compatibility probes beyond symbol exports.
- Soak-test PostgreSQL + NATS Hub replicas under concurrent agent sessions and WebSocket subscribers.
- Add an object-storage artifact backend before scheduling print-job creation across arbitrary Hub pods without shared `PANDAR_SPOOL_DIR`.
- Harden large artifact upload transport beyond server-action form submission if production proxy/body limits become a constraint.
- Add reference-backed live pause/resume/stop controls only after the agent command path is implemented and audited.
- Defer virtual-printer/proxy behavior until plugin compatibility, authenticated agent enrollment, and operator recovery workflows are stable.
