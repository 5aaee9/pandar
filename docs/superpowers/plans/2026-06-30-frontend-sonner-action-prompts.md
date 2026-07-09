# Frontend Sonner Action Prompts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace redirect-backed dashboard action prompts such as `refresh_queued` with shadcn Sonner toasts while preserving durable in-page notification surfaces.

**Architecture:** Keep the existing server-action redirect flow and consume `actionStatus` in `DashboardRuntime` on the client. A new `ActionStatusToast` client component resolves localized status text, emits a Sonner toast, clears the `status` query parameter from the URL, and notifies `DashboardRuntime` so dashboard navigation no longer carries the consumed status.

**Tech Stack:** Next.js 16 App Router, React 19, `next-intl`, Tailwind CSS, local shadcn-style UI primitives, `sonner`, Vitest + Testing Library.

## Global Constraints

- Follow `docs/superpowers/specs/2026-06-30-frontend-sonner-action-prompts-design.md` exactly.
- Do not replace `RuntimeStatusPanel` live activity notifications; they are durable dashboard context.
- Do not replace admin secret action result panels; they expose generated secrets that must remain visible.
- Do not replace fetch/data integrity errors; they remain in-page degraded-content warnings.
- Keep server actions redirecting with `status` codes.
- Add `runtime.actionStatus` entries in both English and Chinese for these known redirect statuses: `refresh_queued`, `refresh_partial`, `job_created`, `tenant_created`, `tenant_token_revoked`, `join_link_accepted`, `join_link_revoked`, `user_created`, `user_role_updated`, `identity_linked`, `retry_queued`, `retry_partial`, `reprint_queued`, `duplicate_queued`, and `printer_control_queued`.
- Use the deterministic toast tone rule from the spec: statuses containing `partial` are warnings; statuses starting with `http_` or missing from the known-positive set are errors; all known positive redirect statuses are success/default toasts.
- Guard against duplicate toasts from React Strict Mode effect double-invocation.
- Preserve all non-`status` query parameters when clearing the consumed status.
- Refresh `nix/pandar.nix` `pandar-web.npmDepsHash` after changing frontend npm dependencies, because CI builds `.#checks.${system}.pandar-web`.
- Update `docs/roadmap.md` after implementation.

---

## File Structure

- `frontend/package.json`: add `sonner` to `pandar-web` dependencies.
- `package-lock.json`: update via npm workspace install.
- `nix/pandar.nix`: refresh `pandar-web.npmDepsHash` after the lockfile changes.
- `frontend/components/ui/sonner.tsx`: create a local shadcn-style wrapper around Sonner's `Toaster`.
- `frontend/app/layout.tsx`: mount `Toaster` once under `NextIntlClientProvider`.
- `frontend/app/action-status-toast.tsx`: create the client toast trigger, status formatter, tone classifier, known status set, and URL clearing helper.
- `frontend/app/action-status-toast.test.tsx`: test toast emission, URL clearing, tone classification, fallback formatting, Strict Mode duplicate protection, and consumed-status navigation behavior.
- `frontend/app/dashboard-runtime.tsx`: use local consumed-status state, render `ActionStatusToast`, remove the old inline cyan banner, and pass a status-free query after consumption.
- `frontend/messages/en.json`: localize every known redirect status.
- `frontend/messages/zh.json`: localize every known redirect status.
- `docs/roadmap.md`: record that redirect-backed dashboard prompts now use Sonner.

---

### Task 1: Add Sonner Dependency and Root Toaster

**Files:**

- Modify: `frontend/package.json`
- Modify: `package-lock.json`
- Modify: `nix/pandar.nix`
- Create: `frontend/components/ui/sonner.tsx`
- Modify: `frontend/app/layout.tsx`

**Interfaces:**

- Produces: `Toaster` exported from `@/components/ui/sonner`.
- Consumes: Sonner package exports `Toaster as Sonner` and `type ToasterProps`.

- [ ] **Step 1: Install Sonner in the frontend workspace**

Run:

```bash
npm install sonner --workspace pandar-web
```

Expected: `frontend/package.json` gains a `sonner` dependency and root `package-lock.json` gains matching lockfile entries.

- [ ] **Step 2: Create the local Sonner wrapper**

Create `frontend/components/ui/sonner.tsx` with this shape:

```tsx
"use client";

import { Toaster as Sonner, type ToasterProps } from "sonner";

function Toaster({ ...props }: ToasterProps) {
  return (
    <Sonner
      className="toaster group"
      closeButton
      richColors
      toastOptions={{
        classNames: {
          toast:
            "group toast rounded-md border border-slate-200 bg-white text-slate-950 shadow-lg",
          description: "text-slate-600",
          actionButton: "bg-slate-900 text-slate-50",
          cancelButton: "bg-slate-100 text-slate-900",
        },
      }}
      {...props}
    />
  );
}

export { Toaster };
```

- [ ] **Step 3: Mount the root Toaster**

Modify `frontend/app/layout.tsx` so it imports `Toaster` and renders it once inside `NextIntlClientProvider`:

```tsx
import { Toaster } from "@/components/ui/sonner";
```

and the provider body becomes:

```tsx
<NextIntlClientProvider locale={locale} messages={messages}>
  <TooltipProvider>{children}</TooltipProvider>
  <Toaster />
</NextIntlClientProvider>
```

- [ ] **Step 4: Run a focused type/build check for the setup**

Update `nix/pandar.nix` after the lockfile changes. First set `pandar-web.npmDepsHash` to an empty hash:

```nix
npmDepsHash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
```

Then run:

```bash
nix build --show-trace .#checks.x86_64-linux.pandar-web
```

Expected: FAIL with a fixed-output hash mismatch that prints the correct `got: sha256-...` hash. Replace the placeholder hash in `nix/pandar.nix` with that `got` value.

- [ ] **Step 5: Run a focused type/build check for the setup**

Run:

```bash
cd frontend && npm run build
```

Expected: The build reaches at least dependency/typechecking for the new wrapper. If unrelated existing build failures appear, record the exact output and continue only if the failure is demonstrably unrelated to this task.

---

### Task 2: Implement Action Status Toast Consumption

**Files:**

- Create: `frontend/app/action-status-toast.tsx`
- Create: `frontend/app/action-status-toast.test.tsx`
- Modify: `frontend/app/dashboard-runtime.tsx`
- Modify: `frontend/messages/en.json`
- Modify: `frontend/messages/zh.json`

**Interfaces:**

- Produces: `ActionStatusToast({ status, onConsumed })` where `status?: string` and `onConsumed: (status: string) => void`.
- Produces: `formatActionStatus(status: string, tStatus: StatusTranslator): string`.
- Produces: `actionStatusTone(status: string): 'success' | 'warning' | 'error'`.
- Produces: `clearStatusQueryFromUrl(): void`.
- Consumes: `toast.success`, `toast.warning`, and `toast.error` from `sonner`.
- Consumes: `runtime.actionStatus` translations from `next-intl`.

- [ ] **Step 1: Write failing tests for toast behavior**

Create `frontend/app/action-status-toast.test.tsx` with tests covering these cases:

```tsx
import { StrictMode, useState } from "react";
import { NextIntlClientProvider } from "next-intl";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi, afterEach } from "vitest";

import en from "../messages/en.json";
import {
  ActionStatusToast,
  actionStatusTone,
  formatActionStatus,
} from "./action-status-toast";
import { DashboardShellHeader } from "./dashboard-shell-header";
import type { DashboardQuery, DashboardView } from "./dashboard-shell";
import type { Tenant } from "./dashboard-types";
import { toast } from "sonner";

// This test file lives in `frontend/app`, so `./dashboard-shell-header` is the correct relative path.

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    warning: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock("../components/ui/sidebar", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("../components/ui/sidebar")>();
  return {
    ...actual,
    SidebarTrigger: ({ className }: { className?: string }) => (
      <button aria-label="Toggle sidebar" className={className} type="button" />
    ),
  };
});

function renderWithMessages(children: React.ReactNode) {
  return render(
    <NextIntlClientProvider locale="en" messages={en}>
      {children}
    </NextIntlClientProvider>,
  );
}

function setUrl(path: string) {
  window.history.pushState({}, "", path);
}

function stubLocationAssign(assign: (url: string) => void) {
  const originalWindow = window;
  const location = {
    ...originalWindow.location,
    assign,
  };

  vi.stubGlobal(
    "window",
    new Proxy(originalWindow, {
      get(target, prop, receiver) {
        if (prop === "location") {
          return location;
        }
        return Reflect.get(target, prop, receiver);
      },
    }),
  );
}

afterEach(() => {
  vi.clearAllMocks();
  vi.unstubAllGlobals();
  window.history.replaceState({}, "", "/");
});

describe("action status toast helpers", () => {
  it("formats translated and fallback status messages", () => {
    const tStatus = Object.assign(
      (key: string) =>
        en.runtime.actionStatus[key as keyof typeof en.runtime.actionStatus],
      { has: (key: string) => key in en.runtime.actionStatus },
    );

    expect(formatActionStatus("refresh_queued", tStatus)).toBe(
      "Refresh queued",
    );
    expect(formatActionStatus("artifact_too_large", tStatus)).toBe(
      "Artifact Too Large",
    );
  });

  it("classifies status tone deterministically", () => {
    expect(actionStatusTone("refresh_queued")).toBe("success");
    expect(actionStatusTone("refresh_partial")).toBe("warning");
    expect(actionStatusTone("http_500")).toBe("error");
    expect(actionStatusTone("artifact_too_large")).toBe("error");
  });
});

describe("ActionStatusToast", () => {
  it("shows a success toast and clears only the status query parameter", async () => {
    const onConsumed = vi.fn();
    setUrl("/devices?tenant=t1&status=refresh_queued");

    renderWithMessages(
      <ActionStatusToast status="refresh_queued" onConsumed={onConsumed} />,
    );

    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Refresh queued"),
    );
    expect(onConsumed).toHaveBeenCalledWith("refresh_queued");
    expect(window.location.pathname + window.location.search).toBe(
      "/devices?tenant=t1",
    );
  });

  it("shows a warning toast and preserves tenant plus command query parameters", async () => {
    const onConsumed = vi.fn();
    setUrl("/devices?tenant=t1&command=c1&status=refresh_partial");

    renderWithMessages(
      <ActionStatusToast status="refresh_partial" onConsumed={onConsumed} />,
    );

    await waitFor(() =>
      expect(toast.warning).toHaveBeenCalledWith(
        "Some refreshes could not be queued — review the list",
      ),
    );
    expect(window.location.pathname + window.location.search).toBe(
      "/devices?tenant=t1&command=c1",
    );
  });

  it("shows an error toast for unexpected backend error codes", async () => {
    const onConsumed = vi.fn();
    setUrl("/devices?tenant=t1&status=artifact_too_large");

    renderWithMessages(
      <ActionStatusToast status="artifact_too_large" onConsumed={onConsumed} />,
    );

    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith("Artifact Too Large"),
    );
  });

  it("does not duplicate toasts under Strict Mode effect replay", async () => {
    const onConsumed = vi.fn();
    setUrl("/devices?tenant=t1&status=refresh_queued");

    renderWithMessages(
      <StrictMode>
        <ActionStatusToast status="refresh_queued" onConsumed={onConsumed} />
      </StrictMode>,
    );

    await waitFor(() => expect(toast.success).toHaveBeenCalledTimes(1));
  });
});

const tenants: Tenant[] = [
  {
    id: "t1",
    slug: "tenant-one",
    display_name: "Tenant One",
    created_at: "2026-06-30T00:00:00Z",
  },
  {
    id: "t2",
    slug: "tenant-two",
    display_name: "Tenant Two",
    created_at: "2026-06-30T00:00:00Z",
  },
];

function DashboardHeaderWithConsumedStatus({
  actionStatus,
}: {
  actionStatus: string;
}) {
  const [consumedStatus, setConsumedStatus] = useState<string | null>(null);
  const pendingActionStatus =
    actionStatus && consumedStatus !== actionStatus ? actionStatus : undefined;
  const query: DashboardQuery = {
    tenant: "t1",
    command: "cmd1",
    status: pendingActionStatus,
  };
  const view: DashboardView = "agents";
  return (
    <>
      <ActionStatusToast
        status={pendingActionStatus}
        onConsumed={setConsumedStatus}
      />
      <DashboardShellHeader
        query={query}
        selectedTenant={tenants[0]}
        tenants={tenants}
        view={view}
      />
    </>
  );
}

describe("consumed action status navigation", () => {
  it("does not preserve consumed status when switching tenants", async () => {
    const user = userEvent.setup();
    const assign = vi.fn<(url: string) => void>();
    stubLocationAssign(assign);
    setUrl("/agents?tenant=t1&command=cmd1&status=refresh_queued");

    renderWithMessages(
      <DashboardHeaderWithConsumedStatus actionStatus="refresh_queued" />,
    );
    await waitFor(() => expect(toast.success).toHaveBeenCalledTimes(1));

    await user.selectOptions(screen.getByRole("combobox"), "t2");

    expect(assign).toHaveBeenCalledWith("/agents?tenant=t2&command=cmd1");
  });
});
```

- [ ] **Step 2: Run tests and verify they fail before implementation**

Run:

```bash
cd frontend && npm run test -- action-status-toast.test.tsx
```

Expected: FAIL because `./action-status-toast` does not exist.

- [ ] **Step 3: Update English action status messages**

Modify `frontend/messages/en.json` under `runtime.actionStatus` so it contains:

```json
"actionStatus": {
  "refresh_queued": "Refresh queued",
  "refresh_partial": "Some refreshes could not be queued — review the list",
  "job_created": "Print job queued",
  "tenant_created": "Tenant created",
  "tenant_token_revoked": "Tenant token revoked",
  "join_link_accepted": "Join link accepted",
  "join_link_revoked": "Join link revoked",
  "user_created": "User created",
  "user_role_updated": "User role updated",
  "identity_linked": "Identity linked",
  "retry_queued": "Retry queued",
  "retry_partial": "Some retries could not be queued — review the list",
  "reprint_queued": "Reprint queued",
  "duplicate_queued": "Duplicate queued",
  "printer_control_queued": "Printer control queued"
}
```

- [ ] **Step 4: Update Chinese action status messages**

Modify `frontend/messages/zh.json` under `runtime.actionStatus` so it contains:

```json
"actionStatus": {
  "refresh_queued": "刷新已入队",
  "refresh_partial": "部分刷新未能入队——请检查列表",
  "job_created": "打印任务已入队",
  "tenant_created": "租户已创建",
  "tenant_token_revoked": "租户 API 令牌已撤销",
  "join_link_accepted": "加入链接已接受",
  "join_link_revoked": "加入链接已撤销",
  "user_created": "用户已创建",
  "user_role_updated": "用户角色已更新",
  "identity_linked": "身份已关联",
  "retry_queued": "重试已入队",
  "retry_partial": "部分重试未能入队——请检查列表",
  "reprint_queued": "重新打印已入队",
  "duplicate_queued": "复制任务已入队",
  "printer_control_queued": "打印机控制已入队"
}
```

- [ ] **Step 5: Implement `ActionStatusToast` and helpers**

Create `frontend/app/action-status-toast.tsx`:

```tsx
"use client";

import { useEffect, useRef } from "react";
import { useTranslations } from "next-intl";
import { toast } from "sonner";

const knownPositiveActionStatuses = new Set([
  "refresh_queued",
  "refresh_partial",
  "job_created",
  "tenant_created",
  "tenant_token_revoked",
  "join_link_accepted",
  "join_link_revoked",
  "user_created",
  "user_role_updated",
  "identity_linked",
  "retry_queued",
  "retry_partial",
  "reprint_queued",
  "duplicate_queued",
  "printer_control_queued",
]);

type ActionStatusTone = "success" | "warning" | "error";

type StatusTranslator = {
  (key: string): string;
  has(key: string): boolean;
};

export function ActionStatusToast({
  status,
  onConsumed,
}: {
  status?: string;
  onConsumed: (status: string) => void;
}) {
  const tStatus = useTranslations("runtime.actionStatus");
  const shownStatuses = useRef<Set<string>>(new Set());

  useEffect(() => {
    if (!status || shownStatuses.current.has(status)) {
      return;
    }
    shownStatuses.current.add(status);

    const message = formatActionStatus(status, tStatus);
    const tone = actionStatusTone(status);
    if (tone === "warning") {
      toast.warning(message);
    } else if (tone === "error") {
      toast.error(message);
    } else {
      toast.success(message);
    }
    clearStatusQueryFromUrl();
    onConsumed(status);
  }, [status, tStatus, onConsumed]);

  return null;
}

export function formatActionStatus(status: string, tStatus: StatusTranslator) {
  if (tStatus.has(status)) {
    return tStatus(status);
  }
  return status
    .split("_")
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

export function actionStatusTone(status: string): ActionStatusTone {
  if (status.includes("partial")) {
    return "warning";
  }
  if (status.startsWith("http_") || !knownPositiveActionStatuses.has(status)) {
    return "error";
  }
  return "success";
}

export function clearStatusQueryFromUrl() {
  const url = new URL(window.location.href);
  url.searchParams.delete("status");
  const nextUrl = `${url.pathname}${url.search}${url.hash}`;
  window.history.replaceState(window.history.state, "", nextUrl);
}
```

- [ ] **Step 6: Wire `DashboardRuntime` to consume action status**

Existing facts in `frontend/app/dashboard-runtime.tsx`: `DashboardRuntimeProps` already includes `actionStatus?: string`; `DashboardRuntime` already destructures `actionStatus`; `frontend/app/dashboard-data.tsx` already passes `actionStatus={actionStatus}` into `DashboardRuntime`; the file currently has `const tStatus = useTranslations('runtime.actionStatus')`; and the old inline cyan banner calls a local `formatActionStatus(actionStatus, tStatus)` helper. Do not add a new prop or change the server render path.

Modify `frontend/app/dashboard-runtime.tsx`:

```tsx
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
```

Add:

```tsx
import { ActionStatusToast } from "./action-status-toast";
```

Remove the local `formatActionStatus` function and the `const tStatus = useTranslations('runtime.actionStatus')` line.

Add consumed state near the other `useState` calls:

```tsx
const [consumedActionStatus, setConsumedActionStatus] = useState<string | null>(
  null,
);
const pendingActionStatus =
  actionStatus && consumedActionStatus !== actionStatus
    ? actionStatus
    : undefined;
const consumeActionStatus = useCallback((status: string) => {
  setConsumedActionStatus(status);
}, []);
```

Replace the existing `dashboardQuery` block with this complete block:

```tsx
const dashboardQuery: DashboardQuery = {
  tenant: selectedTenant?.id,
  command: view === "agents" ? selectedCommandId : undefined,
  status: pendingActionStatus,
};
```

Inside the `<main>` element, render the toast trigger before the errors block. The top of `<main>` should become:

```tsx
<main className="mx-auto flex w-full max-w-7xl flex-col gap-5 px-4 py-5 sm:px-6 lg:px-8">
  <ActionStatusToast
    status={pendingActionStatus}
    onConsumed={consumeActionStatus}
  />

  {errors.length > 0 ? (
    <div className="rounded-md border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-950">
      {tErr("errorsIncomplete")} {errors.join("; ")}.
    </div>
  ) : null}

  <DashboardViewContent
    view={view}
    auth={auth}
    selectedTenant={selectedTenant}
    health={health}
    attentionItems={attentionItems}
    topSeverity={topSeverity}
    liveState={liveState}
    lastEventAt={lastEventAt}
    fleetEmpty={fleetEmpty}
    printers={printers}
    agents={agents}
    jobs={jobs}
    selectedCommand={selectedCommand}
    commandData={commandData}
    notifications={notifications}
    users={users}
    userIdentities={userIdentities}
    tenantTokens={tenantTokens}
    joinLinks={joinLinks}
    auditEvents={auditEvents}
    adminUnavailable={adminUnavailable}
  />
</main>
```

Delete this old inline cyan banner entirely:

```tsx
{
  actionStatus ? (
    <div className="rounded-md border border-cyan-200 bg-cyan-50 px-3 py-2 text-sm text-cyan-950">
      {formatActionStatus(actionStatus, tStatus)}
    </div>
  ) : null;
}
```

- [ ] **Step 7: Run focused tests**

Run:

```bash
cd frontend && npm run test -- action-status-toast.test.tsx dashboard-shell.test.tsx
```

Expected: PASS. If the Strict Mode test fails, keep the duplicate guard mount-scoped and debug why the `useRef` path did not suppress the replay. Do not use a module-scoped or session-scoped guard, because later legitimate repeats of the same status must still be able to toast after a remount.

---

### Task 3: Update Docs and Verify

**Files:**

- Modify: `docs/roadmap.md`

**Interfaces:**

- Consumes: `runtime.actionStatus` lookup from `ActionStatusToast`.
- Produces: roadmap documentation of the Sonner prompt migration.

- [ ] **Step 1: Update the roadmap**

Add a concise bullet near the current frontend/dashboard entries in `docs/roadmap.md`:

```markdown
- Replaced redirect-backed dashboard action prompts (`status=...`, such as refresh/retry/dispatch queued results) with shadcn Sonner toasts; consumed statuses are removed from the URL and dashboard navigation while durable live notifications, admin secret results, and data-integrity errors remain in-page.
```

- [ ] **Step 2: Run full frontend tests**

Run:

```bash
cd frontend && npm run test
```

Expected: PASS.

- [ ] **Step 3: Run production build**

Run:

```bash
cd frontend && npm run build
```

Expected: PASS.

- [ ] **Step 4: Run Nix web packaging check**

Run:

```bash
nix build --show-trace .#checks.x86_64-linux.pandar-web
```

Expected: PASS. If the environment cannot run Nix or the build is blocked by cache/network limits, capture the exact command output and verify that `nix/pandar.nix` contains a refreshed `pandar-web.npmDepsHash` from the lockfile-changing build attempt.

- [ ] **Step 5: Inspect final diff**

Run:

```bash
git status --short
git diff -- frontend package-lock.json nix/pandar.nix docs/superpowers/specs/2026-06-30-frontend-sonner-action-prompts-design.md docs/superpowers/plans/2026-06-30-frontend-sonner-action-prompts.md docs/roadmap.md
```

Expected: Only the files listed in this plan and the approved spec/plan artifacts are changed.

---

## Plan Self-Review

- Spec coverage: Task 1 installs Sonner, mounts it globally, and refreshes the Nix npm dependency hash; Task 2 consumes redirect `status`, removes the inline banner, clears URL status, guards duplicate effects, keeps navigation status-free after consumption, localizes known statuses, and preserves fallback behavior for unexpected backend error codes; Task 3 updates roadmap and verifies frontend plus Nix packaging checks.
- Placeholder scan: no `TBD`, `TODO`, or undefined helper names remain.
- Type consistency: `ActionStatusToast`, `formatActionStatus`, `actionStatusTone`, and `clearStatusQueryFromUrl` names match between implementation and tests.
