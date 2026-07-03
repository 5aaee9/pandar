import { NextIntlClientProvider } from "next-intl";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import zh from "../messages/zh.json";
import { PrinterInventory } from "./dashboard-inventory";
import type { Agent, Printer, Tenant } from "./dashboard-types";

vi.mock("./actions", () => ({
  linkPrinter: vi.fn(),
  refreshPrinterMaterials: vi.fn(),
}));

function renderWithMessages(children: React.ReactNode) {
  return render(
    <NextIntlClientProvider locale="en" messages={en}>
      {children}
    </NextIntlClientProvider>,
  );
}

const tenant: Tenant = {
  id: "tenant-1",
  slug: "acme",
  display_name: "Acme Labs",
  created_at: "2026-07-02T00:00:00Z",
};

const agent: Agent = {
  id: "agent-1",
  tenant_id: tenant.id,
  name: "Shop Agent",
  status: "online",
  created_at: "2026-07-02T00:00:00Z",
};

const printer: Printer = {
  id: "printer-1",
  tenant_id: tenant.id,
  agent_id: agent.id,
  serial_number: "SERIAL123",
  name: "Office A1",
  model: "A1",
  status: "idle",
  last_seen_at: "2026-07-02T00:00:00Z",
  created_at: "2026-07-02T00:00:00Z",
  materials: null,
};

describe("PrinterInventory", () => {
  it("renders inventory content without the tenant subtitle or reported count", () => {
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printer]} agents={[agent]} />,
    );

    expect(screen.getByRole("heading", { name: "Printer inventory" })).toBeVisible();
    expect(screen.queryByText("Acme Labs (acme)")).not.toBeInTheDocument();
    expect(screen.queryByText("1 reported")).not.toBeInTheDocument();
  });

  it("renders printers as individual machine cards", () => {
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printer]} agents={[agent]} />,
    );

    const card = screen.getByRole("article", { name: "Office A1" });
    expect(card).toBeVisible();
    expect(card).toHaveTextContent("A1");
    expect(card).toHaveTextContent("SERIAL123");
    expect(card).toHaveTextContent("Shop Agent");
  });

  it("opens the machine form from the empty printer state", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[]} agents={[agent]} />,
    );

    expect(screen.getByText("No printers reported")).toBeVisible();

    const trigger = screen.getByRole("button", { name: "Link printer" });
    expect(trigger).toHaveAttribute("data-slot", "dialog-trigger");

    await user.click(trigger);

    expect(screen.getByRole("dialog")).toBeVisible();
    expect(screen.getByRole("heading", { name: "Link printer to agent" })).toBeVisible();
    expect(screen.getByLabelText("Printer IPv4 address")).toBeVisible();
  });

  it("renders a localized AMS refresh form with tenant and printer ids", () => {
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printer]} agents={[agent]} />,
    );

    const button = screen.getByRole("button", { name: "Refresh AMS" });
    const form = button.closest("form");

    expect(form).not.toBeNull();
    expect(form?.querySelector('input[name="tenant_id"]')).toHaveValue("tenant-1");
    expect(form?.querySelector('input[name="printer_id"]')).toHaveValue("printer-1");
  });

  it("uses the correct Chinese copy for external spool", () => {
    expect(zh.material.externalSpool).toContain("料盘");
    expect(zh.material.externalSpool).not.toContain("盘子");
  });
});
