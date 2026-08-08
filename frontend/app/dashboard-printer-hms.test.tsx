import { NextIntlClientProvider } from "next-intl";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import en from "../messages/en.json";
import { PrinterHmsPanel } from "./dashboard-printer-hms";
import type { Printer } from "./dashboard-types";

const printer: Printer = {
  id: "printer-1",
  tenant_id: "tenant-1",
  agent_id: "agent-1",
  serial_number: "094123456",
  name: "Office printer",
  model: "X1C",
  status: "RUNNING",
  last_seen_at: "2026-08-09T00:00:00Z",
  created_at: "2026-08-09T00:00:00Z",
  materials: null,
  print: {
    task_generation: 1,
    error_generation: 0,
    job_state: null,
    gcode_state: "RUNNING",
    task_id: null,
    subtask_id: null,
    subtask_name: null,
    gcode_file: null,
    progress_percent: 20,
    remaining_time_minutes: null,
    current_layer: null,
    total_layers: null,
    print_error: null,
    printer_job_id: null,
    hms: [
      { attr: 0x07ff0200, code: 0x00008011 },
      { attr: 0x05000600, code: 0x00020070 },
    ],
  },
};

function renderHms(value: Printer) {
  return render(
    <NextIntlClientProvider locale="en" messages={en}>
      <PrinterHmsPanel printer={value} />
    </NextIntlClientProvider>,
  );
}

describe("PrinterHmsPanel", () => {
  it("shows active HMS messages with Bambu-compatible codes and detail links", () => {
    renderHms(printer);

    expect(screen.getByRole("region", { name: "2 HMS messages" })).toBeVisible();
    expect(screen.getByText("Unknown system message")).toBeVisible();
    expect(screen.getByText("Serious system message")).toBeVisible();
    expect(screen.getByText("HMS 07FF020000008011")).toBeVisible();
    expect(screen.getByText("HMS 0500060000020070")).toBeVisible();

    const links = screen.getAllByRole("link", { name: "View details" });
    expect(links[0]).toHaveAttribute(
      "href",
      "https://e.bambulab.com/index.php?e=07FF020000008011&s=device_hms&lang=en",
    );
    expect(links[0]).toHaveAttribute("target", "_blank");
  });

  it("stays hidden when the printer has no HMS messages", () => {
    const { rerender } = renderHms({ ...printer, print: null });
    expect(screen.queryByText(/HMS messages/)).not.toBeInTheDocument();

    rerender(
      <NextIntlClientProvider locale="en" messages={en}>
        <PrinterHmsPanel
          printer={{ ...printer, print: { ...printer.print!, hms: [] } }}
        />
      </NextIntlClientProvider>,
    );
    expect(screen.queryByText(/HMS messages/)).not.toBeInTheDocument();
  });
});
