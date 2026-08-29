import { NextIntlClientProvider } from "next-intl";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import { AgentsSection } from "./agents-section";
import { QueryClientTestProvider } from "./query-client.test-utils";
import type { Agent, Printer, Tenant } from "./dashboard-types";

vi.mock("./actions", () => ({
  deleteAgent: vi.fn(),
  discoverPrinters: vi.fn(),
  refreshPrinters: vi.fn(),
}));

vi.mock("./admin-actions", () => ({
  createAgentPairing: vi.fn(),
}));

vi.mock("sonner", () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn(),
    warning: vi.fn(),
  },
}));

function renderWithMessages(children: React.ReactNode) {
  return render(
    <QueryClientTestProvider>
      <NextIntlClientProvider locale="en" messages={en}>
        {children}
      </NextIntlClientProvider>
    </QueryClientTestProvider>,
  );
}

const tenant: Tenant = {
  id: "tenant-1",
  slug: "factory",
  display_name: "Factory Floor",
  created_at: "2026-06-30T00:00:00Z",
};

const onlineAgent: Agent = {
  id: "agent-online",
  tenant_id: tenant.id,
  name: "Online agent",
  status: "online",
  created_at: "2026-06-30T00:00:00Z",
};

const offlineAgent: Agent = {
  id: "agent-offline",
  tenant_id: tenant.id,
  name: "Offline agent",
  status: "offline",
  created_at: "2026-06-29T00:00:00Z",
};

const printer: Printer = {
  id: "printer-1",
  tenant_id: tenant.id,
  agent_id: onlineAgent.id,
  serial_number: "SN1",
  name: "Office A1",
  model: "X1C",
  status: "idle",
  last_seen_at: "2026-07-02T00:00:00Z",
  created_at: "2026-07-02T00:00:00Z",
  materials: null,
};

describe("AgentsSection", () => {
  it("renders agent rows with printer counts and actions", () => {
    renderWithMessages(
      <AgentsSection
        adminUnavailable={false}
        agents={[onlineAgent, offlineAgent]}
        printers={[printer]}
        selectedTenant={tenant}
      />,
    );

    expect(screen.getByRole("heading", { name: "Agents" })).toBeVisible();
    expect(screen.getByText("2 agents")).toBeVisible();

    const onlineRow = screen
      .getByText("Online agent")
      .closest("article")!;
    expect(onlineRow).toHaveAttribute("data-agent-id", "agent-online");
    expect(within(onlineRow).getByText("1 printer")).toBeVisible();
    expect(
      within(onlineRow).getByRole("button", { name: "Discover printers with Online agent" }),
    ).toBeEnabled();
    expect(
      within(onlineRow).getByRole("button", { name: "Refresh Online agent" }),
    ).toBeEnabled();
    expect(
      within(onlineRow).getByRole("link", { name: "Settings for Online agent" }),
    ).toHaveAttribute("href", "/agents/agent-online/settings");
    expect(
      within(onlineRow).getByRole("button", { name: "Delete Online agent" }),
    ).toHaveAttribute("aria-disabled", "true");

    const offlineRow = screen
      .getByText("Offline agent")
      .closest("article")!;
    expect(within(offlineRow).getByText("0 printers")).toBeVisible();
    expect(
      within(offlineRow).getByRole("button", { name: "Discover printers with Offline agent" }),
    ).toBeDisabled();

    const refresh = within(onlineRow).getByRole("button", {
      name: "Refresh Online agent",
    });
    expect(refresh.closest("form")).toHaveFormValues({
      tenant_id: tenant.id,
      agent_id: "agent-online",
      return_to: "agents",
    });
  });

  it("opens the discover dialog for an online agent", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <AgentsSection
        adminUnavailable={false}
        agents={[onlineAgent]}
        printers={[]}
        selectedTenant={tenant}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: "Discover printers with Online agent" }),
    );

    const dialog = screen.getByRole("dialog", { name: "Discover printers" });
    expect(dialog).toBeVisible();
    expect(
      within(dialog).getByText(
        "Search the network around Online agent for connectable Bambu Lab printers.",
      ),
    ).toBeVisible();
    expect(within(dialog).getByRole("spinbutton", { name: "Timeout (seconds)" })).toHaveValue(5);

    const submit = within(dialog).getByRole("button", { name: "Start discovery" });
    expect(submit).toHaveAttribute("type", "submit");
    expect(submit.closest("form")).toHaveFormValues({
      tenant_id: tenant.id,
      agent_id: "agent-online",
    });
  });

  it("opens the pairing dialog with guidance and the pairing form", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <AgentsSection
        adminUnavailable={false}
        agents={[onlineAgent]}
        printers={[]}
        selectedTenant={tenant}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Pair agent" }));

    const dialog = screen.getByRole("dialog", { name: "Pair a local agent" });
    expect(dialog).toBeVisible();
    expect(
      within(dialog).getByText("Create a pairing for Factory Floor."),
    ).toBeVisible();
    expect(
      within(dialog).getByText(/Copy the generated environment block/),
    ).toBeVisible();
    expect(within(dialog).getByLabelText("Agent name")).toHaveAttribute(
      "name",
      "name",
    );
    expect(
      within(dialog).getByRole("button", { name: "Create pairing" }),
    ).toHaveAttribute("type", "submit");
  });

  it("shows the empty state with a pairing action when no agents are linked", () => {
    renderWithMessages(
      <AgentsSection
        adminUnavailable={false}
        agents={[]}
        printers={[]}
        selectedTenant={tenant}
      />,
    );

    expect(screen.getByText("No agents linked")).toBeVisible();
    expect(
      screen.getByText(
        "Pair a local agent to connect the printers on its network.",
      ),
    ).toBeVisible();
    expect(
      screen.getAllByRole("button", { name: "Pair agent" }).length,
    ).toBeGreaterThan(0);
  });

  it("explains the admin requirement when pairing is restricted", () => {
    renderWithMessages(
      <AgentsSection
        adminUnavailable={true}
        agents={[]}
        printers={[]}
        selectedTenant={tenant}
      />,
    );

    expect(
      screen.getByText("Pairing a new agent requires tenant admin access."),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "Pair agent" }),
    ).not.toBeInTheDocument();
  });

  it("shows the no-tenant empty state", () => {
    renderWithMessages(
      <AgentsSection
        adminUnavailable={false}
        agents={[]}
        printers={[]}
        selectedTenant={null}
      />,
    );

    expect(screen.getByText("No tenant selected")).toBeVisible();
    expect(
      screen.getByText("Choose a tenant from the sidebar to manage agents."),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "Pair agent" }),
    ).not.toBeInTheDocument();
  });
});
