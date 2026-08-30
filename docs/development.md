# Development And Deployment Notes

This document collects local setup, runtime configuration, authentication, and deployment notes that are too detailed for the README.

## Hub Runtime

`pandar-hub` reads `PANDAR_DATABASE_URL` on startup and defaults to:

```bash
sqlite://pandar.db
```

`pandar-hub` listens for HTTP/WebSocket traffic on `PANDAR_HUB_BIND` and defaults to `127.0.0.1:8080`. The reverse agent gRPC listener uses `PANDAR_HUB_GRPC_BIND` and defaults to `127.0.0.1:50051`; non-loopback gRPC binds require `PANDAR_HUB_GRPC_TLS_CERT` and `PANDAR_HUB_GRPC_TLS_KEY`. Readiness and metrics use the separate `PANDAR_HUB_OBSERVABILITY_BIND`, which defaults to `127.0.0.1:9090`.

`PANDAR_PRINTER_ACCESS_CODE_KEY` is required and must contain the unpadded base64url encoding of exactly 32 random bytes. Generate it with `openssl rand -base64 32 | tr '+/' '-_' | tr -d '='`, inject the same value into every Hub replica through a secret manager, and back it up separately with the database. The Hub stores successful Bambu access codes only as versioned AES-256-GCM envelopes bound to the tenant and printer serial. Startup transactionally encrypts legacy plaintext rows and clears the plaintext column; an absent, changed, or invalid key prevents startup rather than silently dropping saved printer connections.

Set `PANDAR_HUB_NO_AUTH=true` only for local development or explicitly trusted single-user deployments. In this mode, hub HTTP/WebSocket tenant and bootstrap APIs skip bearer-token authentication and role checks, and startup logs a warning. Agent reverse gRPC connections still require agent credentials.

The hub runs backend-specific SQLx migrations automatically when it connects. SQLite migrations live under `crates/pandar-hub/migrations/sqlite`; PostgreSQL migrations live under `crates/pandar-hub/migrations/postgres`.

Repository and HTTP tests use SQLite by default, including `sqlite::memory:` for API tests. PostgreSQL parity tests run when `PANDAR_TEST_POSTGRES_URL` points at a disposable PostgreSQL database whose test role can create and drop schemas. Every test creates, migrates, and drops its own schema, so Nextest can run the PostgreSQL suite in an eight-test parallel group without shared-table truncation or exhausting the server connection budget. The Checks workflow always supplies PostgreSQL, sets `PANDAR_REQUIRE_POSTGRES_TESTS=true` so a missing database cannot turn the gate into skips, and runs `cargo nextest run -p pandar-hub -E 'test(/postgres/)' --no-tests=fail`; run the same command locally with `PANDAR_TEST_POSTGRES_URL` set before changing backend-dependent behavior.

Personal-preset synchronization has focused repository/route coverage with `cargo nextest run -p pandar-hub -E 'test(personal_preset)' --no-tests=fail`; backend parity uses the same filter with `PANDAR_TEST_POSTGRES_URL` set and receives an isolated schema per test. Its compiled Studio seam is `cargo nextest run -p pandar-network-plugin -E 'binary(personal_presets) | binary(studio_abi_probe)' --no-tests=fail`. Automated ABI tests are not a real-Studio compatibility claim: record a packaged two-installation create/update/delete/cancel/outage flow before marking an exact Studio/platform row verified.

## Agent Runtime

`pandar-agent` connects outward to the hub gRPC endpoint. Current local-development identity values are:

```bash
PANDAR_HUB_GRPC_URL=http://127.0.0.1:50051
PANDAR_TENANT_ID=<tenant uuid>
PANDAR_AGENT_ID=<agent uuid>
PANDAR_AGENT_NAME=local-agent
PANDAR_AGENT_VERSION=0.1.0
PANDAR_AGENT_CREDENTIAL=<agent credential from pairing or rotation>
```

Agent-local Bambu printers are configured explicitly with `PANDAR_PRINTERS`:

```bash
PANDAR_PRINTERS='[{"host":"192.0.2.10","serial":"01S00EXAMPLE","access_code":"12345678","model":"A1 Mini","name":"garage-a1"}]'
```

The value is a JSON array. `host`, `serial`, and `access_code` are required; `model` and `name` are optional. Empty, whitespace, or `[]` means no configured printers and the agent will not open Bambu machine sockets. Invalid printer config fails at startup with `PANDAR_PRINTERS` context.

Printers can also be linked at runtime from the dashboard Agents page. After the Agent validates and reports a linked printer, the Hub stores its host and an encrypted access-code envelope so the Agent can restore the connection after restart. In multi-Hub deployments, runtime linking must be routed to the Hub process that owns the agent's reverse gRPC stream; otherwise the API returns `agent_not_connected` and no command row is created.

Dashboard runtime printer linking is separate from `PANDAR_PRINTERS`. The Agents page add-printer form sends `type = BambuLab`, printer IPv4 address, access code, and optional display name to the selected live local agent. Operators do not enter serial number or model for this flow; the agent resolves those values through Bambu SSDP discovery and MQTT validation, then reports them back through the printer snapshot and command result. If SSDP cannot see the submitted IPv4 address or the discovery response lacks a serial number, the link command fails without persisting the access code in Hub storage.

Agent-wide discovery sends Bambu SSDP M-SEARCH to the standard multicast target and directly to peers on operational, non-point-to-point private IPv4 interfaces. Direct scanning is capped to the `/22` containing each agent interface address and never targets public networks, so printers remain discoverable on LANs that suppress multicast without permitting an unbounded scan.

`RefreshPrinters` discovers the printer model at refresh time through the Bambu LAN MQTT `info.get_version` command before requesting the normal `pushall` state report. If model discovery cannot publish, time out, or parse the `ota.product_name` field, the refresh command fails and logs the full error chain instead of falling back to `PANDAR_PRINTERS[].model`. The optional configured `model` remains local metadata for paths that still need a conservative compatibility profile.

Reverse sessions require `PANDAR_AGENT_CREDENTIAL`. Tenant admins create or rotate agent credentials through tenant-token-backed pairing and enrollment APIs. Plaintext credentials are returned once and only hashes are stored by the hub.

## Machine Communication

The agent-side MQTT boundary, `RefreshPrinters` gateway path, and machine file-transfer boundary are implemented from reference behavior. Bambu LAN MQTT, FTPS, and BRTC require a certificate whose common name exactly matches the configured printer serial. The agent verifies X.509 v3 certificate chains against Studio's bundled Bambu printer CA set and supplies reviewed bundled intermediates when printer firmware omits them; this includes the `BBL Device CA N6-V2` intermediate used by X2D BRTC. Bambu X.509 v1 leaves are accepted without a device pin only when a bundled trusted Bambu CA directly signs the leaf with RSA/SHA-256, its inner and outer signature algorithms match, its common name matches the printer serial, and its validity period contains the current time. For models such as P2S that send a leaf-only certificate from an issuer absent from the bundled set, configure `PANDAR_BAMBU_CERTIFICATE_SHA256_PINS` as a JSON object mapping each serial to its independently verified leaf SHA-256 fingerprint (plain 64-digit hex or colon-separated hex). A configured pin must match exactly and certificate validity dates are still enforced; the agent never falls back to accepting an untrusted CN-only certificate.

Runtime Bambu machine communication:

- MQTT over TLS on port `8883`.
- MQTT username `bblp`, password set to the printer access code.
- Report topic `device/{serial}/report`.
- Request topic `device/{serial}/request`.
- For printers whose MQTT certificate common name differs from their inventory serial, the certificate identity replaces `{serial}` only at the MQTT transport boundary.
- Refresh sends `info.get_version` before `pushing.pushall` and fails closed when the model cannot be discovered.
- Machine file transfer through implicit FTPS on port `990`.
- Protected data mode only (`PROT P`). A protected-data failure stops the operation; the agent never downgrades the data channel with `PROT C`.

Unit tests use fakes and must not open real Bambu MQTT or FTPS sockets.

### A1 protected-FTPS firmware gate

The opt-in `verify_a1_protected_ftps` example validates exactly one A1 and one A1 Mini against their
current firmware. It reads `info.get_version`, confirms the firmware-reported model, then lists the
FTPS root through `PROT P`. It does not upload, delete, print, or send a printer control. Supply both
targets through an ephemeral secret environment variable; never commit the value or include it in
captured evidence:

```bash
export PANDAR_A1_FTPS_VALIDATION_TARGETS='[
  {"host":"<a1-ip>","serial":"<a1-serial>","access_code":"<a1-access-code>","model":"A1"},
  {"host":"<a1-mini-ip>","serial":"<a1-mini-serial>","access_code":"<a1-mini-access-code>","model":"A1 Mini"}
]'
cargo run -p pandar-agent --example verify_a1_protected_ftps
unset PANDAR_A1_FTPS_VALIDATION_TARGETS
```

A passing run prints only normalized models, firmware module versions, `PROT P`, and root entry
counts; it omits hosts, serials, access codes, and directory names. Record both firmware versions and
the successful protected-data result before claiming A1/A1 Mini firmware compatibility. This gate is
not part of automated tests because it opens real printer sockets.

## Hub APIs

Printer inventory and live events:

- `GET /api/v1/tenants/{tenant_id}/printers` lists the latest printers reported for a tenant.
- `GET /api/v1/tenants/{tenant_id}/printers/{printer_id}` returns one tenant-scoped printer.
- `POST /api/v1/tenants/{tenant_id}/agents/{agent_id}/refresh-printers` queues a `refresh_printers` command for a live agent through the command ledger.
- `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/materials:refresh` queues an Agent command to refresh the printer's current AMS/external-spool material snapshot. The Agent synchronizes material patches to Hub over gRPC; browser live updates continue through the tenant printer event stream.
- `GET /api/v1/tenants/{tenant_id}/printer-events` upgrades to a tenant-scoped WebSocket for future `printer_snapshot` and `job_progress` events. It does not replay historical state; clients should load initial state over HTTP and treat WebSocket delivery as best-effort live updates.
- The same endpoint with `projection=studio&version=1` is the Bambu Studio network-plugin stream. It authorizes only same-tenant `plugin:studio` tokens and sends one atomic complete printer snapshot followed by typed full-record upserts/removals, sharing the Studio printer projection with the HTTP list endpoint and invalidating on lag or event-epoch gaps. The default Viewer/ticket contract is unchanged.

Tenant-scoped print dispatch:

- `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs` accepts multipart form data with a `file` part plus `filename`, `content_type`, `plate_id`, `use_ams`, `flow_cali`, `timelapse`, and optional material mapping fields, then creates an artifact, linked command, and job transactionally.
- `POST /api/v1/tenants/{tenant_id}/artifact-metadata-preview` accepts multipart artifact data and returns optional advisory slicer metadata without creating artifact, command, or job rows.
- `GET /api/v1/tenants/{tenant_id}/jobs` lists tenant print jobs.
- `GET /api/v1/tenants/{tenant_id}/jobs/{job_id}` returns one tenant-scoped print job.

Artifact storage is selected with `PANDAR_ARTIFACT_STORAGE`, defaulting to `filesystem`. The filesystem backend writes uploaded artifacts under `PANDAR_SPOOL_DIR`, defaulting to `pandar-spool`, and is intended for SQLite or single-Hub deployments. All backends reject artifacts larger than `PANDAR_MAX_ARTIFACT_BYTES`, defaulting to `10485760`. The S3-compatible backend uses `PANDAR_ARTIFACT_STORAGE=s3` plus `PANDAR_ARTIFACT_S3_BUCKET`, `PANDAR_ARTIFACT_S3_REGION`, `PANDAR_ARTIFACT_S3_ENDPOINT`, `PANDAR_ARTIFACT_S3_ACCESS_KEY_ID`, `PANDAR_ARTIFACT_S3_SECRET_ACCESS_KEY`, and optional `PANDAR_ARTIFACT_S3_FORCE_PATH_STYLE=true|false`.

Agents receive a Hub artifact download path in `PrintProjectFile` and fetch bytes from Hub HTTP with their agent credential. Set `PANDAR_HUB_API_URL` for agents when `PANDAR_HUB_GRPC_URL` is not an HTTP(S) URL. `PANDAR_ARTIFACT_ROOT` remains a local fallback for older commands that do not contain a Hub download path.

Print job dispatch success means the agent accepted the command path and completed upload/MQTT dispatch work. Physical progress and terminal printer outcome are tracked separately from MQTT reports.

Slicer metadata is advisory. The hub extracts a bounded subset of 3MF project metadata for display and stores it with the artifact when available, but explicit request fields such as `plate_id`, AMS mapping, calibration, and timelapse remain authoritative. Unsupported files, malformed ZIPs, and parser failures return no metadata and do not block dispatch when the artifact upload itself is valid.

Recovery APIs:

- `POST /api/v1/tenants/{tenant_id}/agents/{agent_id}/refresh-printers` manually refreshes printer state.
- `POST /api/v1/tenants/{tenant_id}/jobs/{job_id}/retry-dispatch` retries dispatch for a failed or cancelled dispatch lifecycle.
- `POST /api/v1/tenants/{tenant_id}/jobs/{job_id}/reprint` queues a reprint from the existing artifact and options.
- `POST /api/v1/tenants/{tenant_id}/jobs/{job_id}/duplicate` creates a new job from the existing artifact with optional printer, plate, and print-flag overrides.
- `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/controls` queues typed, compatibility-gated live printer operations.

Phase 29 live printer operations are dispatch-only operations for compatible printers. Pause, resume, stop, print-speed, home, relative movement, and hotend-temperature requests enqueue audited `printer_operation` commands; physical printer state changes remain report-derived.

The Hub stores and forwards typed printer operations. Bambu-specific MQTT construction remains inside `pandar-agent`. The network plugin maps recognized Studio `send_message_to_printer` G-code to semantic operation requests and uses the authenticated plugin route's narrow typed `gcode_line` path for other string parameters; the normal tenant controls route does not expose that path.

Workspace rename is audited: `PATCH /api/v1/tenants/{tenant_id}` updates the display name for tenant administrators (or no-auth operators). Tenant tokens are rejected, empty names fail validation, and the `tenant.rename` audit metadata records the previous and new names.

Authenticated Bambu Studio personal-preset routes:

- `GET /api/v1/plugin/presets` returns the owner's complete Process/Filament/Printer catalogue; `POST /api/v1/plugin/presets` creates one preset replay-safe by type and name.
- `GET|PATCH|DELETE /api/v1/plugin/presets/{setting_id}` reads, replaces, or idempotently deletes one owned preset.
- All five routes require the live Operator user attached to the exact `plugin:studio` token; no-auth sessions and tenant-wide token identities are rejected. Request bodies are bounded at 512 KiB, responses send `cache-control: no-store`, and quota exhaustion returns error code `14`.

Printer cameras stream through the owning live Agent for whitelisted models (A1, A1 Mini, P1S, A2L):

- `GET /api/v1/tenants/{tenant_id}/printers/{printer_id}/camera.mp4` serves the browser relay path.
- `GET /api/v1/plugin/printers/{printer_id}/camera.mjpeg` serves the Studio network-plugin path; the plugin receives only a random one-use loopback relay URL and never sees the printer host or access code.
- Retained camera responses are capped per tenant with `PANDAR_HUB_CAMERA_MAX_STREAMS_PER_TENANT` (see Operations).

## Frontend Runtime

Run `npm --prefix frontend run lint`, `npm run test:web`, and `npm run typecheck:web` before submitting frontend changes. The workspace production-module guard (`cargo test -p pandar-core --test module_size`) enforces the 400-line limit across Rust, C/C++ headers and sources, and frontend TypeScript/TSX while excluding tests and generated output.

Server-side frontend code reads the hub through `APP_API_URL`, defaulting to `http://localhost:8080` when unset. Browser code never calls the hub directly; reads and mutations cross the Hub proxy (`frontend/app/hub-proxy.ts` and the `frontend/app/api/tenants/[tenantId]/` routes). `APP_BASE_URL` remains the frontend's public URL for deployment wiring. The public `/.well-known/pandar` document advertises `APP_PUBLIC_API_URL`, falling back to `APP_API_URL`, so Studio can derive its Hub URL from the Web URL; set `APP_PUBLIC_API_URL` when the server-side Hub address is not reachable from Studio.

The dashboard resolves the selected tenant from the `pandar.tenant` cookie (falling back to the first effective tenant), so SSR stays correct without hydration flicker. The sidebar switcher writes the cookie and re-renders the current view, server actions receive their target tenant through hidden form fields, and dashboard navigation hrefs stay tenant-free. The standalone plugin/mobile sign-in flows intentionally keep their explicit tenant picker parameter. Dashboard mutations return typed action states rendered as in-place pending feedback and toasts instead of status redirects.

`APP_AUTH_PROVIDER` selects the browser-facing provider metadata for `pandar-web`. Supported values are `clerk`, `logto`, `betterauth`, or unset/`none`; any other value fails Web startup. Provider-specific frontend metadata is configured with `APP_AUTH_CLERK_PUBLISHABLE_KEY`, `APP_AUTH_LOGTO_ENDPOINT`, `APP_AUTH_LOGTO_APP_ID`, or `APP_AUTH_BETTER_AUTH_BASE_URL`. The frontend still forwards only a bearer token from the configured cookie or static single-user bridge to `pandar-hub`; Pandar tenant membership is resolved by the hub.

Server-side bearer credential precedence:

1. Request cookie named by `APP_AUTH_COOKIE_NAME`, default `pandar_auth_token`.
2. Static deployment bridge `APP_AUTH_BEARER_TOKEN`.
3. Existing service token `APP_API_TOKEN`.

`APP_AUTH_BEARER_TOKEN` and `APP_API_TOKEN` are mutually exclusive static identities for smoke tests or explicitly trusted single-user deployments. Neither may be set when `APP_AUTH_PROVIDER` selects Clerk, Logto, or Better Auth, because a static token would turn every browser request into the same API identity and bypass external user authentication. Set `APP_TENANT_ID` in static-token deployments to bind the dashboard to one tenant without relying on global tenant discovery.

Phase 15 browser-safe live runtime updates:

- `POST /api/v1/tenants/{tenant_id}/printer-events/tickets` issues a tenant-scoped, one-use WebSocket ticket for viewers. Tickets expire after 60 seconds and are stored hashed in SQLite/PostgreSQL so another Hub replica can consume a ticket issued by this replica.
- `GET /api/v1/tenants/{tenant_id}/printer-events` accepts either `Authorization: Bearer <tenant credential>` for non-browser clients or `?ticket=<opaque ticket>` for browser clients.
- `POST /api/tenants/{tenantId}/printer-events/ticket` obtains tickets server-side through the Next.js app. Browser code receives only auth metadata and the opaque ticket, never `APP_API_TOKEN`, `APP_AUTH_BEARER_TOKEN`, or HttpOnly cookie token values.
- Fronting proxies and access logs should redact the `ticket` query parameter.
- The dashboard merges live printer snapshots and job progress without refresh, retries WebSocket connections after 1s, 2s, 5s, and 10s, and marks live status unavailable after 3 failed attempts while continuing to retry.
- The dispatch form calls `POST /api/tenants/{tenantId}/artifact-metadata-preview` after a valid artifact is selected. Preview absence or failure does not disable dispatch. Job history and recovery rows display stored slicer metadata summaries returned by the hub.

### Web print monitor and build-plate recovery

The device view renders task/name, percentage, current/total layer, remaining time, and HMS diagnostics from the enriched printer snapshot. Its live path is socket-first, but the WebSocket is future-only: REST supplies the initial baseline and every reconnect/repair baseline. While the page scheduler is active, repairs run on a serialized 30-second start-to-start cadence. Each baseline has a hard 10-second full-body deadline that includes reading, decoding, and applying the response; events buffered during the fetch replay only when their revision is newer. `visibilitychange` to visible and `pageshow` trigger an immediate serialized repair. Failure clears enriched task/recovery state and marks the channel unavailable rather than presenting stale recovery controls. No wall-clock repair bound applies while the browser is suspended, timers are throttled, or the main thread is stalled.

Build-plate mismatch actions use the native model/action catalog and guards. The browser submits only the action and server-issued occurrence generation; Hub transactionally revalidates generation, error marker, task state, model/catalog support, current Agent session, and capability. Web and Studio plugin native recovery share one printer-level single-flight. For Web recovery, Agent sends sequence ID `0` only on a fresh clean MQTT connection and waits for the matching QoS1 PUBACK. That PUBACK confirms transport only, not printer acceptance or physical recovery, so the mismatch stays visible until later printer telemetry clears it.

Recovery remains limited to one active Hub process. NATS fanout and HTTP/gRPC session affinity do not provide the process-local live-command ownership and pending-dispatch cleanup needed for multi-Hub recovery.

Deploy in this exact order: database migration, dual-capability Agents, all Hubs, confirmation that enriched revisions and the target Agent capability are active, then Web. Roll back by first disabling the Web/server recovery action, then draining every recovery command to a terminal state with no process-local pending dispatch, then rolling back Hub, and finally Agent if needed. Leave the additive database columns in place.

## Bambu Studio Network Plugin

`crates/pandar-network-plugin` builds the Hub-backed adapter separately for the `02.06.00`,
`02.06.01`, `02.07.00`, `02.07.01`, `02.08.00`, and `02.08.01` Bambu Studio ABI series. The shared
catalog pins one exact upstream reference version and commit for each series;
`PANDAR_STUDIO_ABI_SERIES` selects the build and defaults to `02.07.01`. Installed four-part Studio
versions resolve by their first three components, so `02.08.01.55` uses the reviewed `02.08.01` ABI
artifact. The target-header caller uses upstream Boost `1.84.0` and freezes each series' 103, 108, or
109 network exports plus 21 File Transfer exports across the C++/Rust ABI boundary.

Important boundaries:

- The plugin connects only to `pandar-hub`.
- The plugin does not connect directly to `pandar-agent` or Bambu machines.
- The plugin does not store Bambu printer access codes.
- Bambu LAN MQTT and machine file transfer remain agent-local.
- Studio's LAN-shaped connect/message ABI is a Hub-backed virtual/local proxy, not Direct LAN. Only an
  authorized `dev_id` selects a target; host/IP, username, password, and SSL inputs are ignored and
  scrubbed, and the plugin opens no direct printer socket.
- Direct discovery, bind/unbind, certificate ownership, printer sockets, and `ft_*` operations remain
  explicit unsupported results.
- Rust owns typed account/persisted-login policy, session selection, subscriptions, virtual-local
  generations, heartbeat delivery, status construction, and message classification. C++ owns only
  ABI/STL adaptation, callback invocation, and required synchronization.
- A Studio cloud target is eligible when it is selected or explicitly subscribed. Heartbeat delivery
  uses the deduplicated union of both ownership sources. Removing one source retains the target while
  the other remains; only removal of both retires cloud initialization, cloud notifications, and cloud
  tickets. Cloud retirement never retires a Local generation or Local ticket.
- Generic message precedence is firmware -> status -> semantic operation -> unsupported. Status is
  successful only after an eligible current callback delivery; ineligible/subscription/refresh/listener
  failures return `-2`.
- `get_user_tasks` is Hub-backed with authorized filters, pagination, stable ids, and typed metadata;
  it does not return an unconditional empty page.

### Native Bambu Studio firmware updates

The Rust plugin layer serves Bambu Studio's native Firmware page from typed Hub data. A bounded live `info.get_version` refresh returns the printer-main module plus every AMS-family module in the printer's ordered report, including future names. Pandar does not filter by printer model or module prefix and does not impose an artificial aggregate module cap. The printer remains authoritative for command support, versions, and upgrade progress.

Cloud/tunnel devices receive the native page, current status, and command transport end to end. The LAN entrypoints use the same protocol handling, but Studio itself suppresses the firmware-update button for a true LAN-mode `MachineObject`; Pandar does not override or claim otherwise. The catalog response is valid but empty because Pandar does not stage or host packages. It never creates an empty-URL selectable package.

The long-lived Agent report stream is the only durable firmware-status writer. Hub exposes firmware state only for the owning Agent's exact current session and active generation, with strictly newer module and status revisions. Every report stream invalidates a generation before publishing its firmware module or status snapshots. On reconnect or ownership replacement, the Agent first re-establishes printer ownership without reusing firmware state, then establishes the current generation; invalidation/reset clears Studio's retained main and AMS fields until fresh exact-generation telemetry arrives. Lower revisions, stale generations, and replaced sessions cannot restore an older snapshot.

A mutation has two live-only phases at each process boundary:

1. The plugin prepares with Hub without sending an action URL, receives a bounded one-use token, and attempts execute at most once.
2. Hub prepares the exact current capable Agent generation without an action or URL; Agent grants a one-use reservation that expires if execute does not claim it in time.
3. Execute carries the typed command, with a signed URL only for `start`, and rechecks current session/generation immediately before printer publish.
4. A known pre-publish failure fails safely. After execute is attempted, ambiguous HTTP, delivery, or publish outcomes are terminal outcome-unknown behavior: do not retry, redispatch, reconstruct, or replay them. Later long-lived printer telemetry alone reports upgrade progress or completion.

Prepared tokens and reservations expire and stale work is cleaned up. Agent replacement, generation invalidation, disconnect, or process shutdown cancels unexecuted work; potentially published work remains outcome-unknown rather than being sent again. The signed start URL may exist transiently in Studio input, the single plugin execute request, Hub/Agent process memory, the live protobuf execute message, and the printer MQTT payload. It is excluded and redacted from durable payload/result JSON, audit metadata, telemetry, errors, and loggable results.

Firmware session ownership, prepared secrets, and result waiters are process-local. A plugin request that reaches a Hub process which does not own the current Agent session returns unavailable and cannot forward or reconstruct the command. Deploy this firmware path with one active Hub process; the general NATS control plane does not remove this feature-specific limitation.

Roll out in the order Agent, Hub schema/protobuf/plugin endpoints with both SQLite and PostgreSQL migrations, then network plugin. The plugin exposes controls only after the exact current Agent session advertises firmware capability. For rollback, first stop and roll back the network plugin so it cannot start new firmware mutations. Before rolling back Hub or Agent, let the owning Hub's process-local URL/result waiters reach a terminal acknowledgement or explicitly fail them, allow outstanding reservations to expire, and never transfer or replay the work. Then roll back Hub and finally Agent, leaving the additive nullable firmware columns in place. A printer already flashing remains under printer control; rollback neither cancels it nor asserts an outcome.

Local verification is deterministic: typed parser and repository tests, fake MQTT/HTTP peers, lifecycle/race tests, and compiled Cloud/LAN ABI fixtures. The phase-local run historically recorded `PANDAR_TEST_POSTGRES_URL` as unset and skipped real PostgreSQL firmware tests. Final13 disposable PostgreSQL 16.14 validation later passed the 55-case filter twice with zero runtime skip markers. Verification downloaded no external firmware package, sent no live printer firmware command, and adds no new real Bambu Studio compatibility evidence. Web/Android remote OTA plus firmware package staging or hosting remain future C work, not implemented behavior.

### Feature-aware Home and XYZ controls

Agent parses nested printer `print.fun` as typed `BambuDeviceFeatures`: one through sixteen ASCII hexadecimal digits representing the complete unsigned 64-bit bitmap. Canonical serialization is uppercase hexadecimal, including `"0"` for a valid zero bitmap. Named checks currently consume bit 32 for MQTT homing and bit 38 for MQTT axis control, but unknown bits and bit 63 remain intact through Agent protobuf, Hub storage, plugin telemetry, and Studio. The Hub migrations add equivalent nullable text bitmap and observation-session columns for SQLite and PostgreSQL so the full unsigned value is not constrained by either database's signed integer range.

The Hub advertises the stored bitmap to Studio only when the owning Agent's exact current observation session matches and that session declares Agent capability 3. Disconnect, session mismatch, invalidation, or a non-capable Agent produces `fun: "0"`. A modern operation carries a typed required feature through semantic operation JSON and protobuf. Hub checks the exact current session before dispatch, and Agent checks its current-process observation again before MQTT publish; either boundary fails closed without sending a printer command, and a modern request never degrades to a legacy action that Studio can no longer reconstruct. An old Agent cannot execute the additive required-feature shape.

With bit 32, an eligible full Home uses `back_to_center`. With bit 38, an eligible one-axis 1 mm or 10 mm movement uses `xyz_ctrl`; the parser accepts only uppercase X/Y/Z, numeric direction -1/1, and numeric mode 0/1, and Agent does not invert Y or Z again. Legacy or requirement-free fallback preserves `G28`, `G28 X`, and requested multi-axis order. Movement uses the ordered seven-line Studio envelope (`M211 S`, `M211 X1 Y1 Z1`, `M1002 push_ref_mode`, `G91`, one `G1`, `M1002 pop_ref_mode`, `M211 R`) without adding printer-state or axis restrictions.

Roll out this protocol in the order Hub (including both database migrations and session gates) -> Agent -> network plugin. During rollback, first stop the plugin from creating new required-feature operations and drain or fail every queued or sent operation whose `required_device_features` list is non-empty. Only then roll back Agent and Hub; leave the additive nullable columns in place. Required operations tied to a replaced or mismatched session fail and are not sent to an older Agent.

Local operator verification for this implementation recorded 1,063/1,063 workspace tests, 288 Agent tests, 656 Hub tests, 84 network-plugin tests, and two compiled Studio ABI probe tests. The protocol tests use deterministic fakes/loopback peers, and the compiled Windows ABI fixture uses MSVC. Its missing `PANDAR_TEST_POSTGRES_URL` was a historical phase-local skip; final13 backend-parity validation supersedes it with two PostgreSQL 55/55 runs and zero runtime skip markers. No Home or XYZ movement was executed against a real printer, and this evidence must not be recorded as real Studio or hardware validation.

### Typed Studio `gcode_line` passthrough

For an actual typed Studio `gcode_line` wrapper, parsing remains semantic-first: recognized Home, axis, and temperature commands keep their existing semantic operation types. Every other string `param` is carried as typed `GcodeLine { param }` without normalization after JSON decoding. This includes empty strings, multiple lines, LF and CRLF, trailing spaces, final newlines, and final blank lines. Plain unwrapped G-code remains unsupported.

Only `POST /api/v1/plugin/printers/{printer_id}/operations` with an authenticated Studio plugin credential accepts this typed operation; `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/controls` rejects it. The existing 64 KiB limit applies to the complete plugin JSON request, not only `param`; an over-limit request returns HTTP 400 `invalid_printer_control` and creates no command. Hub dispatches queued typed G-code only while the exact current Agent session advertises capability 4. Capability 4 is an Agent wire-compatibility bit, not printer `fun`, and there is no fallback or downgrade for an incapable Agent.

The command uses the existing first-dispatch lifecycle: Hub changes `queued` to `sent` before writing to the gRPC channel. A capable replacement may claim work that is still queued, but disconnect, closed channel, missing acknowledgment, or missing result does not automatically requeue or replay a sent command.

Roll out in the order Hub → Agent → network plugin. For rollback, first stop the plugin from creating new typed G-code operations, then drain or explicitly fail every queued and sent `GcodeLine` command to a terminal state before rolling back Agent or Hub. The feature has no database migration.

Verification is local and deterministic: parser and plugin HTTP tests, compiled Cloud and LAN ABI calls against a loopback Hub, and Hub/Agent conversion and lifecycle tests. The omitted `PANDAR_TEST_POSTGRES_URL` round trip belongs to that historical phase-local run; final13 PostgreSQL 16.14 passed the 55-case filter twice with zero runtime skip markers. No live Studio validation or live-printer movement, Homing, or passthrough G-code execution is claimed.

### Native print-error live operations

Native Studio Resume/Ignore/Stop responses to a current printer error are live-only operations. Their Agent session and pending-owner state are process-local: a request received by a Hub replica that does not own the Agent stream returns `printer_operation_unavailable`, and another replica's stale cleanup cannot identify the owning replica's pending set. This action path therefore requires one active Hub process. HTTP/gRPC session affinity alone is insufficient, and NATS does not add cross-replica live-command forwarding or ownership-aware cleanup; both remain unsupported.

Deploy this additive path in the order Agent → Hub with the nullable migration → network plugin. Roll it back in the order network plugin → Hub → Agent, and do not roll Hub or Agent back until every sent/pending command whose payload contains `operation.type:"handle_print_error"` has reached a terminal state. Leave the nullable printer columns in place during binary rollback.

Implemented login flow:

1. Bambu Studio opens the plugin-provided host plus `/sign-in`.
2. The plugin starts a loopback HTTP server on `127.0.0.1:0`; that server is the host returned by `bambu_network_get_bambulab_host`.
3. The local server serves `frontend/plugin-local/dist` with `rust-embed`. The page asks only for the Web URL and reads `/.well-known/pandar` from that Web deployment to discover the Hub URL before sign-in.
4. The local page links to the configured Pandar frontend `/plugin-sign-in` route with the local callback URL.
5. The frontend reads the Hub's public `/api/v1/auth/status` endpoint for the external-auth enabled/readiness booleans; `/readyz` remains private to the observability listener. It then relies on the configured Pandar auth token/cookie bridge and tenant selection through Pandar-managed membership. With Better Auth, `/plugin-sign-in` adds a versioned base64url return intent to the issuer URL; magic-link and passkey completion carry that opaque value back through the dashboard callback, which accepts only `/plugin-sign-in` and never copies the JWT into the return target.
6. The hub issues a short-lived one-use plugin login ticket.
7. The page uses Studio's `get_localhost_url` message and redirects to Studio's local HTTP server with `ticket` and `redirect_url`.
8. Studio calls the plugin's `get_my_token(ticket)` and `get_my_profile(token)` ABI methods.
9. The plugin exchanges the ticket with the selected hub, creating a tenant-owned `["plugin:studio"]` credential. The ABI shim stores Bambu-shaped login state for Studio UI compatibility.
10. Hub-backed plugin calls read printers/jobs and submit prints through `/api/v1/plugin/*` routes using the plugin credential.

The initial plugin Web URL uses `PANDAR_PLUGIN_FRONTEND_URL`, then `APP_BASE_URL`, then `http://localhost:3000`. The local page fetches that deployment's `/.well-known/pandar` document and stores the discovered Hub URL through the local `/config` endpoint. Later hub-facing ABI calls refresh only the Hub URL from that local config; the existing Next.js `/plugin-sign-in` flow remains responsible for authentication and ticket creation.

Plugin credentials are revocable tenant-owned credentials. They do not carry `agent:register`. Phase
23 has automated ABI/status/command/print-task probes and a manual smoke runbook, but real exact-Studio
compatibility claims follow the per-platform evidence in `docs/compatibility/bambu-studio-plugin.md`.

The historical Public Beta final16 Linux candidate froze source `HEAD`
`2ba0d1f2755501ea9e7d4babcf176db40638f643` as `pandar-bambu-final16-019f7b10.tar.gz`: 2,793,904
bytes, archive SHA-256 `24b45dd30c3509c02b609548409f05fa72490512525621dbc0574a05aa62a039`,
and canonical source-tree SHA-256
`c62c92167f466a915400953ec2d0e126bc34b3c6509a747ddee17dce8d52bf30`. The preceding pre-fix
freeze whose SHA-256 begins `6318d190` and ends `ab473` was rejected by P1 review and is not a
candidate.

Final16 Ubuntu 22.04 Nextest run `c9c96abe-5b80-4478-be33-9ceffef62a53` passed 1,808/1,808
executed tests with one configured skip. Fmt, strict workspace Clippy, module-size checks, ABI tool
22/22, release-smoke 25/25, packaged tasks 18/18, the 109-network plus 21-File-Transfer export
contract, and 21 File Transfer entrypoints x 256 ASan/LSan cycles passed. Disposable PostgreSQL
16.14 run `b73d7ce9-d3ab-424b-8d65-b4736e59f24b` passed 7/7 with zero skips; its dedicated
container, network, and volume were removed and their absence was verified.

The exact final16 Linux archive `pandar-final16-linux-amd64-019f7b10.tar.gz` is 24,891,706 bytes
with SHA-256 `023dcad198674c8ad1c20eb9bc34df9ef9685f49dfeca6e6b5ea58188f3a24a3` and sidecar
SHA-256 `bde03e9633839432063d93768e10b0caf845755d216a653e20fa11d1461296f8`. Its only members
are `pandar`, `libpandar_network_plugin.so`, and `libpandar_bambu_source.so`, with respective
SHA-256 values `b1762bfccdfc1f658147b19b23d7016707b5414d14f74be518e0b5663ddb1b22`,
`3bcce9085205d6af67dc9671cf58cd6f9fb694d5a587b43d160dc8b6a9b0712f`, and
`88d34358be39ed3d239aeb317df8f34a92d4652877e86a9849c66e32347c1df2`. The native evidence
archive and sidecar SHA-256 values are
`fe35290675aac4e6ce323a8ebc75bde1c34d373b1df7506f7f8a65b69ffea950` and
`00a560832428e045affad08617646f7e3d322e07c4849d20e5912be6d545595b`.

The final16 official Bambu Studio `02.08.01.55` AppImage, SHA-256
`e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995`, made exactly one
model-task request, received one HTTP 200 response, and produced the ordered lifecycle `request
started` -> `response accepted` -> `callback started` -> `callback returned` exactly once. Its
evidence-manifest SHA-256 is
`c6ba9b6282581119d3baec720e26990ad63efc20eb394b0c71dced89081d5fd9`. This controlled gate
uses a synthetic persisted session and loopback mock; it is not real authentication, Hub, Agent,
database, printer, hardware, print, or firmware evidence. No GitHub Action or Windows Studio process
was used.

Historical final14 Linux evidence freezes source `HEAD`
`2ba0d1f2755501ea9e7d4babcf176db40638f643` as `pandar-bambu-final14-019f7b10.tar.gz`: 2,782,539
bytes and 1,548 regular members, archive SHA-256
`c422d80d89052732db6b8ae87b68fd1e4145c64f588d8382deafef3345d86681`, member-list SHA-256
`5b32472c9372a992c23315d9b33691a0f269248b65db312590ed00556e21aac0`, canonical tree SHA-256
`43a4a577fb90327dad9e59bcb89dc1e91352bad83f27786a32cae34cb62136e5`, and freeze-evidence
SHA-256 `70d545770086c6acde271d3181508adf4f0d91fc8213771363ec78b2792f5ec3`. Determinism passed;
unsafe-member, duplicate, case-collision, reparse-point, membership-diff, and content-diff counts were
zero. Pre-freeze Web 38 files/327 tests, Auth 3 files/9 tests, both typechecks and production builds,
zero-warning Web lint, and the Better Auth callback smoke passed.

Historical final14 Ubuntu 22.04 Nextest run `d2231751-1284-46b0-aee6-2e041ca1a203` passed
1,781/1,781 with one separately reported skip in 812.413 seconds. Fmt, strict Clippy, module-size 2/2,
release-smoke-tool 21/21, the 109-network plus 21-File-Transfer contract in all five native modes,
and 21 File Transfer entrypoints x 256 ASan/LSan cycles passed. Its Linux archive
`pandar-final14-linux-amd64-019f7b10.tar.gz` is 24,854,111 bytes with SHA-256
`4e91f2457197532102544b02d4edac5354dc2982ec55fa707a057cbcba518b68`; its evidence bundle
SHA-256 is `db6a464ce6b9b4b5e4689e1f0f21962dd097349056e78beb57a8779e1352cb02`.
The strict Clippy command exited successfully with Rust `-D warnings`; its captured build log still
contains C++ missing-field-initializer diagnostics and a dependency future-incompatibility warning, so
that final14 gate is not described as warning-free.

Historical final14 official-AppImage attempt 1 loaded both candidate libraries 4/4 and retained
Studio PID `137`/start ticks `193373032` across two offline failures and one successful development
no-auth commit. Undefined-symbol, `dlopen`, certificate, and missing-library counts were zero. Redacted
evidence `pandar-final14-appimage-redacted-evidence-019f7b10.tar.gz` is 10,603 bytes with 23 members
and SHA-256 `7eac6abbc7364928147d60dd1c583d084c02debf1552734bc82a4dec59c941be`.
It explicitly records `authenticated_session_claim=false`; authenticated Studio, real Windows Studio,
macOS, model-task `get_subtask`, hardware actions, and live firmware were not covered by final14.

The historical final13 immutable input is `pandar-bambu-final13-019f7b10.tar.gz` at source `HEAD`
`2ba0d1f2755501ea9e7d4babcf176db40638f643`: 2,751,227 bytes and 1,543 regular members, archive
SHA-256 `71080abb1e7392b0440a179b5bca9fd80638de74a614105b8dc11a0f70959c34`, member-list SHA-256
`87a6ad1dfaa404731ed30d7e265303cca64fc4278a478f9c12192c09373eb880`, source-tree SHA-256
`db0b7c3385c29ff0cdee1930a66f554a6845b58907373ef543563b829c245761`, and freeze-evidence
SHA-256 `4d132e16f91365795f54c97f608483c34b55726c5f614f5bb8ffaac2ede1fb7f`. Determinism passed and
all unsafe/duplicate/case/reparse/diff counts were zero. Pre-freeze plugin run
`da32fbc4-f37e-4198-af5e-c35f73512dcb` passed 368/368 with one separately reported skip.

Final13 Windows clean run `90cb6a69-08a5-4421-a661-58e696c374a3` passed 1,778/1,778 with one
separately reported skip in 1,050.084 seconds; the firmware probe passed in 28.858 seconds. Fmt,
zero-warning strict Clippy, module-size 2/2, both standalone tools 21/21, and frontend 37 files/324
tests plus typecheck, zero-warning lint, and production build passed. `npm ci` recorded six audit
vulnerabilities (three moderate and three high); retain that as dependency-audit evidence rather than
relabeling it a Studio-parity failure. Clean evidence SHA-256 is
`c1ac8807a427ae4b7003681e9ad343d668dab1d6aa7c143d14bc699fe58b7b89`.

Historical final13 PostgreSQL 16.14 harness `0c292295-f9ab-459b-89c2-ea74f2c9ff56` ran
`24b49c19-cd07-42b5-a5a3-6d220345bd7e` and `1f4b8458-6397-4c0b-8ab3-23d37779c68a`; each passed
55/55 with 831 filtered and zero runtime skip markers. Per-run log SHA-256 values are
`b123f495e09de3c57c2c175000a37cc1fa7395dd0a9c52f1c2f72426c2f4dc08` and
`b3e233f50fe1be9df43867e34307fd6193f09a2dc00940318bdfb8827f0a8d54`; normalized evidence SHA-256 is
`7e04ae355f7bca3fb409bbc700b5c8f160194c0d2f9ec82df823c859566a2db7`; source read-only and
cleanup checks passed.

The final13 Windows archive `pandar-final13-windows-amd64-019f7b10.tar.gz` is 21,285,752 bytes with
SHA-256 `6c50e77a0b4008ce46d86de51411117061c5118e18849ca1fb94f4a3f319db64`; native evidence
SHA-256 is `3dab4bffa359e4c46eec77cbfb278ce3a1497f806a1d80343a1735b5a68f025b`. The MSVC candidate
passed exact three-file layout, packaged CLI execution, all five ABI modes, the 109-network plus
21-File-Transfer contract, 21/21 ABI and packaged-smoke checks, and companion sentinel/no-`Bambu_*`
inspection. `dumpbin` reported 271 total plugin exports. Six pre-product manifest-harness calibration
attempts are retained as infrastructure-only history. Build, ABI, and smoke runs were
`0430ad0e-7f96-41c5-b9aa-1c6fd690fd16`, `2f27f859-b795-4420-b04a-30410ae7bcbc`, and
`65ffc0b0-e17e-45da-bd3a-3375f5d88de1`. Real Windows Studio remains untested.

Final13 Linux attempt 2 passed the full native/ASan gate. Nextest run
`6ec3a215-9430-4ad2-adc7-f692ca156333` passed 1,779/1,779 with one separately reported skip; all five
ABI modes, the exact three-file package, runtime audit, and 21 File Transfer entrypoints x 256
ASan/LSan cycles passed. Archive and evidence-bundle SHA-256 values are
`4166e6012e6c1bf7cdf056ba3bfb28f0fbc9d216c31e5ed2e8620adb8b5fcccc` and
`aa7478fe0f74debcc5f3d1f5ec53a2222d726beafe5224935aa3382c24f6097a`. Attempt 1 run
`c8a134c4-e775-4f37-b6ed-74ccb1b79123` remains non-promotable outer-harness history. Final13
exact-AppImage attempt 8 passed the official Ubuntu 22.04 `02.08.01.55` module-load and same-process
development no-auth recovery gate. The AppImage and redacted evidence SHA-256 values are
`e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995` and
`a4453c8dce3829cc1a84a372a772b516812fe1564b310e61db9e9009a11cf9d2`. Studio PID `137`/ticks
`192688662` remained unchanged across two offline failures and one success/commit; both libraries
mapped 4/4, active/total token count was `1/1`, create/revoke/discard counts were `1/0/0`, and loader/
certificate error counts were zero. The final implementation review returned `APPROVE` with no
Blocking, Important, or Minor finding; the final evidence-document review completed after correcting
its sole Minor terminology finding.

The historical final12 build input is `pandar-bambu-final12-019f7b10.tar.gz` at source HEAD
`2ba0d1f2755501ea9e7d4babcf176db40638f643`: 2,740,698 bytes and 1,543 regular members, archive
SHA-256 `17371828ef7a26cace73cfbed321d094bf38323670e8fa6ccf69d6cbfd4b7eee`, canonical member-list
SHA-256 `87a6ad1dfaa404731ed30d7e265303cca64fc4278a478f9c12192c09373eb880`, and source-tree/manifest
SHA-256 `5aa0038dbc3f0962cc172646876263b0db04e1e6df5fbe571553af1967f242a6`.

The frozen final12 disposable PostgreSQL 16.14 validation used harness run
`3e00d36c-7fb9-47d3-b71b-d9735ebe0eae` and Nextest run
`0b708279-6183-4477-9f78-31add8d7f423`; 55/55 focused cases passed, 831 tests were filtered out,
and the evidence contained zero skip markers. Its evidence SHA-256 is
`d7f002f5be8708844cce406895503ef7056b634bf04aad068722eb25ef15247e`. The final12 Windows
amd64 archive `pandar-final12-windows-amd64-019f7b10.tar.gz` has SHA-256
`b4f6913eef7c1d09da9377fbce36b0ab759add25caac2baa0604c07a595440cb`; its native evidence
SHA-256 is `11c38eb3c198cd07b2f96abbfbf70792b078170389e8869b230badbb98a404d2`. The native MSVC
candidate passed exact three-file layout, packaged CLI execution, 109 network plus 21 File Transfer
exports, all five ABI modes, companion PE/sentinel checks, and the 21-case packaged release smoke.
Real Windows Studio remains untested.

The final12 Windows clean gate passed, but subsequent Linux validation exposed the background-
refresh/firmware-callback race, so all final12 results are retained only as historical regression
evidence. The prior final11 clean run `c6a28ae0-1489-4b08-afda-7497be5668cf` (workspace Nextest
1,749/1,749 plus frontend 37 files/324 tests, typecheck, lint, and production build) is historical too.
The historical final11 Ubuntu 22.04 archive SHA-256 is
`7b7ac417e1c781fbb682552676822457cac6f57a1eb1dd288f2d851f1181a0c6`; it passed workspace
Nextest 1,750/1,750, the full ABI/release-smoke path, and 21 File Transfer entrypoints x 256
ASan/LSan ownership cycles. All three historical Linux files require at most `GLIBC_2.34`; none is the
current candidate. Final16 remains historical Public Beta Linux native/ASan evidence with the narrow
exact-AppImage model-task request/response/callback evidence described above. Stable `02.07.01.62`
Linux and authenticated Studio through Hub and Agent, real Windows Studio, printer hardware, print
actions, and live firmware remain untested.

Studio's `dev_id` is resolved to an authorized Hub printer, but only the Hub printer id crosses the
HTTP boundary. Hub owns stable Studio submission/task ids, durable job/command state, plate and
subtask metadata, authorization, and cancellation races. The plugin reports only milestones actually
observed at that boundary; Agent owns artifact transfer and the final Bambu MQTT `project_file`
translation. Roll out equivalent SQLite/PostgreSQL migrations first, then Agent, Hub, and finally the
network plugin plus BambuSource. Roll back by stopping the plugin producer, draining or explicitly
failing nonterminal Studio jobs/commands, then rolling back Hub and Agent while leaving additive
columns in place. Studio must be stopped before replacing or restoring both plugin libraries.

Pinned Studio also requires a platform BambuSource library before it creates the network agent.
`crates/pandar-bambu-source` supplies `pandar_bambu_source_sentinel` plus the pinned 21-entry local-
media ABI. It accepts only the network plugin's authenticated one-use loopback MJPEG relay and is not
cloud/TUTK/Agora media, recording, discovery, or direct machine-transport support.

Compatibility references:

- `docs/compatibility/bambu-studio-plugin.md`
- `docs/compatibility/bambu-studio-plugin-smoke.md`

Build and inspect the plugin:

```bash
cargo test -p pandar-network-plugin
cargo build -p pandar-network-plugin -p pandar-bambu-source
```

The output libraries are under `target/{debug,release}`. A current release candidate contains exactly
the CLI, network plugin, and BambuSource companion at top level. Native release-smoke covers
`linux-amd64`, `macos-amd64`, `macos-arm64`, and `windows-amd64`; it verifies the selected 124-, 129-,
or 130-name network-plugin contract plus the companion sentinel and exact 21 `Bambu_*` exports.
Historical packages with a sentinel-only companion are not current camera candidates.

Install both libraries with the CLI so the companion receives Studio's exact platform name:

```text
pandar install-network-plugin --plugin-file <network-library> --source-file <source-library> --data-dir <BambuStudio-data-dir>
```

Both file flags are optional. Without them, the CLI selects the platform-specific release artifacts
from the current working directory; development builds can continue to pass explicit paths.

On Windows `02.07.01.x`, the unified `pandar-studio-hook` can install the latest native hook/plugin
bundle directly from GitHub Release:

```text
pandar install-studio-hook --studio-dir <Bambu-Studio-program-dir> --data-dir <BambuStudio-data-dir>
```

The installer verifies the release sidecar before extracting the exact three bundle members, installs
the network plugin and BambuSource companion, and writes
`%LOCALAPPDATA%/Pandar/studio-hook/networking_plugins.zip`. The injected `swscale-8.dll`
proxy patches Windows `MoveFileExW`/`MoveFileW` imports and substitutes only Studio's final
`networking_plugins.zip` rename. The replacement is file-version-gated to Studio `2.7.1.*`; a missing
cache blocks that plugin install rather than allowing an official-plugin fallback. The existing log
key patch is enabled independently with `PANDAR_STUDIO_LOG_LOCAL_KEY=1`.

Typical Studio data/plugin locations:

- Linux AppImage or extracted builds: use the installer to place both exact library names in Studio's
  data-directory `plugins` folder, then start the same extracted Studio tree.
- Windows: use the installer to place both exact DLL names in Studio's data-directory `plugins` folder
  and keep both originals for rollback.
- macOS: install both exact dylib names from the matching archive. Local `02.07.01.62` arm64
  module-load evidence exists; distribution signing/notarization and macOS x86_64 Studio loading
  remain separate release gates.

The historical Public Beta final16 official-AppImage gate used AppImage SHA-256
`e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995`, made exactly one
model-task request, received exactly one HTTP 200, and completed one ordered four-event lifecycle
through callback return. Its evidence-manifest SHA-256 is
`c6ba9b6282581119d3baec720e26990ad63efc20eb394b0c71dced89081d5fd9`. It used a synthetic
persisted session and loopback mock, not real authentication, Hub, Agent, database, printer, hardware,
print, or firmware. No GitHub Action or Windows Studio process was used.
The redacted bundle `pandar-final16-real-studio-evidence-019f7b10.tar.gz` is 245,225 bytes with
SHA-256 `f07c369ad9e0354ef40142294d9385e9c454fd534a04badce4be000f49c06eca`; a second
independent generation matched byte-for-byte. It contains only safe `evidence/` and `outer/`
artifacts, with no runner or mock implementation and no synthetic token contents.
Its `.sha256` sidecar has SHA-256
`30c6e5d43b74f9770d19638b86cefddd96d4d861c16155c74d30b488adf7f1b6`, and
`sha256sum --check` passed.

Historical final14 official-AppImage attempt 1 mapped both passed-package libraries 4/4 and retained
Studio PID `137`/start ticks `193373032` while two proven pre-delivery failures were followed by one
successful development no-auth commit. Redacted evidence SHA-256 is
`7eac6abbc7364928147d60dd1c583d084c02debf1552734bc82a4dec59c941be`; its 23 members contain
only hashes and redacted summaries. The run captured no login content, raw database, or key and used no
Setup Wizard interaction, UI injection, authenticated account, Agent, printer, hardware, or firmware.

The historical final13 official AppImage attempt 8 mapped both passed-package libraries 4/4 and retained
Studio PID `137`/start ticks `192688662` while two proven pre-delivery failures were followed by one
successful development no-auth commit. Redacted evidence SHA-256 is
`a4453c8dce3829cc1a84a372a772b516812fe1564b310e61db9e9009a11cf9d2`; the login content, raw
runner state, and database files were not retrieved. No Setup Wizard interaction, UI injection,
authenticated account, Agent, printer, hardware action, or firmware action occurred.

An earlier exact Linux packaged-library load passed but was superseded. The historical final11 official AppImage
run mapped both Ubuntu 22.04 libraries and kept the same Studio process alive across two proven
pre-delivery connection failures; after Hub became ready, retry produced one successful no-auth
response, one committed active plugin session, and one create audit with no duplicate or discarded
credential. The evidence file SHA-256 is
`cc8a0ef1f16bfc3a109345f9ada4e15096ca5fcf6f6b50c82387cce53aee55dd`. Its authenticated
desktop-session checklist, Windows real Studio, macOS, and live printer actions remain separate
evidence gates. A development no-auth or no-print Studio result must not be presented as external
sign-in, print, cancel, command, or hardware validation.

## Authentication And Provisioning

Tenant API clients currently send:

```text
Authorization: Bearer <tenant api token>
```

Roles are `tenant_admin`, `operator`, and `viewer`. Tenant-scoped read APIs and printer event WebSockets require at least `viewer`; print jobs and refresh commands require `operator`; agent creation requires `tenant_admin`.

Tenant-owned scoped tokens are the bearer credential model:

- `tenant_tokens` belong directly to `tenant_id`, not `user_id`.
- Empty `scopes` means read-only tenant access.
- `["*"]` means all tenant-scoped API and agent-registration capabilities.
- `["agent:register"]` means the token can register or rotate agents but cannot read or mutate ordinary tenant API resources.
- `["plugin:studio"]` is used for Bambu Studio plugin credentials issued from login-ticket exchange.
- `created_by_user_id` is nullable audit metadata, not an authorization source.

External identity configuration:

```bash
PANDAR_EXTERNAL_AUTH_PROVIDER=clerk
PANDAR_EXTERNAL_AUTH_ISSUER=https://example.clerk.accounts.dev
PANDAR_EXTERNAL_AUTH_JWKS_URL=https://example.clerk.accounts.dev/.well-known/jwks.json
PANDAR_EXTERNAL_AUTH_AUDIENCE=<required audience>
PANDAR_EXTERNAL_AUTH_ALGORITHMS=RS256
PANDAR_EXTERNAL_AUTH_AUTHORIZED_PARTIES=<optional comma-separated origins>
PANDAR_EXTERNAL_AUTH_REQUIRED_SCOPES=<optional comma-separated scopes>
PANDAR_EXTERNAL_AUTH_LEEWAY_SECONDS=60
PANDAR_EXTERNAL_AUTH_MAX_TOKEN_LIFETIME_SECONDS=86400
PANDAR_AUTH_ALLOW_TENANT_SELF_CREATE=true
```

If `PANDAR_EXTERNAL_AUTH_PROVIDER` is unset, external identity auth is disabled. Partial external-auth configuration fails hub startup instead of silently falling back. External JWTs must include `iat`; Pandar rejects future issuance times and enforces the configured maximum across `exp - iat` rather than only checking remaining validity. `PANDAR_AUTH_ALLOW_TENANT_SELF_CREATE` defaults to `true`; set it to `false` to require join links or bootstrap provisioning for first tenant membership.

For no-auth local development, set `PANDAR_HUB_NO_AUTH=true` on `pandar-hub` and leave `APP_AUTH_PROVIDER`, `APP_API_TOKEN`, and `APP_AUTH_BEARER_TOKEN` unset on `pandar-web`. This exposes all hub HTTP/WebSocket tenant and bootstrap APIs without a bearer token, so do not use it on an untrusted network.

Studio auto-bootstrap remains a development-only convenience. `POST /api/v1/plugin/no-auth-session`
issues a `plugin:studio` credential only when the database contains exactly one tenant. Missing and
ambiguous tenant states fail closed; the ambiguous case returns a stable conflict and commits neither
a token nor a create audit. SQLite reserves the write transaction before the tenant count and
PostgreSQL uses its equivalent locked transaction boundary, so tenant insertion cannot race issuance.

The plugin serializes no-auth bootstrap and retries only a proven connection failure before HTTP
delivery. A retry key binds the Hub generation and account epoch, uses a bounded five-attempt backoff
starting at two seconds and capped at thirty seconds, and admits only one attempt at a time. Hub,
token, account, configuration, logout, or destroy changes fence the captured request. A successful but
stale candidate is not installed and is submitted for revocation instead.

All Studio account files in one config directory are protected by the in-process locks and the
cross-process `.pandar-plugin-account.lock`. Login, pending queue, direct intent, and completion-ledger
updates use atomic namespace replacement/removal plus parent-directory confirmation. A persistence
mutation has three distinct outcomes:

- `Confirmed`: the namespace mutation was published and directory durability was confirmed. Only this
  outcome can install a login candidate or authorize a revocation DELETE.
- `ChangedUnconfirmed`: the namespace change was published, but directory durability could not be
  confirmed. The operation fails closed; login candidates are not admitted and an unconfirmed
  pending/direct intent cannot authorize DELETE.
- Ordinary error: the current canonical namespace is unchanged. This does not claim that a rollback
  has been made crash-durable before its directory metadata reaches stable storage.

Requested logout and passive account loss share one ordered account-transition coordinator but have
different token disposition. Passive loss clears local state without revoking the Hub token; a
concurrent requested logout upgrades that same transition before finalization. Requested logout first
seeks a `Confirmed` entry in `pandar-plugin-pending-revocations.json`. If pending staging cannot be
confirmed, it must persist a `Confirmed` `pandar-plugin-direct-revocation.json` intent before calling
`DELETE /api/v1/plugin/session`. A passive transition that is upgraded after clearing runtime state
retains the complete account snapshot and restores it after an ordinary pre-DELETE preparation
failure. A changed-unconfirmed intent fails without DELETE. Staged/direct remote failure remains
replayable, while `401`/`410` is idempotent success.

Successful revocation records only Hub URL plus token SHA-256 in
`pandar-plugin-completed-revocations.json` before clearing recovery state. Login load/store consults
this ledger plus pending/direct state, blocking a stale process from rewriting a revoked token.
Successful direct completion removes a duplicate pending entry best-effort and idempotently; failure
of that duplicate cleanup does not reverse the completed DELETE. The completed ledger is intentionally
unbounded and has no automatic compaction. Clear it manually only after every Studio process using the
config directory is stopped and every corresponding Hub plugin session is revoked, invalid, or
expired. Hub revocation is tenant-scoped, redacted, and creates at most one revoke audit.

Printer-list and task list/plate/subtask calls use the same no-auth recovery contract. A no-auth
`401`/`410` enters the shared rotation coordinator, refreshes the credential once, and replays the
request once. A second authorization failure is returned without recursion, stale account/configuration
responses are rejected, and an authenticated Studio credential never falls back to no-auth issuance.
These behaviors do not change the external-auth sign-in contract.

Better Auth is supported through the same external JWT/JWKS contract. Configure Better Auth 1.6.25's JWT plugin with `keyPairConfig.alg = "RS256"` and configure Pandar verification with `PANDAR_EXTERNAL_AUTH_ALGORITHMS=RS256`. Better Auth delegates key generation to `jose`, where the RSA signing algorithm value is `RS256`; Pandar's smoke check signs a token and confirms the JWT header is `alg: "RS256"` and the JWKS key is `kty: "RSA"`. Pandar expects a stable `sub` plus verified email claims before creating tenant-local user projections.

Self-hosted Better Auth issuer development lives under `frontend/auth/`:

```bash
npm install
PANDAR_AUTH_DATABASE_FILE=/tmp/pandar-auth.db \
PANDAR_AUTH_BASE_URL=http://127.0.0.1:3001 \
PANDAR_AUTH_TRUSTED_ORIGINS=http://127.0.0.1:3000 \
PANDAR_AUTH_DASHBOARD_CALLBACK_URL=http://127.0.0.1:3000/auth/betterauth/callback \
PANDAR_AUTH_DASHBOARD_SIGN_OUT_URL=http://127.0.0.1:3000/auth/betterauth/session \
PANDAR_AUTH_MAGIC_LINK_TTL_SECONDS=1800 \
PANDAR_AUTH_EMAIL_PROVIDER=resend \
PANDAR_AUTH_EMAIL_FROM='Pandar <auth@example.invalid>' \
RESEND_API_KEY=re_test_key \
BETTER_AUTH_SECRET=local-development-secret \
npm run migrate --workspace pandar-auth
node --experimental-strip-types frontend/auth/scripts/smoke-jwt-and-registration.mjs
npm run build:auth
```

The self-hosted issuer signs users in with email magic links by default and auto-creates first-time Better Auth users from verified email links. `PANDAR_AUTH_EMAIL_PROVIDER` must be `resend` or `smtp` at runtime. Resend uses `RESEND_API_KEY` plus `PANDAR_AUTH_EMAIL_FROM`; SMTP uses `PANDAR_AUTH_SMTP_HOST`, `PANDAR_AUTH_SMTP_PORT`, `PANDAR_AUTH_SMTP_USERNAME`, `PANDAR_AUTH_SMTP_PASSWORD`, and optional `PANDAR_AUTH_SMTP_TLS=starttls|tls|none`. Magic links expire after 30 minutes by default. After a magic-link login, `/auth/complete` offers optional passkey binding with a visible Skip action. Plugin return intents are opaque because Better Auth 1.6.25 decodes its magic-link callback value during verification; placing a nested query there directly would lose later `&`-delimited fields such as the Studio callback.

For local end-to-end testing, run `pandar-auth` on port 3001, `pandar-web` on port 3000 with `APP_AUTH_PROVIDER=betterauth`, `APP_AUTH_BETTER_AUTH_BASE_URL=http://127.0.0.1:3001`, and `APP_AUTH_COOKIE_MAX_AGE_SECONDS=43200`, then configure `pandar-hub` with `PANDAR_EXTERNAL_AUTH_PROVIDER=betterauth`, `PANDAR_EXTERNAL_AUTH_ISSUER=http://127.0.0.1:3001`, `PANDAR_EXTERNAL_AUTH_JWKS_URL=http://127.0.0.1:3001/api/auth/jwks`, `PANDAR_EXTERNAL_AUTH_AUDIENCE=http://127.0.0.1:3001`, and `PANDAR_EXTERNAL_AUTH_ALGORITHMS=RS256`.

`BETTER_AUTH_SECRET` signs Better Auth sessions and encrypts Better Auth's stored JWKS private key by default. If that secret changes, clear or re-encrypt the issuer `jwks` table before expecting JWT issuance to continue.

Keep the local `APP_AUTH_COOKIE_MAX_AGE_SECONDS` value aligned with `PANDAR_AUTH_JWT_MAX_AGE_SECONDS` so the browser cookie and issuer token expire together during Better Auth testing.

External-account onboarding APIs:

```bash
curl -sS "$PANDAR_API/api/v1/me" \
  -H "Authorization: Bearer $EXTERNAL_JWT"

curl -sS -X POST "$PANDAR_API/api/v1/onboarding/tenants" \
  -H "Authorization: Bearer $EXTERNAL_JWT" \
  -H "content-type: application/json" \
  -d '{"slug":"acme","display_name":"Acme"}'

curl -sS -X POST "$PANDAR_API/api/v1/tenants/$TENANT_ID/join-links" \
  -H "Authorization: Bearer $TENANT_ADMIN_TOKEN" \
  -H "content-type: application/json" \
  -d '{"role":"operator","email_constraint":"operator@example.com","expires_in_seconds":604800,"max_uses":1}'

curl -sS -X POST "$PANDAR_API/api/v1/join-links/accept" \
  -H "Authorization: Bearer $EXTERNAL_JWT" \
  -H "content-type: application/json" \
  -d '{"token":"pandar_join_..."}'
```

Join-link tokens are returned once and stored only as hashes. Accepting a link creates a tenant-local `users` row and `user_identities` link from the external identity; existing members keep their current role and do not consume a link use.

Bootstrap cross-tenant administration with `PANDAR_BOOTSTRAP_TOKEN`:

```bash
PANDAR_BOOTSTRAP_TOKEN=<long random token>
```

Create a tenant, tenant admin, and first tenant token without database fixtures:

```bash
curl -sS -X POST "$PANDAR_API/api/v1/bootstrap/tenant-admin" \
  -H "Authorization: Bearer $PANDAR_BOOTSTRAP_TOKEN" \
  -H "content-type: application/json" \
  -d '{
    "tenant_slug": "acme",
    "tenant_display_name": "Acme",
    "admin_email": "admin@example.com",
    "admin_display_name": "Admin",
    "api_token_name": "bootstrap-admin"
  }'
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

Manual user creation and identity linking are not exposed. Use external JWT sign-in plus tenant self-create or join links; existing tenant-local roles remain editable.

Tenant-token examples:

```bash

curl -sS -X POST "$PANDAR_API/api/v1/tenants/$TENANT_ID/tenant-tokens" \
  -H "Authorization: Bearer $TENANT_ADMIN_TOKEN" \
  -H "content-type: application/json" \
  -d '{"name":"automation","scopes":["*"],"expires_at":null}'

curl -sS -X POST "$PANDAR_API/api/v1/tenants/$TENANT_ID/tenant-tokens/$TOKEN_ID/rotate" \
  -H "Authorization: Bearer $TENANT_ADMIN_TOKEN" \
  -H "content-type: application/json" \
  -d '{"expires_at":null}'

curl -sS -X DELETE "$PANDAR_API/api/v1/tenants/$TENANT_ID/tenant-tokens/$TOKEN_ID" \
  -H "Authorization: Bearer $TENANT_ADMIN_TOKEN"
```

Agent setup should use the pairing bundle API instead of hand-copying IDs from separate responses:

```bash
curl -sS -X POST "$PANDAR_API/api/v1/tenants/$TENANT_ID/agent-pairings" \
  -H "Authorization: Bearer $TENANT_ADMIN_TOKEN" \
  -H "content-type: application/json" \
  -d '{"name":"workshop-agent"}'
```

The pairing bundle returns `PANDAR_TENANT_ID`, `PANDAR_AGENT_ID`, `PANDAR_AGENT_NAME`, and `PANDAR_AGENT_CREDENTIAL`. Store the credential only in the agent runtime environment.

Hub audit records are stored in `audit_events` for successful user-triggered mutations such as agent creation, refresh commands, and print job creation. Bambu printer access codes from successful links are encrypted at rest with `PANDAR_PRINTER_ACCESS_CODE_KEY`; they must never appear in command rows, audit metadata, logs, frontend environment variables, or unencrypted backups.

## Operations

Readiness and metrics:

- `GET /readyz` on `PANDAR_HUB_OBSERVABILITY_BIND` checks database access, artifact storage access, scaled storage topology, gRPC bind configuration, and external-auth JWKS readiness when configured. Public details are sanitized.
- `GET /metrics` on `PANDAR_HUB_OBSERVABILITY_BIND` exposes Prometheus text metrics for agent sessions, command/job/report counters, WebSocket tickets/subscriptions, control-plane publish/receive counters, and readiness gauges. Tenant labels are hashed before export.
- Artifact uploads reserve tenant/global capacity in the database before writing filesystem/S3 data and hold that reservation through the artifact-row commit. They default to 1 GiB/10,000 objects per tenant and 10 GiB/100,000 objects globally. Override with `PANDAR_TENANT_ARTIFACT_QUOTA_BYTES`, `PANDAR_TENANT_ARTIFACT_QUOTA_COUNT`, `PANDAR_GLOBAL_ARTIFACT_QUOTA_BYTES`, and `PANDAR_GLOBAL_ARTIFACT_QUOTA_COUNT`. Multipart staging is separately capped at 16 concurrent uploads globally and 2 per tenant, with a 120-second total parse deadline; cancellation removes both partial and completed staging files.
- Retained camera HTTP responses are limited per tenant, defaulting to 8. Override the limit with the positive integer `PANDAR_HUB_CAMERA_MAX_STREAMS_PER_TENANT`. Hub does not apply a process-wide camera response limit, so production multi-tenant deployments should bound aggregate camera connections at the reverse proxy.

Cleanup CLI:

```bash
cargo run -p pandar-app -- cleanup --dry-run
cargo run -p pandar-app -- cleanup --execute
```

Cleanup removes expired or terminal records according to retention environment variables. In execute mode one database transaction removes unreferenced artifact rows and records their object keys in the durable `artifact_deletions` queue. It then drains that queue through the configured storage backend; deletion is idempotent, and storage failures retain the queued key and full failure context for a later cleanup retry without restoring stale ownership rows.

Backup and restore examples:

```bash
sqlite3 pandar.db ".backup 'pandar-backup.db'"
sqlite3 pandar-restored.db ".restore 'pandar-backup.db'"
# Back up the filesystem artifact directory, for example:
tar -C "${PANDAR_SPOOL_DIR:-pandar-spool}" -czf pandar-artifacts.tar.gz .

pg_dump "$PANDAR_DATABASE_URL" > pandar.sql
psql "$PANDAR_DATABASE_URL" < pandar.sql
# Back up the configured S3-compatible bucket with your object-store tooling.
```

## Deployment Examples

```bash
PANDAR_PRINTER_ACCESS_CODE_KEY=<base64url key> APP_API_TOKEN=<tenant token> APP_TENANT_ID=<tenant uuid> docker compose -f docker-compose.sqlite.yml up --build
PANDAR_PRINTER_ACCESS_CODE_KEY=<base64url key> POSTGRES_PASSWORD=<db password> APP_API_TOKEN=<tenant token> APP_TENANT_ID=<tenant uuid> docker compose -f docker-compose.postgres.yml up --build
PANDAR_PRINTER_ACCESS_CODE_KEY=<base64url key> POSTGRES_PASSWORD=<db password> APP_API_TOKEN=<tenant token> APP_TENANT_ID=<tenant uuid> PANDAR_CONTROL_PLANE=nats PANDAR_ARTIFACT_STORAGE=s3 PANDAR_ARTIFACT_S3_BUCKET=<bucket> PANDAR_ARTIFACT_S3_REGION=<region> PANDAR_ARTIFACT_S3_ENDPOINT=<endpoint> PANDAR_ARTIFACT_S3_ACCESS_KEY_ID=<access key> PANDAR_ARTIFACT_S3_SECRET_ACCESS_KEY=<secret> docker compose -f docker-compose.postgres.yml --profile nats up --build
```

`pandar-hub` defaults to the in-process control plane. Use `PANDAR_CONTROL_PLANE=nats` with PostgreSQL and `PANDAR_NATS_URL` for the broker-backed control plane required by horizontally scaled Hub replicas. The compose example above starts one API service with fixed host ports; multiple replicas need an external HTTP/gRPC routing layer and per-container port planning. SQLite rejects the NATS control plane because it is intended for lightweight single-process deployments.

NATS is internal Hub infrastructure only: tenants, browsers, and `pandar-agent` still authenticate to Hub over the existing HTTP/WebSocket/gRPC APIs. PostgreSQL remains the shared fact source. For PostgreSQL plus NATS, use S3-compatible artifact storage, or set `PANDAR_ARTIFACT_FILESYSTEM_SHARED=true` only when every Hub replica truly mounts the same filesystem artifact directory. NATS does not replicate artifacts.

The Phase 26 local HA/failure smoke harness exercises the default cross-Hub contract without live PostgreSQL, NATS, MinIO, cloud S3, or Docker services:

```bash
export PANDAR_PRINTER_ACCESS_CODE_KEY=<base64url key>
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --iterations 2 --concurrency 2
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --scenario storage
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --live-preflight
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --live --iterations 1 --concurrency 2
```

`--dry-run` uses local process fixtures, a shared SQLite database, shared fake object storage, and loopback HTTP/WebSocket only. Treat it as local convergence evidence for command wakeups, WebSocket fanout, plugin calls, storage failures, restart simulation, and terminal report idempotence. It is not live PostgreSQL/NATS/object-storage soak evidence.

`--live-preflight` checks only the disposable live soak environment contract; it does not connect to PostgreSQL, NATS, or object storage and is not live soak evidence. It verifies that the PostgreSQL URL uses a PostgreSQL scheme, contains a disposable marker (`soak`, `disposable`, `ephemeral`, or `test`), and does not contain production markers (`prod` or `production`). It also checks that the NATS URL uses `nats://`, the S3 endpoint uses HTTP(S), and bucket/region/access-key/secret values are not blank or placeholder-looking.

`--live` runs the artifact, fanout, restart, and terminal scenarios against disposable PostgreSQL, NATS, and S3-compatible object storage. `--live --scenario storage` is rejected because storage failure injection is local-only. A successful `--live-preflight` is not live soak evidence; a successful `--live` command with real disposable dependencies is required before updating the live evidence row to passed.

Live soak evidence variables:

- `PANDAR_SOAK_DATABASE_URL`: disposable PostgreSQL database, for example `postgres://pandar_soak@localhost/pandar_soak`.
- `PANDAR_SOAK_NATS_URL`: disposable NATS server, for example `nats://127.0.0.1:4222`.
- `PANDAR_SOAK_ARTIFACT_S3_BUCKET`, `PANDAR_SOAK_ARTIFACT_S3_REGION`, `PANDAR_SOAK_ARTIFACT_S3_ENDPOINT`, `PANDAR_SOAK_ARTIFACT_S3_ACCESS_KEY_ID`, `PANDAR_SOAK_ARTIFACT_S3_SECRET_ACCESS_KEY`: disposable object-storage bucket, for example a local S3-compatible endpoint such as `http://127.0.0.1:9000`.
- `PANDAR_SOAK_NATS_SUBJECT`: optional NATS subject; defaults to `pandar.soak.control`.
- `PANDAR_SOAK_ARTIFACT_S3_FORCE_PATH_STYLE`: optional S3 path-style flag; defaults to `true` and accepts only `true` or `false`.

Do not point live soak at production data. When disposable live dependencies are available, record PostgreSQL latency or transaction-conflict observations, NATS reconnect behavior, object-storage behavior, command output, and commit SHA in `docs/compatibility/phase-26-soak-evidence.md`.

Release packaging references:

- `docs/release-installation.md`
- `docs/compatibility/release-artifacts.md`

## Verification

```bash
cargo fmt
cargo clippy --workspace
cargo nextest run --manifest-path "Cargo.toml" --workspace
npm run build:web
```

Focused hub checks:

```bash
cargo test -p pandar-hub
cargo fmt --check -p pandar-hub
```
