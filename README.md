# Pandar

Bambu Studio cloud alternative.

## Architecture

```text
Client -(HTTP / WebSocket)-> pandar-hub
pandar-agent -(gRPC)-> pandar-hub
pandar-agent -(SFTP / MQTT)-> Bambu machines
```

See [docs/architecture.md](docs/architecture.md) for the reference-derived architecture notes and [docs/roadmap.md](docs/roadmap.md) for the implementation roadmap.

## Workspace

- `crates/pandar-core` - shared domain types.
- `crates/pandar-hub` - Axum API server for users and reverse agent connections.
- `crates/pandar-agent` - deployable local agent for Bambu machine access.
- `crates/pandar-app` - operator CLI.
- `frontend` - Next.js frontend.
- `proto` - gRPC contracts.

Communication with Bambu machines should be implemented from the behavior in `reference/BambuStudio` and `reference/bambuddy`, without copying unrelated application code into the main workspace.

## Development

`pandar-hub` reads `PANDAR_DATABASE_URL` on startup and defaults to:

```bash
sqlite://pandar.db
```

`pandar-hub` listens for HTTP/WebSocket traffic on `PANDAR_HUB_BIND` and defaults to `0.0.0.0:8080`. The reverse agent gRPC listener uses `PANDAR_HUB_GRPC_BIND` and defaults to `0.0.0.0:50051`.

The hub runs backend-specific SQLx migrations automatically when it connects. SQLite migrations live under `crates/pandar-hub/migrations/sqlite`; PostgreSQL migrations live under `crates/pandar-hub/migrations/postgres`.

Repository and HTTP tests use SQLite by default, including `sqlite::memory:` for API tests. Optional PostgreSQL repository tests run only when `PANDAR_TEST_POSTGRES_URL` points at a disposable PostgreSQL database.

`pandar-agent` connects outward to the hub gRPC endpoint. Required local-development identity values are:

```bash
PANDAR_HUB_GRPC_URL=http://127.0.0.1:50051
PANDAR_TENANT_ID=<tenant uuid>
PANDAR_AGENT_ID=<agent uuid>
PANDAR_AGENT_NAME=local-agent
PANDAR_AGENT_VERSION=0.1.0
```

Agent-local Bambu printers are configured explicitly with `PANDAR_PRINTERS`:

```bash
PANDAR_PRINTERS='[{"host":"192.0.2.10","serial":"01S00EXAMPLE","access_code":"12345678","model":"A1 Mini","name":"garage-a1"}]'
```

The value is a JSON array. `host`, `serial`, and `access_code` are required; `model` and `name` are optional. Empty, whitespace, or `[]` means no configured printers and the agent will not open Bambu machine sockets. Invalid printer config fails at startup with `PANDAR_PRINTERS` context.

Phase 3 adds the agent-side MQTT boundary, `RefreshPrinters` gateway path, and machine file-transfer boundary. Bambu LAN MQTT uses printer-local TLS certificates, so the MQTT adapter uses a Bambu-specific rustls verifier policy instead of platform CA/hostname validation. Unit tests use fakes and must not open real Bambu MQTT or FTPS sockets.

Phase 4 adds hub-owned printer inventory/state APIs:

- `GET /api/v1/tenants/{tenant_id}/printers` lists the latest printers reported for a tenant.
- `GET /api/v1/tenants/{tenant_id}/printers/{printer_id}` returns one tenant-scoped printer.
- `POST /api/v1/tenants/{tenant_id}/agents/{agent_id}/refresh-printers` queues a `refresh_printers` command for a live agent through the command ledger.
- `GET /api/v1/tenants/{tenant_id}/printer-events` upgrades to a tenant-scoped WebSocket for future `printer_snapshot` events. It does not replay historical state; clients should load initial state with the HTTP printer list and treat WebSocket delivery as best-effort live updates.

The frontend reads the hub through `APP_API_URL`, defaulting to `http://localhost:8080` when unset. Its Phase 4 dashboard uses HTTP only and fetches summary, tenants, and the first tenant's printers with uncached server-side requests.

Phase 5 adds tenant-scoped print dispatch:

- `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs` accepts `filename`, `content_type`, `artifact_base64`, `plate_id`, `use_ams`, `flow_cali`, and `timelapse`, then creates an artifact, linked command, and job transactionally.
- `GET /api/v1/tenants/{tenant_id}/jobs` lists tenant print jobs.
- `GET /api/v1/tenants/{tenant_id}/jobs/{job_id}` returns one tenant-scoped print job.

`pandar-hub` writes uploaded artifacts under `PANDAR_SPOOL_DIR`, defaulting to `pandar-spool`, and rejects decoded artifacts larger than `PANDAR_MAX_ARTIFACT_BYTES`, defaulting to `10485760`. `pandar-agent` reads job artifacts from `PANDAR_ARTIFACT_ROOT`, defaulting to the current directory, plus the hub-provided relative storage path. In local deployments the hub spool root and agent artifact root must point at the same shared filesystem path or print dispatch fails when the agent reads the artifact.

Print job `succeeded` currently means the agent accepted the command path and completed dispatch work, not that the physical printer finished printing. Physical progress and terminal printer outcome still require later MQTT report reconciliation. The default runtime machine gateway validates configured serials but returns an explicit unavailable error for real FTPS upload until the machine file-transfer runtime is implemented; fake tests cover the upload plus MQTT `project_file` composition boundary.

The frontend includes an HTTP-only print dispatch form for the selected tenant's reported printers. It posts through the Rust hub API using `APP_API_URL`; `APP_BASE_URL` remains the frontend's public URL for deployment wiring.

Phase 6 adds tenant-scoped API token authentication for tenant APIs. HTTP and WebSocket clients send:

```text
Authorization: Bearer <tenant api token>
```

Roles are `tenant_admin`, `operator`, and `viewer`. Tenant-scoped read APIs and printer event WebSockets require at least `viewer`; print jobs and refresh commands require `operator`; agent creation requires `tenant_admin`.

Phase 10 adds optional Clerk/Logto-style external identity authentication. `pandar-hub` still accepts Phase 6 tenant API tokens first; when no API token matches and external auth is configured, it verifies the bearer token as a JWT and resolves `{provider, subject}` through local Pandar user identity links. Clerk/Logto authenticate the user identity only. Pandar's database remains the source of truth for tenant access and tenant roles.

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

The Next.js frontend reads bearer credentials server-side with this precedence:

1. Request cookie named by `APP_AUTH_COOKIE_NAME`, default `pandar_auth_token`.
2. Static deployment bridge `APP_AUTH_BEARER_TOKEN`.
3. Existing service token `APP_API_TOKEN`.

`APP_AUTH_BEARER_TOKEN` is useful for smoke tests or single-user deployments, but it is not a per-browser identity source and should not be used for multi-user browser deployments. Set `APP_TENANT_ID` in deployed frontends to bind the dashboard to one tenant without relying on global tenant discovery. Provider SDK wiring, sign-in UI, invite flows, and user-facing identity-link management are later product phases.

Phase 11 protects cross-tenant administration with `PANDAR_BOOTSTRAP_TOKEN`. Set it on the hub before using `/api/v1/summary`, `/api/v1/tenants`, or the bootstrap endpoint:

```bash
PANDAR_BOOTSTRAP_TOKEN=<long random token>
```

Create a tenant, tenant admin, and first tenant API token without database fixtures:

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

Tenant admins can manage local users, API tokens, and identity links through tenant-scoped APIs. API-token creation returns plaintext only once; revocation makes that token unusable for future bearer authentication.

```bash
curl -sS -X POST "$PANDAR_API/api/v1/tenants/$TENANT_ID/users" \
  -H "Authorization: Bearer $TENANT_ADMIN_TOKEN" \
  -H "content-type: application/json" \
  -d '{"email":"operator@example.com","display_name":"Operator","role":"operator"}'

curl -sS -X POST "$PANDAR_API/api/v1/tenants/$TENANT_ID/users/$USER_ID/identities" \
  -H "Authorization: Bearer $TENANT_ADMIN_TOKEN" \
  -H "content-type: application/json" \
  -d '{"provider":"clerk","subject":"user_123"}'

curl -sS -X POST "$PANDAR_API/api/v1/tenants/$TENANT_ID/users/$USER_ID/api-tokens" \
  -H "Authorization: Bearer $TENANT_ADMIN_TOKEN" \
  -H "content-type: application/json" \
  -d '{"name":"automation"}'

curl -sS -X DELETE "$PANDAR_API/api/v1/tenants/$TENANT_ID/api-tokens/$TOKEN_ID" \
  -H "Authorization: Bearer $TENANT_ADMIN_TOKEN"
```

Agent setup should use the pairing bundle API instead of hand-copying IDs from separate responses:

```bash
curl -sS -X POST "$PANDAR_API/api/v1/tenants/$TENANT_ID/agent-pairings" \
  -H "Authorization: Bearer $TENANT_ADMIN_TOKEN" \
  -H "content-type: application/json" \
  -d '{"name":"workshop-agent"}'
```

The Phase 11 pairing bundle returns `PANDAR_TENANT_ID`, `PANDAR_AGENT_ID`, and `PANDAR_AGENT_NAME`. A later gRPC auth phase will enforce short-lived agent pairing secrets and credential rotation on the reverse connection.

Hub audit records are stored in `audit_events` for successful user-triggered mutations such as agent creation, refresh commands, and print job creation. Bambu printer access codes remain agent-local in `PANDAR_PRINTERS`; do not store them in hub database rows or frontend environment variables.

Deployment examples:

```bash
APP_API_TOKEN=<tenant token> APP_TENANT_ID=<tenant uuid> docker compose -f docker-compose.sqlite.yml up --build
POSTGRES_PASSWORD=<db password> APP_API_TOKEN=<tenant token> APP_TENANT_ID=<tenant uuid> docker compose -f docker-compose.postgres.yml up --build
```

```bash
cargo fmt
cargo clippy --workspace
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Focused hub checks:

```bash
cargo test -p pandar-hub
cargo fmt --check -p pandar-hub
```
