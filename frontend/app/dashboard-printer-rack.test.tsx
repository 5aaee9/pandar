import { NextIntlClientProvider } from "next-intl";

import { QueryClientTestProvider } from "./query-client.test-utils";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import { PrinterRackPanel } from "./dashboard-printer-rack";
import type { Printer } from "./dashboard-types";
import { printerCompatibility } from "./printer-compatibility.test-utils";

vi.mock("./actions", () => ({
  controlPrinter: vi.fn(),
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

const printer: Printer = {
  id: "printer-1",
  tenant_id: "tenant-1",
  agent_id: "agent-1",
  serial_number: "H2CSERIAL",
  name: "Lab H2C",
  model: "H2C",
  compatibility: printerCompatibility("unknown"),
  status: "idle",
  last_seen_at: "2026-08-04T00:00:00Z",
  created_at: "2026-08-04T00:00:00Z",
  materials: null,
};

const rackPrinter: Printer = {
  ...printer,
  nozzle_system: {
    nozzle: {
      exist: 1,
      state: 0,
      src_id: 16,
      tar_id: 18,
      info: [
        { id: 0, diameter: 0.4000000059604645, type: "HS01", wear: 0.12 },
        { id: 1, diameter: 0.6, type: "XH05" },
        { id: 16, diameter: 0.4, type: "XS01", wear: 0.5 },
        { id: 17, diameter: 0.6, type: "HH05" },
        { id: 20, diameter: 0.2, type: "AB99" },
      ],
    },
    holder: { stat: 0, pos: 1, info: 1 },
  },
};

describe("PrinterRackPanel", () => {
  it("renders nothing without nozzle system telemetry", () => {
    const { container } = renderWithMessages(<PrinterRackPanel printer={printer} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("renders mounted and rack hotends with the holder status", () => {
    renderWithMessages(<PrinterRackPanel printer={rackPrinter} />);

    expect(screen.getByText("Hotend rack")).toBeVisible();
    expect(screen.getByText("Fixed")).toBeVisible();
    expect(screen.getByText("0.6 mm Tungsten Carbide")).toBeVisible();
    expect(screen.getByText("Swappable")).toBeVisible();
    expect(screen.getByText("0.4 mm Hardened Steel")).toBeVisible();
    expect(screen.getByText("Mounted")).toBeVisible();
    expect(screen.getByText("A top · Calibrated")).toBeVisible();
    expect(screen.getByRole("button", { name: "Rack slot 1, 0.4 mm Hardened Steel" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Rack slot 2, 0.6 mm Tungsten Carbide" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Rack slot 3, Empty" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Rack slot 5, 0.2 mm AB99" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Rack slot 6, Empty" })).toBeVisible();
  });

  it("wires rack move and global operation forms", () => {
    renderWithMessages(<PrinterRackPanel printer={rackPrinter} />);

    const centreForm = screen.getByRole("button", { name: "Centre" }).closest("form");
    expect(centreForm?.querySelector('input[name="action"]')).toHaveValue("nozzle_holder_ctrl");
    expect(centreForm?.querySelector('input[name="holder_action"]')).toHaveValue("0");
    const moveAForm = screen.getByRole("button", { name: "A top" }).closest("form");
    expect(moveAForm?.querySelector('input[name="holder_action"]')).toHaveValue("1");
    const moveBForm = screen.getByRole("button", { name: "B top" }).closest("form");
    expect(moveBForm?.querySelector('input[name="holder_action"]')).toHaveValue("2");

    const rereadAllForm = screen.getByRole("button", { name: "Re-read all" }).closest("form");
    expect(rereadAllForm?.querySelector('input[name="action"]')).toHaveValue("holder_nozzle_refresh");
    expect(rereadAllForm?.querySelector('input[name="nozzle_id"]')).toHaveValue("255");
    const confirmAllForm = screen.getByRole("button", { name: "Confirm all" }).closest("form");
    expect(confirmAllForm?.querySelector('input[name="action"]')).toHaveValue("nozzle_info_confirm");
    expect(confirmAllForm?.querySelector('input[name="nozzle_id"]')).toHaveValue("255");
  });

  it("offers per-slot re-read and confirm forms for occupied slots", async () => {
    const user = userEvent.setup();
    renderWithMessages(<PrinterRackPanel printer={rackPrinter} />);

    await user.click(screen.getByRole("button", { name: "Rack slot 2, 0.6 mm Tungsten Carbide" }));

    expect(await screen.findByText("High Flow")).toBeVisible();
    const rereadForm = (await screen.findByRole("button", { name: "Re-read hotend" })).closest("form");
    expect(rereadForm?.querySelector('input[name="action"]')).toHaveValue("holder_nozzle_refresh");
    expect(rereadForm?.querySelector('input[name="nozzle_id"]')).toHaveValue("17");
    const confirmForm = screen.getByRole("button", { name: "Confirm hotend" }).closest("form");
    expect(confirmForm?.querySelector('input[name="action"]')).toHaveValue("nozzle_info_confirm");
    expect(confirmForm?.querySelector('input[name="nozzle_id"]')).toHaveValue("17");
  });

  it("disables rack operations while a print is running", () => {
    const printing: Printer = {
      ...rackPrinter,
      status: "RUNNING",
      print: {
        task_generation: 1,
        error_generation: 0,
        hms: [],
        job_state: null,
        gcode_state: "RUNNING",
        task_id: null,
        subtask_id: null,
        subtask_name: null,
        gcode_file: null,
        progress_percent: null,
        speed_level: null,
        remaining_time_minutes: null,
        current_layer: null,
        total_layers: null,
        print_error: null,
        printer_job_id: null,
      },
    };
    renderWithMessages(<PrinterRackPanel printer={printing} />);

    expect(screen.getByRole("button", { name: "Centre" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Re-read all" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Confirm all" })).toBeDisabled();
  });
});
