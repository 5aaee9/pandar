# Phase 15 Runtime UX And Notifications Design

## Purpose

Phase 15 turns the existing operations dashboard into a live monitoring surface for day-to-day print operation. The hub already publishes tenant-scoped `printer_snapshot` and `job_progress` WebSocket events, and the frontend already loads initial tenant, agent, printer, diagnostic, and job state over HTTP. This phase connects those pieces without adding a new persistence model or changing Bambu machine communication.

## Scope

In scope:

- Browser-side consumption of authenticated tenant printer events through short-lived WebSocket tickets.
- Live merging of `printer_snapshot` events into the displayed printer inventory.
- Live merging of `job_progress` events into displayed job history.
- Focused operator notifications for:
  - WebSocket disconnect or subscription failure, which is the visible proxy for losing live hub/agent updates.
  - Printer offline snapshots.
  - Hub dispatch, agent upload, or MQTT publish failures reported through the dispatch/job error surface.
  - Physical print failures reported through `job.print`.
  - Physical print completion reported through `job.print`.
- Job history/detail display that separates dispatch status from physical print status and includes artifact metadata, machine diagnostics/error text, material mapping, layers, remaining time, and timestamps.
- A tenant settings panel that exposes existing tenant operational details: linked agents, agent pairing endpoint, API token management endpoint, auth cookie name, printer compatibility actions, and selected command diagnostics.
- Documentation updates for live frontend behavior and browser WebSocket authentication.

Out of scope:

- Clerk or Logto frontend SDK integration, sign-in UI, invite UI, or user identity management UI.
- New database tables, durable notification history, durable WebSocket replay, or cross-tab notification storage.
- New Bambu machine protocol behavior.
- Storing Bambu access codes in the hub or frontend.
- Full tenant-admin CRUD forms for user/token/agent management.

## Existing Constraints

- The Rust hub WebSocket route currently authorizes only the `Authorization: Bearer <token>` header.
- Browser `WebSocket` cannot set custom request headers.
- Frontend HTTP requests already use `apiHeaders()` to forward the request cookie named by `APP_AUTH_COOKIE_NAME`, then `APP_AUTH_BEARER_TOKEN`, then `APP_API_TOKEN`.
- The hub event stream is future-only and best-effort; initial state must still come from HTTP list/detail APIs.
- Frontend package dependencies are intentionally small: Next.js, React, React DOM, Zustand, TypeScript, and Tailwind.

## Authentication Design

Add a hub-supported browser WebSocket ticket path. Service/static bearer tokens and HttpOnly request-cookie tokens must never be sent to browser JavaScript or WebSocket URLs.

New ticket endpoint:

- `POST /api/v1/tenants/{tenant_id}/printer-events/tickets`
- Requires the same tenant `viewer` authorization as the WebSocket route through the existing `Authorization: Bearer <token>` header.
- Returns `{ "ticket": "...", "expires_at": "..." }`.
- Phase 22 supersedes the original in-memory ticket storage: tickets are random, opaque, stored hashed in SQLite/PostgreSQL, tenant-scoped, viewer-scoped, one-use, and short-lived. The implementation target remains 60 seconds.
- Phase 22 persists ticket hashes so sibling Hub replicas can consume tickets issued by another replica.

WebSocket authorization paths:

Authorization precedence:

1. `Authorization: Bearer <token>` header.
2. `ticket=<opaque-ticket>` query parameter.

Header bearer auth remains the canonical service-client path and keeps existing tests valid. Browser clients use only `ticket`, never `access_token`.

Errors remain consistent with existing tenant auth:

- missing both credentials returns `401 missing_auth_token`;
- malformed header returns `401 invalid_auth_token`;
- unknown, expired, consumed, or wrong-tenant ticket returns `401 invalid_auth_token`;
- valid but unlinked or wrong-tenant identities return the existing `403` errors.

Token source policy:

- Request-cookie tokens may be read only by server-side Next.js code and forwarded only to HTTP APIs as headers.
- `APP_AUTH_BEARER_TOKEN` and `APP_API_TOKEN` may be read only by server-side Next.js code and forwarded only to HTTP APIs as headers.
- The browser may receive only the opaque WebSocket ticket and ticket expiry, never the source bearer token.
- If no server-side auth token is available, the dashboard renders the initial HTTP state and marks live updates unavailable.

Security considerations:

- Query-string tickets can still appear in browser history or reverse-proxy request logs. This is acceptable only because tickets are one-use, tenant-scoped, and expire quickly.
- The hub must not log full request URIs containing `ticket`.
- Documentation must state that fronting proxies should redact the `ticket` query parameter from access logs.
- Any future hub URI/request logging middleware must redact `ticket` before emitting logs.

Tests should verify header auth still works, ticket creation requires viewer auth, ticket WebSocket auth works for browser-style clients, and invalid/consumed tickets fail before upgrade.

## Frontend Architecture

Split the dashboard into a server data loader and a client runtime component:

- `app/page.tsx` remains responsible for tenant selection and uncached initial HTTP fetches.
- A new client component receives serializable initial data, the selected tenant, `APP_API_URL`, frontend auth source metadata, and the currently selected command data.
- A Next.js route handler issues browser tickets by calling the hub ticket endpoint with server-side `apiHeaders()`.
- The client component initializes local state from HTTP data, fetches a ticket from the local Next.js route, and opens a WebSocket to `/api/v1/tenants/{tenant_id}/printer-events?ticket=<encoded ticket>`.
- The client component converts `http:`/`https:` API URLs to `ws:`/`wss:` URLs, preserving any configured path prefix from `APP_API_URL`.
- If no tenant or no server-side token is available, the runtime component renders the initial HTTP state and a live-status notification explaining that live updates are unavailable.

Reconnection contract:

- The client fetches a fresh ticket before each WebSocket connect attempt.
- Initial state is `connecting`.
- On close or connect failure, add a "Live connection" notification and retry after 1s, then 2s, then 5s, then 10s for subsequent attempts.
- After three consecutive ticket or connection failures, state becomes `unavailable` while retries continue every 10s.
- If the socket closes after having been live, state becomes `disconnected` until the next successful connection.
- A successful connection resets the retry counter and state becomes `live`.

State management remains local to the dashboard component. Zustand is not required for this phase because there is no cross-page or persisted client state to share.

## Event Merge Semantics

Initial HTTP state is authoritative for records that have not received live updates.

For `printer_snapshot`:

- Upsert by printer `id`.
- Replace the matching printer row with the event payload.
- Add a notification when a future live event changes a printer from any non-`offline` status to `offline`.
- Initial HTTP state does not synthesize offline notifications.
- An offline -> online -> offline cycle may notify again; repeated offline events without an intervening non-offline status must not notify again.

For `job_progress`:

- Upsert by job `id`.
- Replace the matching job row with the event payload.
- Add notifications when:
  - dispatch status changes to `failed` or `job.error` becomes non-empty;
  - `job.print.status` changes to `failed` and `job.print.error` is non-empty or the physical status becomes terminal failed;
  - `job.print.status` changes to `completed`.
- Initial HTTP state does not synthesize failed/completed notifications.
- `cancelled` physical print status is intentionally not a Phase 15 notification trigger.

Notification dedupe is in-memory and based on transition edge plus affected printer/job id plus terminal status/error. Repeated identical terminal events should not spam the operator.

## Runtime Status And Notifications UI

Add a compact live-status panel near the top of the dashboard showing:

- selected tenant;
- WebSocket state: connecting, live, disconnected, unavailable, or error;
- last event timestamp;
- recent notifications, newest first.

Notification copy must identify where the problem appears to have occurred:

- "Live connection" for WebSocket subscription failure or disconnect.
- "Printer state" for offline snapshots.
- "Dispatch/upload/MQTT" for dispatch job failures and `job.error`; the exact sub-stage is taken from existing error text when available.
- "Physical print" for `job.print.status` failures.
- "Print complete" for physical completion.

## Job Detail And History UI

Replace the dense job table with a responsive job history surface that is still compact enough for operations use:

- Show one job row/card per job with stable dimensions and no nested cards.
- Keep dispatch and physical print status as separate badges.
- Surface command id/kind, artifact filename/content type/size, printer id, created/updated timestamps, progress, layers, remaining time, active file, physical start/finish timestamps, material mapping, and error/diagnostic text.
- Use existing format helpers where possible and add only small helpers needed for status copy.
- Do not hide raw IDs that operators need for API or log correlation.

## Tenant Settings Panel

Add a settings panel using existing data only:

- Selected tenant id, slug, and display name.
- Linked agents and current status.
- API paths for agent pairing and API-token management.
- Frontend auth cookie name and whether server-side ticket minting is using a request cookie, `APP_AUTH_BEARER_TOKEN`, `APP_API_TOKEN`, or no token. The panel must not display token values.
- Printer compatibility actions by linking each printer to the existing diagnostic action.

This is informational plus existing diagnostic forms; it does not create new tenant-admin mutation forms.

## Testing

Rust tests:

- Existing header-auth WebSocket tests still pass.
- New test: ticket creation requires at least linked viewer auth.
- New test: browser-style WebSocket request with `ticket` query succeeds after ticket creation.
- New test: reusing a consumed ticket returns `401 invalid_auth_token` before upgrade.
- New test: invalid or wrong-tenant ticket returns `401 invalid_auth_token` before upgrade.

Frontend validation:

- TypeScript build must compile the client dashboard and event types.
- `next build` must succeed.
- If no frontend test runner exists, rely on `npm run build` plus Rust route tests for this phase.

Workspace verification:

- `cargo fmt`
- `cargo clippy --workspace --all-targets`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
- frontend build from `frontend/`

Docs:

- Update `docs/roadmap.md` Phase 15 status and immediate next section.
- Update `docs/architecture.md` frontend and WebSocket sections.
- Update the `README.md` `printer-events` paragraph and auth notes to describe ticket-based browser WebSocket consumption.

## Acceptance Criteria

- A browser can subscribe to the existing tenant event stream without setting custom WebSocket headers and without receiving server-side bearer tokens.
- Header-based WebSocket clients keep working.
- Phase 22 supersedes the original non-persistent ticket plan: WebSocket tickets are short-lived, one-use, tenant-scoped, and stored hashed in SQLite/PostgreSQL.
- The dashboard updates printer and job rows after future WebSocket events without refreshing.
- Operators see concise in-page notifications for live disconnects, offline printers, dispatch/upload/MQTT failures, physical print failures, and print completion.
- Job history clearly distinguishes hub dispatch/agent upload/MQTT failure from physical print failure.
- Tenant settings expose existing pairing/token/compatibility operational paths without storing Bambu credentials.
- Roadmap and architecture docs describe the completed Phase 15 behavior and remaining limits.
