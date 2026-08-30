import { NextIntlClientProvider } from "next-intl";
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import { QueryClientTestProvider } from "./query-client.test-utils";
import { AgentsView, type AgentsViewProps } from "./dashboard-view-content";
import type { Command, Tenant } from "./dashboard-types";

vi.mock("./actions", () => ({
  controlPrinter: vi.fn(),
  deleteAgent: vi.fn(),
  diagnosePrinter: vi.fn(),
  discoverPrinters: vi.fn(),
  linkPrinter: vi.fn(),
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

const agent = {
  id: "agent-1",
  tenant_id: tenant.id,
  name: "Lab agent",
  status: "online",
  created_at: "2026-06-30T00:00:00Z",
};

function command(id: string, kind: string, status = "succeeded"): Command {
  return {
    id,
    tenant_id: tenant.id,
    agent_id: agent.id,
    printer_id: null,
    kind,
    status,
    payload_json: "{}",
    error: null,
    result_json: null,
    created_at: "2026-07-02T00:00:00Z",
    updated_at: "2026-07-02T00:00:05Z",
  };
}

const baseProps: AgentsViewProps = {
  selectedTenant: tenant,
  printers: [],
  agents: [agent],
  selectedCommand: null,
  commandData: null,
  adminUnavailable: false,
};

describe("Agents view composition", () => {
  it("renders agents before diagnostics without the selected command", async () => {
    renderWithMessages(<AgentsView {...baseProps} />);

    const agentsHeading = screen.getByRole("heading", { name: "Agents" });
    const diagnosticsHeading = await screen.findByRole("heading", {
      name: "Diagnostics",
    });

    expect(
      agentsHeading.compareDocumentPosition(diagnosticsHeading) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      screen.queryByRole("heading", { name: "Discovered printers" }),
    ).not.toBeInTheDocument();
  });

  it("routes a selected discovery command to the discovery section", async () => {
    renderWithMessages(
      <AgentsView
        {...baseProps}
        selectedCommand={command("cmd-1", "discover_printers", "sent")}
        discoveryCommand={command("cmd-1", "discover_printers", "sent")}
        discoveryData={null}
      />,
    );

    expect(
      screen.getByRole("heading", { name: "Discovered printers" }),
    ).toBeVisible();
    expect(screen.getByText("Discovering printers…")).toBeVisible();

    const diagnosticsHeading = await screen.findByRole("heading", {
      name: "Diagnostics",
    });
    const diagnosticsSection = diagnosticsHeading.closest("section")!;
    expect(diagnosticsSection).toHaveTextContent("No command selected");
    expect(diagnosticsSection).not.toHaveTextContent("Discovering printers");
  });

  it("keeps discovery results visible alongside a link command result", async () => {
    const linkCommand = command("cmd-2", "link_printer");
    const discovery = command("cmd-1", "discover_printers");
    renderWithMessages(
      <AgentsView
        {...baseProps}
        selectedCommand={linkCommand}
        commandData={{
          type: "printer_link",
          serial_number: "SN-NEW",
          host: "192.0.2.10",
          name: "Garage P1S",
          status: "linked",
        }}
        discoveryCommand={discovery}
        discoveryData={{
          type: "printer_discovery",
          printers: [{ serial_number: "SN-NEW", host: "192.0.2.10" }],
        }}
      />,
    );

    const discoveryHeading = screen.getByRole("heading", {
      name: "Discovered printers",
    });
    const diagnosticsHeading = await screen.findByRole("heading", {
      name: "Diagnostics",
    });

    expect(
      discoveryHeading.compareDocumentPosition(diagnosticsHeading) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Adopt 192.0.2.10" }),
    ).toBeEnabled();
    const diagnosticsSection = diagnosticsHeading.closest("section")!;
    expect(diagnosticsSection).toHaveTextContent("SN-NEW");
  });
});
