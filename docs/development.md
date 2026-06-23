# Development And Deployment Notes

This document collects local setup, runtime configuration, authentication, and deployment notes that are too detailed for the README.

## Hub Runtime

`pandar-hub` reads `PANDAR_DATABASE_URL` on startup and defaults to:

```bash
sqlite://pandar.db
```

`pandar-hub` listens for HTTP/WebSocket traffic on `PANDAR_HUB_BIND` and defaults to `0.0.0.0:8080`. The reverse agent gRPC listener uses `PANDAR_HUB_GRPC_BIND` and defaults to `0.0.0.0:50051`.

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

Reverse sessions require `PANDAR_AGENT_CREDENTIAL`. Tenant admins create or rotate agent credentials through tenant-token-backed pairing and enrollment APIs. Plaintext credentials are returned once and only hashes are stored by the hub.

## Machine Communication

The agent-side MQTT boundary, `RefreshPrinters` gateway path, and machine file-transfer boundary are implemented from reference behavior. Bambu LAN MQTT uses printer-local TLS certificates, so the MQTT adapter uses a Bambu-specific rustls verifier policy instead of platform CA/hostname validation.

Runtime Bambu machine communication:

- MQTT over TLS on port `8883`.
- MQTT username `bblp`, password set to the printer access code.
- Report topic `device/{serial}/report`.
- Request topic `device/{serial}/request`.
- Machine file transfer through implicit FTPS on port `990`.
- Protected data mode first, with model-specific clear-data fallback where required.

Unit tests use fakes and must not open real Bambu MQTT or FTPS sockets.

## Hub APIs

Printer inventory and live events:

- `GET /api/v1/tenants/{tenant_id}/printers` lists the latest printers reported for a tenant.
- `GET /api/v1/tenants/{tenant_id}/printers/{printer_id}` returns one tenant-scoped printer.
- `POST /api/v1/tenants/{tenant_id}/agents/{agent_id}/refresh-printers` queues a `refresh_printers` command for a live agent through the command ledger.
- `GET /api/v1/tenants/{tenant_id}/printer-events` upgrades to a tenant-scoped WebSocket for future `printer_snapshot` and `job_progress` events. It does not replay historical state; clients should load initial state over HTTP and treat WebSocket delivery as best-effort live updates.

Tenant-scoped print dispatch:

- `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs` accepts `filename`, `content_type`, `artifact_base64`, `plate_id`, `use_ams`, `flow_cali`, `timelapse`, and optional material mapping fields, then creates an artifact, linked command, and job transactionally.
- `GET /api/v1/tenants/{tenant_id}/jobs` lists tenant print jobs.
- `GET /api/v1/tenants/{tenant_id}/jobs/{job_id}` returns one tenant-scoped print job.

`pandar-hub` writes uploaded artifacts under `PANDAR_SPOOL_DIR`, defaulting to `pandar-spool`, and rejects decoded artifacts larger than `PANDAR_MAX_ARTIFACT_BYTES`, defaulting to `10485760`. The frontend displays a 256 MiB artifact cap and configures the Next.js server-action body limit for base64 form submissions. `pandar-agent` reads job artifacts from `PANDAR_ARTIFACT_ROOT`, defaulting to the current directory, plus the hub-provided relative storage path. In local deployments the hub spool root and agent artifact root must point at the same shared filesystem path or print dispatch fails when the agent reads the artifact.

Print job dispatch success means the agent accepted the command path and completed upload/MQTT dispatch work. Physical progress and terminal printer outcome are tracked separately from MQTT reports.

Recovery APIs:

- `POST /api/v1/tenants/{tenant_id}/agents/{agent_id}/refresh-printers` manually refreshes printer state.
- `POST /api/v1/tenants/{tenant_id}/jobs/{job_id}/retry-dispatch` retries dispatch for a failed or cancelled dispatch lifecycle.
- `POST /api/v1/tenants/{tenant_id}/jobs/{job_id}/reprint` queues a reprint from the existing artifact and options.
- `POST /api/v1/tenants/{tenant_id}/jobs/{job_id}/duplicate` creates a new job from the existing artifact with optional printer, plate, and print-flag overrides.

Pause, resume, and stop are not implemented yet; the UI labels them unavailable instead of pretending they are physical-printer controls.

## Frontend Runtime

The frontend reads the hub through `APP_API_URL`, defaulting to `http://localhost:8080` when unset. `APP_BASE_URL` remains the frontend's public URL for deployment wiring.

Server-side bearer credential precedence:

1. Request cookie named by `APP_AUTH_COOKIE_NAME`, default `pandar_auth_token`.
2. Static deployment bridge `APP_AUTH_BEARER_TOKEN`.
3. Existing service token `APP_API_TOKEN`.

`APP_AUTH_BEARER_TOKEN` is useful for smoke tests or single-user deployments, but it is not a per-browser identity source and should not be used for multi-user browser deployments. Set `APP_TENANT_ID` in deployed frontends to bind the dashboard to one tenant without relying on global tenant discovery.

Phase 15 browser-safe live runtime updates:

- `POST /api/v1/tenants/{tenant_id}/printer-events/tickets` issues a tenant-scoped, one-use WebSocket ticket for viewers. Tickets expire after 60 seconds and are stored hashed in SQLite/PostgreSQL so another Hub replica can consume a ticket issued by this replica.
- `GET /api/v1/tenants/{tenant_id}/printer-events` accepts either `Authorization: Bearer <tenant credential>` for non-browser clients or `?ticket=<opaque ticket>` for browser clients.
- `POST /api/tenants/{tenantId}/printer-events/ticket` obtains tickets server-side through the Next.js app. Browser code receives only auth metadata and the opaque ticket, never `APP_API_TOKEN`, `APP_AUTH_BEARER_TOKEN`, or HttpOnly cookie token values.
- Fronting proxies and access logs should redact the `ticket` query parameter.
- The dashboard merges live printer snapshots and job progress without refresh, retries WebSocket connections after 1s, 2s, 5s, and 10s, and marks live status unavailable after 3 failed attempts while continuing to retry.

## Bambu Studio Network Plugin

`crates/pandar-network-plugin` builds as a dynamic-library replacement scaffold for Bambu Studio's network plugin ABI. It uses `reference/open-bamboo-networking` for ABI coverage and `reference/BambuStudio` for caller behavior.

Important boundaries:

- The plugin connects only to `pandar-hub`.
- The plugin does not connect directly to `pandar-agent` or Bambu machines.
- The plugin does not store Bambu printer access codes.
- Bambu LAN MQTT and machine file transfer remain agent-local.

Implemented login flow:

1. Bambu Studio opens the plugin-provided host plus `/sign-in`.
2. The Pandar sign-in page lets the user enter or confirm the Pandar frontend URL when needed.
3. The frontend relies on the configured Pandar auth token/cookie bridge and tenant selection through Pandar-managed membership.
4. The hub issues a short-lived one-use plugin login ticket.
5. The page uses Studio's `get_localhost_url` message and redirects to Studio's local HTTP server with `ticket` and `redirect_url`.
6. Studio calls the plugin's `get_my_token(ticket)` and `get_my_profile(token)` ABI methods.
7. The plugin exchanges the ticket with the hub, creating a tenant-owned `["plugin:studio"]` credential. The ABI shim stores Bambu-shaped login state for Studio UI compatibility.
8. Hub-backed plugin calls read printers/jobs and submit prints through `/api/v1/plugin/*` routes using the plugin credential.

Plugin credentials are revocable tenant-owned credentials. They do not carry `agent:register`. The Phase 21 shim exports all required symbols, delegates login/printer/job/print calls to the hub, and returns stable no-op or unsupported values for direct LAN/printer operations; real Studio compatibility still needs manual Studio testing.

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
```

If `PANDAR_EXTERNAL_AUTH_PROVIDER` is unset, external identity auth is disabled. Partial external-auth configuration fails hub startup instead of silently falling back.

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

Hub audit records are stored in `audit_events` for successful user-triggered mutations such as agent creation, refresh commands, and print job creation. Bambu printer access codes remain agent-local in `PANDAR_PRINTERS`; do not store them in hub database rows or frontend environment variables.

## Operations

Readiness and metrics:

- `GET /readyz` checks database access, artifact spool access, gRPC bind configuration, and external-auth JWKS readiness when configured. Public details are sanitized.
- `GET /metrics` exposes Prometheus text metrics for agent sessions, command/job/report counters, WebSocket tickets/subscriptions, and readiness gauges. Tenant labels are hashed before export.

Cleanup CLI:

```bash
cargo run -p pandar-app -- cleanup --dry-run
cargo run -p pandar-app -- cleanup --execute
```

Cleanup removes expired or terminal records according to retention environment variables and removes artifact files before deleting their database rows. A file-removal failure leaves artifact rows for retry.

Backup and restore examples:

```bash
sqlite3 pandar.db ".backup 'pandar-backup.db'"
sqlite3 pandar-restored.db ".restore 'pandar-backup.db'"

pg_dump "$PANDAR_DATABASE_URL" > pandar.sql
psql "$PANDAR_DATABASE_URL" < pandar.sql
```

## Deployment Examples

```bash
APP_API_TOKEN=<tenant token> APP_TENANT_ID=<tenant uuid> docker compose -f docker-compose.sqlite.yml up --build
POSTGRES_PASSWORD=<db password> APP_API_TOKEN=<tenant token> APP_TENANT_ID=<tenant uuid> docker compose -f docker-compose.postgres.yml up --build
POSTGRES_PASSWORD=<db password> APP_API_TOKEN=<tenant token> APP_TENANT_ID=<tenant uuid> PANDAR_CONTROL_PLANE=nats docker compose -f docker-compose.postgres.yml --profile nats up --build
```

`pandar-hub` defaults to the in-process control plane. Use `PANDAR_CONTROL_PLANE=nats` with PostgreSQL and `PANDAR_NATS_URL` for horizontally scaled Hub replicas. SQLite rejects the NATS control plane because it is intended for lightweight single-process deployments.

NATS is internal Hub infrastructure only: tenants, browsers, and `pandar-agent` still authenticate to Hub over the existing HTTP/WebSocket/gRPC APIs. PostgreSQL remains the shared fact source, and print artifacts still use `PANDAR_SPOOL_DIR`; multi-replica print-job creation needs that directory on shared storage or a later object-storage backend. NATS does not replicate artifacts.

## Verification

```bash
cargo fmt
cargo clippy --workspace
cargo nextest run --manifest-path "Cargo.toml" --workspace
npm --prefix frontend run build
```

Focused hub checks:

```bash
cargo test -p pandar-hub
cargo fmt --check -p pandar-hub
```
