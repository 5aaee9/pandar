import { NextIntlClientProvider } from "next-intl";
import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import { AppSidebar } from "../components/app-sidebar";
import { SidebarProvider } from "../components/ui/sidebar";
import { FleetStatusStrip } from "./dashboard-overview";
import { DashboardShellHeader } from "./dashboard-shell-header";
import { DashboardViewContent } from "./dashboard-view-content";
import {
  DASHBOARD_VIEWS,
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
    expect(screen.queryByRole("button", { name: "EN" })).not.toBeInTheDocument();
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
      "dark:border-emerald-900/60",
      "dark:bg-emerald-950/30",
    );
    expect(screen.getByRole("region", { name: "Fleet status" }).firstElementChild).not.toHaveClass(
      "sm:flex-row",
      "sm:items-center",
    );
    expect(screen.getByRole("region", { name: "Fleet status" }).firstElementChild).toHaveClass(
      "lg:flex-row",
      "lg:items-center",
    );
    expect(screen.getByText("0/0 online").closest("[aria-hidden]")).not.toHaveClass(
      "sm:grid-cols-3",
      "sm:gap-0",
      "sm:pl-5",
    );
    expect(screen.getByText("0/0 online").closest("[aria-hidden]")).toHaveClass(
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
        "lg:before:bg-emerald-200",
        "dark:lg:before:bg-emerald-900/60",
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
    expect(screen.getByText("0/0 online")).toHaveClass("dark:text-foreground");
    expect(screen.getByText("Printers")).toHaveClass("dark:text-muted-foreground");
  });
});

describe("AppSidebar", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("preserves the agents command context when switching tenants from the sidebar", () => {
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

    expect(screen.getByRole("link", { name: "Tenant Two" })).toHaveAttribute(
      "href",
      dashboardTenantHref("agents", "t2", query),
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
  it("renders the language selector in settings", () => {
    renderWithMessages(
      <DashboardViewContent
        view="settings"
        auth={auth}
        selectedTenant={null}
        health={{
          printersTotal: 0,
          printersOnline: 0,
          agentsTotal: 0,
          agentsConnected: 0,
          jobsActive: 0,
          jobsFailed: 0,
        }}
        attentionItems={[]}
        topSeverity={null}
        liveState="idle"
        lastEventAt={null}
        fleetEmpty={true}
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

    expect(screen.getByRole("heading", { name: "Language" })).toBeVisible();
    expect(screen.getByRole("button", { name: "EN" })).toBeVisible();
    expect(screen.getByRole("button", { name: "中文" })).toBeVisible();
  });

  it("renders the theme selector in settings", () => {
    renderWithMessages(
      <DashboardViewContent
        view="settings"
        auth={auth}
        selectedTenant={null}
        health={{
          printersTotal: 0,
          printersOnline: 0,
          agentsTotal: 0,
          agentsConnected: 0,
          jobsActive: 0,
          jobsFailed: 0,
        }}
        attentionItems={[]}
        topSeverity={null}
        liveState="idle"
        lastEventAt={null}
        fleetEmpty={true}
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

    expect(screen.getByRole("heading", { name: "Theme" })).toBeVisible();
    expect(screen.getByRole("button", { name: "System" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Light" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Dark" })).toBeVisible();
  });
});

describe("DashboardViewContent", () => {
  const baseProps = {
    auth,
    selectedTenant: tenants[0],
    health: {
      printersTotal: 0,
      printersOnline: 0,
      agentsTotal: 1,
      agentsConnected: 1,
      jobsActive: 0,
      jobsFailed: 0,
    },
    attentionItems: [],
    topSeverity: null,
    liveState: "idle" as const,
    lastEventAt: null,
    fleetEmpty: false,
    printers: [],
    agents: [],
    jobs: [],
    selectedCommand: null,
    commandData: null,
    notifications: [],
    users: [],
    userIdentities: [],
    tenantTokens: [],
    joinLinks: [],
    auditEvents: [],
    adminUnavailable: false,
  };

  it("keeps devices focused on overview and printer inventory", () => {
    renderWithMessages(<DashboardViewContent {...baseProps} view="devices" />);

    expect(screen.getByText("All systems nominal")).toBeVisible();
    expect(screen.getByRole("heading", { name: "Printer inventory" })).toBeVisible();
    expect(screen.queryByRole("heading", { name: "Print jobs" })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Dispatch print job" })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Recovery actions" })).not.toBeInTheDocument();
  });

  it("links overview agent and job stats to their dashboard pages", () => {
    renderWithMessages(<DashboardViewContent {...baseProps} view="devices" />);

    expect(screen.getByRole("link", { name: "Agents 1/1 connected" })).toHaveAttribute(
      "href",
      "/agents?tenant=t1",
    );
    expect(screen.getByRole("link", { name: "Active jobs 0 active" })).toHaveAttribute(
      "href",
      "/jobs?tenant=t1",
    );
  });

  it("renders job history, dispatch, and recovery on jobs", () => {
    renderWithMessages(<DashboardViewContent {...baseProps} view="jobs" />);

    expect(screen.getByRole("heading", { name: "Print jobs" })).toBeVisible();
    expect(screen.getByRole("heading", { name: "Dispatch print job" })).toBeVisible();
    expect(screen.getByRole("heading", { name: "Recovery actions" })).toBeVisible();
    expect(screen.queryByRole("heading", { name: "Printer inventory" })).not.toBeInTheDocument();
  });
});
