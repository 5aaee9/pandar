# Jobs Dashboard Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a route-backed Jobs dashboard page and keep Devices focused on overview, attention, and device inventory.

**Architecture:** Reuse the existing dashboard runtime and data-loading path. Add `jobs` to the dashboard view union, route it through the same page delegator pattern, add sidebar labels/navigation, and split existing Devices view composition into Devices-only and Jobs-only sections.

**Tech Stack:** Next.js App Router, React, TypeScript, next-intl JSON messages, Vitest + React Testing Library.

---

## File Structure

- Modify `frontend/app/dashboard-shell.ts`: include `jobs` in the route-backed view list.
- Modify `frontend/components/app-sidebar.tsx`: add the Jobs sidebar navigation item with a lucide icon.
- Create `frontend/app/jobs/page.tsx`: route-backed page delegating to `renderDashboardView("jobs", props)`.
- Modify `frontend/app/dashboard-view-content.tsx`: add `JobsView`; remove job sections from `DevicesView`.
- Modify `frontend/app/dashboard-runtime.tsx`: pass action status into sidebar query only for Jobs so tenant switching preserves it there.
- Modify `frontend/app/actions.ts`: allow job/recovery server actions to redirect back to Jobs when submitted with `return_to=jobs`, while keeping Devices as the default.
- Modify `frontend/app/recovery-actions.tsx`: include the Jobs return marker in recovery-page forms.
- Modify `frontend/app/dispatch-form.tsx`: redirect successful/failed dispatch uploads back to `/jobs`, with a minimal injectable redirect callback for component tests.
- Modify `frontend/messages/en.json` and `frontend/messages/zh.json`: add sidebar/header labels for Jobs.
- Modify `frontend/app/dashboard-shell.test.tsx`: add route helper, sidebar label, and view composition coverage.
- Modify `frontend/app/dashboard-runtime.test.tsx`: verify Jobs tenant switching preserves status through the rendered runtime/sidebar query path.
- Modify `frontend/app/actions.test.ts`: verify Jobs-marked recovery actions redirect to `/jobs` and default Devices actions still redirect to `/devices`.
- Create `frontend/app/dispatch-form.test.tsx`: verify dispatch upload redirects to `/jobs`.
- Modify `docs/roadmap.md`: record the completed dashboard split.

---

### Task 1: Add Failing Dashboard Shell Tests

**Files:**
- Modify: `frontend/app/dashboard-shell.test.tsx`
- Modify: `frontend/app/dashboard-runtime.test.tsx`
- Modify: `frontend/app/actions.test.ts`
- Create: `frontend/app/dispatch-form.test.tsx`

- [ ] **Step 1: Add tests that describe the new Jobs route behavior**

Update the existing `defines the route-backed dashboard views` expectation so the view list includes `jobs` after `devices`:

```tsx
expect(DASHBOARD_VIEWS).toEqual([
  "devices",
  "jobs",
  "agents",
  "users",
  "settings",
]);
```

In `builds dashboard redirect and navigation URLs`, add:

```tsx
expect(
  dashboardSidebarHref("jobs", {
    tenant: "t1",
    command: "cmd1",
    status: "done",
  }),
).toBe("/jobs?tenant=t1");
expect(
  dashboardTenantHref("jobs", "t2", {
    tenant: "t1",
    command: "cmd1",
    status: "done",
  }),
).toBe("/jobs?tenant=t2&status=done");
```

Add an `AppSidebar` test:

```tsx
it("renders a jobs navigation link", () => {
  renderWithMessages(
    <SidebarProvider>
      <AppSidebar
        activeView="jobs"
        auth={auth}
        query={{ tenant: "t1", command: "cmd1", status: "done" }}
        selectedTenant={tenants[0]}
        tenants={tenants}
      />
    </SidebarProvider>,
  );

  expect(screen.getByRole("link", { name: "Jobs" })).toHaveAttribute(
    "href",
    "/jobs?tenant=t1",
  );
});
```

In `frontend/app/dashboard-runtime.test.tsx`, import `screen` from `@testing-library/react`:

```tsx
import { render, screen, waitFor } from "@testing-library/react";
```

Add a second tenant fixture:

```tsx
const otherTenant: Tenant = {
  id: "t2",
  slug: "tenant-two",
  display_name: "Tenant Two",
  created_at: "2026-06-30T00:00:00Z",
};
```

Update `renderRuntime` so tests can override view, action status, selected command id, and tenants:

```tsx
function renderRuntime(
  auth: AuthMetadata = noAuth,
  options: {
    view?: "devices" | "jobs"
    actionStatus?: string
    selectedCommandId?: string
    tenants?: Tenant[]
  } = {},
) {
  return render(
    <NextIntlClientProvider locale="en" messages={en}>
      <DashboardRuntime
        apiUrl="http://localhost:8080"
        view={options.view ?? "devices"}
        summary={null}
        tenants={options.tenants ?? [tenant]}
        selectedTenant={tenant}
        initialPrinters={[]}
        agents={[]}
        initialJobs={[]}
        users={[]}
        userIdentities={[]}
        tenantTokens={[]}
        joinLinks={[]}
        auditEvents={[]}
        adminUnavailable={false}
        actionStatus={options.actionStatus}
        selectedCommand={null}
        selectedCommandId={options.selectedCommandId}
        commandData={null}
        errors={[]}
        auth={auth}
      />
    </NextIntlClientProvider>,
  );
}
```

Add this runtime/sidebar integration test:

```tsx
it("preserves action status when switching tenants from jobs", () => {
  vi.stubGlobal("fetch", vi.fn());
  vi.stubGlobal(
    "WebSocket",
    class {
      close() {}
    },
  );

  renderRuntime(noAuth, {
    view: "jobs",
    actionStatus: "refresh_queued",
    selectedCommandId: "cmd1",
    tenants: [tenant, otherTenant],
  });

  expect(screen.getByRole("link", { name: "Tenant Two" })).toHaveAttribute(
    "href",
    "/jobs?tenant=t2&status=refresh_queued",
  );
});
```

In `frontend/app/actions.test.ts`, import the recovery actions:

```ts
import {
  controlPrinter,
  duplicateJob,
  linkPrinter,
  refreshAllAgents,
  refreshPrinterMaterials,
  refreshPrinters,
  reprintJob,
  retryDispatchJob,
  retryDispatchJobs,
} from "./actions";
```

Add a helper:

```ts
function okFetch() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () =>
      new Response(JSON.stringify({ id: "command-1" }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    ),
  );
}
```

Add a Jobs redirect test for each Jobs-marked action:

```ts
it.each([
  ["refreshPrinters", refreshPrinters, [["agent_id", "agent-1"]]],
  ["refreshAllAgents", refreshAllAgents, [["agent_id", "agent-1"]]],
  ["retryDispatchJob", retryDispatchJob, [["job_id", "job-1"]]],
  ["retryDispatchJobs", retryDispatchJobs, [["job_id", "job-1"]]],
  ["reprintJob", reprintJob, [["job_id", "job-1"]]],
  ["duplicateJob", duplicateJob, [["job_id", "job-1"]]],
  ["controlPrinter", controlPrinter, [["printer_id", "printer-1"], ["action", "pause"]]],
] as const)("redirects %s back to jobs when submitted from jobs", async (_name, action, fields) => {
  okFetch();
  const formData = new FormData();
  formData.set("tenant_id", "tenant-1");
  formData.set("return_to", "jobs");
  for (const [name, value] of fields) {
    formData.append(name, value);
  }

  await expect(action(formData)).rejects.toThrow(/^NEXT_REDIRECT:\/jobs\?tenant=tenant-1&status=/);
});
```

Keep the existing `refreshPrinterMaterials` test expecting `/devices?...`, and add one default recovery-action test:

```ts
it("keeps recovery actions on devices by default", async () => {
  okFetch();
  const formData = new FormData();
  formData.set("tenant_id", "tenant-1");
  formData.set("job_id", "job-1");

  await expect(retryDispatchJob(formData)).rejects.toThrow(
    "NEXT_REDIRECT:/devices?tenant=tenant-1&status=retry_queued",
  );
});
```

Create `frontend/app/dispatch-form.test.tsx` with a component-level redirect test:

```tsx
import { NextIntlClientProvider } from "next-intl";
import { fireEvent, render, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import { DispatchForm } from "./dispatch-form";

describe("DispatchForm", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("redirects dispatch results to jobs", async () => {
    const user = userEvent.setup();
    const onRedirect = vi.fn();
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        new Response(JSON.stringify({}), {
          status: 200,
          headers: { "content-type": "application/json" },
        }),
      ),
    );

    const { container } = render(
      <NextIntlClientProvider locale="en" messages={en}>
        <DispatchForm
          selectedTenant={{ id: "tenant-1" }}
          printers={[{ id: "printer-1", name: "Printer One", serial_number: "SN1" }]}
          onRedirect={onRedirect}
        />
      </NextIntlClientProvider>,
    );
    const fileInput = container.querySelector('input[type="file"]');
    expect(fileInput).toBeInstanceOf(HTMLInputElement);
    await user.upload(
      fileInput as HTMLInputElement,
      new File(["3mf"], "benchy.3mf", { type: "model/3mf" }),
    );
    const form = container.querySelector("form");
    expect(form).toBeInstanceOf(HTMLFormElement);
    fireEvent.submit(form as HTMLFormElement);

    await waitFor(() =>
      expect(onRedirect).toHaveBeenCalledWith("/jobs?tenant=tenant-1&status=job_created"),
    );
  });
});
```

Add a `DashboardViewContent` test that verifies Devices no longer renders job sections:

```tsx
it("keeps devices focused on overview and printer inventory", () => {
  renderWithMessages(
    <DashboardViewContent
      view="devices"
      auth={auth}
      selectedTenant={tenants[0]}
      health={{
        printersTotal: 0,
        printersOnline: 0,
        agentsTotal: 1,
        agentsConnected: 1,
        jobsActive: 0,
        jobsFailed: 0,
      }}
      attentionItems={[]}
      topSeverity={null}
      liveState="idle"
      lastEventAt={null}
      fleetEmpty={false}
      printers={[]}
      agents={[]}
      jobs={[]}
      selectedCommand={null}
      commandData={null}
      notifications={[]}
      users={[]}
      userIdentities={[]}
      tenantTokens={[]}
      joinLinks={[]}
      auditEvents={[]}
      adminUnavailable={false}
    />,
  );

  expect(screen.getByText("All systems nominal")).toBeVisible();
  expect(screen.getByRole("heading", { name: "Printer inventory" })).toBeVisible();
  expect(screen.queryByRole("heading", { name: "Print jobs" })).not.toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Dispatch print job" })).not.toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Recovery actions" })).not.toBeInTheDocument();
});
```

Add a `DashboardViewContent` test that verifies Jobs renders job sections:

```tsx
it("renders job history, dispatch, and recovery on jobs", () => {
  renderWithMessages(
    <DashboardViewContent
      view="jobs"
      auth={auth}
      selectedTenant={tenants[0]}
      health={{
        printersTotal: 0,
        printersOnline: 0,
        agentsTotal: 1,
        agentsConnected: 1,
        jobsActive: 0,
        jobsFailed: 0,
      }}
      attentionItems={[]}
      topSeverity={null}
      liveState="idle"
      lastEventAt={null}
      fleetEmpty={false}
      printers={[]}
      agents={[]}
      jobs={[]}
      selectedCommand={null}
      commandData={null}
      notifications={[]}
      users={[]}
      userIdentities={[]}
      tenantTokens={[]}
      joinLinks={[]}
      auditEvents={[]}
      adminUnavailable={false}
    />,
  );

  expect(screen.getByRole("heading", { name: "Print jobs" })).toBeVisible();
  expect(screen.getByRole("heading", { name: "Dispatch print job" })).toBeVisible();
  expect(screen.getByRole("heading", { name: "Recovery actions" })).toBeVisible();
  expect(screen.queryByRole("heading", { name: "Printer inventory" })).not.toBeInTheDocument();
});
```

- [ ] **Step 2: Run the tests and verify they fail for missing Jobs support**

Run:

```powershell
npm --prefix frontend run test -- dashboard-shell.test.tsx dashboard-runtime.test.tsx actions.test.ts dispatch-form.test.tsx
```

Expected: failures mention `jobs` is not assignable, Jobs route/sidebar/content/status-preservation is missing, or job actions still redirect to Devices.

---

### Task 2: Implement Jobs Route and View Split

**Files:**
- Modify: `frontend/app/dashboard-shell.ts`
- Modify: `frontend/components/app-sidebar.tsx`
- Create: `frontend/app/jobs/page.tsx`
- Modify: `frontend/app/dashboard-view-content.tsx`
- Modify: `frontend/app/dashboard-runtime.tsx`
- Modify: `frontend/app/actions.ts`
- Modify: `frontend/app/recovery-actions.tsx`
- Modify: `frontend/app/dispatch-form.tsx`
- Modify: `frontend/messages/en.json`
- Modify: `frontend/messages/zh.json`

- [ ] **Step 1: Add `jobs` to dashboard views**

In `frontend/app/dashboard-shell.ts`, update:

```ts
export const DASHBOARD_VIEWS = [
  "devices",
  "jobs",
  "agents",
  "users",
  "settings",
] as const;
```

- [ ] **Step 2: Add the Jobs sidebar item**

In `frontend/components/app-sidebar.tsx`, import `ClipboardListIcon` from `lucide-react` and add Jobs after Devices:

```tsx
import {
  BotIcon,
  Building2Icon,
  ClipboardListIcon,
  LogOutIcon,
  MonitorIcon,
  SettingsIcon,
  UsersIcon,
} from "lucide-react"
```

```tsx
const navItems: Array<{
  view: DashboardView
  icon: React.ComponentType<{ className?: string }>
}> = [
  { view: "devices", icon: MonitorIcon },
  { view: "jobs", icon: ClipboardListIcon },
  { view: "agents", icon: BotIcon },
  { view: "users", icon: UsersIcon },
  { view: "settings", icon: SettingsIcon },
]
```

- [ ] **Step 3: Add the Jobs route page**

Create `frontend/app/jobs/page.tsx`:

```tsx
import {
  renderDashboardView,
  type DashboardPageProps,
} from "../dashboard-data";

export default function JobsPage(props: DashboardPageProps) {
  return renderDashboardView("jobs", props);
}
```

- [ ] **Step 4: Split Devices and Jobs view composition**

In `frontend/app/dashboard-view-content.tsx`, update the dispatcher:

```tsx
if (props.view === 'devices') {
  return <DevicesView {...props} />
}
if (props.view === 'jobs') {
  return <JobsView {...props} />
}
```

Remove `jobs` from `DevicesView` props and remove the three job-oriented sections:

```tsx
function DevicesView({
  health,
  attentionItems,
  topSeverity,
  liveState,
  lastEventAt,
  fleetEmpty,
  selectedTenant,
  printers,
  agents,
}: DashboardViewContentProps) {
  return (
    <>
      <FleetStatusStrip
        health={health}
        attentionCount={attentionItems.length}
        topSeverity={topSeverity}
        liveState={liveState}
        lastEventAt={lastEventAt}
        fleetEmpty={fleetEmpty}
      />
      <NeedsAttention items={attentionItems} selectedTenant={selectedTenant} />
      <PrinterInventory selectedTenant={selectedTenant} printers={printers} agents={agents} />
    </>
  )
}
```

Add `JobsView`:

```tsx
function JobsView({
  selectedTenant,
  printers,
  agents,
  jobs,
}: DashboardViewContentProps) {
  return (
    <>
      <JobHistory selectedTenant={selectedTenant} jobs={jobs} printers={printers} agents={agents} />
      <DispatchForm selectedTenant={selectedTenant} printers={printers} />
      <RecoveryActions selectedTenant={selectedTenant} agents={agents} printers={printers} jobs={jobs} />
    </>
  )
}
```

- [ ] **Step 5: Preserve action status in Jobs sidebar tenant switching**

In `frontend/app/dashboard-runtime.tsx`, update the dashboard query:

```tsx
const dashboardQuery: DashboardQuery = {
  tenant: selectedTenant?.id,
  command: view === 'agents' ? selectedCommandId : undefined,
  status: view === 'jobs' ? actionStatus : undefined,
}
```

- [ ] **Step 6: Redirect Jobs-page actions back to Jobs**

In `frontend/app/actions.ts`, add a return-view helper:

```ts
function statusUrlForForm(formData: FormData, tenantId: string, status: string) {
  return statusUrl(tenantId, status, stringField(formData, "return_to"));
}

function statusUrl(tenantId: string, status: string, returnTo?: string) {
  const view = returnTo === "jobs" ? "jobs" : "devices";
  return `/${view}?tenant=${encodeURIComponent(tenantId)}&status=${encodeURIComponent(status)}`;
}
```

Update these actions to redirect with `statusUrlForForm(formData, tenantId, ...)`:

```ts
refreshPrinters
refreshAllAgents
retryDispatchJob
retryDispatchJobs
reprintJob
duplicateJob
controlPrinter
```

Do not change `refreshPrinterMaterials`, tenant token/user/admin redirects, `agentsStatusUrl`, or `commandUrl`.

In `frontend/app/recovery-actions.tsx`, add the Jobs return marker to every form rendered by `RecoveryActions`, including nested helper forms:

```tsx
<input name="return_to" type="hidden" value="jobs" />
```

This applies to refresh-all, refresh-one-agent, bulk retry, retry, reprint, duplicate, pause/resume/stop/speed forms.

In `frontend/app/dispatch-form.tsx`, add a minimal redirect callback prop:

```tsx
export function DispatchForm({
  selectedTenant,
  printers,
  onRedirect = (url) => window.location.assign(url),
}: {
  selectedTenant: DispatchTenant | null
  printers: DispatchPrinter[]
  onRedirect?: (url: string) => void
}) {
```

Then change the upload completion redirect:

```tsx
onRedirect(
  `/jobs?tenant=${encodeURIComponent(selectedTenant.id)}&status=${encodeURIComponent(status)}`,
)
```

- [ ] **Step 7: Add localized labels**

In `frontend/messages/en.json`, add `jobs` under `dashboardShell`:

```json
"jobs": "Jobs"
```

In `frontend/messages/zh.json`, add `jobs` under `dashboardShell`:

```json
"jobs": "任务"
```

- [ ] **Step 8: Run the focused tests and verify they pass**

Run:

```powershell
npm --prefix frontend run test -- dashboard-shell.test.tsx dashboard-runtime.test.tsx actions.test.ts dispatch-form.test.tsx
```

Expected: all focused dashboard, action, and dispatch form tests pass.

---

### Task 3: Docs and Full Verification

**Files:**
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update roadmap**

Add a completed item near the other recent frontend shell entries:

```markdown
- Split job history, print dispatch, and recovery actions into a dedicated Jobs dashboard page while keeping Devices focused on overview, attention, and printer inventory.
```

- [ ] **Step 2: Run full verification**

Run:

```powershell
npm --prefix frontend run test
cargo fmt --check
cargo clippy
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: all frontend tests pass; Rust formatting, clippy, and workspace nextest all pass.

- [ ] **Step 3: Review the final diff**

Run:

```powershell
git status --short
git diff --stat
```

Expected: only the dashboard route/view/sidebar/runtime/actions implementation files, focused tests, messages, roadmap, and SDD docs changed.

- [ ] **Step 4: Leave commit and push to the SDD coordinator**

Do not let an implementation subagent commit or push this task. After final implementation review approves the completed diff, the SDD coordinator will commit and push because the user explicitly invoked `$sdd-workflow`, whose workflow includes commit and push after reviewed work.

The coordinator should use a Conventional Commit message:

```powershell
git add -- frontend/app/dashboard-shell.ts frontend/components/app-sidebar.tsx frontend/app/jobs/page.tsx frontend/app/dashboard-view-content.tsx frontend/app/dashboard-runtime.tsx frontend/app/actions.ts frontend/app/recovery-actions.tsx frontend/app/dispatch-form.tsx frontend/messages/en.json frontend/messages/zh.json frontend/app/dashboard-shell.test.tsx frontend/app/dashboard-runtime.test.tsx frontend/app/actions.test.ts frontend/app/dispatch-form.test.tsx docs/roadmap.md docs/superpowers/specs/2026-07-03-jobs-dashboard-page-design.md docs/superpowers/plans/2026-07-03-jobs-dashboard-page.md
git commit -m "feat(frontend): add jobs dashboard page"
git push
```
