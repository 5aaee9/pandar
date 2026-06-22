# Phase 15 Runtime UX And Notifications Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add browser-safe live printer/job monitoring to the Pandar dashboard with short-lived WebSocket tickets, runtime notifications, richer job history, tenant operational settings, and docs.

**Architecture:** Keep initial state loading in the existing Next.js server component, add a client dashboard runtime for future WebSocket events, and add a hub-local in-memory ticket issuer so browsers never receive source bearer tokens. The Rust hub continues to support header-auth WebSocket clients while browser clients use one-use tenant-scoped tickets.

**Tech Stack:** Rust 2024, axum WebSockets, tokio, time/uuid, Next.js 16 App Router, React 19 client components, TypeScript, Tailwind CSS.

---

## File Structure

- Modify `crates/pandar-hub/src/printer_events.rs`: add in-memory ticket issue/consume support alongside the existing broadcast hub.
- Modify `crates/pandar-hub/src/routes/printer_events.rs`: add ticket endpoint and accept ticket query auth before upgrade.
- Modify `crates/pandar-hub/src/routes.rs`: route `POST /api/v1/tenants/{tenant_id}/printer-events/tickets`.
- Modify `crates/pandar-hub/src/routes/tests/printer_events_ws.rs`: add ticket auth tests and preserve header-auth tests.
- Modify `frontend/app/api-auth.ts`: expose server-side auth source metadata without exposing token values.
- Create `frontend/app/api/tenants/[tenantId]/printer-events/ticket/route.ts`: local Next.js ticket proxy that calls the hub ticket endpoint with `apiHeaders()`.
- Create `frontend/app/dashboard-runtime.tsx`: client runtime state, WebSocket ticket flow, event merge, notifications, and dashboard rendering.
- Modify `frontend/app/page.tsx`: keep data loading, pass initial state into the runtime component, remove duplicated server-rendered dashboard body.
- Modify `frontend/app/dashboard-types.ts`: add printer event, ticket, auth source metadata types, and any response fields already returned by Rust but not typed.
- Modify `frontend/app/dashboard-ui.tsx`: keep shared display helpers and add small reusable layout/status pieces only if used by the runtime.
- Modify `README.md`, `docs/architecture.md`, and `docs/roadmap.md`: document Phase 15 behavior and limits after implementation approval.

## Task 1: Hub WebSocket Ticket Auth

**Files:**
- Modify: `crates/pandar-hub/src/printer_events.rs`
- Modify: `crates/pandar-hub/src/routes/printer_events.rs`
- Modify: `crates/pandar-hub/src/routes.rs`
- Test: `crates/pandar-hub/src/routes/tests/printer_events_ws.rs`

- [ ] **Step 1: Add a failing Rust test for ticket creation auth**

Add a test in `crates/pandar-hub/src/routes/tests/printer_events_ws.rs`:

```rust
#[tokio::test]
async fn printer_events_ticket_requires_linked_viewer() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let linked = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Viewer,
        "ticket-viewer",
    )
    .await;
    let unlinked = jwt_for(
        "unlinked-ticket-viewer",
        TEST_ISSUER,
        TEST_AUDIENCE,
        "test-key",
        300,
    );

    let (status, body) = request(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/printer-events/tickets", tenant.id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, json!({ "error": "missing_auth_token" }));

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/printer-events/tickets", tenant.id),
        None,
        &unlinked,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({ "error": "tenant_forbidden" }));

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{}/printer-events/tickets", tenant.id),
        None,
        &linked,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["ticket"].as_str().is_some_and(|ticket| !ticket.is_empty()));
    assert!(body["expires_at"].as_str().is_some_and(|value| !value.is_empty()));
}
```

- [ ] **Step 2: Add failing Rust tests for ticket WebSocket use**

Add tests covering successful use, one-use enforcement, invalid tickets, and wrong-tenant tickets:

```rust
#[tokio::test]
async fn printer_events_websocket_accepts_browser_ticket_once() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "ticket-ws-token",
    )
    .await;
    let http_addr = serve_http(app.clone()).await;
    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{}/printer-events/tickets", tenant.id),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ticket = body["ticket"].as_str().unwrap();

    let (ws, _) = tokio_tungstenite::connect_async(format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events?ticket={ticket}",
        tenant.id
    ))
    .await
    .unwrap();
    drop(ws);

    let err = tokio_tungstenite::connect_async(format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events?ticket={ticket}",
        tenant.id
    ))
    .await
    .unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("401") || message.contains("Unauthorized"),
        "unexpected reused-ticket error: {message}"
    );
}

#[tokio::test]
async fn printer_events_websocket_rejects_invalid_ticket_before_upgrade() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();

    let (status, body) = request(
        app,
        Method::GET,
        &format!(
            "/api/v1/tenants/{}/printer-events?ticket=not-a-ticket",
            tenant.id
        ),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, json!({ "error": "invalid_auth_token" }));
}

#[tokio::test]
async fn printer_events_websocket_rejects_wrong_tenant_ticket_before_upgrade() {
    let state = state().await;
    let app = router(state.clone());
    let tenant_a = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let tenant_b = state.tenants().create("beta", "Beta Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_a.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "tenant-a-ticket",
    )
    .await;
    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/printer-events/tickets", tenant_a.id),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ticket = body["ticket"].as_str().unwrap();

    let (status, body) = request(
        app,
        Method::GET,
        &format!(
            "/api/v1/tenants/{}/printer-events?ticket={ticket}",
            tenant_b.id
        ),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, json!({ "error": "invalid_auth_token" }));
}
```

- [ ] **Step 3: Run the focused test and confirm it fails**

Run:

```bash
cargo test -p pandar-hub routes::tests::printer_events_ws::printer_events_ticket_requires_linked_viewer -- --nocapture
```

Expected before implementation: failure because `/printer-events/tickets` does not exist.

- [ ] **Step 4: Implement ticket storage in `printer_events.rs`**

Add ticket fields and methods to `PrinterEventHub` while preserving existing broadcast behavior:

```rust
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

const TICKET_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
struct PrinterEventTicket {
    tenant_id: TenantId,
    expires_at: Instant,
}

#[derive(Debug, Clone, Serialize)]
pub struct IssuedPrinterEventTicket {
    pub ticket: String,
    pub expires_at: String,
}
```

Extend `PrinterEventHub` with a `tickets: Arc<Mutex<HashMap<String, PrinterEventTicket>>>` field. Add:

```rust
pub async fn issue_ticket(&self, tenant_id: TenantId) -> IssuedPrinterEventTicket {
    let ticket = uuid::Uuid::new_v4().to_string();
    let expires_at = Instant::now() + TICKET_TTL;
    let expires_at_text = time::OffsetDateTime::now_utc()
        .saturating_add(time::Duration::seconds(TICKET_TTL.as_secs() as i64))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| String::new());
    let mut tickets = self.tickets.lock().await;
    tickets.retain(|_, value| value.expires_at > Instant::now());
    tickets.insert(ticket.clone(), PrinterEventTicket { tenant_id, expires_at });
    IssuedPrinterEventTicket {
        ticket,
        expires_at: expires_at_text,
    }
}

pub async fn consume_ticket(&self, tenant_id: TenantId, ticket: &str) -> bool {
    let mut tickets = self.tickets.lock().await;
    tickets.retain(|_, value| value.expires_at > Instant::now());
    tickets
        .remove(ticket)
        .is_some_and(|value| value.tenant_id == tenant_id && value.expires_at > Instant::now())
}
```

Use `time::Duration` with a fully qualified path if needed to avoid conflicting with `std::time::Duration`.

- [ ] **Step 5: Add ticket route and query auth**

In `crates/pandar-hub/src/routes/printer_events.rs`, add:

```rust
use axum::extract::Query;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub(super) struct PrinterEventQuery {
    ticket: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct PrinterEventTicketResponse {
    ticket: String,
    expires_at: String,
}
```

Change `printer_events` to accept `Query(query): Query<PrinterEventQuery>`. If `Authorization` exists, call `auth::authorize_tenant` as today. If no header exists and `query.ticket` is present, call `state.printer_events().consume_ticket(tenant_id, ticket).await` and return `401 invalid_auth_token` on false. If neither exists, return `401 missing_auth_token`. Do not log the ticket.

Add:

```rust
pub(super) async fn create_printer_event_ticket(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<PrinterEventTicketResponse>, ApiError> {
    let tenant_id = super::parse_tenant_id(&tenant_id)?;
    auth::authorize_tenant(&state, &headers, tenant_id, UserRole::Viewer).await?;
    state.printers().list_for_tenant(tenant_id).await?;
    let issued = state.printer_events().issue_ticket(tenant_id).await;
    Ok(Json(PrinterEventTicketResponse {
        ticket: issued.ticket,
        expires_at: issued.expires_at,
    }))
}
```

- [ ] **Step 6: Register the ticket route**

In `crates/pandar-hub/src/routes.rs`, add before the existing printer-events route:

```rust
.route(
    "/api/v1/tenants/{tenant_id}/printer-events/tickets",
    post(printer_events::create_printer_event_ticket),
)
```

- [ ] **Step 7: Run focused ticket tests**

Run:

```bash
cargo test -p pandar-hub routes::tests::printer_events_ws -- --nocapture
```

Expected after implementation: all printer event WebSocket tests pass.

- [ ] **Step 8: Run formatting for Rust touched files**

Run:

```bash
cargo fmt
```

Expected: exit 0.

## Task 2: Frontend Ticket Proxy And Runtime Types

**Files:**
- Modify: `frontend/app/api-auth.ts`
- Modify: `frontend/app/dashboard-types.ts`
- Create: `frontend/app/api/tenants/[tenantId]/printer-events/ticket/route.ts`

- [ ] **Step 1: Add auth source metadata helper**

Modify `frontend/app/api-auth.ts` so `apiHeaders()` keeps existing behavior and add:

```ts
export type AuthSource = 'request_cookie' | 'app_auth_bearer_token' | 'app_api_token' | 'none'

export async function authSource(): Promise<{ source: AuthSource; cookieName: string }> {
  const cookieStore = await cookies()
  if (cookieStore.get(authCookieName)?.value) {
    return { source: 'request_cookie', cookieName: authCookieName }
  }
  if (staticAuthToken) {
    return { source: 'app_auth_bearer_token', cookieName: authCookieName }
  }
  if (apiToken) {
    return { source: 'app_api_token', cookieName: authCookieName }
  }
  return { source: 'none', cookieName: authCookieName }
}
```

This helper must not return token values.

- [ ] **Step 2: Add dashboard event and ticket types**

Modify `frontend/app/dashboard-types.ts`:

```ts
export type AuthMetadata = {
  source: 'request_cookie' | 'app_auth_bearer_token' | 'app_api_token' | 'none'
  cookieName: string
}

export type PrinterEvent =
  | {
      type: 'printer_snapshot'
      printer: Printer
    }
  | {
      type: 'job_progress'
      job: Job
    }

export type PrinterEventTicket = {
  ticket: string
  expires_at: string
}
```

Extend `Job['artifact']` to include fields the Rust response already returns and the job detail UI needs:

```ts
artifact: {
  id: string
  tenant_id: string
  filename: string
  content_type: string
  size_bytes: number
  storage_path: string
  created_at: string
}
```

- [ ] **Step 3: Create the Next.js ticket proxy route**

Create `frontend/app/api/tenants/[tenantId]/printer-events/ticket/route.ts`:

```ts
import { NextResponse } from 'next/server'

import { apiHeaders } from '../../../../../api-auth'
import type { PrinterEventTicket } from '../../../../../dashboard-types'

const apiUrl = process.env.APP_API_URL ?? 'http://localhost:8080'

type RouteContext = {
  params: Promise<{
    tenantId: string
  }>
}

export async function POST(_request: Request, context: RouteContext) {
  const { tenantId } = await context.params
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${encodeURIComponent(tenantId)}/printer-events/tickets`,
    {
      method: 'POST',
      cache: 'no-store',
      headers: await apiHeaders(),
    },
  )

  if (!response.ok) {
    return NextResponse.json({ error: 'ticket_unavailable' }, { status: response.status })
  }

  const ticket = (await response.json()) as PrinterEventTicket
  return NextResponse.json(ticket)
}
```

If TypeScript rejects the relative import depth, adjust it using the actual file depth. Do not introduce path aliases.

- [ ] **Step 4: Run frontend type/build check**

Run:

```bash
cd frontend && npm run build
```

Expected after Task 2 alone: build may still fail if `page.tsx` has not yet passed `AuthMetadata`; record the exact output. If it fails only because runtime integration is not complete, proceed to Task 3.

## Task 3: Frontend Runtime Dashboard

**Files:**
- Create: `frontend/app/dashboard-runtime.tsx`
- Modify: `frontend/app/page.tsx`
- Modify: `frontend/app/dashboard-ui.tsx`
- Reuse: `frontend/app/diagnostics-panel.tsx`
- Reuse: `frontend/app/dispatch-form.tsx`
- Reuse: `frontend/app/job-format.ts`

- [ ] **Step 1: Create client runtime component shell**

Create `frontend/app/dashboard-runtime.tsx` with:

```tsx
'use client'

import { useEffect, useMemo, useRef, useState } from 'react'

import { DiagnosticsSection, LinkedAgentsSection } from './diagnostics-panel'
import { DispatchForm } from './dispatch-form'
import type {
  Agent,
  AuthMetadata,
  Command,
  CommandResultData,
  Job,
  Printer,
  PrinterEvent,
  PrinterEventTicket,
  Summary,
  Tenant,
  TenantList,
} from './dashboard-types'
import { EmptyState, formatBytes, formatDate, Metric, StatusBadge } from './dashboard-ui'
import { formatLayers, formatProgress, formatRemaining } from './job-format'
```

Define props:

```ts
type RuntimeProps = {
  apiUrl: string
  configuredTenantId: string | undefined
  selectedTenant: Tenant | null
  tenants: Tenant[]
  summary: Summary | null
  printers: Printer[]
  agents: Agent[]
  jobs: Job[]
  selectedCommand: Command | null
  commandData: CommandResultData | null
  errors: string[]
  auth: AuthMetadata
}
```

The first render must match the previous dashboard content using prop state.

- [ ] **Step 2: Move existing dashboard markup from `page.tsx`**

Move the existing `<main>` body from `frontend/app/page.tsx` into `DashboardRuntime`. Keep helper functions `formatPrinterMaterials` and `formatJobMaterial` in `dashboard-runtime.tsx` unless another component already owns them. Replace direct `printers`/`jobs` references with state variables:

```ts
const [runtimePrinters, setRuntimePrinters] = useState(printers)
const [runtimeJobs, setRuntimeJobs] = useState(jobs)

useEffect(() => {
  setRuntimePrinters(printers)
}, [printers])

useEffect(() => {
  setRuntimeJobs(jobs)
}, [jobs])
```

In `page.tsx`, import `DashboardRuntime` and return it after fetching data:

```tsx
return (
  <DashboardRuntime
    apiUrl={apiUrl}
    configuredTenantId={configuredTenantId}
    selectedTenant={selectedTenant}
    tenants={tenants}
    summary={summaryResult.data}
    printers={printers}
    agents={agents}
    jobs={jobs}
    selectedCommand={selectedCommand}
    commandData={commandData}
    errors={errors}
    auth={await authSource()}
  />
)
```

Remove imports from `page.tsx` that are now only used in the client component.

- [ ] **Step 3: Add ticket fetch and WebSocket lifecycle**

In `dashboard-runtime.tsx`, add:

```ts
type LiveState = 'connecting' | 'live' | 'disconnected' | 'unavailable' | 'error'
type NotificationKind =
  | 'live_connection'
  | 'printer_state'
  | 'dispatch_upload_mqtt'
  | 'physical_print'
  | 'print_complete'

type RuntimeNotification = {
  id: string
  kind: NotificationKind
  title: string
  detail: string
  createdAt: string
}
```

Add `buildWebSocketUrl(apiUrl, tenantId, ticket)` that uses `new URL(apiUrl)`, changes protocol to `ws:` or `wss:`, appends `/api/v1/tenants/${tenantId}/printer-events`, and sets `ticket`.

Use `useEffect` to:

- skip when `selectedTenant` is null or `auth.source === 'none'`;
- POST `/api/tenants/${selectedTenant.id}/printer-events/ticket`;
- open `WebSocket` with the returned ticket;
- parse messages as `PrinterEvent`;
- set live state and last event time;
- cleanup by closing the socket and clearing retry timers.

Use retry delays `[1000, 2000, 5000, 10000]`. After three consecutive failures set live state to `unavailable`; continue retrying every 10s.

- [ ] **Step 4: Add event merge and notification functions**

Add pure helpers in `dashboard-runtime.tsx`:

```ts
function upsertPrinter(printers: Printer[], printer: Printer) {
  return upsertById(printers, printer)
}

function upsertJob(jobs: Job[], job: Job) {
  return upsertById(jobs, job)
}

function upsertById<T extends { id: string }>(items: T[], item: T) {
  const index = items.findIndex((candidate) => candidate.id === item.id)
  if (index === -1) {
    return [item, ...items]
  }
  const next = [...items]
  next[index] = item
  return next
}
```

Track previous state from the current runtime arrays before replacement:

- printer offline notification only when previous status exists and is not `offline`, new status is `offline`;
- dispatch notification when previous `job.status !== 'failed'` and new `job.status === 'failed'`, or when previous `job.error !== job.error` and new error exists;
- physical failure notification when previous `job.print.status !== 'failed'` and new `job.print.status === 'failed'`;
- completion notification when previous `job.print.status !== 'completed'` and new `job.print.status === 'completed'`;
- do not notify on initial HTTP state or `cancelled`.

Use a `Set<string>` ref for dedupe keys. Keep only the newest 12 notifications.

- [ ] **Step 5: Add live status panel**

Place a compact panel after errors and before metrics. Include selected tenant display, live status badge, last event timestamp, auth source label, and recent notifications. Do not display token values.

Use copy:

- `Live connection`: `Live updates disconnected; retrying.`
- `Printer state`: `${printer.name} reported offline.`
- `Dispatch/upload/MQTT`: `Dispatch path failed for ${job.artifact.filename}.`
- `Physical print`: `Physical print failed for ${job.artifact.filename}.`
- `Print complete`: `${job.artifact.filename} completed.`

Add error details from `job.error` or `job.print.error` when available.

- [ ] **Step 6: Replace job table with compact job history rows**

In `dashboard-runtime.tsx`, replace the existing job table with a responsive list:

- each job has a bordered row or un-nested section with stable padding;
- first line: artifact filename, dispatch badge, physical badge, progress;
- details: job id, command id/kind, printer id, artifact id/content type/size/storage path, created/updated, active file, start/finish, layers, remaining, material mapping, dispatch error, print error.

Use existing helpers `formatProgress`, `formatLayers`, `formatRemaining`, `formatBytes`, `formatDate`, and `StatusBadge`.

- [ ] **Step 7: Add tenant settings panel**

Add a section near the bottom of `DashboardRuntime`:

- selected tenant display name, slug, id;
- auth source and cookie name without token values;
- API paths:
  - `POST /api/v1/tenants/{tenant_id}/agent-pairings`
  - `GET/POST /api/v1/tenants/{tenant_id}/users/{user_id}/api-tokens`
- linked agents with status;
- printer compatibility actions can reuse existing `DiagnosticsSection` forms already present, so the settings panel should point to the diagnostics section rather than duplicate forms.

- [ ] **Step 8: Run frontend build**

Run:

```bash
cd frontend && npm run build
```

Expected: exit 0.

## Task 4: Documentation And Roadmap

**Files:**
- Modify: `README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update README WebSocket/auth notes**

Update the `printer-events` paragraph to state:

- header bearer auth is supported for service WebSocket clients;
- browsers should use `POST /api/v1/tenants/{tenant_id}/printer-events/tickets` through the frontend server route;
- tickets are one-use, short-lived, in-memory, tenant-scoped, and not durable replay.

Add a deployment note that fronting proxies should redact `ticket` query parameters from logs.

- [ ] **Step 2: Update architecture live frontend section**

In `docs/architecture.md`, update the frontend paragraph to say Phase 15 consumes live events through ticketed browser WebSockets and merges future events over HTTP initial state. Note that the dashboard does not synthesize notifications from initial HTTP state.

- [ ] **Step 3: Update roadmap**

In `docs/roadmap.md`:

- add Phase 15 to Completed;
- mark Phase 15 bullets and exit criteria as completed;
- replace Immediate Next with a concrete later phase such as `Define Phase 16 after product feedback; likely candidates are tenant-admin management UI or provider SDK sign-in wiring.`

- [ ] **Step 4: Run docs diff check**

Run:

```bash
git diff -- README.md docs/architecture.md docs/roadmap.md
```

Expected: docs reflect implemented behavior and do not claim durable replay or Bambu credential storage.

## Task 5: Full Verification And Final Review Prep

**Files:**
- No new feature files; run verification and inspect the diff.

- [ ] **Step 1: Run Rust formatting check**

Run:

```bash
cargo fmt --check
```

Expected: exit 0.

- [ ] **Step 2: Run Rust clippy**

Run:

```bash
cargo clippy --workspace --all-targets
```

Expected: exit 0.

- [ ] **Step 3: Run workspace tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: exit 0.

- [ ] **Step 4: Run frontend build**

Run:

```bash
cd frontend && npm run build
```

Expected: exit 0.

- [ ] **Step 5: Check generated protobuf output is not tracked**

Run:

```bash
git status --short -- ':(glob)**/*.pb.rs' ':(glob)**/*.tonic.rs'
```

Expected: no output.

- [ ] **Step 6: Inspect intended diff**

Run:

```bash
git status --short
git diff --stat
git diff -- crates/pandar-hub/src/printer_events.rs crates/pandar-hub/src/routes/printer_events.rs frontend/app docs README.md
```

Expected: only Phase 15 source/docs/spec/plan files changed.

- [ ] **Step 7: Prepare SDD final implementation review**

Collect:

- reviewed spec path and content;
- reviewed plan path and content;
- base SHA before Phase 15 implementation;
- final diff;
- verification command outputs.

Use these for the SDD-required native reviewer and opencode reviewer. Do not commit until both final implementation reviewers return `VERDICT: APPROVE`.
