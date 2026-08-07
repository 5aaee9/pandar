import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import { DiscoverySection } from "./discovery-section";
import type {
  Agent,
  Command,
  DiscoveryResultData,
  Printer,
  Tenant,
} from "./dashboard-types";

vi.mock("./actions", () => ({
  linkPrinter: vi.fn(),
}));

function renderWithMessages(children: React.ReactNode) {
  return render(
    <NextIntlClientProvider locale="en" messages={en}>
      <QueryClientProvider client={new QueryClient()}>
        {children}
      </QueryClientProvider>
    </NextIntlClientProvider>,
  );
}

const tenant: Tenant = {
  id: "tenant-1",
  slug: "factory",
  display_name: "Factory Floor",
  created_at: "2026-06-30T00:00:00Z",
};

const agent: Agent = {
  id: "agent-1",
  tenant_id: tenant.id,
  name: "Lab agent",
  status: "online",
  created_at: "2026-06-30T00:00:00Z",
};

function discoveryCommand(status: string): Command {
  return {
    id: "cmd-discovery",
    tenant_id: tenant.id,
    agent_id: agent.id,
    printer_id: null,
    kind: "discover_printers",
    status,
    payload_json: "{}",
    error: status === "failed" ? "agent unreachable" : null,
    result_json: null,
    created_at: "2026-07-02T00:00:00Z",
    updated_at: "2026-07-02T00:00:05Z",
  };
}

const discoveryData: DiscoveryResultData = {
  type: "printer_discovery",
  printers: [
    {
      serial_number: "SN-NEW",
      host: "192.0.2.10",
      name: "Garage P1S",
      model: "P1S",
      source: "ssdp",
    },
    {
      serial_number: "SN-LINKED",
      host: "192.0.2.11",
      name: "Office A1",
      model: "A1",
      source: "ssdp",
    },
  ],
};

const linkedPrinter: Printer = {
  id: "printer-1",
  tenant_id: tenant.id,
  agent_id: agent.id,
  serial_number: "SN-LINKED",
  name: "Office A1",
  model: "A1",
  status: "idle",
  last_seen_at: "2026-07-02T00:00:00Z",
  created_at: "2026-07-02T00:00:00Z",
  materials: null,
};

function renderSection({
  command = discoveryCommand("succeeded"),
  data = discoveryData,
  printers = [linkedPrinter],
}: {
  command?: Command;
  data?: DiscoveryResultData | null;
  printers?: Printer[];
} = {}) {
  return renderWithMessages(
    <DiscoverySection
      agents={[agent]}
      command={command}
      data={data}
      printers={printers}
      selectedTenant={tenant}
    />,
  );
}

describe("DiscoverySection", () => {
  it("shows a pending state while the discovery command is running", () => {
    renderSection({ command: discoveryCommand("sent"), data: null });

    expect(
      screen.getByRole("heading", { name: "Discovered printers" }),
    ).toBeVisible();
    expect(screen.getByText("Reported by Lab agent · sent")).toBeVisible();
    expect(screen.getByText("Discovering printers…")).toBeVisible();
    expect(
      screen.getByText(/Waiting for Lab agent to scan its network/),
    ).toBeVisible();
  });

  it("shows the failure when the discovery command failed", () => {
    renderSection({ command: discoveryCommand("failed"), data: null });

    expect(screen.getByText("Discovery failed")).toBeVisible();
    expect(screen.getByText("agent unreachable")).toBeVisible();
  });

  it("shows an empty state when nothing was discovered", () => {
    renderSection({ data: { type: "printer_discovery", printers: [] } });

    expect(screen.getByText("No printers discovered")).toBeVisible();
  });

  it("marks already-linked machines and offers adopt for the rest", () => {
    renderSection();

    const linkedRow = screen.getByText("SN-LINKED").closest("tr")!;
    expect(within(linkedRow).getByText("Linked")).toBeVisible();
    expect(
      within(linkedRow).queryByRole("button", { name: /Adopt/ }),
    ).not.toBeInTheDocument();

    const newRow = screen.getByText("SN-NEW").closest("tr")!;
    expect(
      within(newRow).getByRole("button", { name: "Adopt Garage P1S" }),
    ).toBeEnabled();
  });

  it("prefills the adopt dialog from the discovered machine", async () => {
    const user = userEvent.setup();
    renderSection();

    await user.click(screen.getByRole("button", { name: "Adopt Garage P1S" }));

    const dialog = screen.getByRole("dialog", {
      name: "Adopt discovered printer",
    });
    expect(
      within(dialog).getByText(
        "Link 192.0.2.10 to Lab agent. Enter the printer's access code to finish.",
      ),
    ).toBeVisible();

    const form = within(dialog)
      .getByRole("button", { name: "Adopt printer" })
      .closest("form")!;
    expect(form).toHaveFormValues({
      tenant_id: tenant.id,
      agent_id: agent.id,
      type: "BambuLab",
      host: "192.0.2.10",
      name: "Garage P1S",
      access_code: "",
    });
    expect(within(dialog).getByLabelText("Access code")).toHaveAttribute(
      "type",
      "password",
    );
    expect(
      within(dialog).getByRole("button", { name: "Show access code" }),
    ).toBeVisible();
  });
});
