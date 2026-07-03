import { NextIntlClientProvider } from "next-intl";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import zh from "../messages/zh.json";
import { PrinterInventory } from "./dashboard-inventory";
import type { Agent, Printer, Tenant } from "./dashboard-types";

vi.mock("./actions", () => ({
  controlPrinter: vi.fn(),
  deletePrinter: vi.fn(),
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

const printerWithMaterials: Printer = {
  ...printer,
  materials: {
    ams_units: [
      {
        unit_id: "0",
        humidity: 1,
        temperature_celsius: 24,
        toolhead: "R",
        trays: [
          {
            tray_id: "0",
            type: "PLA",
            color: "00C853",
            remaining_estimate: "72",
            k_value: "0.020",
            exists: true,
          },
          {
            tray_id: "1",
            type: "PETG",
            color: "FFA726",
            remaining_estimate: "42",
            exists: true,
          },
        ],
      },
    ],
    external_spools: [
      {
        external_id: "254",
        tray_id: "0",
        type: "TPU",
        color: "8D6E63",
        remaining_estimate: "36",
        toolhead: "L",
        exists: true,
      },
    ],
    active_tray: {
      kind: "ams",
      ams_id: "0",
      tray_id: "1",
      global_tray_id: 1,
    },
    observed_at: "2026-07-02T00:00:00Z",
  },
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
    expect(card).not.toHaveTextContent("Managed by");
    expect(screen.queryByText("Managed by")).not.toBeInTheDocument();
  });

  it("places the managing agent chip beside the summary status badge", () => {
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printer]} agents={[agent]} />,
    );

    const chip = screen.getByText("Shop Agent").parentElement;
    expect(chip?.parentElement).toHaveTextContent("Idle");
  });

  it("opens a printer actions menu with delete", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printer]} agents={[agent]} />,
    );

    await user.click(screen.getByRole("button", { name: "Details" }));

    expect(screen.getByRole("menu")).toBeVisible();
    expect(screen.getByRole("menuitem", { name: "Delete printer" })).toBeVisible();
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

  it("replaces the filament summary with AMS and external slot loading details", () => {
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printerWithMaterials]} agents={[agent]} />,
    );

    const card = screen.getByRole("article", { name: "Office A1" });
    expect(card).toHaveTextContent("AMS-A");
    expect(card).toHaveTextContent("1%");
    expect(card).toHaveTextContent("24.0°C");
    expect(card).toHaveTextContent("R");
    expect(card).toHaveTextContent("PLA");
    expect(card).toHaveTextContent("PETG");
    expect(card).toHaveTextContent("External");
    expect(card).toHaveTextContent("TPU");
    expect(card).not.toHaveTextContent("8 AMS trays");
  });

  it("formats active AMS slots as a unit letter and one-based tray position", () => {
    const activePrinter: Printer = {
      ...printerWithMaterials,
      materials: {
        ...printerWithMaterials.materials!,
        active_tray: {
          kind: "ams",
          ams_id: "0",
          tray_id: "2",
          global_tray_id: 2,
        },
      },
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[activePrinter]} agents={[agent]} />,
    );

    const card = screen.getByRole("article", { name: "Office A1" });
    expect(card).toHaveTextContent("AMS A - 3");
    expect(card).not.toHaveTextContent("AMS 0:2");
  });

  it("opens an AMS slot menu on hover with RFID, load, and unload operations", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printerWithMaterials]} agents={[agent]} />,
    );

    await user.hover(screen.getByRole("button", { name: "AMS-A slot 2 PETG" }));

    expect(screen.getByRole("menu")).toBeVisible();
    expect(screen.getByText("Orange")).toBeVisible();
    expect(screen.getByText("42%")).toBeVisible();
    expect(screen.getByRole("menuitem", { name: "Re-read RFID" })).toBeVisible();
    expect(screen.getByRole("menuitem", { name: "Load" })).toBeVisible();
    expect(screen.getByRole("menuitem", { name: "Unload" })).toBeVisible();

    const menu = screen.getByRole("menu");
    expect(menu).toHaveClass("top-full");
    expect(menu).toHaveClass("pt-1");
    expect(menu).not.toHaveClass("mt-1");

    const rereadForm = screen.getByRole("menuitem", { name: "Re-read RFID" }).closest("form");
    const loadForm = screen.getByRole("menuitem", { name: "Load" }).closest("form");
    expect(rereadForm?.querySelector('input[name="global_tray_id"]')).toBeNull();
    expect(loadForm?.querySelector('input[name="global_tray_id"]')).toHaveValue("1");
    expect(loadForm?.querySelector('input[name="extruder_id"]')).toHaveValue("0");
  });

  it("renders unsupported AMS remaining estimates as unsupported with a gray progress bar", async () => {
    const user = userEvent.setup();
    const unsupportedPrinter: Printer = {
      ...printerWithMaterials,
      materials: {
        ...printerWithMaterials.materials!,
        ams_units: [
          {
            ...printerWithMaterials.materials!.ams_units[0],
            trays: [
              {
                ...printerWithMaterials.materials!.ams_units[0].trays![0],
                remaining_estimate: "-1",
              },
              printerWithMaterials.materials!.ams_units[0].trays![1],
            ],
          },
        ],
      },
    };
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[unsupportedPrinter]} agents={[agent]} />,
    );

    await user.hover(screen.getByRole("button", { name: "AMS-A slot 1 PLA" }));

    expect(screen.getByText("Unsupported")).toBeVisible();
    expect(screen.queryByText("-1%")).not.toBeInTheDocument();
    for (const progress of screen.getAllByLabelText("Unsupported remaining progress")) {
      expect(progress).toHaveClass("bg-slate-400");
      expect(progress).toHaveClass("dark:bg-slate-600");
      expect(progress.querySelector(".bg-emerald-500")).toBeNull();
    }
  });

  it("uses the correct Chinese copy for external spool", () => {
    expect(zh.material.externalSpool).toContain("料盘");
    expect(zh.material.externalSpool).not.toContain("盘子");
  });
});
