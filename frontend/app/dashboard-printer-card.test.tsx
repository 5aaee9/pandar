import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";

import en from "../messages/en.json";
import type { Printer } from "./dashboard-types";
import { PrinterCard } from "./dashboard-printer-card";
import { DashboardCameraProvider } from "./dashboard-printer-camera-control";

const actionMocks = vi.hoisted(() => ({
  controlPrinter: vi.fn(async () => ({ ok: true as const })),
  deletePrinter: vi.fn(),
  refreshPrinterMaterials: vi.fn(),
  updatePrinter: vi.fn(),
}));

vi.mock("./actions", () => actionMocks);

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    warning: vi.fn(),
    error: vi.fn(),
  },
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

function renderPrinterCard(queryClient = new QueryClient()) {
  return render(
    <QueryClientProvider client={queryClient}>
      <NextIntlClientProvider locale="en" messages={en}>
        <DashboardCameraProvider>
          <PrinterCard
            agentName="Agent One"
            materialDetail="No material data"
            nowMs={Date.parse("2026-07-17T00:00:30Z")}
            printer={printer}
          />
        </DashboardCameraProvider>
      </NextIntlClientProvider>
    </QueryClientProvider>,
  );
}

describe("PrinterCard actions", () => {
  it("uses an anchored popover with the expected ARIA contract", async () => {
    const user = userEvent.setup();
    renderPrinterCard();

    const trigger = screen.getByRole("button", { name: "Actions for Printer One" });
    expect(trigger).toHaveAttribute("aria-haspopup", "dialog");

    await user.click(trigger);

    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("button", { name: "Edit printer" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Refresh AMS" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Delete printer" })).toBeVisible();
  });

  it("closes by Escape and outside interaction", async () => {
    const user = userEvent.setup();
    renderPrinterCard();
    const trigger = screen.getByRole("button", { name: "Actions for Printer One" });

    await user.click(trigger);
    await user.keyboard("{Escape}");
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Edit printer" })).not.toBeInTheDocument(),
    );

    await user.click(trigger);
    expect(screen.getByRole("button", { name: "Edit printer" })).toBeVisible();
    await user.click(document.body);
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Edit printer" })).not.toBeInTheDocument(),
    );
  });

  it("closes the popover before opening an existing action flow", async () => {
    const user = userEvent.setup();
    renderPrinterCard();

    await user.click(screen.getByRole("button", { name: "Actions for Printer One" }));
    await user.click(screen.getByRole("button", { name: "Edit printer" }));

    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Refresh AMS" })).not.toBeInTheDocument(),
    );
    expect(
      screen.getByRole("dialog", { name: "Edit printer" }),
    ).toBeVisible();
  });
});

describe("PrinterCard mutations", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("refreshes AMS in place without navigation and blocks repeats while pending", async () => {
    const user = userEvent.setup();
    let resolveAction: (value: { ok: true }) => void = () => undefined;
    actionMocks.refreshPrinterMaterials.mockImplementation(
      () =>
        new Promise<{ ok: true }>((resolve) => {
          resolveAction = resolve;
        }),
    );
    renderPrinterCard();

    await user.click(screen.getByRole("button", { name: "Actions for Printer One" }));
    await user.click(screen.getByRole("button", { name: "Refresh AMS" }));

    expect(actionMocks.refreshPrinterMaterials).toHaveBeenCalledTimes(1);
    expect(window.location.pathname).toBe("/");

    await user.click(screen.getByRole("button", { name: "Actions for Printer One" }));
    const refreshItem = screen.getByRole("button", { name: "Refresh AMS" });
    expect(refreshItem).toBeDisabled();
    expect(refreshItem.querySelector("svg.animate-spin")).not.toBeNull();

    await user.click(refreshItem);
    expect(actionMocks.refreshPrinterMaterials).toHaveBeenCalledTimes(1);

    await act(async () => {
      resolveAction({ ok: true });
    });
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("AMS refresh queued"),
    );
  });

  it("keeps the delete dialog open with a loading confirm until deletion succeeds", async () => {
    const user = userEvent.setup();
    let resolveAction: (value: { ok: true }) => void = () => undefined;
    actionMocks.deletePrinter.mockImplementation(
      () =>
        new Promise<{ ok: true }>((resolve) => {
          resolveAction = resolve;
        }),
    );
    const queryClient = new QueryClient();
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");
    renderPrinterCard(queryClient);

    await user.click(screen.getByRole("button", { name: "Actions for Printer One" }));
    await user.click(screen.getByRole("button", { name: "Delete printer" }));
    const dialog = screen.getByRole("dialog", { name: "Delete printer" });
    const confirmButton = within(dialog).getByRole("button", { name: "Delete printer" });
    await user.click(confirmButton);

    expect(actionMocks.deletePrinter).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(confirmButton).toBeDisabled());
    expect(confirmButton.querySelector("svg.animate-spin")).not.toBeNull();
    expect(dialog).toBeVisible();

    await user.click(confirmButton);
    expect(actionMocks.deletePrinter).toHaveBeenCalledTimes(1);

    await act(async () => {
      resolveAction({ ok: true });
    });
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Delete printer" })).not.toBeInTheDocument(),
    );
    expect(toast.success).toHaveBeenCalledWith("Printer deleted");
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: ["route", "devices", "tenant-1"],
    });
  });

  it("keeps the delete dialog open and toasts the error when deletion fails", async () => {
    const user = userEvent.setup();
    actionMocks.deletePrinter.mockResolvedValue({
      ok: false,
      error: "agent_not_connected",
    });
    renderPrinterCard();

    await user.click(screen.getByRole("button", { name: "Actions for Printer One" }));
    await user.click(screen.getByRole("button", { name: "Delete printer" }));
    const dialog = screen.getByRole("dialog", { name: "Delete printer" });
    await user.click(within(dialog).getByRole("button", { name: "Delete printer" }));

    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(
        "Agent is not connected to this Hub process",
      ),
    );
    expect(screen.getByRole("dialog", { name: "Delete printer" })).toBeVisible();
    expect(toast.success).not.toHaveBeenCalled();
  });

  it("saves edits with a loading submit and closes the dialog on success", async () => {
    const user = userEvent.setup();
    let resolveAction: (value: { ok: true }) => void = () => undefined;
    actionMocks.updatePrinter.mockImplementation(
      () =>
        new Promise<{ ok: true }>((resolve) => {
          resolveAction = resolve;
        }),
    );
    const queryClient = new QueryClient();
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");
    renderPrinterCard(queryClient);

    await user.click(screen.getByRole("button", { name: "Actions for Printer One" }));
    await user.click(screen.getByRole("button", { name: "Edit printer" }));
    const dialog = screen.getByRole("dialog", { name: "Edit printer" });
    const saveButton = within(dialog).getByRole("button", { name: "Save changes" });
    await user.click(saveButton);

    expect(actionMocks.updatePrinter).toHaveBeenCalledTimes(1);
    const submitted = actionMocks.updatePrinter.mock.calls[0][1] as FormData;
    expect(submitted.get("printer_id")).toBe("printer-1");
    await waitFor(() => expect(saveButton).toBeDisabled());
    expect(saveButton.querySelector("svg.animate-spin")).not.toBeNull();
    expect(dialog).toBeVisible();

    await act(async () => {
      resolveAction({ ok: true });
    });
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Edit printer" })).not.toBeInTheDocument(),
    );
    expect(toast.success).toHaveBeenCalledWith("Printer updated");
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: ["route", "devices", "tenant-1"],
    });
  });
});
