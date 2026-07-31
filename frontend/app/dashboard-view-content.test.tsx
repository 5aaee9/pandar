import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import zh from "../messages/zh.json";
import { DashboardViewContent } from "./dashboard-view-content";
import { SettingsDashboard } from "./settings-dashboard";
import type { AuthMetadata, Printer, Tenant, TenantToken } from "./dashboard-types";

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: vi.fn() }),
}));

function createTestQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
}

function renderWithMessages(children: React.ReactNode, locale = "en") {
  return render(
    <QueryClientProvider client={createTestQueryClient()}>
      <NextIntlClientProvider
        locale={locale}
        messages={locale === "zh" ? zh : en}
      >
        {children}
      </NextIntlClientProvider>
    </QueryClientProvider>,
  );
}

const auth: AuthMetadata = {
  source: "none",
  cookieName: "pandar_auth",
  provider: "none",
  signInUrl: null,
  signOutUrl: null,
};

const tenant: Tenant = {
  id: "t1",
  slug: "tenant-one",
  display_name: "Tenant One",
  created_at: "2026-06-30T00:00:00Z",
};

const printer: Printer = {
  id: "printer-1",
  tenant_id: "t1",
  agent_id: "agent-1",
  serial_number: "SN1",
  name: "Office A1",
  model: "X1C",
  status: "idle",
  last_seen_at: "2026-07-02T00:00:00Z",
  created_at: "2026-07-02T00:00:00Z",
  materials: null,
};

const nowMs = Date.parse("2026-07-17T00:00:00Z");

const tenantTokens: TenantToken[] = [
  {
    id: "future-token",
    tenant_id: tenant.id,
    name: "Future token",
    scopes: ["plugin:studio"],
    created_by_user_id: null,
    created_at: "2026-07-16T23:00:00Z",
    last_used_at: "2026-07-16T23:50:00Z",
    expires_at: "2026-07-17T00:05:00Z",
    revoked_at: null,
  },
  {
    id: "past-token",
    tenant_id: tenant.id,
    name: "Past token",
    scopes: ["*"],
    created_by_user_id: null,
    created_at: "2026-07-16T21:00:00Z",
    last_used_at: null,
    expires_at: "2026-07-16T23:55:00Z",
    revoked_at: null,
  },
  {
    id: "never-token",
    tenant_id: tenant.id,
    name: "Never token",
    scopes: ["*"],
    created_by_user_id: null,
    created_at: "2026-07-16T22:00:00Z",
    last_used_at: null,
    expires_at: null,
    revoked_at: null,
  },
  {
    id: "revoked-token",
    tenant_id: tenant.id,
    name: "Revoked token",
    scopes: [],
    created_by_user_id: null,
    created_at: "2026-07-16T20:00:00Z",
    last_used_at: "2026-07-16T22:00:00Z",
    expires_at: "2026-07-17T01:00:00Z",
    revoked_at: "2026-07-16T23:59:00Z",
  },
];

describe("DashboardViewContent", () => {
  const baseProps = {
    auth,
    selectedTenant: tenant,
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
    nowMs: 0,
    selectedCommand: null,
    commandData: null,
    notifications: [],
    tenantTokens: [],
    auditEvents: [],
    adminUnavailable: false,
    adminLoadError: false,
    canManageJobs: true,
  };
  const settingsProps = {
    auth,
    selectedTenant: tenant,
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

  it("keeps devices focused on overview and printer inventory", () => {
    renderWithMessages(<DashboardViewContent {...baseProps} view="devices" />);

    expect(screen.getByText("All systems nominal")).toBeVisible();
    expect(
      screen.getByRole("heading", { name: "Printer inventory" }),
    ).toBeVisible();
    expect(
      screen.queryByRole("heading", { name: "Print jobs" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: "Dispatch print job" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: "Recovery actions" }),
    ).not.toBeInTheDocument();
  });

  it("links overview agent and job stats to their dashboard pages", () => {
    renderWithMessages(<DashboardViewContent {...baseProps} view="devices" />);

    expect(
      screen.getByRole("link", { name: "Agents 1/1 connected" }),
    ).toHaveAttribute("href", "/agents?tenant=t1");
    expect(
      screen.getByRole("link", { name: "Active jobs 0 active" }),
    ).toHaveAttribute("href", "/jobs?tenant=t1");
  });

  it("opens the dispatch form in a dialog from jobs", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <DashboardViewContent {...baseProps} view="jobs" printers={[printer]} />,
    );

    const jobsHeading = screen.getByRole("heading", { name: "Print jobs" });
    expect(jobsHeading).toBeVisible();
    expect(
      screen.queryByRole("heading", { name: "Dispatch print job" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: "Recovery actions" }),
    ).not.toBeInTheDocument();
    expect(
      within(jobsHeading.parentElement!.parentElement!).queryByText("0 jobs"),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Clear jobs" })).toBeDisabled();
    expect(
      screen.queryByRole("heading", { name: "Printer inventory" }),
    ).not.toBeInTheDocument();

    const newButton = screen.getByRole("button", { name: "New print job" });
    expect(newButton).toHaveAttribute("aria-haspopup", "dialog");

    await user.click(newButton);

    expect(
      screen.getByRole("dialog", { name: "Dispatch print job" }),
    ).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Close" }));

    expect(
      screen.queryByRole("heading", { name: "Dispatch print job" }),
    ).not.toBeInTheDocument();
  });

  it("presents tenant tokens as a status-aware management list", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <SettingsDashboard
        {...settingsProps}
        nowMs={nowMs}
        tenantTokens={tenantTokens}
      />,
    );

    expect(screen.getByText("2 active · 4 total")).toBeVisible();
    expect(screen.getByText("Studio plugin")).toBeVisible();
    expect(screen.getByText("Read-only")).toBeVisible();
    expect(screen.getByText("10 minutes ago")).toBeVisible();

    const futureExpiration = screen.getByText("Expires in 5 minutes");
    expect(futureExpiration).toBeVisible();
    expect(futureExpiration).toHaveAttribute(
      "datetime",
      "2026-07-17T00:05:00Z",
    );
    expect(futureExpiration).toHaveAttribute("title", "Jul 17, 2026, 12:05 AM");
    expect(screen.getByText("Expired 5 minutes ago")).toBeVisible();
    expect(screen.getByText("Expires never")).toBeVisible();

    const futureRow = screen.getByText("Future token").closest("article")!;
    const pastRow = screen.getByText("Past token").closest("article")!;
    const revokedRow = screen.getByText("Revoked token").closest("article")!;
    expect(futureRow).toHaveAttribute("data-token-status", "active");
    expect(pastRow).toHaveAttribute("data-token-status", "expired");
    expect(revokedRow).toHaveAttribute("data-token-status", "revoked");
    expect(within(futureRow).getByText("Active")).toHaveClass(
      "text-success",
    );
    expect(within(pastRow).getByText("Expired")).toHaveClass(
      "text-warning",
    );
    expect(within(revokedRow).getByText("Revoked")).toHaveClass(
      "text-muted-foreground",
    );
    expect(
      within(futureRow).getByRole("button", { name: "Rotate token Future token" }),
    ).toBeVisible();
    expect(
      within(futureRow).getByRole("button", { name: "Revoke token Future token" }),
    ).toHaveClass("text-destructive", "bg-destructive/10");
    expect(
      within(pastRow).getByRole("button", { name: "Rotate token Past token" }),
    ).toBeVisible();
    expect(
      within(pastRow).getByRole("button", { name: "Revoke token Past token" }),
    ).toBeVisible();
    expect(within(revokedRow).queryByRole("button")).not.toBeInTheDocument();

    const rows = screen.getAllByRole("article");
    expect(rows.map((row) => row.getAttribute("data-token-id"))).toEqual([
      "future-token",
      "never-token",
      "past-token",
      "revoked-token",
    ]);

    expect(screen.getByRole("heading", { name: "Make Pandar yours" })).toBeVisible();
    expect(screen.getByRole("navigation", { name: "Settings sections" })).toBeVisible();
    expect(
      screen.queryByRole("dialog", { name: "Create tenant token" }),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Create token" }));

    const createDialog = screen.getByRole("dialog", {
      name: "Create tenant token",
    });
    expect(within(createDialog).getByLabelText("Name")).toHaveClass(
      "border-input",
      "bg-background",
      "text-foreground",
    );
    expect(screen.getByText("future-token")).toHaveClass(
      "break-all",
      "font-mono",
    );
  });

  it("preserves the current expiration for an explicit token rotation", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <SettingsDashboard
        {...settingsProps}
        nowMs={nowMs}
        tenantTokens={[tenantTokens[0]]}
      />,
    );

    const row = screen.getByText("Future token").closest("article")!;
    await user.click(within(row).getByRole("button", { name: "Rotate token Future token" }));

    const dialog = screen.getByRole("dialog", { name: "Rotate tenant token" });
    expect(within(dialog).getByLabelText("Expires at")).toHaveValue(
      "2026-07-17T00:05:00Z",
    );
    expect(within(dialog).getByText(/keeps this expiration/)).toBeVisible();
  });

  it("clears an expired token expiration by default when rotating", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <SettingsDashboard
        {...settingsProps}
        nowMs={nowMs}
        tenantTokens={[tenantTokens[1]]}
      />,
    );

    const row = screen.getByText("Past token").closest("article")!;
    expect(within(row).getByRole("button", { name: "Revoke token Past token" })).toBeVisible();

    await user.click(within(row).getByRole("button", { name: "Rotate token Past token" }));

    const dialog = screen.getByRole("dialog", { name: "Rotate tenant token" });
    expect(within(dialog).getByLabelText("Expires at")).toHaveValue("");
    expect(
      within(dialog).getByText(/expired.*new future expiration/i),
    ).toBeVisible();
  });

  it("treats the exact expiration instant as expired and gives revoked precedence", () => {
    const exactExpiration: TenantToken = {
      ...tenantTokens[0],
      id: "exact-expiration-token",
      name: "Exact expiration token",
      expires_at: new Date(nowMs).toISOString(),
    };
    const expiredAndRevoked: TenantToken = {
      ...tenantTokens[1],
      id: "expired-and-revoked-token",
      name: "Expired and revoked token",
      revoked_at: "2026-07-16T23:59:00Z",
    };

    renderWithMessages(
      <SettingsDashboard
        {...settingsProps}
        nowMs={nowMs}
        tenantTokens={[exactExpiration, expiredAndRevoked]}
      />,
    );

    const exactExpirationRow = screen
      .getByText("Exact expiration token")
      .closest("article")!;
    const expiredAndRevokedRow = screen
      .getByText("Expired and revoked token")
      .closest("article")!;
    expect(exactExpirationRow).toHaveAttribute("data-token-status", "expired");
    expect(within(exactExpirationRow).getByText("Expired")).toBeVisible();
    expect(expiredAndRevokedRow).toHaveAttribute(
      "data-token-status",
      "revoked",
    );
    expect(within(expiredAndRevokedRow).getByText("Revoked")).toBeVisible();
  });

  it("localizes tenant token relative expiration in Chinese", () => {
    renderWithMessages(
      <SettingsDashboard
        {...settingsProps}
        nowMs={nowMs}
        tenantTokens={tenantTokens}
      />,
      "zh",
    );

    expect(screen.getByText("5 分钟内过期")).toBeVisible();
    expect(screen.getByText("5 分钟前过期")).toBeVisible();
    expect(screen.getByText("永不过期")).toBeVisible();
    expect(screen.getByText("有效 2 个 · 共 4 个")).toBeVisible();
    expect(
      within(screen.getByText("Past token").closest("article")!).getByText(
        "已过期",
      ),
    ).toBeVisible();
    expect(
      within(screen.getByText("Revoked token").closest("article")!).getByText(
        "已吊销",
      ),
    ).toBeVisible();
  });

  it("keeps the absolute token expiration fallback until the dashboard clock starts", () => {
    renderWithMessages(
      <SettingsDashboard
        {...settingsProps}
        tenantTokens={[tenantTokens[0]]}
      />,
    );

    expect(screen.getByText("Expires Jul 17, 2026, 12:05 AM")).toBeVisible();
  });
});
