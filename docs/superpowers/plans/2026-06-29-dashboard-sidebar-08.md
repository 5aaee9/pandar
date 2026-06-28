# Dashboard Sidebar-08 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current one-page Pandar dashboard with a shadcn sidebar-08 shell and four route-backed pages: Devices, Agents, Users, and Settings.

**Architecture:** Keep server-side dashboard data loading centralized, then route `/devices`, `/agents`, `/users`, and `/settings` through the same loader and client runtime. Extract pure route/query/logout helpers for smoke coverage. Use the shadcn-generated sidebar primitives for the app shell while preserving existing domain panels, actions, live updates, auth redirects, tenant selection, and i18n.

**Tech Stack:** Next.js 16 app router, React 19, Tailwind CSS 4, next-intl, shadcn sidebar-08, standalone Node smoke tests with `--experimental-transform-types`.

---

## File Structure

- Create `frontend/app/dashboard-shell.ts`: pure dashboard view/query/logout contract for smoke tests and UI.
- Create `frontend/app/dashboard-shell.smoke.mjs`: route/view/query/logout smoke coverage.
- Create `frontend/app/dashboard-data.tsx`: shared server loader extracted from the current `frontend/app/page.tsx`.
- Modify `frontend/app/page.tsx`: root redirect only, using `dashboardRootRedirectTarget`.
- Create `frontend/app/devices/page.tsx`, `frontend/app/agents/page.tsx`, `frontend/app/users/page.tsx`, `frontend/app/settings/page.tsx`: route entries that call the shared loader with a fixed view.
- Modify `frontend/app/dashboard-runtime.tsx`: render the sidebar shell and page-specific dashboard content.
- Modify `frontend/app/dashboard-types.ts`: extend `AuthMetadata` with `signInUrl` and `signOutUrl`.
- Modify `frontend/app/admin-panel.tsx`: keep shared admin types and any wrapper that remains necessary.
- Create `frontend/app/admin-users-panel.tsx`: users and join-link admin UI extracted from the current admin panel.
- Create `frontend/app/admin-settings-panel.tsx`: tenant token, agent pairing, and audit-event admin UI extracted from the current admin panel.
- Modify `frontend/app/dashboard-runtime-sections.tsx`: keep tenant/runtime settings usable in Settings with the extended auth metadata.
- Modify `frontend/app/actions.ts` and `frontend/app/dispatch-form.tsx`: keep action feedback routed through root redirect or explicit dashboard route helpers.
- Modify `frontend/messages/en.json` and `frontend/messages/zh.json`: add dashboard shell labels.
- Create or modify shadcn files under `frontend/components.json`, `frontend/components/ui/*`, `frontend/components/app-sidebar.tsx`, `frontend/hooks/*`, and `frontend/lib/utils.ts` via `npx shadcn@latest add sidebar-08`.
- Modify `frontend/tsconfig.json`: add `baseUrl` and `@/*` alias if shadcn keeps `@/` imports.
- Modify `frontend/app/globals.css` only as required by shadcn/Tailwind variables and Pandar visual consistency.
- Modify `docs/roadmap.md`: record the dashboard sidebar-08 reorganization.

## Task 1: Pure Dashboard Shell Contract and Smoke Test

**Files:**

- Create: `frontend/app/dashboard-shell.ts`
- Create: `frontend/app/dashboard-shell.smoke.mjs`

- [x] **Step 1: Write the failing smoke test**

Create `frontend/app/dashboard-shell.smoke.mjs`:

```js
import assert from "node:assert/strict";
import { pathToFileURL } from "node:url";

const shellModuleUrl = pathToFileURL(
  new URL("./dashboard-shell.ts", import.meta.url).pathname,
);

const {
  DASHBOARD_VIEWS,
  dashboardRootRedirectTarget,
  dashboardSidebarHref,
  dashboardTenantHref,
  dashboardViewTitleKey,
  logoutHref,
} = await import(shellModuleUrl.href);

assert.deepEqual(DASHBOARD_VIEWS, ["devices", "agents", "users", "settings"]);
assert.equal(dashboardViewTitleKey("devices"), "devices");
assert.equal(dashboardViewTitleKey("agents"), "agents");
assert.equal(dashboardViewTitleKey("users"), "users");
assert.equal(dashboardViewTitleKey("settings"), "settings");

assert.equal(dashboardRootRedirectTarget({}), "/devices");
assert.equal(
  dashboardRootRedirectTarget({ tenant: "tenant 1", status: "job_created" }),
  "/devices?tenant=tenant+1&status=job_created",
);
assert.equal(
  dashboardRootRedirectTarget({
    tenant: "t1",
    command: "cmd1",
    status: "refresh_queued",
  }),
  "/agents?tenant=t1&command=cmd1&status=refresh_queued",
);

assert.equal(
  dashboardSidebarHref("agents", {
    tenant: "t1",
    command: "cmd1",
    status: "done",
  }),
  "/agents?tenant=t1",
);
assert.equal(dashboardSidebarHref("users", {}), "/users");

assert.equal(
  dashboardTenantHref("agents", "t2", {
    tenant: "t1",
    command: "cmd1",
    status: "done",
  }),
  "/agents?tenant=t2&command=cmd1&status=done",
);
assert.equal(
  dashboardTenantHref("devices", "t2", {
    tenant: "t1",
    command: "cmd1",
    status: "done",
  }),
  "/devices?tenant=t2&status=done",
);

assert.equal(logoutHref({ signOutUrl: null }), null);
assert.equal(
  logoutHref({ signOutUrl: "https://auth.example/sign-out" }),
  "https://auth.example/sign-out",
);
```

- [x] **Step 2: Run the smoke test and verify it fails because the module does not exist**

Run from `frontend/`:

```bash
node --experimental-transform-types app/dashboard-shell.smoke.mjs
```

Expected: FAIL with a module-not-found error for `dashboard-shell.ts`.

- [x] **Step 3: Implement the pure shell contract**

Create `frontend/app/dashboard-shell.ts`:

```ts
export const DASHBOARD_VIEWS = [
  "devices",
  "agents",
  "users",
  "settings",
] as const;

export type DashboardView = (typeof DASHBOARD_VIEWS)[number];

export type DashboardQuery = {
  tenant?: string;
  command?: string;
  status?: string;
};

export function dashboardViewTitleKey(view: DashboardView) {
  return view;
}

export function dashboardRootRedirectTarget(query: DashboardQuery) {
  return dashboardPath(query.command ? "agents" : "devices", query);
}

export function dashboardSidebarHref(
  view: DashboardView,
  query: DashboardQuery,
) {
  return dashboardPath(view, { tenant: query.tenant });
}

export function dashboardTenantHref(
  view: DashboardView,
  tenant: string,
  query: DashboardQuery,
) {
  return dashboardPath(view, {
    tenant,
    status: query.status,
    command: view === "agents" ? query.command : undefined,
  });
}

export function dashboardPath(view: DashboardView, query: DashboardQuery = {}) {
  const params = new URLSearchParams();
  if (query.tenant) params.set("tenant", query.tenant);
  if (query.command) params.set("command", query.command);
  if (query.status) params.set("status", query.status);
  const suffix = params.toString();
  return suffix ? `/${view}?${suffix}` : `/${view}`;
}

export function logoutHref({ signOutUrl }: { signOutUrl: string | null }) {
  return signOutUrl;
}
```

- [x] **Step 4: Run the smoke test and verify it passes**

Run from `frontend/`:

```bash
node --experimental-transform-types app/dashboard-shell.smoke.mjs
```

Expected: PASS with no output.

## Task 2: shadcn Sidebar-08 Installation and Build Aliases

**Files:**

- Create/modify: `frontend/components.json`
- Create/modify: `frontend/components/ui/*`
- Create/modify: `frontend/components/app-sidebar.tsx`
- Create/modify: `frontend/hooks/*`
- Create/modify: `frontend/lib/utils.ts`
- Modify: `frontend/tsconfig.json`
- Modify: `frontend/package.json`
- Modify: `frontend/package-lock.json`
- Modify: `frontend/app/globals.css`

- [x] **Step 1: Initialize shadcn root config if missing**

Run only from `/home/indexyz/pandar/frontend`:

```bash
npx shadcn@latest init --defaults --yes --cwd /home/indexyz/pandar/frontend
```

Expected: root `frontend/components.json` exists. Do not run this in `frontend/auth`.

- [x] **Step 2: Add sidebar-08 using shadcn**

Run only from `/home/indexyz/pandar/frontend`:

```bash
npx shadcn@latest add sidebar-08 --yes --cwd /home/indexyz/pandar/frontend
```

If the CLI asks for a registry-qualified block, run:

```bash
npx shadcn@latest add @shadcn/sidebar-08 --yes --cwd /home/indexyz/pandar/frontend
```

Expected: generated sidebar primitives exist, including `components/ui/sidebar.tsx`. Inspect `git diff` immediately and keep generated UI primitives, but do not keep a sample dashboard page that overwrites Pandar behavior.

- [x] **Step 3: Ensure aliases resolve**

If generated files import from `@/`, modify `frontend/tsconfig.json` under `compilerOptions`:

```json
{
  "baseUrl": ".",
  "paths": {
    "@/*": ["./*"]
  }
}
```

Keep existing compiler options unchanged.

- [x] **Step 4: Run a frontend type/build check**

Run from `frontend/`:

```bash
npm run build
```

Expected: build may still fail until Tasks 3-4 integrate generated files, but failures must be from unfinished integration, not missing `@/` aliases or missing shadcn dependencies.

## Task 3: Shared Server Loader and Route Entries

**Files:**

- Create: `frontend/app/dashboard-data.tsx`
- Modify: `frontend/app/page.tsx`
- Create: `frontend/app/devices/page.tsx`
- Create: `frontend/app/agents/page.tsx`
- Create: `frontend/app/users/page.tsx`
- Create: `frontend/app/settings/page.tsx`
- Modify: `frontend/app/dashboard-types.ts`

- [x] **Step 1: Extract server dashboard loading**

Move the fetch/auth/tenant selection logic from `frontend/app/page.tsx` into `frontend/app/dashboard-data.tsx`. The new exported function should have this shape:

```ts
import { redirect } from "next/navigation";

import { apiHeaders, authSource } from "./api-auth";
import { dashboardAuthRedirectTarget } from "./auth-redirect";
import { authProviderConfig } from "./auth-provider";
import type { DashboardView } from "./dashboard-shell";
import { DashboardRuntime } from "./dashboard-runtime";
import { OnboardingPanel } from "./onboarding-panel";

export type DashboardPageProps = {
  searchParams?: Promise<{
    tenant?: string | string[];
    command?: string | string[];
    status?: string | string[];
  }>;
};

export async function renderDashboardView(
  view: DashboardView,
  props: DashboardPageProps,
) {
  const auth = await authSource();
  const authProvider = authProviderConfig();
  const initialRedirect = dashboardAuthRedirectTarget({
    source: auth.source,
    provider: authProvider,
  });
  if (initialRedirect) {
    redirect(initialRedirect);
  }

  const runtimeAuth = {
    source: auth.source,
    cookieName: auth.cookieName,
    provider: auth.provider,
    signInUrl: authProvider.signInUrl,
    signOutUrl: authProvider.signOutUrl,
  };

  const params = await props.searchParams;
  const requestedTenant = firstParam(params?.tenant);
  const requestedCommand = firstParam(params?.command);
  const actionStatus = firstParam(params?.status);

  // Move the current dashboard fetch sequence here. After fetching meResult,
  // keep the existing second redirect check:
  // dashboardAuthRedirectTarget({ source: auth.source, provider: authProvider, meStatus: meResult.status }).
  // Pass every current DashboardRuntime prop unchanged, plus view,
  // selectedCommandId={requestedCommand}, and runtimeAuth.
}
```

The function must preserve:

- `authSource()` and `authProviderConfig()` redirect checks.
- external onboarding behavior.
- configured tenant behavior.
- fetching summary, tenants, me, printers, agents, jobs, users, identities, tenant tokens, join links, audit events, and selected command.
- `errors` and `actionStatus` handling.

- [x] **Step 2: Extend `AuthMetadata`**

Modify `frontend/app/dashboard-types.ts`:

```ts
export type AuthMetadata = {
  source: "request_cookie" | "app_auth_bearer_token" | "app_api_token" | "none";
  cookieName: string;
  provider: "clerk" | "logto" | "betterauth" | "none";
  signInUrl: string | null;
  signOutUrl: string | null;
};
```

When constructing the `auth` prop in `renderDashboardView`, merge `authSource()` with provider URLs:

```ts
const runtimeAuth = {
  source: auth.source,
  cookieName: auth.cookieName,
  provider: auth.provider,
  signInUrl: authProvider.signInUrl,
  signOutUrl: authProvider.signOutUrl,
};
```

- [x] **Step 3: Replace root page with redirect**

Modify `frontend/app/page.tsx` to normalize query params and redirect:

```ts
import { redirect } from "next/navigation";

import {
  dashboardRootRedirectTarget,
  type DashboardQuery,
} from "./dashboard-shell";
import type { DashboardPageProps } from "./dashboard-data";

export default async function Page({ searchParams }: DashboardPageProps) {
  const params = await searchParams;
  const query: DashboardQuery = {
    tenant: firstParam(params?.tenant),
    command: firstParam(params?.command),
    status: firstParam(params?.status),
  };
  redirect(dashboardRootRedirectTarget(query));
}

function firstParam(value?: string | string[]) {
  return Array.isArray(value) ? value[0] : value;
}
```

- [x] **Step 4: Add four route pages**

Create each route page with the matching view:

```ts
import {
  renderDashboardView,
  type DashboardPageProps,
} from "../dashboard-data";

export default function DevicesPage(props: DashboardPageProps) {
  return renderDashboardView("devices", props);
}
```

Repeat for `agents`, `users`, and `settings` with their corresponding view names.

- [x] **Step 5: Run smoke and build**

Run from `frontend/`:

```bash
node --experimental-transform-types app/dashboard-shell.smoke.mjs
npm run build
```

Expected: smoke passes. Build may still fail until Task 4 updates `DashboardRuntime`; do not proceed with unresolved route/type errors from this task.

## Task 4: Sidebar Shell and Page-Specific Dashboard Runtime

**Files:**

- Modify: `frontend/app/dashboard-runtime.tsx`
- Modify: `frontend/app/admin-panel.tsx`
- Create: `frontend/app/admin-users-panel.tsx`
- Create: `frontend/app/admin-settings-panel.tsx`
- Modify: `frontend/app/dashboard-runtime-sections.tsx`
- Modify: `frontend/app/actions.ts`
- Modify: `frontend/app/dispatch-form.tsx`
- Modify: `frontend/messages/en.json`
- Modify: `frontend/messages/zh.json`

- [x] **Step 1: Split admin panel exports without changing behavior**

Move the users and join-link UI from `frontend/app/admin-panel.tsx` into `frontend/app/admin-users-panel.tsx`:

```ts
export type TenantUsersPanelProps = {
  selectedTenant: Tenant | null;
  users: User[];
  userIdentities: UserIdentity[];
  joinLinks: JoinLink[];
  unavailable: boolean;
};
```

Export `TenantUsersPanel` from that file. Its JSX should be the users and join-link section moved from the current `TenantAdminPanel`.

Move tenant tokens, agent pairing, and audit events into `frontend/app/admin-settings-panel.tsx`:

```ts
export type TenantSecretsPanelProps = {
  selectedTenant: Tenant | null;
  tenantTokens: TenantToken[];
  agents: Agent[];
  unavailable: boolean;
};

export type TenantAuditPanelProps = {
  selectedTenant: Tenant | null;
  auditEvents: AuditEvent[];
  unavailable: boolean;
};
```

Export `TenantSecretsPanel` and `TenantAuditPanel` from that file. Their JSX should be the tenant token, agent pairing, and audit-event sections moved from the current `TenantAdminPanel`.

After extraction, `TenantAdminPanel` may remain as a thin wrapper that composes `TenantUsersPanel`, `TenantSecretsPanel`, and `TenantAuditPanel` for any still-existing imports. The new runtime must import the smaller panels directly for the Users and Settings pages.

Run this LOC check after the split and keep each edited admin module under the project limit:

```bash
wc -l frontend/app/admin-panel.tsx frontend/app/admin-users-panel.tsx frontend/app/admin-settings-panel.tsx
```

- [x] **Step 2: Update action feedback targets where explicit**

In `frontend/app/actions.ts`, add helper functions:

```ts
function statusUrl(tenantId: string, status: string) {
  return `/devices?tenant=${encodeURIComponent(tenantId)}&status=${encodeURIComponent(status)}`;
}

function commandUrl(tenantId: string, commandId: string) {
  return `/agents?tenant=${encodeURIComponent(tenantId)}&command=${encodeURIComponent(commandId)}`;
}
```

Use `commandUrl` for `discoverPrinters` and `diagnosePrinter`. Update `frontend/app/dispatch-form.tsx` upload redirect to construct `/devices?tenant=${encodeURIComponent(selectedTenantId)}&status=${encodeURIComponent(status)}` with the same success and error status values it uses today.

- [x] **Step 3: Replace the runtime shell**

Modify `DashboardRuntimeProps` to include:

```ts
view: DashboardView;
selectedCommandId?: string;
```

Remove the old `Header` and `SectionNav` imports from `DashboardRuntime`. Render a sidebar app shell using the generated shadcn sidebar primitives, the `LanguageSwitcher`, and the pure helpers. The runtime should derive:

- `dashboardQuery` from `selectedTenant?.id`, `actionStatus`, and `selectedCommandId` when `view === "agents"`.
- `pageTitleKey` from `dashboardViewTitleKey(view)`.
- navigation hrefs from `dashboardSidebarHref`.
- tenant switch hrefs from `dashboardTenantHref`.
- logout href from `logoutHref(auth)`.

```tsx
<SidebarProvider>
  <AppSidebar
    activeView={view}
    auth={auth}
    query={{
      tenant: selectedTenant?.id,
      command: view === "agents" ? selectedCommandId : undefined,
      status: actionStatus,
    }}
    selectedTenant={selectedTenant}
    tenants={tenants}
  />
  <SidebarInset>
    <DashboardShellHeader />
    <DashboardShellNotices />
    <DashboardViewContent />
  </SidebarInset>
</SidebarProvider>
```

Use the shadcn-generated `components/app-sidebar.tsx` as the starting point, adapted to Pandar:

- nav items: Devices, Agents, Users, Settings.
- active state from `view`.
- hrefs from `dashboardSidebarHref`.
- tenant switch links from `dashboardTenantHref`.
- visible Logout link from `logoutHref(auth)`.

- [x] **Step 4: Render page-specific content**

Keep live WebSocket state and `computeAttention` in `DashboardRuntime`, but render each view from existing panels:

- Devices: `FleetStatusStrip`, `NeedsAttention`, `PrinterInventory`, `JobHistory`, `DispatchForm`, and `RecoveryActions`.
- Agents: `LinkedAgentsSection` and `DiagnosticsSection`.
- Users: a logout/account section plus `TenantUsersPanel`.
- Settings: `TenantSettings`, `TenantSecretsPanel`, `RuntimeStatusPanel`, and `TenantAuditPanel`.

Do not duplicate backend calls; all data still comes from `renderDashboardView`.

- [x] **Step 5: Add i18n labels**

Add `dashboardShell` to both `frontend/messages/en.json` and `frontend/messages/zh.json`:

```json
{
  "dashboardShell": {
    "brand": "Pandar",
    "devices": "Devices",
    "agents": "Agents",
    "users": "Users",
    "settings": "Settings",
    "tenant": "Tenant",
    "view": "View",
    "logout": "Logout",
    "logoutUnavailable": "Logout is unavailable for this auth mode",
    "signedInWith": "Signed in with {provider}",
    "apiSource": "API {apiUrl}"
  }
}
```

Use equivalent Chinese labels in `zh.json`.

- [x] **Step 6: Run smoke and build**

Run from `frontend/`:

```bash
node --experimental-transform-types app/dashboard-shell.smoke.mjs
npm run build
```

Expected: both pass.

## Task 5: Documentation and Full Verification

**Files:**

- Modify: `docs/roadmap.md`

- [x] **Step 1: Update roadmap**

Add a Completed bullet to `docs/roadmap.md`:

```md
- Replaced the single-page dashboard shell with a shadcn sidebar-08 app shell and split authenticated operations into Devices, Agents, Users, and Settings route-backed pages, with tenant-aware navigation, provider logout, and smoke-tested route/query helpers.
```

- [x] **Step 2: Run frontend verification**

Run:

```bash
cd frontend
node --experimental-transform-types app/dashboard-shell.smoke.mjs
npm run build
```

Expected: both pass.

- [x] **Step 3: Run repo verification**

Run from repo root:

```bash
cargo fmt
cargo clippy
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: commands pass. If a command fails due to a missing tool or unrelated environment issue, capture the exact failure.

- [x] **Step 4: Inspect final diff**

Run:

```bash
git status --short
git diff --stat
git diff -- frontend/app/dashboard-runtime.tsx frontend/app/dashboard-data.tsx frontend/app/dashboard-shell.ts frontend/messages/en.json frontend/messages/zh.json docs/roadmap.md
```

Expected: only dashboard/sidebar, i18n, dependency, smoke, and roadmap files changed.
