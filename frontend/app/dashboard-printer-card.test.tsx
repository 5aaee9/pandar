import { NextIntlClientProvider } from "next-intl";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import type { Printer } from "./dashboard-types";
import { PrinterCard } from "./dashboard-printer-card";

vi.mock("./actions", () => ({
  controlPrinter: vi.fn(),
  deletePrinter: vi.fn(),
  refreshPrinterMaterials: vi.fn(),
  updatePrinter: vi.fn(),
}));

const printer: Printer = {
  id: "printer-1",
  tenant_id: "tenant-1",
  agent_id: "agent-1",
  serial_number: "SN1",
  name: "Printer One",
  model: "X1C",
  status: "idle",
  last_seen_at: "2026-07-17T00:00:00Z",
  created_at: "2026-07-01T00:00:00Z",
  materials: null,
};

function renderPrinterCard() {
  return render(
    <NextIntlClientProvider locale="en" messages={en}>
      <PrinterCard
        agentName="Agent One"
        materialDetail="No material data"
        nowMs={Date.parse("2026-07-17T00:00:30Z")}
        printer={printer}
      />
    </NextIntlClientProvider>,
  );
}

describe("PrinterCard actions", () => {
  it("uses an anchored menu with the expected ARIA contract", async () => {
    const user = userEvent.setup();
    renderPrinterCard();

    const trigger = screen.getByRole("button", { name: "Details" });
    expect(trigger).toHaveAttribute("aria-haspopup", "menu");

    await user.click(trigger);

    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("menu")).toBeVisible();
    expect(screen.getAllByRole("menuitem")).toHaveLength(3);
  });

  it("closes by Escape and outside interaction", async () => {
    const user = userEvent.setup();
    renderPrinterCard();
    const trigger = screen.getByRole("button", { name: "Details" });

    await user.click(trigger);
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("menu")).not.toBeInTheDocument());

    await user.click(trigger);
    expect(screen.getByRole("menu")).toBeVisible();
    await user.click(document.body);
    await waitFor(() => expect(screen.queryByRole("menu")).not.toBeInTheDocument());
  });

  it("closes the menu before opening an existing action flow", async () => {
    const user = userEvent.setup();
    renderPrinterCard();

    await user.click(screen.getByRole("button", { name: "Details" }));
    await user.click(screen.getByRole("menuitem", { name: "Edit printer" }));

    await waitFor(() => expect(screen.queryByRole("menu")).not.toBeInTheDocument());
    expect(
      screen.getByRole("dialog", { name: "Edit printer" }),
    ).toBeVisible();
  });
});
