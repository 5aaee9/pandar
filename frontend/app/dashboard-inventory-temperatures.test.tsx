import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

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
  it("keeps the active nozzle switch in the temperature grid with nozzle details on separate lines", async () => {
    const dualNozzlePrinter: Printer = {
      ...printerWithMaterials,
      nozzle_temperatures: [
        {
          label: "L",
          current_celsius: "41",
          target_celsius: "220",
          diameter_mm: "0.4",
          nozzle_type: "HH05",
        },
        {
          label: "R",
          current_celsius: "42",
          target_celsius: "230",
          diameter_mm: "0.4",
          nozzle_type: "HH05",
        },
      ],
      active_nozzle: "R",
      bed_temperature_celsius: "60",
      chamber_temperature_celsius: "32",
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[dualNozzlePrinter]} agents={[agent]} nowMs={0} />,
    );

    const switchButton = await screen.findByRole("button", { name: "Switch to nozzle L" });
    const switchForm = switchButton.closest("form");
    const temperatureGrid = switchForm?.parentElement;
    expect(temperatureGrid).toHaveClass("grid-cols-2", "lg:grid-cols-4");
    expect(switchForm).not.toHaveClass("col-span-3");
    expect(switchForm).toHaveClass("h-full");
    expect(switchButton).toHaveClass("h-full");
    expect(switchForm?.querySelector('input[name="action"]')).toHaveValue("select_extruder");
    expect(switchForm?.querySelector('input[name="printer_id"]')).toHaveValue("printer-1");
    expect(switchForm?.querySelector('input[name="extruder_id"]')).toHaveValue("1");
    expect(switchButton).toHaveTextContent("L");
    expect(switchButton).toHaveTextContent("R");
    expect(switchButton).toHaveTextContent("0.4 mm");
    const diameters = within(switchButton).getAllByText("0.4 mm");
    expect(diameters).toHaveLength(2);
    for (const diameter of diameters) {
      expect(diameter.parentElement).toHaveClass("flex-col");
      expect(diameter.nextElementSibling).toHaveTextContent("HH05");
    }
    expect(within(switchButton).getByText("R").parentElement?.parentElement).toHaveClass("text-primary");
  });

  it("renders a single nozzle without a duplicate label or target temperature", async () => {
    const heatingPrinter: Printer = {
      ...printer,
      nozzle_temperatures: [{ label: null, current_celsius: "27", target_celsius: "0" }],
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[heatingPrinter]} agents={[agent]} nowMs={0} />,
    );

    const card = screen.getByRole("article", { name: "Office A1" });
    await waitFor(() => expect(card).toHaveTextContent("Nozzle"));
    expect(card).toHaveTextContent("27°");
    expect(card).not.toHaveTextContent("Nozzle Nozzle");
    expect(card).not.toHaveTextContent("27° / 0°");
  });

  it("opens a single-nozzle temperature menu with preset controls", async () => {
    const user = userEvent.setup();
    const heatingPrinter: Printer = {
      ...printer,
      nozzle_temperatures: [{ label: null, current_celsius: "27", target_celsius: "220" }],
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[heatingPrinter]} agents={[agent]} nowMs={0} />,
    );

    await user.click(await screen.findByRole("button", { name: "Set nozzle temperature" }));

    expect(screen.getByText("Set nozzle temperature")).toBeVisible();
    expect(screen.getByText("Current 27°C")).toBeVisible();
    expect(screen.getByText("Target 220°C")).toBeVisible();
    const preset = screen.getByRole("button", { name: "220 C" });
    const form = preset.closest("form");
    expect(form?.querySelector('input[name="action"]')).toHaveValue("set_hotend_temperature");
    expect(form?.querySelector('input[name="temperature_celsius"]')).toHaveValue("220");
    expect(form?.querySelector('input[name="extruder_id"]')).toBeNull();
    expect(screen.getByPlaceholderText("Custom")).toBeVisible();
  });

  it("opens dual-nozzle temperature controls with active nozzle highlighted", async () => {
    const user = userEvent.setup();
    const dualNozzlePrinter: Printer = {
      ...printer,
      nozzle_temperatures: [
        { label: "L", current_celsius: "41", target_celsius: "220" },
        { label: "R", current_celsius: "42", target_celsius: "0" },
      ],
      active_nozzle: "R",
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[dualNozzlePrinter]} agents={[agent]} nowMs={0} />,
    );

    await user.click(await screen.findByRole("button", { name: "Set nozzle temperatures" }));

    expect(screen.getByText("Set nozzle temperatures")).toBeVisible();
    const rightPanel = screen.getByText("Right temp").closest("div");
    expect(rightPanel).toHaveClass("border-primary");

    const rightOff = within(rightPanel!).getByRole("button", { name: "Off" });
    const rightForm = rightOff.closest("form");
    expect(rightForm?.querySelector('input[name="action"]')).toHaveValue("set_hotend_temperature");
    expect(rightForm?.querySelector('input[name="temperature_celsius"]')).toHaveValue("0");
    expect(rightForm?.querySelector('input[name="extruder_id"]')).toHaveValue("0");

    const leftPanel = screen.getByText("Left temp").closest("div");
    expect(within(leftPanel!).getByText("Current 41°C")).toBeVisible();
    expect(within(leftPanel!).getByText("Target 220°C")).toBeVisible();
    expect(within(leftPanel!).getAllByText(/41°C/)).toHaveLength(1);
    const leftPreset = within(leftPanel!).getByRole("button", { name: "260 C" });
    expect(leftPreset.closest("form")?.querySelector('input[name="extruder_id"]')).toHaveValue("1");

    expect(within(rightPanel!).getByText("Current 42°C")).toBeVisible();
    expect(within(rightPanel!).queryByText(/Target/)).not.toBeInTheDocument();
    expect(within(rightPanel!).getAllByText(/42°C/)).toHaveLength(1);
  });

  it("opens bed temperature controls with bed presets", async () => {
    const user = userEvent.setup();
    const heatingPrinter: Printer = {
      ...printer,
      bed_temperature_celsius: "24",
      bed_target_temperature_celsius: "75",
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[heatingPrinter]} agents={[agent]} nowMs={0} />,
    );

    await user.click(await screen.findByRole("button", { name: "Set bed temperature" }));

    expect(screen.getByText("Set bed temperature")).toBeVisible();
    expect(screen.getByText("Current 24°C")).toBeVisible();
    expect(screen.getByText("Target 75°C")).toBeVisible();
    const preset = screen.getByRole("button", { name: "75 C" });
    const form = preset.closest("form");
    expect(form?.querySelector('input[name="action"]')).toHaveValue("set_bed_temperature");
    expect(form?.querySelector('input[name="temperature_celsius"]')).toHaveValue("75");
    expect(screen.getByPlaceholderText("Custom")).toBeVisible();
  });

  it("opens chamber temperature controls with chamber presets", async () => {
    const user = userEvent.setup();
    const heatingPrinter: Printer = {
      ...printer,
      chamber_temperature_celsius: "25",
      chamber_target_temperature_celsius: "45",
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[heatingPrinter]} agents={[agent]} nowMs={0} />,
    );

    await user.click(await screen.findByRole("button", { name: "Set chamber temperature" }));

    expect(screen.getByText("Set chamber temperature")).toBeVisible();
    expect(screen.getByText("Current 25°C")).toBeVisible();
    expect(screen.getByText("Target 45°C")).toBeVisible();
    const preset = screen.getByRole("button", { name: "45 C" });
    const form = preset.closest("form");
    expect(form?.querySelector('input[name="action"]')).toHaveValue("set_chamber_temperature");
    expect(form?.querySelector('input[name="temperature_celsius"]')).toHaveValue("45");
    expect(screen.getByPlaceholderText("Custom")).toBeVisible();
  });
});
