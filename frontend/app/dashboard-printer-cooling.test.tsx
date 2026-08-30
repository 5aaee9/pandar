import { NextIntlClientProvider } from "next-intl";

import { QueryClientTestProvider } from "./query-client.test-utils";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import en from "../messages/en.json";
import { PrinterCoolingSystem } from "./dashboard-printer-cooling";
import type { Printer } from "./dashboard-types";
import { printerCompatibility } from "./printer-compatibility.test-utils";

const printer: Printer = {
  id: "printer-1",
  tenant_id: "tenant-1",
  agent_id: "agent-1",
  serial_number: "SERIAL123",
  name: "Office X2D",
  model: "X2D",
  compatibility: printerCompatibility("x2d"),
  status: "RUNNING",
  last_seen_at: "2026-08-08T00:00:00Z",
  created_at: "2026-08-08T00:00:00Z",
  materials: null,
  cooling_system: {
    mode: "cooling",
    fans: [
      { kind: "part_cooling", speed_percent: 100 },
      { kind: "auxiliary", speed_percent: 60 },
      { kind: "chamber", speed_percent: 40 },
      { kind: "hotend", speed_percent: 20 },
    ],
  },
};

function renderCooling(value: Printer) {
  return render(
    <QueryClientTestProvider>
      <NextIntlClientProvider locale="en" messages={en}>
        <PrinterCoolingSystem printer={value} />
      </NextIntlClientProvider>
    </QueryClientTestProvider>,
  );
}

describe("PrinterCoolingSystem", () => {
  it("shows the Bambu cooling mode and reported fan percentages", () => {
    renderCooling(printer);

    expect(screen.getByRole("region", { name: "Cooling system" })).toBeVisible();
    expect(screen.getByText("Cooling")).toBeVisible();
    expect(screen.getByText("Part cooling")).toBeVisible();
    expect(screen.getByText("Auxiliary")).toBeVisible();
    expect(screen.getByText("Chamber / exhaust")).toBeVisible();
    expect(screen.getByText("Hotend")).toBeVisible();
    expect(screen.getByText("100%")).toBeVisible();
    expect(screen.getByText("60%")).toBeVisible();
    expect(screen.getByText("40%")).toBeVisible();
    expect(screen.getByText("20%")).toBeVisible();
    expect(screen.getByText("Part cooling")).not.toHaveClass("truncate");
  });

  it("offers Bambu fan controls for user-adjustable airduct fans", async () => {
    const user = userEvent.setup();
    renderCooling(printer);

    await user.click(screen.getByRole("button", { name: "Set Part cooling fan speed" }));

    expect(screen.getByRole("group", { name: "Set Part cooling fan speed" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Off" })).toBeEnabled();
    const halfSpeed = screen.getByRole("button", { name: "50%" });
    expect(halfSpeed).toBeEnabled();
    expect(halfSpeed.closest("form")).toHaveFormValues({
      tenant_id: "tenant-1",
      printer_id: "printer-1",
      action: "set_fan_speed",
      fan_index: "1",
      speed_percent: "50",
      airduct: "true",
    });
    expect(screen.getByRole("button", { name: "100%" })).toBeEnabled();
  });

  it.each([
    ["absent", undefined],
    ["unknown", printerCompatibility("unknown")],
    ["unsupported", printerCompatibility("a1")],
  ])("does not expose a chamber fan when support is %s", (_state, compatibility) => {
    renderCooling({
      ...printer,
      model: "A1",
      compatibility,
      cooling_system: {
        mode: null,
        fans: [{ kind: "chamber", speed_percent: 0 }],
      },
    });

    expect(screen.queryByRole("region", { name: "Cooling system" })).not.toBeInTheDocument();
  });
});
