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
- Added NixOS VM tests for SQLite and PostgreSQL hub deployments, and split CI into native x86_64/aarch64 package and VM-test matrices.
- Limited aarch64 package CI to the server, agent, CLI, and web artifacts while keeping the Bambu Studio network plugin package check on x86_64, where the current Linux GNU export-map strategy is supported.
- Added tag-driven GitHub Release CI for `pandar` CLI and `pandar-network-plugin` artifacts using `cargo-zigbuild`, covering Linux, Windows, and macOS on amd64 and arm64 with per-target checksums; macOS CLI artifacts are ordinary release Mach-O binaries rather than fully static binaries.
- Verified a real LAN Bambu printer at `10...24` through the agent MQTT path, raised the MQTT packet limit for full `pushall` reports, and confirmed authenticated status refresh returns `IDLE`.
- Added full-chain warning logs for MQTT report receive failures so errors such as `payload size limit exceeded` are visible during printer refresh/report polling.
- Documented the 2026-06-24 Bambu LAN printer probe, including MQTT topics, tested commands, device details, transport findings, verification, and follow-up notes.
- Added refresh-time printer model discovery through MQTT `info.get_version`; refresh now fails and logs the full error chain when the model cannot be discovered instead of falling back to configured model metadata.
- Added Phase 25 Task 6 Hub-mediated agent artifact downloads: agent bearer auth, agent/artifact ownership checks, storage-backed download responses, agent HTTP artifact fetching through `PANDAR_HUB_API_URL`, and local artifact-reader fallback for legacy command payloads.
- Added Phase 25 Task 8 readiness and cleanup hardening: `/readyz` and Prometheus now report `artifact_storage`, scaled PostgreSQL+NATS filesystem deployments require an explicit shared-filesystem override or object storage, and cleanup execute deletes artifact storage objects before artifact rows while preserving rows on storage delete failure.
- Added Phase 25 scaled artifact storage: browser and Bambu Studio plugin print submission now use multipart artifact uploads, Hub commands carry Hub-mediated `artifact_download_path` values instead of inline base64 artifact payloads, S3-compatible object storage is available for PostgreSQL+NATS deployments, and the scaled smoke harness verifies cross-Hub dispatch/download without a shared local spool.
- Updated deployment, architecture, release, and Docker Compose docs so filesystem storage is documented as the SQLite/single-node default while PostgreSQL+NATS deployments use object storage or an explicit shared-filesystem override.
- Added Phase 26 local HA/failure smoke coverage: the scaled smoke harness now exercises command wake convergence across Hub states, WebSocket `printer_snapshot` and `job_progress` fanout, restart simulation, plugin print pressure, artifact storage put/open/delete failures, and terminal print-report idempotence without Docker or live services.
- Added Phase 26 focused failure observability: Prometheus exports control-plane publish/receive counters, publish failure after durable job/command commit is observable without rolling back state, WebSocket ticket safety is covered across replicas, and storage write/read/delete failure tests pin stable behavior.
- Added Phase 26 operations docs and evidence tracking for SQLite single-node and PostgreSQL+NATS+object-storage deployments, including explicit live soak variables and a `docs/compatibility/phase-26-soak-evidence.md` table for local and live evidence.
- Added a Phase 26 `tools/scaled-artifact-smoke --live` runner entry point for disposable PostgreSQL, NATS, and S3-compatible object storage; disposable local live soak plus explicit NATS and PostgreSQL reconnect evidence are now recorded.
- Refreshed Phase 23/24/26 local evidence after Phase 28: the plugin ABI probe, release-smoke unit coverage, and scaled artifact smoke dry-run are recorded against current code, and the smoke tool now carries the optional artifact metadata field.
- Added Phase 27 live printer-control groundwork: shared model compatibility policy moved into `pandar-core`, Hub now enqueues audited tenant/printer-scoped `printer_control` commands for compatible models, gRPC carries typed printer controls to agents, and agents dispatch typed pause/resume/stop/print-speed MQTT payloads without relying on local model metadata. Local no-network tests cover compatibility, Hub enqueue/route/gRPC behavior, agent command handling, and fake MQTT payload dispatch; real pause/resume/stop/print-speed printer probes are not recorded.
- Added Phase 28 reference-backed slicer metadata: bounded 3MF metadata parsing, SQLite/PostgreSQL `job_artifacts.metadata_json` persistence, tenant preview API, job/plugin response metadata, dashboard upload preview, and compact job/recovery metadata summaries. Local parser, SQLite route/repository/plugin/frontend verification, and disposable PostgreSQL metadata repository verification are recorded.
- Added Phase 29 protocol-level printer operations: Hub now persists and forwards semantic `printer_operation` commands instead of Bambu-specific control strings, tenant `/controls` and plugin `/operations` requests share semantic validation, agents translate operations to Bambu MQTT/G-code locally, and the network plugin parses supported Studio G-code messages into semantic operation JSON before contacting Hub.

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
- Completed the initial hub filesystem artifact storage with `PANDAR_SPOOL_DIR`, `PANDAR_MAX_ARTIFACT_BYTES`, filename sanitization, and scoped cleanup on repository failure; Phase 25 later moved artifact bytes behind the configured storage boundary.
- Completed tenant-scoped print job HTTP APIs:
  - `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs`
  - `GET /api/v1/tenants/{tenant_id}/jobs`
  - `GET /api/v1/tenants/{tenant_id}/jobs/{job_id}`
- Completed atomic print job creation: artifact metadata, linked command, and job row commit together.
- Completed print command dispatch over the existing agent reverse gRPC stream, including printer id, Bambu serial number, artifact metadata, and print options.
- Completed command/job lifecycle coupling for print jobs through repository-level SQLite/PostgreSQL transactions.
- Completed agent `PANDAR_ARTIFACT_ROOT` handling, safe relative artifact path resolution, missing-artifact failure reporting, and unknown-serial rejection before artifact I/O.
- Completed configured agent gateway composition for uploading a project artifact through `MachineFileTransfer`, then publishing MQTT `project_file` with job identity and print flags; fake tests verify upload-before-publish and no-publish-on-upload-failure behavior without live Bambu sockets.
- Completed frontend print job history, per-printer dispatch API visibility, and the initial HTTP-only dispatch form; Phase 25 later moved browser artifact transport to multipart upload.
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
- Completed tenant-token-backed agent enrollment credentials for `agent:register` and `*` scopes.
- Completed `pandar-agent` reverse gRPC authentication with tenant-scoped agent credentials instead of trusting only `PANDAR_TENANT_ID` and `PANDAR_AGENT_ID`.
- Completed hash-only agent credential persistence with plaintext credentials returned once for pairing and rotation.
- Completed authenticated-session binding for gRPC command, heartbeat, snapshot, print report, and command result updates.
- Preserved stale-session protection and replacement-session behavior from Phase 2.
- Updated deployment docs so API automation and agent pairing use tenant tokens instead of user-owned API tokens or long-lived bootstrap credentials.

Exit criteria:

- Completed: existing user-scoped API tokens are no longer accepted for bearer API authentication after the migration.
- Completed: a tenant can own multiple active tenant tokens with independent scopes, revocation, rotation, and audit metadata.
- Completed: empty-scope tenant tokens can read tenant resources but cannot mutate them.
- Completed: `*` tenant tokens can perform all tenant-scoped operations.
- Completed: `agent:register` tenant tokens can register or rotate agents but cannot access ordinary tenant API resources.
- Completed: a fresh agent can be enrolled through tenant-token-authorized pairing and connect without manual database identifiers.
- Completed: revoked or rotated tenant tokens and agent credentials cannot open or mutate protected sessions.
- Completed: existing command dispatch and printer/job report tests prove authenticated agent identity is enforced.

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
- Phase 27 later added pause, resume, stop, and print-speed controls; Phase 29 moves customer controls to protocol-defined `printer_operation` commands rather than physical-state mutations.
- Show command state transitions and latest structured result details inline with the affected printer or job.
- Added safe retry affordances for failed dispatch/upload/MQTT operations without creating duplicate physical prints accidentally.
- Kept raw Bambu commands behind diagnostics/admin boundaries; normal operators use typed controls.

Exit criteria:

- Operators can recover common failed or stuck jobs without leaving the dashboard.
- Retrying a failed dispatch is explicit and does not confuse command success with physical print completion.
- Command controls preserve audit events and role authorization.

## Phase 19: Operational Reliability And Observability

Goal: make Pandar easier to operate in long-running self-hosted deployments.

- Completed `/readyz` checks for database, configured artifact storage, gRPC bind configuration, and external-auth JWKS readiness.
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

- Completed artifact upload UX with selected filename/size, upload state, displayed max size, and stable backend error-code labels; Phase 25 later replaced browser-side base64 conversion with multipart upload.
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
  - `bambu_network_get_bambulab_host` starts and returns a plugin-local loopback webserver that serves a Studio-compatible sign-in entry page.
  - The sign-in page is built from the `frontend/plugin-local` monorepo package, embedded in the plugin with `rust-embed`, and lets the user inspect defaults or switch the target web/hub server.
  - The local page then redirects to the configured Pandar frontend authentication flow.
  - The frontend authenticates with Clerk or Logto, selects a tenant through Pandar-managed membership, creates a short-lived one-use plugin login ticket, and returns it through Studio's expected local callback path.
  - The web page uses Bambu Studio's `get_localhost_url` message when available, then sends the browser to Studio's localhost HTTP server with `ticket` and `redirect_url`.
  - Studio calls the plugin's `get_my_token(ticket)` and `get_my_profile(token)` ABI methods; the plugin exchanges the ticket with `pandar-hub` and returns Bambu-shaped token/profile JSON that lets Studio call `change_user(login_info)`.
- Represented the resulting plugin credential as a tenant-owned `["plugin:studio"]` token issued by the hub, not as a user-owned API token.
- Kept Bambu printer access codes and LAN addresses out of the plugin. Those remain agent-local.
- Completed local webserver coverage for embedded assets, default configuration prompts, target-server switching, and ABI handoff through `cargo test -p pandar-network-plugin --test local_webserver` and `--test studio_abi_probe`.
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
- Added GitHub Actions Nix checks with Hestia caching, package matrices for x86_64/aarch64, and SQLite/PostgreSQL NixOS VM tests.
- Adjusted the NixOS VM tests to run without requiring the Nix `kvm` system feature so GitHub's native arm64 runner can execute them through QEMU fallback when `/dev/kvm` is unavailable.
- Kept the aarch64 package matrix focused on `pandar-hub`, `pandar-agent`, `pandar-cli`, and `pandar-web`; `pandar-network-plugin` remains checked on x86_64 until its C++ ABI export path is reworked for arm64 GNU linking.

Exit criteria:

- SQLite + no broker remains the lightweight single-machine deployment path.
- PostgreSQL + NATS can fan out Hub control messages across replicas while preserving tenant-token authorization at Hub boundaries.
- Print artifacts now use the configured artifact-storage boundary; filesystem storage remains available for single-node deployments, while PostgreSQL + NATS deployments should use object storage or explicitly declare a shared filesystem.

## Phase 23: Real Bambu Studio Plugin Compatibility

Goal: turn the Phase 21 network-plugin scaffold into a verified Bambu Studio integration on real desktop installs.

- Run real Bambu Studio load/login/print-flow smoke tests on Linux, Windows, and macOS using the generated release artifacts.
- Capture the exact Studio caller behavior for plugin initialization, sign-in, token/profile retrieval, printer listing, job listing, print submission, logout, and offline transitions.
- Harden `pandar-network-plugin` HTTP behavior beyond symbol exports:
  - preserve useful hub/network error details without exposing bearer tokens, plugin tickets, artifact paths, or local filesystem paths;
  - map Pandar hub responses into stable Bambu-shaped response bodies where Studio expects them;
  - add compatibility probes for Studio versions that call plugin methods in a different order.
- Validate the sign-in loop from Bambu Studio WebView through `frontend/app/plugin-sign-in`, plugin login-ticket exchange, and `studio_userlogin`/`studio_useroffline` callbacks.
- Document known compatible Studio versions, operating systems, plugin replacement paths, and unsupported plugin ABI functions.
- Keep direct LAN/MQTT/FTPS behavior out of the plugin; Studio talks to `pandar-hub`, and Bambu machine credentials remain agent-local.
- Completed local Phase 23 scaffolding: compatibility manifest, smoke runbook, stable plugin error mapping, and a local C++ ABI probe against a mock hub.
- Refreshed local probe evidence on 2026-06-24: `cargo test -p pandar-network-plugin` passed 20 tests against the current code.
- Added a Phase 23 Studio preflight helper that validates local Studio/plugin prerequisite metadata and prints a redacted manifest row template before manual real-Studio testing; it does not claim compatibility without a real Studio run.
- Checked real Studio test prerequisites on 2026-06-24: no local Bambu Studio command and no Windows/macOS host were available, so Phase 23 real Studio rows are blocked until matching Studio installations exist. Matching plugin artifact availability is tracked separately in the Phase 24 release evidence manifest.
- Real Studio compatibility remains unverified until `docs/compatibility/bambu-studio-plugin.md` records real Studio runs for each platform.

Exit criteria:

- Bambu Studio can load the Pandar plugin without missing symbols on every supported desktop platform.
- A user can sign in through Studio, receive a tenant-scoped plugin credential, list Pandar printers/jobs, and submit a print through the hub-backed plugin route.
- Plugin failure modes are visible enough to diagnose invalid hub URL, expired ticket, revoked plugin token, offline hub, bad artifact, and unauthorized printer/job access.
- The compatibility evidence is documented from real Studio runs, not only unit tests or export inspection.

## Phase 24: Cross-Platform Release Validation And Packaging

Goal: make release artifacts predictable enough for operators to install without building from source.

- Validate tag-driven GitHub Release artifacts on real Linux, Windows, and macOS hosts, including CLI startup, dynamic-library loadability, checksums, and archive layout.
- Completed local Phase 24 release-smoke scaffolding: a standalone helper crate that validates release archive checksums and top-level CLI/plugin layout without joining the main Cargo workspace.
- Completed packaged-artifact release-smoke checks for CLI startup on native runners and plugin ABI export inspection from the unpacked release plugin library.
- Refreshed local release-smoke unit evidence on 2026-06-24: `cargo test --manifest-path tools/release-smoke/Cargo.toml` passed 17 tests.
- Added local linux-amd64 artifact evidence on 2026-06-24: a `pandar-release-local-a79bcae-linux-amd64.tar.gz` archive built from release binaries passed checksum/layout, `pandar --help`, and 129 packaged plugin ABI export checks through `tools/release-smoke`.
- Wired the tag-driven GitHub Release workflow to run checksum verification and release-smoke before uploading release artifacts.
- Added operator release installation docs, a release artifact evidence manifest, and the explicit Phase 24 signing decision: `unsigned-accepted`.
- Initial pre-workflow release artifact availability check on 2026-06-24 found no GitHub Releases, no `release.yml` workflow runs, and no tags. Later workflow_dispatch runs uploaded artifacts for five target families (see run evidence below), but no tagged GitHub Release archive exists yet. The generated local linux-amd64 archive proves local artifact smoke only; workflow run `28102001464` now has local Linux x86_64 host install evidence, while tagged-release install validation and the other target families remain blocked.
- Triggered `release.yml` workflow_dispatch run `28098334876` on 2026-06-24: linux-amd64 and linux-arm64 artifacts uploaded, but the full matrix failed because macOS CLI builds linked through cargo-zigbuild on native macOS runners and Windows plugin builds did not find the `cc` shim object. The release workflow now uses native `cargo build` for non-zig macOS targets and accepts both `.o` and `.obj` shim objects before the next workflow evidence run.
- Triggered `release.yml` workflow_dispatch run `28099917011` on 2026-06-24: linux-amd64 and linux-arm64 artifact jobs passed in CI and local downloaded artifact smoke; macOS CLI/package smoke reached the plugin export check but failed because the Mach-O export table omitted required ABI symbols; Windows plugin builds now find the shim object but fail to link C++ runtime symbols. The plugin build script now prepares a macOS exported-symbol list from the canonical ABI file and links Windows shim builds against libc++/libc++abi before the next workflow evidence run.
- Triggered `release.yml` workflow_dispatch run `28102001464` on 2026-06-24: linux-amd64, linux-arm64, macos-amd64, macos-arm64, and windows-amd64 artifact jobs passed packaged release-smoke and uploaded artifacts. Windows arm64 built and packaged but failed plugin export inspection because Ubuntu's default `objdump` did not recognize ARM64 PE and no LLVM PE inspector was present. The release workflow now installs LLVM tools before Windows PE export inspection.
- Triggered `release.yml` workflow_dispatch run `28103772270` on 2026-06-24 after the LLVM inspector fix, but GitHub Actions did not start any build steps because account payments failed or the spending limit needs to be increased. Re-run the workflow after billing is restored before treating Windows arm64 release evidence as updated.
- Rechecked release availability on 2026-06-25: no GitHub Releases or remote git tags exist, and run `28102001464` artifacts remain unexpired for linux-amd64, linux-arm64, windows-amd64, macos-amd64, and macos-arm64. Local static follow-up checks passed release-smoke for linux-arm64 and windows-amd64 and checksum/layout/file-type inspection for both macOS artifacts; these do not replace target-host install evidence.
- Real host installation evidence now covers only the `linux-amd64` workflow artifact from run `28102001464`; tagged GitHub Release installs and the other target families remain unverified until `docs/compatibility/release-artifacts.md` records target-family rows from actual release artifact installs.
- Rework the Linux `pandar-network-plugin` export strategy if arm64 plugin releases remain a target, because the current GNU export-map path is known to be fragile around Rust `cdylib` plus C++ shim exports.

Exit criteria:

- A release tag produces downloadable archives whose contents are validated on the target OS family before the release is treated as usable.
- Operators can install the CLI, hub/web services, agent, and plugin from documented artifacts without reading CI internals.
- Any unsupported target is explicit in docs and CI output instead of silently publishing an incomplete artifact.

## Phase 25: Scaled Artifact Storage And Upload Pipeline

Goal: remove shared-local-spool as the limiting factor for horizontally scaled print-job creation.

- Add an artifact-storage boundary with at least:
  - completed filesystem backend for SQLite/single-node deployments;
  - completed S3-compatible object-storage backend suitable for PostgreSQL + multi-Hub deployments.
- Completed metadata persistence in PostgreSQL/SQLite while moving artifact bytes behind the storage backend.
- Completed create-job, duplicate, reprint, plugin print, cleanup, metrics, readiness, and backup/restore docs through the storage boundary instead of assuming `PANDAR_SPOOL_DIR` is local to one Hub process.
- Completed browser and plugin artifact upload transport hardening beyond server-action/base64 submission:
  - multipart uploads avoid browser/server-action base64 body amplification;
  - backend validation and stable error-code labels remain authoritative;
  - storage paths are generated by the Hub, not trusted from browser or plugin callers.
- Completed Hub-mediated agent artifact downloads through bearer-authenticated `artifact_download_path` values, so agents do not need browser/plugin payload bytes or object-store credentials.
- Completed final transport hardening for plugin-side streamed multipart uploads, S3 staged-file streaming, handler-owned upload error labels, same-tenant cross-agent artifact `403` classification, backend download failure classification, and redacted Hub-download failure context.
- Added `tools/scaled-artifact-smoke` to exercise multipart plugin submission on one Hub state, command dequeue on another Hub state, and agent download through a Hub HTTP artifact route without a shared local spool.
- Kept slicer files opaque; this phase changed storage and transport, not slicer parsing.
- Live scaled-deployment evidence is tracked in Phase 26; local dry-run coverage remains the Phase 25 storage/transport baseline.

Exit criteria:

- PostgreSQL + NATS deployments can create print jobs from arbitrary Hub replicas without requiring a shared POSIX spool directory.
- Filesystem storage remains the simple default for SQLite/single-node deployments.
- Cleanup, retry, duplicate, reprint, plugin submission, and audit behavior remain consistent across storage backends.
- Large artifact upload failures preserve actionable cause chains without leaking sensitive paths or tokens.

## Phase 26: Production Soak, HA, And Failure Injection

Goal: prove the scaled Hub and agent model under realistic concurrent use before expanding product surface area.

- Completed local dry-run evidence for concurrent agent-session wake convergence, WebSocket subscribers, plugin clients, print-job creation, restart simulation, storage failures, and terminal print-report idempotence.
- Refreshed scaled smoke evidence on 2026-06-24 after Phase 28 metadata persistence: `tools/scaled-artifact-smoke` now constructs print jobs with explicit `artifact_metadata_json: None`, and `--dry-run --iterations 1 --concurrency 2` passed all local scenarios.
- Fixed and re-verified Phase 26 local concurrent plugin pressure after reproducing a SQLite `database is locked` failure: print-job audit transactions now use SQLite immediate write transactions, and `--dry-run --iterations 2 --concurrency 2` passed all local scenarios with scenario-context diagnostics.
- Checked live soak prerequisites on 2026-06-24: local PostgreSQL binaries were available and `tools/scaled-artifact-smoke --live-preflight` verified required variables, input shape, and disposable safety markers for PostgreSQL/NATS/object-storage; the first pass remained blocked until disposable NATS/object-storage endpoints were configured.
- Added a live runner entry point for artifact, fanout, restart, and terminal scenarios against disposable PostgreSQL, NATS, and S3-compatible object storage. The storage failure scenario remains local-only.
- Completed disposable local live soak on 2026-06-25 using PostgreSQL, NATS, and MinIO containers: `--live-preflight` passed, and `--live --iterations 2 --concurrency 2` passed artifact, fanout, restart, and terminal scenarios twice.
- Fixed and re-verified a live-runner assertion that counted prior persistent commands globally during concurrent plugin pressure; live pressure now counts queued print commands for the current pressure fixtures only.
- Added explicit NATS interruption evidence on 2026-06-25: the live `nats-reconnect` scenario waited after Hub B subscribed, the disposable NATS container was stopped and started, and a subsequent plugin print from Hub A still woke the Hub B agent session and dequeued the persisted command.
- Added explicit PostgreSQL restart/reconnect evidence on 2026-06-25: the live `postgres-reconnect` scenario seeded data before a controlled PostgreSQL stop/start, then fresh plugin print creation, command dequeue, and terminal print-report persistence succeeded through the reused pool.
- Deferred proxy-style artificial SQL latency injection beyond Phase 26; current acceptance uses concurrent pressure plus controlled PostgreSQL restart/reconnect as database fault evidence without adding proxy tooling.
- Exercise failure modes:
  - Hub restart is covered locally through shared database/storage/control-plane reconstruction;
  - NATS disconnect/reconnect is covered by the disposable local `nats-reconnect` live scenario;
  - PostgreSQL restart/reconnect is covered by the disposable local `postgres-reconnect` live scenario; artificial SQL latency injection is deferred beyond Phase 26 unless future incidents require proxy-level delay testing;
  - WebSocket ticket consumption across replicas is covered locally;
  - control-plane subscriber decode failure and continuation are covered by focused tests;
  - artifact-storage write/read/delete failures are covered locally.
- Metrics and logs distinguish app, database, broker/control-plane, storage, agent/session, and printer/report failures through `/readyz`, `/metrics`, and full-chain error logging.
- Recommended deployment topologies and operational runbooks for SQLite single-node and PostgreSQL + NATS scaled deployments are documented.

Exit criteria:

- Local scaled dry-run has repeatable evidence for agent sessions, command dispatch/wake, WebSocket fanout, plugin calls, and print-job creation.
- Operators can identify which subsystem failed from `/readyz`, `/metrics`, logs, and documented runbooks.
- Recovery from local Hub restart simulation does not duplicate terminal machine events or regress physical print state.
- Live PostgreSQL + NATS + object-storage artifact, fanout, restart, terminal, explicit broker interruption, and PostgreSQL restart/reconnect scenarios have disposable local evidence. Artificial SQL latency injection is documented as deferred beyond Phase 26 rather than required Phase 26 evidence.

## Phase 27: Reference-Backed Live Printer Controls

Goal: add typed pause, resume, stop, and related live printer controls only after the command path is audited against Bambu reference behavior.

- Completed reference-backed payload policy for pause, resume, stop, and print-speed dispatch.
- Completed typed agent command builders and gateway methods for supported controls; raw command dispatch remains behind diagnostics/admin boundaries.
- Completed Hub-side compatibility gating so unsupported models or unknown capabilities reject enqueue instead of sending speculative commands.
- Completed command lifecycle, audit event, structured result, and physical print-status separation in local tests.
- Added Phase 27 compatibility documentation with local no-network verification commands and explicit real-printer probe status.
- Frontend controls were updated in this phase and covered by a 2026-06-25 `frontend/` production build; browser-level e2e interaction and real-printer probes are not recorded in this workspace.
- Checked Phase 27 live-control probe prerequisites on 2026-06-24: no `PANDAR_PRINTERS` configuration or printer access code is available in this workspace, so pause/resume/stop/print-speed hardware probes are blocked until an operator supplies safe printer state and agent-local LAN credentials outside source control.
- Real-printer probes for pause, resume, stop, and print speed are not recorded; `docs/bambu-lan-printer-probe-2026-06-24.md` covers other MQTT commands only.

Exit criteria:

- Operators can queue supported live printer controls with tenant role enforcement and audit records.
- UI state distinguishes command dispatch success from physical printer state changes reported later over MQTT.
- Unsupported or unknown printer/control combinations stay unavailable with diagnostic context.

## Phase 29: Protocol Printer Operations

Goal: make customer-facing printer actions device-neutral so non-Bambu agents can translate the same semantic operation contract later.

- Completed `PrinterOperation` protobuf dispatch for pause, resume, stop, speed, home, relative axis movement, and hotend temperature.
- Completed Hub persistence and audit of semantic `printer_operation` payloads; Hub validates ownership, compatibility, ranges, axes, and unknown fields without constructing Bambu MQTT JSON or G-code.
- Completed Bambu agent translation for semantic operations, including bare `G28` for every home request, relative move `gcode_line`, and `M104`/`M109` hotend commands.
- Completed network plugin parsing of supported control G-code into semantic Hub operation requests; unsupported or ambiguous G-code returns stable plugin errors before network dispatch.
- Real-printer probes for Phase 29 home/move/hotend are not recorded in this workspace.

Exit criteria:

- Hub sends `HubCommand::PrinterOperation` for customer controls.
- Agent-local adapters own all device-specific command conversion.
- Studio plugin live control messages never forward raw G-code to Hub.

## Phase 28: Reference-Backed Slicer Metadata

Goal: improve artifact inspection and print defaults by reading safe metadata from project files without turning the hub into a slicer.

- Completed a narrow parser boundary for Bambu/3MF project metadata needed by Pandar:
  - plate count and selected plate defaults;
  - model/project display name;
  - material mapping hints;
  - estimated filament/time fields when safely available.
- Completed reference-derived fixtures and bounded parsing; the hub does not parse or execute arbitrary slicer logic.
- Completed optional advisory persistence in both SQLite and PostgreSQL migrations. Backend validation and operator-selected print settings remain authoritative.
- Completed metadata preview, dashboard display, job responses, and plugin responses.
- Completed disposable PostgreSQL metadata verification for create/list/get hydration and reprint/duplicate reuse through the repository boundary.
- Preserved opaque artifact handling for unknown, unsupported, or malformed files.
- Added Windows MSVC build compatibility for the network plugin shim by passing the MSVC C++17 compiler flag.
- Fixed filesystem artifact storage key validation on Windows so rooted paths such as `/tmp/escape` are rejected consistently.
- Made the Phase 21 network plugin export verification locate Visual Studio `dumpbin.exe` on Windows when it is not on `PATH`.
- Added Bambu Studio sign-in route aliases so Studio's localized `/en/sign-in` WebView entry reaches the plugin sign-in page instead of a Next.js 404.

Exit criteria:

- Completed locally: operators can inspect practical project metadata before dispatching a print.
- Completed locally: metadata parsing failures do not block upload or dispatch unless the artifact itself is invalid.
- Completed locally: parsed values never override explicit user settings or compatibility rules.
- Completed locally: disposable PostgreSQL repository verification covers persisted metadata hydration and artifact reuse.

## Optional Later: Virtual Printer And Proxy

- Decide whether virtual-printer/proxy behavior from `reference/bambuddy` is in scope.
- If accepted, isolate it as a separate local-agent feature because it changes LAN behavior, port ownership, MQTT/FTPS proxying, and discovery semantics.

## Immediate Next

- Provide Bambu Studio installations and matching plugin artifacts, then run Phase 23 real compatibility testing on Linux, Windows, and macOS.
- Produce a tag/release artifact, then validate it while running Phase 23 so Phase 24 can use the same platform evidence.
- Record real Bambu Studio plugin compatibility evidence for Phase 23.
- Produce or select a tagged GitHub Release archive, then record live release artifact install evidence for Phase 24 target families that are still unverified.
- Run Phase 27 pause/resume/stop/print-speed and Phase 29 home/move/hotend hardware probes only after a safe printer state and agent-local LAN credentials are available.
- Keep virtual-printer/proxy behavior deferred until plugin compatibility, scaled artifact storage, and operator recovery workflows are stable.
