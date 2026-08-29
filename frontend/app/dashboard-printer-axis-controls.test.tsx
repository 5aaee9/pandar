import { NextIntlClientProvider } from "next-intl";

import { QueryClientTestProvider } from "./query-client.test-utils";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import zh from "../messages/zh.json";
import { PrinterAxisControls } from "./dashboard-printer-axis-controls";
import type { Printer } from "./dashboard-types";

const controlPrinterMock = vi.hoisted(() =>
  vi.fn(async (_prev: unknown, _formData: FormData) => ({ ok: true as const })),
);

vi.mock("./actions", () => ({
  controlPrinter: controlPrinterMock,
}));

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    warning: vi.fn(),
    error: vi.fn(),
  },
}));

function renderWithMessages(children: React.ReactNode, locale = "en") {
  return render(
    <QueryClientTestProvider>
      <NextIntlClientProvider locale={locale} messages={locale === "zh" ? zh : en}>
        {children}
      </NextIntlClientProvider>
    </QueryClientTestProvider>,
  );
}

const printer: Printer = {
  id: "printer-1",
  tenant_id: "tenant-1",
  agent_id: "agent-1",
  serial_number: "SERIAL123",
  name: "Office A1",
  model: "A1",
  status: "idle",
  last_seen_at: "2026-07-02T00:00:00Z",
  created_at: "2026-07-02T00:00:00Z",
  materials: null,
};

describe("PrinterAxisControls", () => {
  beforeEach(() => {
    controlPrinterMock.mockClear();
  });

  const axes = ["X", "Y", "Z"] as const;
  const distances = [-10, -1, 1, 10] as const;

  it("renders exact signed axis forms without status or feature gates", async () => {
    const user = userEvent.setup();
    const offlinePrinter = Object.assign({}, printer, { status: "offline" });
    renderWithMessages(<PrinterAxisControls printer={offlinePrinter} />);

    const trigger = screen.getByRole("button", { name: "Move axes" });
    expect(trigger).toBeEnabled();
    await user.click(trigger);

    for (const axis of axes) {
      for (const distance of distances) {
        const signed = distance > 0 ? `+${distance}` : String(distance);
        const button = screen.getByRole("button", {
          name: `Move ${axis} by ${signed} mm`,
        });
        expect(button).toBeEnabled();
        const form = button.closest("form");
        expect(form?.querySelector('input[name="action"]')).toHaveValue("move_axes");
        expect(form?.querySelector('input[name="axis"]')).toHaveValue(axis.toLowerCase());
        expect(form?.querySelector('input[name="delta_mm"]')).toHaveValue(String(distance));
        expect(form?.querySelector('input[name="feedrate_mm_per_min"]')).toHaveValue(
          axis === "Z" ? "900" : "3000",
        );
        expect(form?.querySelector('input[name="required_device_features"]')).toBeNull();
      }
    }
  });

  it("requires a portaled modal confirmation before full-axis Home", async () => {
    const user = userEvent.setup();
    renderWithMessages(<PrinterAxisControls printer={printer} />);
    await user.click(screen.getByRole("button", { name: "Move axes" }));
    const axisDialog = screen.getByRole("dialog", { name: "Move printer axes" });
    const homeButton = screen.getByRole("button", { name: "Home all axes" });
    const homeForm = homeButton.closest("form");
    expect(homeForm?.querySelector('input[name="action"]')).toHaveValue("home");
    expect(homeForm?.querySelector('input[name="required_device_features"]')).toBeNull();
    await user.click(homeButton);

    expect(controlPrinterMock).not.toHaveBeenCalled();
    const homeDialog = screen.getByRole("dialog", { name: "Auto homing" });
    expect(homeDialog).toBeVisible();
    expect(homeDialog).toHaveAttribute("data-slot", "dialog-content");
    expect(axisDialog).not.toContainElement(homeDialog);
    await waitFor(() =>
      expect(homeDialog).toContainElement(document.activeElement as HTMLElement | null),
    );

    await user.keyboard("{Escape}");
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Auto homing" })).not.toBeInTheDocument(),
    );
    expect(controlPrinterMock).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Home all axes" }));
    await user.click(screen.getByRole("button", { name: "Homing" }));
    await waitFor(() => expect(controlPrinterMock).toHaveBeenCalledTimes(1));
    const submitted = controlPrinterMock.mock.calls[0][1];
    expect(Object.fromEntries(submitted.entries())).toEqual({
      tenant_id: "tenant-1",
      printer_id: "printer-1",
      action: "home",
    });
  });

  it("renders localized Chinese axis controls", async () => {
    const user = userEvent.setup();
    renderWithMessages(<PrinterAxisControls printer={printer} />, "zh");
    await user.click(screen.getByRole("button", { name: "移动轴" }));
    expect(screen.getByRole("heading", { name: "移动打印机轴" })).toBeVisible();
    expect(screen.getByRole("button", { name: "将 X 轴移动 +10 毫米" })).toBeVisible();
  });
});
