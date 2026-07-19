# Development And Deployment Notes

This document collects local setup, runtime configuration, authentication, and deployment notes that are too detailed for the README.

## Hub Runtime

`pandar-hub` reads `PANDAR_DATABASE_URL` on startup and defaults to:

```bash
sqlite://pandar.db
```

`pandar-hub` listens for HTTP/WebSocket traffic on `PANDAR_HUB_BIND` and defaults to `0.0.0.0:8080`. The reverse agent gRPC listener uses `PANDAR_HUB_GRPC_BIND` and defaults to `0.0.0.0:50051`.

`PANDAR_PRINTER_ACCESS_CODE_KEY` is required and must contain the unpadded base64url encoding of exactly 32 random bytes. Generate it with `openssl rand -base64 32 | tr '+/' '-_' | tr -d '='`, inject the same value into every Hub replica through a secret manager, and back it up separately with the database. The Hub stores successful Bambu access codes only as versioned AES-256-GCM envelopes bound to the tenant and printer serial. Startup transactionally encrypts legacy plaintext rows and clears the plaintext column; an absent, changed, or invalid key prevents startup rather than silently dropping saved printer connections.

Set `PANDAR_HUB_NO_AUTH=true` only for local development or explicitly trusted single-user deployments. In this mode, hub HTTP/WebSocket tenant and bootstrap APIs skip bearer-token authentication and role checks, and startup logs a warning. Agent reverse gRPC connections still require agent credentials.

The hub runs backend-specific SQLx migrations automatically when it connects. SQLite migrations live under `crates/pandar-hub/migrations/sqlite`; PostgreSQL migrations live under `crates/pandar-hub/migrations/postgres`.

Repository and HTTP tests use SQLite by default, including `sqlite::memory:` for API tests. Optional PostgreSQL repository tests run only when `PANDAR_TEST_POSTGRES_URL` points at a disposable PostgreSQL database.

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

`RefreshPrinters` discovers the printer model at refresh time through the Bambu LAN MQTT `info.get_version` command before requesting the normal `pushall` state report. If model discovery cannot publish, time out, or parse the `ota.product_name` field, the refresh command fails and logs the full error chain instead of falling back to `PANDAR_PRINTERS[].model`. The optional configured `model` remains local metadata for paths that still need a conservative compatibility profile.

Reverse sessions require `PANDAR_AGENT_CREDENTIAL`. Tenant admins create or rotate agent credentials through tenant-token-backed pairing and enrollment APIs. Plaintext credentials are returned once and only hashes are stored by the hub.

## Machine Communication

The agent-side MQTT boundary, `RefreshPrinters` gateway path, and machine file-transfer boundary are implemented from reference behavior. Bambu LAN MQTT, FTPS, and BRTC use printer-local TLS certificates, including X.509 v1 certificates, so the agent uses a Bambu-specific verifier instead of platform CA/hostname validation while still verifying handshake signatures against the certificate public key.

Runtime Bambu machine communication:

- MQTT over TLS on port `8883`.
- MQTT username `bblp`, password set to the printer access code.
- Report topic `device/{serial}/report`.
- Request topic `device/{serial}/request`.
- For printers whose MQTT certificate common name differs from their inventory serial, the certificate identity replaces `{serial}` only at the MQTT transport boundary.
- Refresh sends `info.get_version` before `pushing.pushall` and fails closed when the model cannot be discovered.
- Machine file transfer through implicit FTPS on port `990`.
- Protected data mode first, with model-specific clear-data fallback where required.

Unit tests use fakes and must not open real Bambu MQTT or FTPS sockets.

## Hub APIs

Printer inventory and live events:

- `GET /api/v1/tenants/{tenant_id}/printers` lists the latest printers reported for a tenant.
- `GET /api/v1/tenants/{tenant_id}/printers/{printer_id}` returns one tenant-scoped printer.
- `POST /api/v1/tenants/{tenant_id}/agents/{agent_id}/refresh-printers` queues a `refresh_printers` command for a live agent through the command ledger.
- `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/materials:refresh` queues an Agent command to refresh the printer's current AMS/external-spool material snapshot. The Agent synchronizes material patches to Hub over gRPC; browser live updates continue through the tenant printer event stream.
- `GET /api/v1/tenants/{tenant_id}/printer-events` upgrades to a tenant-scoped WebSocket for future `printer_snapshot` and `job_progress` events. It does not replay historical state; clients should load initial state over HTTP and treat WebSocket delivery as best-effort live updates.

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

## Frontend Runtime

Run `npm --prefix frontend run lint`, `npm run test:web`, and `npm run typecheck:web` before submitting frontend changes. The workspace production-module guard (`cargo test -p pandar-core --test module_size`) enforces the 400-line limit across Rust, C/C++ headers and sources, and frontend TypeScript/TSX while excluding tests and generated output.

The frontend reads the hub through `APP_API_URL`, defaulting to `http://localhost:8080` when unset. `APP_BASE_URL` remains the frontend's public URL for deployment wiring.

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

`crates/pandar-network-plugin` builds as a dynamic-library replacement scaffold for Bambu Studio's network plugin ABI. It uses `reference/open-bamboo-networking` for ABI coverage and `reference/BambuStudio` for caller behavior.

Important boundaries:

- The plugin connects only to `pandar-hub`.
- The plugin does not connect directly to `pandar-agent` or Bambu machines.
- The plugin does not store Bambu printer access codes.
- Bambu LAN MQTT and machine file transfer remain agent-local.

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

Local verification is deterministic: typed parser and repository tests, fake MQTT/HTTP peers, lifecycle/race tests, and compiled Cloud/LAN ABI fixtures. `PANDAR_TEST_POSTGRES_URL` was unset, so real PostgreSQL firmware tests were explicitly skipped; SQLite behavior and SQLite/PostgreSQL migration parity were covered. Verification downloaded no external firmware package, sent no live printer firmware command, and adds no new real Bambu Studio compatibility evidence. Web/Android remote OTA plus firmware package staging or hosting remain future C work, not implemented behavior.

### Feature-aware Home and XYZ controls

Agent parses nested printer `print.fun` as typed `BambuDeviceFeatures`: one through sixteen ASCII hexadecimal digits representing the complete unsigned 64-bit bitmap. Canonical serialization is uppercase hexadecimal, including `"0"` for a valid zero bitmap. Named checks currently consume bit 32 for MQTT homing and bit 38 for MQTT axis control, but unknown bits and bit 63 remain intact through Agent protobuf, Hub storage, plugin telemetry, and Studio. The Hub migrations add equivalent nullable text bitmap and observation-session columns for SQLite and PostgreSQL so the full unsigned value is not constrained by either database's signed integer range.

The Hub advertises the stored bitmap to Studio only when the owning Agent's exact current observation session matches and that session declares Agent capability 3. Disconnect, session mismatch, invalidation, or a non-capable Agent produces `fun: "0"`. A modern operation carries a typed required feature through semantic operation JSON and protobuf. Hub checks the exact current session before dispatch, and Agent checks its current-process observation again before MQTT publish; either boundary fails closed without sending a printer command, and a modern request never degrades to a legacy action that Studio can no longer reconstruct. An old Agent cannot execute the additive required-feature shape.

With bit 32, an eligible full Home uses `back_to_center`. With bit 38, an eligible one-axis 1 mm or 10 mm movement uses `xyz_ctrl`; the parser accepts only uppercase X/Y/Z, numeric direction -1/1, and numeric mode 0/1, and Agent does not invert Y or Z again. Legacy or requirement-free fallback preserves `G28`, `G28 X`, and requested multi-axis order. Movement uses the ordered seven-line Studio envelope (`M211 S`, `M211 X1 Y1 Z1`, `M1002 push_ref_mode`, `G91`, one `G1`, `M1002 pop_ref_mode`, `M211 R`) without adding printer-state or axis restrictions.

Roll out this protocol in the order Hub (including both database migrations and session gates) -> Agent -> network plugin. During rollback, first stop the plugin from creating new required-feature operations and drain or fail every queued or sent operation whose `required_device_features` list is non-empty. Only then roll back Agent and Hub; leave the additive nullable columns in place. Required operations tied to a replaced or mismatched session fail and are not sent to an older Agent.

Local operator verification for this implementation recorded 1,063/1,063 workspace tests, 288 Agent tests, 656 Hub tests, 84 network-plugin tests, and two compiled Studio ABI probe tests. The protocol tests use deterministic fakes/loopback peers, and the compiled Windows ABI fixture uses MSVC. `PANDAR_TEST_POSTGRES_URL` was not configured, so the real PostgreSQL device-feature test was explicitly skipped; migration parity and SQLite behavior were still covered locally. No Home or XYZ movement was executed against a real printer, and this evidence must not be recorded as real Studio or hardware validation.

### Typed Studio `gcode_line` passthrough

For an actual typed Studio `gcode_line` wrapper, parsing remains semantic-first: recognized Home, axis, and temperature commands keep their existing semantic operation types. Every other string `param` is carried as typed `GcodeLine { param }` without normalization after JSON decoding. This includes empty strings, multiple lines, LF and CRLF, trailing spaces, final newlines, and final blank lines. Plain unwrapped G-code remains unsupported.

Only `POST /api/v1/plugin/printers/{printer_id}/operations` with an authenticated Studio plugin credential accepts this typed operation; `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/controls` rejects it. The existing 64 KiB limit applies to the complete plugin JSON request, not only `param`; an over-limit request returns HTTP 400 `invalid_printer_control` and creates no command. Hub dispatches queued typed G-code only while the exact current Agent session advertises capability 4. Capability 4 is an Agent wire-compatibility bit, not printer `fun`, and there is no fallback or downgrade for an incapable Agent.

The command uses the existing first-dispatch lifecycle: Hub changes `queued` to `sent` before writing to the gRPC channel. A capable replacement may claim work that is still queued, but disconnect, closed channel, missing acknowledgment, or missing result does not automatically requeue or replay a sent command.

Roll out in the order Hub → Agent → network plugin. For rollback, first stop the plugin from creating new typed G-code operations, then drain or explicitly fail every queued and sent `GcodeLine` command to a terminal state before rolling back Agent or Hub. The feature has no database migration.

Verification is local and deterministic: parser and plugin HTTP tests, compiled Cloud and LAN ABI calls against a loopback Hub, and Hub/Agent conversion and lifecycle tests. `PANDAR_TEST_POSTGRES_URL` was unset, so the real PostgreSQL round trip was skipped. No live Studio validation or live-printer movement, Homing, or passthrough G-code execution is claimed.

### Native print-error live operations

Native Studio Resume/Ignore/Stop responses to a current printer error are live-only operations. Their Agent session and pending-owner state are process-local: a request received by a Hub replica that does not own the Agent stream returns `printer_operation_unavailable`, and another replica's stale cleanup cannot identify the owning replica's pending set. This action path therefore requires one active Hub process. HTTP/gRPC session affinity alone is insufficient, and NATS does not add cross-replica live-command forwarding or ownership-aware cleanup; both remain unsupported.

Deploy this additive path in the order Agent → Hub with the nullable migration → network plugin. Roll it back in the order network plugin → Hub → Agent, and do not roll Hub or Agent back until every sent/pending command whose payload contains `operation.type:"handle_print_error"` has reached a terminal state. Leave the nullable printer columns in place during binary rollback.

Implemented login flow:

1. Bambu Studio opens the plugin-provided host plus `/sign-in`.
2. The plugin starts a loopback HTTP server on `127.0.0.1:0`; that server is the host returned by `bambu_network_get_bambulab_host`.
3. The local server serves `frontend/plugin-local/dist` with `rust-embed`. The page shows default web/hub URLs when no configuration is present and lets the user switch the target server before sign-in.
4. The local page links to the configured Pandar frontend `/plugin-sign-in` route with the local callback URL.
5. The frontend relies on the configured Pandar auth token/cookie bridge and tenant selection through Pandar-managed membership.
6. The hub issues a short-lived one-use plugin login ticket.
7. The page uses Studio's `get_localhost_url` message and redirects to Studio's local HTTP server with `ticket` and `redirect_url`.
8. Studio calls the plugin's `get_my_token(ticket)` and `get_my_profile(token)` ABI methods.
9. The plugin exchanges the ticket with the selected hub, creating a tenant-owned `["plugin:studio"]` credential. The ABI shim stores Bambu-shaped login state for Studio UI compatibility.
10. Hub-backed plugin calls read printers/jobs and submit prints through `/api/v1/plugin/*` routes using the plugin credential.

Plugin URL configuration uses this precedence:

- Frontend URL: `PANDAR_PLUGIN_FRONTEND_URL`, then `APP_BASE_URL`, then `http://localhost:3000`.
- Hub URL: `PANDAR_PLUGIN_HUB_URL`, then `APP_API_URL`, then `http://127.0.0.1:8080`.

The local `/config` endpoint stores an in-process target-server override. Later hub-facing ABI calls refresh only the hub URL from that local config; the existing Next.js `/plugin-sign-in` flow remains responsible for authentication and ticket creation.

Plugin credentials are revocable tenant-owned credentials. They do not carry `agent:register`. Phase 23 adds a compatibility manifest, manual smoke runbook, stable plugin error mapping, and a local ABI probe. Real Bambu Studio compatibility remains unverified until `docs/compatibility/bambu-studio-plugin.md` contains a real Studio evidence row.

Compatibility references:

- `docs/compatibility/bambu-studio-plugin.md`
- `docs/compatibility/bambu-studio-plugin-smoke.md`

Build and inspect the plugin:

```bash
cargo test -p pandar-network-plugin
cargo build -p pandar-network-plugin
```

The output library is under `target/{debug,release}` as `libpandar_network_plugin.so`, `libpandar_network_plugin.dylib`, or `pandar_network_plugin.dll`.

Typical replacement paths:

- Linux AppImage or extracted builds: replace the bundled Bambu network plugin library next to the extracted Studio libraries, then start Studio from that extracted tree.
- Windows: replace the Bambu Studio network plugin DLL in the Studio installation's plugin/library directory and keep the original DLL for rollback.
- macOS: replace the network plugin dylib inside the Bambu Studio `.app` bundle's Frameworks/plugin library area. Gatekeeper signing/notarization for redistributed bundles is not completed by this package.

Packaging and signing are optional and not completed here.

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
PANDAR_EXTERNAL_AUTH_AUDIENCE=<optional audience>
PANDAR_EXTERNAL_AUTH_ALGORITHMS=RS256
PANDAR_EXTERNAL_AUTH_AUTHORIZED_PARTIES=<optional comma-separated origins>
PANDAR_EXTERNAL_AUTH_REQUIRED_SCOPES=<optional comma-separated scopes>
PANDAR_EXTERNAL_AUTH_LEEWAY_SECONDS=60
PANDAR_AUTH_ALLOW_TENANT_SELF_CREATE=true
```

If `PANDAR_EXTERNAL_AUTH_PROVIDER` is unset, external identity auth is disabled. Partial external-auth configuration fails hub startup instead of silently falling back. `PANDAR_AUTH_ALLOW_TENANT_SELF_CREATE` defaults to `true`; set it to `false` to require join links or bootstrap provisioning for first tenant membership.

For no-auth local development, set `PANDAR_HUB_NO_AUTH=true` on `pandar-hub` and leave `APP_AUTH_PROVIDER`, `APP_API_TOKEN`, and `APP_AUTH_BEARER_TOKEN` unset on `pandar-web`. This exposes all hub HTTP/WebSocket tenant and bootstrap APIs without a bearer token, so do not use it on an untrusted network.

Better Auth is supported through the same external JWT/JWKS contract. Configure Better Auth 1.6.23's JWT plugin with `keyPairConfig.alg = "RS256"` and configure Pandar verification with `PANDAR_EXTERNAL_AUTH_ALGORITHMS=RS256`. Better Auth delegates key generation to `jose`, where the RSA signing algorithm value is `RS256`; Pandar's smoke check signs a token and confirms the JWT header is `alg: "RS256"` and the JWKS key is `kty: "RSA"`. Pandar expects a stable `sub` plus verified email claims before creating tenant-local user projections.

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

The self-hosted issuer signs users in with email magic links by default and auto-creates first-time Better Auth users from verified email links. `PANDAR_AUTH_EMAIL_PROVIDER` must be `resend` or `smtp` at runtime. Resend uses `RESEND_API_KEY` plus `PANDAR_AUTH_EMAIL_FROM`; SMTP uses `PANDAR_AUTH_SMTP_HOST`, `PANDAR_AUTH_SMTP_PORT`, `PANDAR_AUTH_SMTP_USERNAME`, `PANDAR_AUTH_SMTP_PASSWORD`, and optional `PANDAR_AUTH_SMTP_TLS=starttls|tls|none`. Magic links expire after 30 minutes by default. After a magic-link login, `/auth/complete` offers optional passkey binding with a visible Skip action.

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

Tenant-admin provisioning examples:

```bash
curl -sS -X POST "$PANDAR_API/api/v1/tenants/$TENANT_ID/users" \
  -H "Authorization: Bearer $TENANT_ADMIN_TOKEN" \
  -H "content-type: application/json" \
  -d '{"email":"operator@example.com","display_name":"Operator","role":"operator"}'

curl -sS -X POST "$PANDAR_API/api/v1/tenants/$TENANT_ID/users/$USER_ID/identities" \
  -H "Authorization: Bearer $TENANT_ADMIN_TOKEN" \
  -H "content-type: application/json" \
  -d '{"provider":"clerk","subject":"user_123"}'
```

Manual user creation and identity linking are transitional/admin-only compatibility APIs. New deployments should use external JWT sign-in plus self-create or join links instead.

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

- `GET /readyz` checks database access, artifact storage access, scaled storage topology, gRPC bind configuration, and external-auth JWKS readiness when configured. Public details are sanitized.
- `GET /metrics` exposes Prometheus text metrics for agent sessions, command/job/report counters, WebSocket tickets/subscriptions, control-plane publish/receive counters, and readiness gauges. Tenant labels are hashed before export.

Cleanup CLI:

```bash
cargo run -p pandar-app -- cleanup --dry-run
cargo run -p pandar-app -- cleanup --execute
```

Cleanup removes expired or terminal records according to retention environment variables. In execute mode it builds the configured artifact storage backend, deletes unreferenced artifact objects before deleting their database rows, and leaves artifact rows for retry if storage deletion fails.

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
