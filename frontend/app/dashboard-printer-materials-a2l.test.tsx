import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { NextIntlClientProvider } from "next-intl";
import { describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import { PrinterMaterialsPanel } from "./dashboard-printer-materials";
import type { Printer } from "./dashboard-types";

vi.mock("./actions", () => ({ controlPrinter: vi.fn() }));

describe("A2L mixed AMS Lite controls", () => {
  it("loads a mixed AMS Lite slot with global tray id 24", async () => {
    const user = userEvent.setup();
    render(
      <NextIntlClientProvider locale="en" messages={en}>
        <PrinterMaterialsPanel printer={a2lPrinter()} />
      </NextIntlClientProvider>,
    );

    await user.click(screen.getByRole("button", { name: "AMS-A slot 1, PLA, Active" }));

    const loadForm = screen.getByRole("button", { name: "Load" }).closest("form");
    expect(loadForm?.querySelector('input[name="ams_id"]')).toHaveValue("0");
    expect(loadForm?.querySelector('input[name="slot_id"]')).toHaveValue("0");
    expect(loadForm?.querySelector('input[name="global_tray_id"]')).toHaveValue("24");
  });
});

function a2lPrinter(): Printer {
  return {
    id: "printer-a2l",
    tenant_id: "tenant-1",
    agent_id: "agent-1",
    serial_number: "A2L001",
    name: "Office A2L",
    model: "Bambu Lab A2L",
    status: "idle",
    last_seen_at: "2026-08-01T00:00:00Z",
    created_at: "2026-08-01T00:00:00Z",
    materials: {
      observed_at: "2026-08-01T00:00:00Z",
      active_tray: {
        kind: "ams",
        global_tray_id: 24,
      },
      external_spools: [],
      ams_units: [{
        unit_id: "0",
        unit_kind: "ams_lite_mixed",
        trays: [{
          tray_id: "0",
          type: "PLA",
          color: "FF0000",
          exists: true,
        }],
      }],
    },
  };
}
