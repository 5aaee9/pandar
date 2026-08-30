import { NextIntlClientProvider } from "next-intl";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import en from "../messages/en.json";
import zh from "../messages/zh.json";
import { PrinterHmsPanel } from "./dashboard-printer-hms";
import type { Printer } from "./dashboard-types";
import { printerCompatibility } from "./printer-compatibility.test-utils";

const printer: Printer = {
  id: "printer-1",
  tenant_id: "tenant-1",
  agent_id: "agent-1",
  serial_number: "20P123456",
  name: "Office printer",
  model: "A1",
  compatibility: printerCompatibility("a1"),
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
    speed_level: 2,
    remaining_time_minutes: null,
    current_layer: null,
    total_layers: null,
    print_error: null,
    printer_job_id: null,
    hms: [
      { attr: 0x18002000, code: 0x00020026 },
      { attr: 0x05000600, code: 0x00020070 },
    ],
  },
};

function renderHms(value: Printer, locale: "en" | "zh" = "en") {
  return render(
    <NextIntlClientProvider locale={locale} messages={locale === "zh" ? zh : en}>
      <PrinterHmsPanel printer={value} />
    </NextIntlClientProvider>,
  );
}

describe("PrinterHmsPanel", () => {
  it("shows only cataloged HMS messages with Bambu text, codes, and detail links", () => {
    renderHms(printer);

    expect(screen.getByRole("region", { name: "1 HMS message" })).toBeVisible();
    expect(screen.getByText(
      "AMS-HT A assist motor overloaded. Excessive resistance in the filament tube between the AMS and the filament track switch.",
    )).toBeVisible();
    expect(screen.getByText("Serious system message")).toBeVisible();
    expect(screen.getByText("HMS 1800200000020026")).toBeVisible();
    expect(screen.queryByText("HMS 0500060000020070")).not.toBeInTheDocument();

    const link = screen.getByRole("link", { name: "View details" });
    expect(link).toHaveAttribute(
      "href",
      "https://e.bambulab.com/index.php?e=1800200000020026&s=device_hms&lang=en",
    );
    expect(link).toHaveAttribute("target", "_blank");
  });

  it("hides the internal 0500060000020070 report like Bambu Studio", () => {
    renderHms({
      ...printer,
      print: { ...printer.print!, hms: [printer.print!.hms[1]] },
    });

    expect(screen.queryByText("Serious system message")).not.toBeInTheDocument();
    expect(screen.queryByText("HMS 0500060000020070")).not.toBeInTheDocument();
  });

  it("uses the localized Bambu catalog", () => {
    renderHms({
      ...printer,
      print: { ...printer.print!, hms: [printer.print!.hms[0]] },
    }, "zh");

    expect(screen.getByText(
      "AMS-HT A 助力电机过载，AMS至耗材变轨器之间料管阻力过大。",
    )).toBeVisible();
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
