import { NextIntlClientProvider } from "next-intl";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";

import en from "../messages/en.json";
import { DashboardCameraProvider } from "./dashboard-printer-camera-control";
import { PrinterControlsPanel } from "./dashboard-printer-temperature-controls";
import type { Printer } from "./dashboard-types";

const controlPrinterMock = vi.hoisted(() => vi.fn());

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

function renderWithMessages(children: React.ReactNode) {
  return render(
    <NextIntlClientProvider locale="en" messages={en}>
      <DashboardCameraProvider>{children}</DashboardCameraProvider>
    </NextIntlClientProvider>,
  );
}

const runningPrinter: Printer = {
  id: "printer-1",
  tenant_id: "tenant-1",
  agent_id: "agent-1",
  serial_number: "SERIAL123",
  name: "Office A1",
  model: "A1",
  status: "RUNNING",
  last_seen_at: "2026-07-02T00:00:00Z",
  created_at: "2026-07-02T00:00:00Z",
  materials: null,
  print: {
    task_generation: 1,
    error_generation: 0,
    hms: [],
    job_state: 0,
    gcode_state: "RUNNING",
    task_id: "task-1",
    subtask_id: "subtask-1",
    subtask_name: "Benchy",
    gcode_file: null,
    progress_percent: 42,
    speed_level: 2,
    remaining_time_minutes: 10,
    current_layer: 12,
    total_layers: 100,
    print_error: 0,
    printer_job_id: "native-job",
  },
};

describe("usePrinterControl", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("blocks repeat submissions and shows a spinner while the command is in flight", async () => {
    const user = userEvent.setup();
    let resolveAction: (value: { ok: true }) => void = () => undefined;
    controlPrinterMock.mockImplementation(
      () =>
        new Promise<{ ok: true }>((resolve) => {
          resolveAction = resolve;
        }),
    );
    renderWithMessages(<PrinterControlsPanel printer={runningPrinter} />);

    const lightButton = screen.getByRole("button", { name: "Light Off" });
    await user.click(lightButton);

    expect(controlPrinterMock).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(lightButton).toBeDisabled());
    expect(lightButton.querySelector("svg.animate-spin")).not.toBeNull();

    await user.click(lightButton);
    expect(controlPrinterMock).toHaveBeenCalledTimes(1);

    await act(async () => {
      resolveAction({ ok: true });
    });
    await waitFor(() => expect(lightButton).toBeEnabled());
    expect(lightButton.querySelector("svg.animate-spin")).toBeNull();
    expect(toast.success).toHaveBeenCalledWith("Printer control queued");
    expect(toast.error).not.toHaveBeenCalled();
  });

  it("shows an error toast when the hub rejects the command", async () => {
    const user = userEvent.setup();
    controlPrinterMock.mockResolvedValue({
      ok: false,
      error: "agent_not_connected",
    });
    renderWithMessages(<PrinterControlsPanel printer={runningPrinter} />);

    await user.click(screen.getByRole("button", { name: "Light Off" }));

    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(
        "Agent is not connected to this Hub process",
      ),
    );
    expect(toast.success).not.toHaveBeenCalled();
  });

  it("sends the stop command after confirmation and disables the trigger while pending", async () => {
    const user = userEvent.setup();
    let resolveAction: (value: { ok: true }) => void = () => undefined;
    controlPrinterMock.mockImplementation(
      () =>
        new Promise<{ ok: true }>((resolve) => {
          resolveAction = resolve;
        }),
    );
    renderWithMessages(<PrinterControlsPanel printer={runningPrinter} />);

    const stopButton = screen.getByRole("button", { name: "Stop print" });
    await user.click(stopButton);
    const dialog = screen.getByRole("dialog", { name: "Stop print" });
    await user.click(within(dialog).getByRole("button", { name: "Stop print" }));

    expect(controlPrinterMock).toHaveBeenCalledTimes(1);
    const submitted = controlPrinterMock.mock.calls[0][1] as FormData;
    expect(submitted.get("action")).toBe("stop");
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Stop print" })).toBeDisabled(),
    );
    expect(dialog).not.toBeInTheDocument();

    await act(async () => {
      resolveAction({ ok: true });
    });
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Stop print" })).toBeEnabled(),
    );
    expect(toast.success).toHaveBeenCalledWith("Printer control queued");
  });

  it("offers all four print speed modes and submits the selected mode", async () => {
    const user = userEvent.setup();
    controlPrinterMock.mockResolvedValue({ ok: true });
    renderWithMessages(<PrinterControlsPanel printer={runningPrinter} />);

    expect(screen.getByRole("button", { name: "Silent" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Standard" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Sport" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Ludicrous" })).toBeEnabled();
    expect(screen.getAllByText("Standard")).toHaveLength(2);

    await user.click(screen.getByRole("button", { name: "Sport" }));

    expect(controlPrinterMock).toHaveBeenCalledTimes(1);
    const submitted = controlPrinterMock.mock.calls[0][1] as FormData;
    expect(submitted.get("action")).toBe("set_print_speed");
    expect(submitted.get("speed_mode")).toBe("3");
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Printer control queued"),
    );
  });

  it("disables print speed switching when the printer is idle", () => {
    renderWithMessages(
      <PrinterControlsPanel
        printer={{ ...runningPrinter, status: "idle", print: null }}
      />,
    );

    expect(screen.getByRole("button", { name: "Silent" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Standard" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Sport" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Ludicrous" })).toBeDisabled();
  });
});
