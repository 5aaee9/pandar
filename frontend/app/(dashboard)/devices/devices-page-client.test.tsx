import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { afterEach, describe, expect, it, vi } from "vitest";

import en from "../../../messages/en.json";
import type { Printer, Tenant } from "../../dashboard-types";
import { printerCompatibility } from "../../printer-compatibility.test-utils";
import { DevicesPageClient } from "./devices-page-client";

const tenant: Tenant = {
  id: "tenant-1",
  slug: "tenant-1",
  display_name: "Tenant 1",
  created_at: "2026-07-28T00:00:00Z",
};

const printer: Printer = {
  id: "printer-1",
  tenant_id: tenant.id,
  agent_id: "agent-1",
  serial_number: "SERIAL-1",
  name: "Office X2D",
  model: "X2D",
  compatibility: printerCompatibility("x2d"),
  status: "FAILED",
  last_seen_at: "2026-07-28T18:33:46.063265169Z",
  created_at: "2026-07-28T00:00:00Z",
  materials: null,
};

describe("DevicesPageClient", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("keeps a recently reporting printer online after its previous task failed", async () => {
    vi.spyOn(Date, "now").mockReturnValue(Date.parse("2026-07-28T18:34:00Z"));
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: string | URL | Request) => {
        const url = String(input);
        const body = url.endsWith("/printers")
          ? { printers: [printer] }
          : url.endsWith("/agents")
            ? { agents: [] }
            : { jobs: [] };
        return Response.json(body);
      }),
    );
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });

    render(
      <QueryClientProvider client={queryClient}>
        <NextIntlClientProvider locale="en" messages={en}>
          <DevicesPageClient selectedTenant={tenant} />
        </NextIntlClientProvider>
      </QueryClientProvider>,
    );

    expect(await screen.findByText("1/1 online")).toBeVisible();
    expect(screen.queryByText("1 offline")).not.toBeInTheDocument();
    const fleetStatus = screen.getByRole("region", { name: "Fleet status" });
    const printerInventory = screen
      .getByRole("heading", { name: "Printer inventory" })
      .closest("section");
    expect(fleetStatus.parentElement).toBe(printerInventory?.parentElement);
    expect(fleetStatus.parentElement).toHaveClass("space-y-4");
    const card = screen.getByRole("article", { name: "Office X2D" });
    expect(within(card).getByText("Online")).toBeVisible();
    expect(within(card).getAllByText("Failed")[0]).toBeVisible();
  });
});
