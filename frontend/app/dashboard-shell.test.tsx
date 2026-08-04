import { NextIntlClientProvider } from "next-intl";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import { AppSidebar } from "../components/app-sidebar";
import { Sidebar, SidebarProvider } from "../components/ui/sidebar";
import { FleetStatusStrip } from "./dashboard-overview";
import { DashboardShellHeader } from "./dashboard-shell-header";
import { DashboardShellLayout } from "./dashboard-shell-layout";
import { DashboardShellProvider } from "./dashboard-shell-provider";
import { SettingsDashboard } from "./settings-dashboard";
import {
  DASHBOARD_VIEWS,
  agentSettingsHref,
  dashboardRootRedirectTarget,
  dashboardSidebarHref,
  dashboardTenantHref,
  dashboardViewTitleKey,
  logoutHref,
  type DashboardQuery,
} from "./dashboard-shell";
import type { AuthMetadata, Tenant } from "./dashboard-types";

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: vi.fn() }),
  usePathname: () => window.location.pathname,
  useSearchParams: () => new URLSearchParams(window.location.search),
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

function preventNavigation(link: HTMLElement) {
  link.addEventListener("click", (event) => event.preventDefault(), { once: true });
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

const auth: AuthMetadata = {
  source: "none",
  cookieName: "pandar_auth",
  provider: "none",
  signInUrl: null,
  signOutUrl: null,
};

describe("dashboard shell helpers", () => {
  it("defines the route-backed dashboard views", () => {
    expect(DASHBOARD_VIEWS).toEqual([
      "devices",
      "jobs",
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
    expect(
      dashboardSidebarHref("jobs", {
        tenant: "t1",
        command: "cmd1",
        status: "done",
      }),
    ).toBe("/jobs?tenant=t1");
    expect(dashboardSidebarHref("users", {})).toBe("/users");
    expect(agentSettingsHref("tenant 1", "agent/1")).toBe(
      "/agents/agent%2F1/settings?tenant=tenant+1",
    );
    expect(agentSettingsHref("t1", "a1")).toBe(
      "/agents/a1/settings?tenant=t1",
    );

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
    expect(
      dashboardTenantHref("jobs", "t2", {
        tenant: "t1",
        command: "cmd1",
        status: "done",
      }),
    ).toBe("/jobs?tenant=t2&status=done");
  });

  it("returns a logout href only when a provider sign-out URL exists", () => {
    expect(logoutHref({ signOutUrl: null })).toBeNull();
    expect(logoutHref({ signOutUrl: "https://auth.example/sign-out" })).toBe(
      "https://auth.example/sign-out",
    );
  });
});

describe("DashboardShellHeader", () => {
  it("does not render tenant or language selectors in the top bar", () => {
    renderWithMessages(<DashboardShellHeader view="agents" />);

    expect(screen.getByRole("heading", { name: "Agents" })).toBeVisible();
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "English" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "中文" })).not.toBeInTheDocument();
  });

  it("uses theme colors so the top bar remains readable in dark mode", () => {
    renderWithMessages(<DashboardShellHeader view="devices" />);

    expect(screen.getByRole("banner")).toHaveClass(
      "border-border",
      "bg-background/95",
    );
    expect(screen.getByText("Pandar")).toHaveClass("text-muted-foreground");
    expect(screen.getByRole("heading", { name: "Devices" })).toHaveClass(
      "text-foreground",
    );
  });
});

describe("FleetStatusStrip", () => {
  it("uses dark mode contrast classes for the status strip and stat values", () => {
    renderWithMessages(
      <FleetStatusStrip
        health={{
          printersTotal: 0,
          printersOnline: 0,
          agentsTotal: 1,
          agentsConnected: 1,
          jobsActive: 0,
          jobsFailed: 0,
        }}
        attentionCount={0}
        topSeverity={null}
        liveState="idle"
        lastEventAt={null}
        fleetEmpty={false}
      />,
    );

    expect(screen.getByRole("region", { name: "Fleet status" })).toHaveClass(
      "border-success/40",
      "bg-success/10",
    );
    expect(screen.getByRole("region", { name: "Fleet status" }).firstElementChild).not.toHaveClass(
      "sm:flex-row",
      "sm:items-center",
    );
    expect(screen.getByRole("region", { name: "Fleet status" }).firstElementChild).toHaveClass(
      "lg:flex-row",
      "lg:items-center",
    );
    expect(screen.getByText("0/0 online").closest(".grid")).not.toHaveClass(
      "sm:grid-cols-3",
      "sm:gap-0",
      "sm:pl-5",
    );
    expect(screen.getByText("0/0 online").closest(".grid")).toHaveClass(
      "sm:grid-cols-2",
      "lg:grid-cols-3",
      "lg:gap-0",
      "lg:pl-5",
    );
    expect(screen.getByText("0/0 online").closest("a")?.parentElement).not.toHaveClass(
      "sm:divide-x",
      "sm:border-l",
    );
    for (const name of [
      "Printers 0/0 online",
      "Agents 1/1 connected",
      "Active jobs 0 active",
    ]) {
      const link = screen.getByRole("link", { name });
      expect(link).not.toHaveClass("sm:before:absolute", "sm:ml-4");
      expect(link).not.toHaveClass("sm:ml-2");
      expect(link).toHaveClass("lg:ml-4");
      expect(link.parentElement).toHaveClass(
        "lg:before:absolute",
        "lg:before:left-2",
        "lg:before:top-2",
        "lg:before:bottom-2",
        "lg:before:w-px",
        "lg:before:bg-success/30",
      );
    }
    expect(screen.getByRole("link", { name: "Agents 1/1 connected" }).parentElement).not.toHaveClass(
      "sm:before:absolute",
      "sm:before:left-0",
      "sm:before:top-0",
      "sm:before:h-full",
    );
    expect(screen.getByRole("link", { name: "Agents 1/1 connected" }).parentElement).toHaveClass(
      "lg:before:absolute",
      "lg:before:w-px",
    );
    expect(screen.getByText("0/0 online")).toHaveClass("text-foreground");
    expect(screen.getByText("Printers")).toHaveClass("text-muted-foreground");
  });
});

describe("AppSidebar", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("switches desktop sidebar state on the keyboard shortcut", () => {
    const { container } = renderWithMessages(
      <SidebarProvider defaultOpen>
        <Sidebar>Navigation</Sidebar>
      </SidebarProvider>,
    );
    const sidebar = container.querySelector('[data-slot="sidebar"][data-state]');

    expect(sidebar).toHaveAttribute("data-state", "expanded");
    fireEvent.keyDown(window, { key: "b", ctrlKey: true });
    expect(sidebar).toHaveAttribute("data-state", "collapsed");
  });

  it("keeps the mobile sidebar on the Sheet interaction path", async () => {
    vi.stubGlobal("innerWidth", 500);
    const { container } = renderWithMessages(
      <SidebarProvider defaultOpen>
        <Sidebar>Mobile navigation</Sidebar>
      </SidebarProvider>,
    );

    await waitFor(() => {
      expect(container.querySelector('[data-slot="sidebar"][data-state]')).toBeNull();
    });
    fireEvent.keyDown(window, { key: "b", ctrlKey: true });

    expect(await screen.findByRole("dialog", { name: "Sidebar" })).toBeVisible();
  });

  it("keeps tenant access available inside the mobile sidebar", async () => {
    const user = userEvent.setup();
    vi.stubGlobal("innerWidth", 500);
    renderWithMessages(
      <SidebarProvider defaultOpen>
        <AppSidebar
          activeView="devices"
          auth={auth}
          query={{ tenant: "t1" }}
          selectedTenant={tenants[0]}
          tenants={tenants}
        />
      </SidebarProvider>,
    );

    fireEvent.keyDown(window, { key: "b", ctrlKey: true });
    const sidebar = await screen.findByRole("dialog", { name: "Dashboard" });
    await user.click(
      within(sidebar).getByRole("button", { name: "Select tenant access" }),
    );

    const menu = screen.getByRole("dialog", { name: "Tenant access" });
    const tenantLink = within(menu).getByRole("link", { name: "Tenant Two" });
    expect(tenantLink).toHaveAttribute(
      "href",
      "/devices?tenant=t2",
    );
    preventNavigation(tenantLink);
    await user.click(tenantLink);

    await waitFor(() => {
      expect(screen.queryByRole("dialog", { name: "Tenant access" })).not.toBeInTheDocument();
      expect(screen.queryByRole("dialog", { name: "Dashboard" })).not.toBeInTheDocument();
    });
  });

  it("switches tenant access while preserving the agents command context", async () => {
    const user = userEvent.setup();
    const query: DashboardQuery = {
      tenant: "t1",
      command: "cmd1",
      status: "done",
    };

    renderWithMessages(
      <SidebarProvider>
        <AppSidebar
          activeView="agents"
          auth={auth}
          query={query}
          selectedTenant={tenants[0]}
          tenants={tenants}
        />
      </SidebarProvider>,
    );

    const trigger = screen.getByRole("button", { name: "Select tenant access" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    await user.click(trigger);

    const menu = screen.getByRole("dialog", { name: "Tenant access" });
    expect(within(menu).getByText("Tenant access")).toBeVisible();
    expect(screen.getByRole("link", { name: "Tenant One" })).toHaveAttribute(
      "aria-current",
      "true",
    );
    const tenantLink = screen.getByRole("link", { name: "Tenant Two" });
    expect(tenantLink).toHaveAttribute(
      "href",
      dashboardTenantHref("agents", "t2", query),
    );
    preventNavigation(tenantLink);
    await user.click(tenantLink);
    await waitFor(() => {
      expect(trigger).toHaveAttribute("aria-expanded", "false");
      expect(screen.queryByRole("dialog", { name: "Tenant access" })).not.toBeInTheDocument();
    });
  });

  it("opens and closes tenant access from the collapsed desktop sidebar", async () => {
    const user = userEvent.setup();
    const { container } = renderWithMessages(
      <SidebarProvider defaultOpen={false}>
        <AppSidebar
          activeView="devices"
          auth={auth}
          query={{ tenant: "t1" }}
          selectedTenant={tenants[0]}
          tenants={tenants}
        />
      </SidebarProvider>,
    );

    expect(container.querySelector('[data-slot="sidebar"][data-state]')).toHaveAttribute(
      "data-state",
      "collapsed",
    );
    await user.click(screen.getByRole("button", { name: "Select tenant access" }));
    const tenantLink = screen.getByRole("link", { name: "Tenant Two" });
    preventNavigation(tenantLink);
    await user.click(tenantLink);

    await waitFor(() => {
      expect(screen.queryByRole("dialog", { name: "Tenant access" })).not.toBeInTheDocument();
    });
  });

  it("wires route command and status context through the dashboard layout", async () => {
    const user = userEvent.setup();
    window.history.replaceState({}, "", "/agents?tenant=t1&command=cmd1&status=done");
    renderWithMessages(
      <DashboardShellProvider initialTenants={tenants}>
        <DashboardShellLayout
          auth={auth}
          sidebarDefaultOpen
          tenants={tenants}
        >
          Dashboard content
        </DashboardShellLayout>
      </DashboardShellProvider>,
    );

    await user.click(screen.getByRole("button", { name: "Select tenant access" }));
    expect(screen.getByRole("link", { name: "Tenant Two" })).toHaveAttribute(
      "href",
      "/agents?tenant=t2&command=cmd1&status=done",
    );
  });

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
});

describe("SettingsView", () => {
  const settingsProps = {
    auth,
    selectedTenant: tenants[0],
    membershipRole: "tenant_admin",
    agents: [],
    printers: [],
    tenantTokens: [],
    auditEvents: [],
    adminUnavailable: false,
    adminLoadError: false,
    adminLoading: false,
    nowMs: 0,
  };

  it("renders the language selector in settings", () => {
    renderWithMessages(<SettingsDashboard {...settingsProps} />);

    expect(screen.getByText("Display language")).toBeVisible();
    expect(screen.getByRole("button", { name: "English" })).toBeVisible();
    expect(screen.getByRole("button", { name: "中文" })).toBeVisible();
  });

  it("renders the theme selector in settings", () => {
    renderWithMessages(<SettingsDashboard {...settingsProps} />);

    expect(screen.getByText("Color theme")).toBeVisible();
    expect(screen.getByRole("button", { name: "System" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Light" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Dark" })).toBeVisible();
  });
});
