import type { ReactNode } from "react";
import { NextIntlClientProvider } from "next-intl";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { toast } from "sonner";

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
  linkPrinter: vi.fn(),
  refreshPrinters: vi.fn(),
  retryDispatchJob: vi.fn(),
  retryDispatchJobs: vi.fn(),
  revokeJoinLink: vi.fn(),
  revokeTenantToken: vi.fn(),
  rotateTenantToken: vi.fn(),
  updateTenantUserRole: vi.fn(),
}));

vi.mock("sonner", () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn(),
    warning: vi.fn(),
  },
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
    expect(pairingHeading.closest("section")?.parentElement).toHaveClass(
      "grid",
      "gap-4",
    );
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
      screen.getByText("Choose a tenant from the sidebar to enable pairing creation."),
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

  it("renders Refresh in each linked Agent row and returns to Agents", () => {
    renderAgentsView({
      agents: [
        {
          id: "agent-online",
          tenant_id: tenant.id,
          name: "Online agent",
          status: "online",
          created_at: "2026-06-30T00:00:00Z",
        },
      ],
    });

    const refresh = screen.getByRole("button", {
      name: "Refresh Online agent",
    });
    expect(refresh).toHaveTextContent(/^Refresh$/);
    expect(refresh).toBeEnabled();
    expect(refresh.closest("form")).toHaveFormValues({
      tenant_id: tenant.id,
      agent_id: "agent-online",
      return_to: "agents",
    });
  });

  it("keeps delete controls visible for online agents and explains why deletion is disabled", async () => {
    const user = userEvent.setup();
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

    const offlineDelete = screen.getByRole("button", {
      name: "Delete Offline agent",
    });
    const onlineDelete = screen.getByRole("button", {
      name: "Delete Online agent",
    });
    expect(offlineDelete).toHaveTextContent(/^Delete$/);
    expect(onlineDelete).toHaveTextContent(/Delete/);
    expect(offlineDelete).toBeEnabled();
    expect(onlineDelete).toHaveAttribute("aria-disabled", "true");
    expect(onlineDelete).toHaveAccessibleDescription(
      "Online agent is online, cannot be deleted",
    );

    await user.click(offlineDelete);
    expect(screen.getByRole("dialog", { name: "Delete agent" })).toBeVisible();
    expect(
      screen.getByText(
        "Delete Offline agent? Its reported printers, commands, jobs, and machine events will be removed. Tenant users, tokens, and settings are kept.",
      ),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Cancel" }));

    await user.hover(onlineDelete);
    expect(
      await screen.findByText("Online agent is online, cannot be deleted", {
        selector: '[data-slot="hover-card-content"]',
      }),
    ).toBeVisible();

    await user.click(onlineDelete);
    expect(toast.warning).toHaveBeenCalledWith(
      "Online agent is online, cannot be deleted",
    );
  });

  it("renders link-printer form between pairing guidance and linked agents", () => {
    renderAgentsView({
      agents: [
        { id: "agent-online", tenant_id: tenant.id, name: "Online agent", status: "online", created_at: "2026-06-30T00:00:00Z" },
        { id: "agent-offline", tenant_id: tenant.id, name: "Offline agent", status: "offline", created_at: "2026-06-30T00:00:00Z" },
      ],
    });

    const pairingHeading = screen.getByRole("heading", { name: "Pair a local agent" });
    const linkHeading = screen.getByRole("heading", { name: "Link printer to agent" });
    const linkedAgentsHeading = screen.getByRole("heading", { name: "Linked agents" });

    expect(pairingHeading.compareDocumentPosition(linkHeading) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(linkHeading.compareDocumentPosition(linkedAgentsHeading) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.getByLabelText("Agent")).toHaveValue("agent-online");
    expect(screen.getByLabelText("Type")).toHaveValue("BambuLab");
    expect(screen.getByLabelText("Printer IPv4 address")).toHaveAttribute("name", "host");
    expect(screen.getByLabelText("Access code")).toHaveAttribute("name", "access_code");
    expect(screen.getByLabelText("Name")).toHaveAttribute("name", "name");
    expect(screen.queryByLabelText("Serial number")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Model")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Link printer" })).toHaveAttribute("type", "submit");
  });

  it("shows link-printer empty state when no tenant is selected", () => {
    renderAgentsView({ selectedTenant: null });

    expect(screen.getByRole("heading", { name: "Link printer to agent" })).toBeVisible();
    expect(screen.getByText("Select a tenant to link a printer."));
    expect(screen.getByText("Choose a tenant from the sidebar before submitting printer connection details."));
    expect(screen.queryByLabelText("Access code")).not.toBeInTheDocument();
  });

  it("shows link-printer empty state when no agents are linked", () => {
    renderAgentsView();

    expect(screen.getByRole("heading", { name: "Link printer to agent" })).toBeVisible();
    expect(screen.getByText("No agents available for printer linking."));
    expect(screen.getByText("Pair an agent before linking a printer."));
    expect(screen.queryByLabelText("Access code")).not.toBeInTheDocument();
  });
});
