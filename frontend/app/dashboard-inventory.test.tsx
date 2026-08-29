import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { PrinterInventory } from "./dashboard-inventory";
import type { Printer } from "./dashboard-types";
import {
  FilterRouteHarness,
  agent,
  printer,
  printerWithMaterials,
  renderWithMessages,
  tenant,
} from "./dashboard-inventory.test.context";

describe("PrinterInventory", () => {
  it("resets its typed filters after navigating away and back", async () => {
    const user = userEvent.setup();
    renderWithMessages(<FilterRouteHarness />);

    await user.type(
      screen.getByRole("searchbox", { name: "Search name or serial" }),
      "Office",
    );
    expect(
      screen.getByRole("article", { name: "Office A1" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("article", { name: "Workshop X1" }),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Switch route" }));
    await user.click(screen.getByRole("button", { name: "Switch route" }));

    expect(
      screen.getByRole("article", { name: "Office A1" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("article", { name: "Workshop X1" }),
    ).toBeInTheDocument();
  });

  it("renders enriched native print details in the existing card summary", () => {
    const livePrinter: Printer = {
      ...printerWithMaterials,
      status: "RUNNING",
      print: {
        task_generation: 3,
        error_generation: 0,
        hms: [],
        job_state: 0,
        gcode_state: "RUNNING",
        task_id: "task-1",
        subtask_id: "subtask-1",
        subtask_name: "Live Benchy",
        gcode_file: "/cache/plate_1.gcode.3mf",
        progress_percent: 37,
        speed_level: 2,
        remaining_time_minutes: 65,
        current_layer: 12,
        total_layers: 100,
        print_error: 0,
        printer_job_id: "native-job",
      },
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[livePrinter]} agents={[agent]} nowMs={0} />,
    );

    const status = screen.getByTestId("printer-print-status");
    expect(status).toHaveTextContent("Printing");
    expect(status).toHaveTextContent("Live Benchy");
    expect(status).toHaveTextContent("37%");
    expect(status).toHaveTextContent("Layers 12/100");
    expect(status).toHaveTextContent("Remaining 1h 5m");
    expect(screen.getByRole("article", { name: "Office A1" })).toHaveTextContent("AMS-A");
  });

  it("renders a persistent inline mismatch warning on the affected card", () => {
    const mismatchPrinter: Printer = {
      ...printer,
      status: "RUNNING",
      serial_number: "20P123",
      print: {
        task_generation: 1,
        error_generation: 9,
        hms: [],
        job_state: 0,
        gcode_state: "PAUSE",
        task_id: null,
        subtask_id: null,
        subtask_name: "Benchy",
        gcode_file: null,
        progress_percent: 42,
        speed_level: 2,
        remaining_time_minutes: 10,
        current_layer: 12,
        total_layers: 100,
        print_error: 83_918_929,
        printer_job_id: "native-job",
      },
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[mismatchPrinter]} agents={[agent]} nowMs={0} />,
    );

    const card = screen.getByRole("article", { name: "Office A1" });
    expect(within(card).getByText("Build plate mismatch")).toBeVisible();
    expect(
      within(card).getByRole("button", { name: "Review build plate mismatch for Office A1" }),
    ).toBeVisible();
  });

  it("renders inventory content without the tenant subtitle or reported count", () => {
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printer]} agents={[agent]} nowMs={0} />,
    );

    expect(screen.getByRole("heading", { name: "Printer inventory" })).toBeVisible();
    expect(screen.queryByText("Acme Labs (acme)")).not.toBeInTheDocument();
    expect(screen.queryByText("1 reported")).not.toBeInTheDocument();
  });

  it("renders printers as individual machine cards", () => {
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printer]} agents={[agent]} nowMs={0} />,
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
      <PrinterInventory selectedTenant={tenant} printers={[printer]} agents={[agent]} nowMs={0} />,
    );

    const chip = screen.getByText("Shop Agent").parentElement;
    expect(chip?.parentElement).toHaveTextContent("Idle");
  });

  it("opens a printer actions popover with delete", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printer]} agents={[agent]} nowMs={0} />,
    );

    await user.click(screen.getByRole("button", { name: "Actions for Office A1" }));

    expect(screen.getByRole("button", { name: "Edit printer" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Delete printer" })).toBeVisible();
  });

  it("opens an edit printer dialog from the printer actions popover", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printer]} agents={[agent]} nowMs={0} />,
    );

    await user.click(screen.getByRole("button", { name: "Actions for Office A1" }));
    await user.click(screen.getByRole("button", { name: "Edit printer" }));

    expect(screen.getByRole("dialog")).toBeVisible();
    expect(screen.getByRole("heading", { name: "Edit printer" })).toBeVisible();
    expect(screen.getByLabelText("Name")).toHaveValue("Office A1");
    expect(screen.getByLabelText("Printer IPv4 address")).toHaveAttribute("name", "host");
    expect(screen.getByLabelText("Printer IPv4 address")).not.toBeRequired();
    expect(screen.getByLabelText("Access code")).toHaveAttribute("name", "access_code");
    expect(screen.getByLabelText("Access code")).not.toBeRequired();

    const form = screen.getByRole("button", { name: "Save changes" }).closest("form");
    expect(form?.querySelector('input[name="tenant_id"]')).toHaveValue("tenant-1");
    expect(form?.querySelector('input[name="printer_id"]')).toHaveValue("printer-1");
  });

  it("opens the machine form from the empty printer state", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[]} agents={[agent]} nowMs={0} />,
    );

    expect(screen.getByText("No printers reported")).toBeVisible();

    const trigger = screen.getByRole("button", { name: "Link printer" });
    expect(trigger).toHaveAttribute("data-slot", "dialog-trigger");

    await user.click(trigger);

    const dialog = screen.getByRole("dialog", { name: "Link printer to agent" });
    const form = within(dialog)
      .getByRole("button", { name: "Link printer" })
      .closest("form")!;
    expect(form).toHaveFormValues({
      tenant_id: tenant.id,
      agent_id: agent.id,
      type: "BambuLab",
      host: "",
      name: "",
      access_code: "",
    });

    await user.type(within(dialog).getByLabelText("Access code"), "SECRET-LINK-CODE");
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    await user.click(trigger);

    expect(screen.getByLabelText("Access code")).toHaveValue("");
  });

  it("renders AMS refresh inside the printer actions popover with tenant and printer ids", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printer]} agents={[agent]} nowMs={0} />,
    );

    expect(screen.queryByRole("button", { name: "Refresh AMS" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Actions for Office A1" }));

    const button = screen.getByRole("button", { name: "Refresh AMS" });
    const form = button.closest("form");

    expect(form).not.toBeNull();
    expect(form?.querySelector('input[name="tenant_id"]')).toHaveValue("tenant-1");
    expect(form?.querySelector('input[name="printer_id"]')).toHaveValue("printer-1");
  });
});
