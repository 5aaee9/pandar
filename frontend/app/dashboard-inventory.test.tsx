import { NextIntlClientProvider } from "next-intl";
import { render, screen, within } from "@testing-library/react";
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

  it("renders AMS refresh inside the printer actions menu with tenant and printer ids", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printer]} agents={[agent]} />,
    );

    expect(screen.queryByRole("button", { name: "Refresh AMS" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Details" }));

    const button = screen.getByRole("menuitem", { name: "Refresh AMS" });
    const form = button.closest("form");

    expect(form).not.toBeNull();
    expect(form?.querySelector('input[name="tenant_id"]')).toHaveValue("tenant-1");
    expect(form?.querySelector('input[name="printer_id"]')).toHaveValue("printer-1");
  });

  it("renders printer temperatures and controls in separate sections", () => {
    const heatingPrinter: Printer = {
      ...printer,
      nozzle_temperatures: [
        { label: "L", current_celsius: "41", target_celsius: "220" },
        { label: "R", current_celsius: "42", target_celsius: "230" },
      ],
      active_nozzle: "R",
      bed_temperature_celsius: "60",
      chamber_temperature_celsius: "32",
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[heatingPrinter]} agents={[agent]} />,
    );

    const card = screen.getByRole("article", { name: "Office A1" });
    expect(card).toHaveTextContent("Nozzle");
    expect(card).toHaveTextContent("L / R");
    expect(card).toHaveTextContent("41° / 42°");
    expect(card).toHaveTextContent("Bed");
    expect(card).toHaveTextContent("60°C");
    expect(card).toHaveTextContent("Chamber");
    expect(card).toHaveTextContent("32°C");
    expect(card).toHaveTextContent("Controls");

    const cardText = card.textContent ?? "";
    expect(cardText.indexOf("Controls")).toBeGreaterThan(cardText.indexOf("Status"));
    expect(cardText.indexOf("Controls")).toBeLessThan(cardText.indexOf("Filaments"));

    const controls = screen.getByRole("group", { name: "Controls" });
    expect(controls).toHaveClass("grid-cols-2");
    expect(controls).not.toHaveClass("sm:grid-cols-1");

    const stopForm = screen.getByRole("button", { name: "Stop" }).closest("form");
    const pauseForm = screen.getByRole("button", { name: "Pause" }).closest("form");
    expect(stopForm?.querySelector('input[name="action"]')).toHaveValue("stop");
    expect(stopForm?.querySelector('input[name="printer_id"]')).toHaveValue("printer-1");
    expect(pauseForm?.querySelector('input[name="action"]')).toHaveValue("pause");
  });

  it("shows active nozzle switch control for dual-nozzle printers", () => {
    const dualNozzlePrinter: Printer = {
      ...printerWithMaterials,
      nozzle_temperatures: [
        { label: "L", current_celsius: "41", target_celsius: "220" },
        { label: "R", current_celsius: "42", target_celsius: "230" },
      ],
      active_nozzle: "R",
      bed_temperature_celsius: "60",
      chamber_temperature_celsius: "32",
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[dualNozzlePrinter]} agents={[agent]} />,
    );

    const switchButton = screen.getByRole("button", { name: "Switch nozzle L R Nozzle" });
    const switchForm = switchButton.closest("form");
    expect(switchForm).toHaveClass("sm:col-start-4");
    expect(switchForm?.querySelector('input[name="action"]')).toHaveValue("select_extruder");
    expect(switchForm?.querySelector('input[name="printer_id"]')).toHaveValue("printer-1");
    expect(switchForm?.querySelector('input[name="extruder_id"]')).toHaveValue("1");
    expect(switchButton).toHaveTextContent("L");
    expect(switchButton).toHaveTextContent("R");
    expect(within(switchButton).getByText("R")).toHaveClass("text-primary");
  });

  it("renders a single nozzle without a duplicate label or target temperature", () => {
    const heatingPrinter: Printer = {
      ...printer,
      nozzle_temperatures: [{ label: null, current_celsius: "27", target_celsius: "0" }],
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[heatingPrinter]} agents={[agent]} />,
    );

    const card = screen.getByRole("article", { name: "Office A1" });
    expect(card).toHaveTextContent("Nozzle");
    expect(card).toHaveTextContent("27°");
    expect(card).not.toHaveTextContent("Nozzle Nozzle");
    expect(card).not.toHaveTextContent("27° / 0°");
  });

  it("hides zero bed target temperature", () => {
    const heatingPrinter: Printer = {
      ...printer,
      bed_temperature_celsius: "26",
      bed_target_temperature_celsius: "0",
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[heatingPrinter]} agents={[agent]} />,
    );

    const card = screen.getByRole("article", { name: "Office A1" });
    expect(card).toHaveTextContent("Bed");
    expect(card).toHaveTextContent("26°C");
    expect(card).not.toHaveTextContent("26° / 0°");
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
