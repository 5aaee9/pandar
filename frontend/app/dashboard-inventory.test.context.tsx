import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { useState, type ReactNode } from "react";
import { vi } from "vitest";

import en from "../messages/en.json";
import zh from "../messages/zh.json";
import { PrinterInventory } from "./dashboard-inventory";
import {
  CameraDialogControl,
  DashboardCameraProvider,
} from "./dashboard-printer-camera-control";
import type { Agent, Printer, Tenant } from "./dashboard-types";

vi.mock("./actions", () => ({
  controlPrinter: vi.fn(),
  deletePrinter: vi.fn(),
  linkPrinter: vi.fn(),
  refreshPrinterMaterials: vi.fn(),
  updatePrinter: vi.fn(),
}));

export function renderWithMessages(children: ReactNode, locale = "en") {
  return render(
    <NextIntlClientProvider locale={locale} messages={locale === "zh" ? zh : en}>
      <QueryClientProvider client={new QueryClient()}>
        <DashboardCameraProvider>{children}</DashboardCameraProvider>
      </QueryClientProvider>
    </NextIntlClientProvider>,
  );
}

export const tenant: Tenant = {
  id: "tenant-1",
  slug: "acme",
  display_name: "Acme Labs",
  created_at: "2026-07-02T00:00:00Z",
};

export const agent: Agent = {
  id: "agent-1",
  tenant_id: tenant.id,
  name: "Shop Agent",
  status: "online",
  created_at: "2026-07-02T00:00:00Z",
};

export const printer: Printer = {
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

export const printerWithMaterials: Printer = {
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

export function CameraRouteHarness() {
  const [view, setView] = useState<"devices" | "jobs">("devices");

  return (
    <>
      <button onClick={() => setView("jobs")} type="button">
        Go to jobs
      </button>
      {view === "devices" ? (
        <CameraDialogControl printer={printer} />
      ) : (
        <div>Jobs page</div>
      )}
    </>
  );
}

export function FilterRouteHarness() {
  const [view, setView] = useState<"devices" | "jobs">("devices");
  const printers = [
    printer,
    { ...printer, id: "printer-2", name: "Workshop X1" },
  ];

  return (
    <>
      <button
        onClick={() =>
          setView((current) => (current === "devices" ? "jobs" : "devices"))
        }
        type="button"
      >
        Switch route
      </button>
      {view === "devices" ? (
        <PrinterInventory
          selectedTenant={tenant}
          printers={printers}
          agents={[agent]}
          nowMs={0}
        />
      ) : (
        <div>Jobs route</div>
      )}
    </>
  );
}
