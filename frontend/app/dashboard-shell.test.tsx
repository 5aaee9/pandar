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
  const actual = await importOriginal<typeof import("../components/ui/sidebar")>();
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
    expect(DASHBOARD_VIEWS).toEqual([
      "devices",
      "agents",
      "users",
      "settings",
    ]);

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
