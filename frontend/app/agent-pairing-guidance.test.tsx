import type { ReactNode } from "react";
import { NextIntlClientProvider } from "next-intl";
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import {
  DashboardViewContent,
  type DashboardViewContentProps,
} from "./dashboard-view-content";
import type { Tenant } from "./dashboard-types";

vi.mock("./actions", () => ({
  controlPrinter: vi.fn(),
  createAgentPairing: vi.fn(),
  createJoinLink: vi.fn(),
  createTenantFromExternal: vi.fn(),
  createTenantToken: vi.fn(),
  deleteAgent: vi.fn(),
  diagnosePrinter: vi.fn(),
  discoverPrinters: vi.fn(),
  duplicateJob: vi.fn(),
  refreshAllAgents: vi.fn(),
  refreshPrinters: vi.fn(),
  retryDispatchJob: vi.fn(),
  retryDispatchJobs: vi.fn(),
  reprintJob: vi.fn(),
  revokeJoinLink: vi.fn(),
  revokeTenantToken: vi.fn(),
  rotateTenantToken: vi.fn(),
  updateTenantUserRole: vi.fn(),
}));

function renderWithMessages(children: ReactNode) {
  return render(
    <NextIntlClientProvider locale="en" messages={en}>
      {children}
    </NextIntlClientProvider>,
  );
}

const tenant: Tenant = {
  id: "tenant-1",
  slug: "factory",
  display_name: "Factory Floor",
  created_at: "2026-06-30T00:00:00Z",
};

const baseProps: DashboardViewContentProps = {
  view: "agents",
  auth: {
    source: "none",
    cookieName: "pandar_session",
    provider: "none",
    signInUrl: null,
    signOutUrl: null,
  },
  selectedTenant: tenant,
  health: {
    printersTotal: 0,
    printersOnline: 0,
    agentsTotal: 0,
    agentsConnected: 0,
    jobsActive: 0,
    jobsFailed: 0,
  },
  attentionItems: [],
  topSeverity: null,
  liveState: "idle",
  lastEventAt: null,
  fleetEmpty: true,
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

function renderAgentsView(overrides: Partial<DashboardViewContentProps> = {}) {
  return renderWithMessages(
    <DashboardViewContent {...{ ...baseProps, ...overrides }} />,
  );
}

describe("Agents view pairing guidance", () => {
  it("renders guidance above linked agents and shows the pairing form for an available tenant admin context", () => {
    renderAgentsView();

    const pairingHeading = screen.getByRole("heading", {
      name: "Pair a local agent",
    });
    const linkedAgentsHeading = screen.getByRole("heading", {
      name: "Linked agents",
    });

    expect(pairingHeading).toBeVisible();
    expect(
      pairingHeading.compareDocumentPosition(linkedAgentsHeading) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      screen.getByText("Create a pairing for Factory Floor."),
    ).toBeVisible();
    expect(
      screen.getByText(/Copy the generated environment block/),
    ).toBeVisible();
    expect(
      screen.getByText(/Start or restart pandar-agent/),
    ).toBeVisible();
    expect(screen.getByLabelText("Agent name")).toHaveAttribute(
      "name",
      "name",
    );
    expect(
      screen.getByRole("button", { name: "Create pairing" }),
    ).toHaveAttribute("type", "submit");
  });

  it("does not render the pairing form when no tenant is selected", () => {
    renderAgentsView({ selectedTenant: null });

    expect(
      screen.getByRole("heading", { name: "Pair a local agent" }),
    ).toBeVisible();
    expect(
      screen.getByText("Select a tenant before creating an agent pairing."),
    ).toBeVisible();
    expect(
      screen.getByText("Choose a tenant from the header to enable pairing creation."),
    ).toBeVisible();
    expect(screen.queryByLabelText("Agent name")).not.toBeInTheDocument();
  });

  it("does not render the pairing form when admin resources are restricted", () => {
    renderAgentsView({ adminUnavailable: true });

    expect(
      screen.getByText(
        "Only a tenant administrator or agent-registration-capable principal can create pairings.",
      ),
    ).toBeVisible();
    expect(
      screen.getByText("Use a tenant admin account or scoped registration token before creating this pairing."),
    ).toBeVisible();
    expect(screen.queryByLabelText("Agent name")).not.toBeInTheDocument();
  });

  it("renders delete controls only for agents that are not online", () => {
    renderAgentsView({
      agents: [
        {
          id: "agent-offline",
          tenant_id: tenant.id,
          name: "Offline agent",
          status: "offline",
          created_at: "2026-06-30T00:00:00Z",
        },
        {
          id: "agent-online",
          tenant_id: tenant.id,
          name: "Online agent",
          status: "online",
          created_at: "2026-06-30T00:00:00Z",
        },
      ],
    });

    expect(screen.getByRole("button", { name: "Delete Offline agent" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Online agent is online" })).toBeDisabled();
  });
});
