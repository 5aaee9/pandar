import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import zh from "../messages/zh.json";
import { PrinterInventory } from "./dashboard-inventory";
import type { Printer } from "./dashboard-types";
import {
  agent,
  printer,
  printerWithMaterials,
  renderWithMessages,
  tenant,
} from "./dashboard-inventory.test.context";

describe("PrinterInventory", () => {
  it("hides zero bed target temperature from the card and menu", async () => {
    const user = userEvent.setup();
    const heatingPrinter: Printer = {
      ...printer,
      bed_temperature_celsius: "26",
      bed_target_temperature_celsius: "0",
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[heatingPrinter]} agents={[agent]} nowMs={0} />,
    );

    const card = screen.getByRole("article", { name: "Office A1" });
    await waitFor(() => expect(card).toHaveTextContent("Bed"));
    expect(card).toHaveTextContent("26°C");
    expect(card).not.toHaveTextContent("26° / 0°");

    await user.click(await screen.findByRole("button", { name: "Set bed temperature" }));

    expect(screen.getByText("Current 26°C")).toBeVisible();
    expect(screen.queryByText(/^Target /)).not.toBeInTheDocument();
  });

  it("replaces the filament summary with AMS and external slot loading details", () => {
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printerWithMaterials]} agents={[agent]} nowMs={0} />,
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
      <PrinterInventory selectedTenant={tenant} printers={[activePrinter]} agents={[agent]} nowMs={0} />,
    );

    const card = screen.getByRole("article", { name: "Office A1" });
    expect(card).toHaveTextContent("AMS A - 3");
    expect(card).not.toHaveTextContent("AMS 0:2");
  });

  it("opens an AMS slot popover on click with RFID, load, and unload operations", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printerWithMaterials]} agents={[agent]} nowMs={0} />,
    );

    await user.click(
      screen.getByRole("button", { name: "AMS-A slot 2, PETG, Active, Remaining: 42%" }),
    );

    expect(screen.getByText("#FFA726")).toBeVisible();
    expect(screen.getByText("42%")).toBeVisible();
    expect(screen.getByRole("button", { name: "Re-read RFID" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Load" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Unload" })).toBeVisible();

    const rereadForm = screen.getByRole("button", { name: "Re-read RFID" }).closest("form");
    const loadForm = screen.getByRole("button", { name: "Load" }).closest("form");
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
      <PrinterInventory selectedTenant={tenant} printers={[unsupportedPrinter]} agents={[agent]} nowMs={0} />,
    );

    await user.click(
      screen.getByRole("button", { name: "AMS-A slot 1, PLA, Remaining: Unsupported" }),
    );

    expect(screen.getByText("Unsupported")).toBeVisible();
    expect(screen.queryByText("-1%")).not.toBeInTheDocument();
  });

  it("uses the correct Chinese copy for external spool", () => {
    expect(zh.material.externalSpool).toContain("料盘");
    expect(zh.material.externalSpool).not.toContain("盘子");
  });
});
