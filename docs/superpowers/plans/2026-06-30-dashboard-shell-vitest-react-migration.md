# Dashboard Shell Vitest React Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the dashboard shell standalone smoke coverage into the root frontend Vitest suite and add a focused React Testing Library check for tenant switching.

**Architecture:** Keep production dashboard code unchanged. Add one colocated Vitest test file that covers the pure dashboard shell helper contract and renders `DashboardShellHeader` through the existing next-intl test pattern. Remove the standalone smoke script once the Vitest test covers its assertions, then update docs to point at the standard frontend test command.

**Tech Stack:** Next.js 16 app router, React 19, Vitest 4, jsdom, React Testing Library, `@testing-library/user-event`, next-intl.

## Global Constraints

- No legacy fallback.
- Keep changes simple and focused.
- Update `docs/roadmap.md` after code changes.
- Do not migrate other standalone smoke scripts under `frontend/app/`.
- Do not change dashboard runtime behavior, route paths, query semantics, localization text, sidebar UI, backend APIs, database state, dependencies, or the root `frontend/package.json` test script.
- Production dashboard code should not change for this migration.
- After implementation, run `npm --prefix frontend run test -- app/dashboard-shell.test.tsx`, `npm --prefix frontend run test`, `npm --prefix frontend run build`, `cargo fmt`, `cargo clippy`, and `cargo nextest run --manifest-path "Cargo.toml" --workspace`; record exact failures if environment/tooling blocks any command.

---

## File Structure

- Create `frontend/app/dashboard-shell.test.tsx`: Vitest helper-contract tests plus a focused React Testing Library tenant-switch test for `DashboardShellHeader`.
- Delete `frontend/app/dashboard-shell.smoke.mjs`: removed standalone Node smoke coverage after equivalent Vitest assertions exist.
- Modify `docs/superpowers/specs/2026-06-29-dashboard-sidebar-08-design.md`: update the stale active acceptance criterion that names the deleted smoke file so it points to the Vitest test command instead.
- Modify `docs/roadmap.md`: record the smoke-to-Vitest/React Testing Library migration in the Completed section.

### Task 1: Dashboard Shell Vitest And React Test Migration

**Files:**

- Create: `frontend/app/dashboard-shell.test.tsx`
- Delete: `frontend/app/dashboard-shell.smoke.mjs`
- Modify: `docs/superpowers/specs/2026-06-29-dashboard-sidebar-08-design.md`
- Modify: `docs/roadmap.md`

**Interfaces:**

- Consumes: `DASHBOARD_VIEWS`, `dashboardRootRedirectTarget`, `dashboardSidebarHref`, `dashboardTenantHref`, `dashboardViewTitleKey`, `logoutHref`, and `type DashboardView` from `frontend/app/dashboard-shell.ts`.
- Consumes: `DashboardShellHeader` from `frontend/app/dashboard-shell-header.tsx`.
- Consumes: `NextIntlClientProvider` and `frontend/messages/en.json` for localized header rendering.
- Produces: `frontend/app/dashboard-shell.test.tsx` discovered by the existing `vitest run` script.

- [ ] **Step 1: Run the focused Vitest target before the migration test exists**

Run from the repo root:

```bash
npm --prefix frontend run test -- app/dashboard-shell.test.tsx
```

Expected: FAIL because `frontend/app/dashboard-shell.test.tsx` does not exist or no matching test file is found. This confirms the new command is not already covered before adding the migrated test.

- [ ] **Step 2: Write the Vitest helper-contract test**

Create `frontend/app/dashboard-shell.test.tsx` with the pure helper assertions first:

```tsx
import { describe, expect, it } from "vitest";

import {
  DASHBOARD_VIEWS,
  dashboardRootRedirectTarget,
  dashboardSidebarHref,
  dashboardTenantHref,
  dashboardViewTitleKey,
  logoutHref,
} from "./dashboard-shell";

describe("dashboard shell helpers", () => {
  it("defines the route-backed dashboard views", () => {
    expect(DASHBOARD_VIEWS).toEqual(["devices", "agents", "users", "settings"]);

    for (const view of DASHBOARD_VIEWS) {
      expect(dashboardViewTitleKey(view)).toBe(view);
    }
  });

  it("builds dashboard redirect and navigation URLs", () => {
    expect(dashboardRootRedirectTarget({})).toBe("/devices");
    expect(
      dashboardRootRedirectTarget({
        tenant: "tenant 1",
        status: "job_created",
      }),
    ).toBe("/devices?tenant=tenant+1&status=job_created");
    expect(
      dashboardRootRedirectTarget({
        tenant: "t1",
        command: "cmd1",
        status: "refresh_queued",
      }),
    ).toBe("/agents?tenant=t1&command=cmd1&status=refresh_queued");

    expect(
      dashboardSidebarHref("agents", {
        tenant: "t1",
        command: "cmd1",
        status: "done",
      }),
    ).toBe("/agents?tenant=t1");
    expect(dashboardSidebarHref("users", {})).toBe("/users");

    expect(
      dashboardTenantHref("agents", "t2", {
        tenant: "t1",
        command: "cmd1",
        status: "done",
      }),
    ).toBe("/agents?tenant=t2&command=cmd1&status=done");
    expect(
      dashboardTenantHref("devices", "t2", {
        tenant: "t1",
        command: "cmd1",
        status: "done",
      }),
    ).toBe("/devices?tenant=t2&status=done");
  });

  it("returns a logout href only when a provider sign-out URL exists", () => {
    expect(logoutHref({ signOutUrl: null })).toBeNull();
    expect(logoutHref({ signOutUrl: "https://auth.example/sign-out" })).toBe(
      "https://auth.example/sign-out",
    );
  });
});
```

- [ ] **Step 3: Run the helper-contract test to verify it passes before deleting the smoke script**

Run from the repo root:

```bash
npm --prefix frontend run test -- app/dashboard-shell.test.tsx
```

Expected: PASS. This confirms the Vitest file is discovered and the pure helper contract matches the standalone smoke assertions before removing the old script.

- [ ] **Step 4: Add the React Testing Library tenant-switch test**

Extend `frontend/app/dashboard-shell.test.tsx` so the complete file becomes:

```tsx
import { NextIntlClientProvider } from "next-intl";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import { DashboardShellHeader } from "./dashboard-shell-header";
import {
  DASHBOARD_VIEWS,
  dashboardRootRedirectTarget,
  dashboardSidebarHref,
  dashboardTenantHref,
  dashboardViewTitleKey,
  logoutHref,
  type DashboardQuery,
  type DashboardView,
} from "./dashboard-shell";
import type { Tenant } from "./dashboard-types";

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: vi.fn() }),
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

function stubLocationAssign(assign: (url: string) => void) {
  const originalWindow = window;
  const location = new Proxy(originalWindow.location, {
    get(target, prop, receiver) {
      if (prop === "assign") {
        return assign;
      }
      return Reflect.get(target, prop, receiver);
    },
  });

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

describe("dashboard shell helpers", () => {
  it("defines the route-backed dashboard views", () => {
    expect(DASHBOARD_VIEWS).toEqual(["devices", "agents", "users", "settings"]);

    for (const view of DASHBOARD_VIEWS) {
      expect(dashboardViewTitleKey(view)).toBe(view);
    }
  });

  it("builds dashboard redirect and navigation URLs", () => {
    expect(dashboardRootRedirectTarget({})).toBe("/devices");
    expect(
      dashboardRootRedirectTarget({
        tenant: "tenant 1",
        status: "job_created",
      }),
    ).toBe("/devices?tenant=tenant+1&status=job_created");
    expect(
      dashboardRootRedirectTarget({
        tenant: "t1",
        command: "cmd1",
        status: "refresh_queued",
      }),
    ).toBe("/agents?tenant=t1&command=cmd1&status=refresh_queued");

    expect(
      dashboardSidebarHref("agents", {
        tenant: "t1",
        command: "cmd1",
        status: "done",
      }),
    ).toBe("/agents?tenant=t1");
    expect(dashboardSidebarHref("users", {})).toBe("/users");

    expect(
      dashboardTenantHref("agents", "t2", {
        tenant: "t1",
        command: "cmd1",
        status: "done",
      }),
    ).toBe("/agents?tenant=t2&command=cmd1&status=done");
    expect(
      dashboardTenantHref("devices", "t2", {
        tenant: "t1",
        command: "cmd1",
        status: "done",
      }),
    ).toBe("/devices?tenant=t2&status=done");
  });

  it("returns a logout href only when a provider sign-out URL exists", () => {
    expect(logoutHref({ signOutUrl: null })).toBeNull();
    expect(logoutHref({ signOutUrl: "https://auth.example/sign-out" })).toBe(
      "https://auth.example/sign-out",
    );
  });
});

describe("DashboardShellHeader", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("preserves the agents command context when switching tenants", async () => {
    const user = userEvent.setup();
    const assign = vi.fn<(url: string) => void>();
    stubLocationAssign(assign);
    const query: DashboardQuery = {
      tenant: "t1",
      command: "cmd1",
      status: "done",
    };
    const view: DashboardView = "agents";

    renderWithMessages(
      <DashboardShellHeader
        query={query}
        selectedTenant={tenants[0]}
        tenants={tenants}
        view={view}
      />,
    );

    await user.selectOptions(screen.getByRole("combobox"), "t2");

    expect(assign).toHaveBeenCalledWith(dashboardTenantHref(view, "t2", query));
  });
});
```

- [ ] **Step 5: Run the React test and resolve only test-harness issues**

Run:

```bash
npm --prefix frontend run test -- app/dashboard-shell.test.tsx
```

Expected: PASS. If the `window` proxy setup needs adjustment for jsdom, keep the same behavior assertion and change only the local test harness. Do not change production code to make the test easier.

- [ ] **Step 6: Delete the standalone dashboard shell smoke script**

Delete `frontend/app/dashboard-shell.smoke.mjs`.

- [ ] **Step 7: Update stale active dashboard-shell smoke documentation**

In `docs/superpowers/specs/2026-06-29-dashboard-sidebar-08-design.md`, replace this acceptance criterion:

```md
- Focused smoke test follows the repo's standalone smoke pattern at `frontend/app/dashboard-shell.smoke.mjs` and is runnable with `node --experimental-transform-types app/dashboard-shell.smoke.mjs` from `frontend/`. It covers the dashboard route/view contract, query preservation, and visible Logout href behavior.
```

with:

```md
- Focused dashboard shell coverage runs through `frontend/app/dashboard-shell.test.tsx` with `npm --prefix frontend run test -- app/dashboard-shell.test.tsx`. It covers the dashboard route/view contract, query preservation, visible Logout href behavior, and tenant-switch rendering behavior.
```

Do not rewrite the older completed implementation plan steps in `docs/superpowers/plans/2026-06-29-dashboard-sidebar-08.md`; they are historical execution records for the original sidebar task.

- [ ] **Step 8: Update the roadmap**

Add this bullet near the existing dashboard sidebar/onboarding/frontend test bullets in `docs/roadmap.md`:

```md
- Migrated the dashboard shell helper smoke coverage into the root frontend Vitest suite and added React Testing Library coverage for tenant switching, so the route/query/logout contract now runs under `npm --prefix frontend run test` instead of a standalone Node smoke script.
```

- [ ] **Step 9: Run focused and full frontend verification**

Run:

```bash
npm --prefix frontend run test -- app/dashboard-shell.test.tsx
npm --prefix frontend run test
npm --prefix frontend run build
```

Expected: all commands exit 0. If `npm --prefix frontend run build` fails for an environment or pre-existing issue, capture the exact failing command and error output.

- [ ] **Step 10: Run required repository verification**

Run from the repo root:

```bash
cargo fmt
cargo clippy
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: all commands exit 0. If any command fails because tooling or environment is missing, capture the exact failing command and error output.
