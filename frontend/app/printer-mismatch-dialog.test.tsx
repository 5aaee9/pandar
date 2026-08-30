import { NextIntlClientProvider } from "next-intl";
import { StrictMode } from "react";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import zh from "../messages/zh.json";
import type { Printer } from "./dashboard-types";
import { printerCompatibility } from "./printer-compatibility.test-utils";
import {
  PrinterMismatchCoordinator,
  PrinterMismatchWarning,
} from "./printer-mismatch-dialog";
import { handlePrintError } from "./printer-recovery-actions";

vi.mock("./printer-recovery-actions", () => ({
  handlePrintError: vi.fn(),
}));

function mismatchPrinter(
  id: string,
  name: string,
  overrides: Partial<Printer> = {},
): Printer {
  const {
    compatibility = printerCompatibility("a1"),
    ...otherOverrides
  } = overrides;
  return {
    id,
    tenant_id: "tenant-1",
    agent_id: "agent-1",
    serial_number: "20P123",
    name,
    model: "A1",
    compatibility,
    status: "running",
    last_seen_at: "2026-07-10T00:00:00Z",
    created_at: "2026-07-10T00:00:00Z",
    materials: null,
    print: {
      task_generation: 1,
      error_generation: 9,
      hms: [],
      job_state: 1,
      gcode_state: "PAUSE",
      task_id: null,
      subtask_id: null,
      subtask_name: "Benchy",
      gcode_file: null,
      progress_percent: 42,
      speed_level: 2,
      remaining_time_minutes: 10,
      current_layer: 12,
      total_layers: 100,
      print_error: 83_918_929,
      printer_job_id: "native-job",
    },
    ...otherOverrides,
  };
}

function renderCoordinator(printers: Printer[], locale: "en" | "zh" = "en") {
  return render(
    <NextIntlClientProvider locale={locale} messages={locale === "zh" ? zh : en}>
      <PrinterMismatchCoordinator printers={printers}>
        {printers.map((printer) => (
          <PrinterMismatchWarning key={printer.id} printer={printer} />
        ))}
      </PrinterMismatchCoordinator>
    </NextIntlClientProvider>,
  );
}

describe("PrinterMismatchCoordinator", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(handlePrintError).mockResolvedValue({ status: "sent" });
  });

  it.each([
    [
      "en",
      "Warning",
      "Build plate mismatch with slicer settings. Please use the correct build plate or update the slicer settings and try again.",
      ["Problem Solved and Resume", "Ignore this and Resume", "Stop Printing"],
      "Review build plate mismatch for First",
      "Close mismatch dialog",
    ],
    [
      "zh",
      "警告",
      "打印板类型与切片设置不符。请更换匹配的打印板，或修改切片参数后重新打印。",
      ["问题已解决，继续", "忽略此问题，继续", "停止打印"],
      "查看 First 的打印板不匹配问题",
      "关闭打印板不匹配对话框",
    ],
  ] as const)(
    "renders native mismatch details and ordered actions in %s",
    async (locale, title, explanation, labels, warningLabel, closeLabel) => {
      renderCoordinator([mismatchPrinter("p1", "First")], locale);

      const dialog = await screen.findByRole("dialog");
      expect(within(dialog).getByRole("heading", { name: title })).toBeVisible();
      expect(within(dialog).getByText("0500-8051")).toBeVisible();
      expect(within(dialog).getByText(explanation)).toBeVisible();
      const actionButtons = within(dialog)
        .getAllByRole("button")
        .filter((button) => button.getAttribute("name") === "error_action");
      expect(actionButtons.map((button) => button.textContent)).toEqual(labels);
      expect(actionButtons[2]).toHaveClass("text-destructive");
      expect(within(dialog).getByRole("button", { name: closeLabel })).toBeVisible();
      expect(screen.getByRole("button", { name: warningLabel, hidden: true })).toBeVisible();
    },
  );

  it("uses the current runtime marker copy for a 094 printer", async () => {
    const printer = mismatchPrinter("p1", "First", {
      serial_number: "094123",
      print: {
        ...mismatchPrinter("p1", "First").print!,
        print_error: 83_918_946,
      },
    });
    renderCoordinator([printer]);

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("heading", { name: "Warning" })).toBeVisible();
    expect(within(dialog).getByText("0500-8062")).toBeVisible();
    expect(
      within(dialog).getByText(
        "The print plate marker was not detected. Please confirm the print plate is correctly positioned on the heatbed with all four corners aligned, and the marker is visible. If strong light is shining on the print sheet, consider closing the front door and blocking external light sources.",
      ),
    ).toBeVisible();
  });

  it.each([
    [
      83_918_945,
      "Warning",
      "0500-8061",
      "No print plate detected. Please make sure it is placed correctly.",
      ["Ignore this and Resume", "Problem Solved and Resume"],
    ],
    [
      83_918_988,
      "Warning",
      "0500-808C",
      "Detected build plate offset. Please align the build plate with the heatbed, and then continue.",
      ["Ignore this and Resume", "Problem Solved and Resume"],
    ],
    [
      83_919_003,
      "Warning",
      "0500-809B",
      "Build plate not properly positioned, may collide with the waste chute. Please reposition build plate and align with heatbed.",
      ["Ignore this and Resume", "Problem Solved and Resume"],
    ],
    [
      83_919_008,
      "Warning",
      "0500-80A0",
      "The visual encoder board was not detected. Please check if the board is properly placed and aligned at all four corners, and ensure the positioning markings are clear and free from wear.",
      ["Problem Solved and Resume", "Ignore this and Resume"],
    ],
  ] as const)(
    "renders additional Studio plate recovery error %s",
    async (printError, title, code, explanation, labels) => {
      const printer = mismatchPrinter("p1", "First", {
        print: {
          ...mismatchPrinter("p1", "First").print!,
          print_error: printError,
        },
      });
      renderCoordinator([printer]);

      const dialog = await screen.findByRole("dialog");
      expect(within(dialog).getByRole("heading", { name: title })).toBeVisible();
      expect(within(dialog).getByText(code)).toBeVisible();
      expect(within(dialog).getByText(explanation)).toBeVisible();
      const actionButtons = within(dialog)
        .getAllByRole("button")
        .filter((button) => button.getAttribute("name") === "error_action");
      expect(actionButtons.map((button) => button.textContent)).toEqual(labels);
    },
  );

  it.each([
    [
      "en",
      "Warning",
      "The print plate marker was not detected. Please confirm the print plate is correctly positioned on the heatbed with all four corners aligned, and the marker is visible. If strong light is shining on the print sheet, consider closing the front door and blocking external light sources.",
      ["Ignore this and Resume", "Problem Solved and Resume"],
      "Review build plate marker issue for First",
      "Close build plate marker dialog",
    ],
    [
      "zh",
      "警告",
      "未检测到打印板定位标识。请确保打印板放置正确，定位标识清晰。如有强光照射打印板，建议关闭前门并适当遮挡外部光源。",
      ["忽略此问题，继续", "问题已解决，继续"],
      "查看 First 的打印板标记问题",
      "关闭打印板标记对话框",
    ],
  ] as const)(
    "renders native marker details and ordered actions in %s",
    async (locale, title, explanation, labels, warningLabel, closeLabel) => {
      const printer = mismatchPrinter("p1", "First", {
        print: {
          ...mismatchPrinter("p1", "First").print!,
          print_error: 83_918_946,
        },
      });
      renderCoordinator([printer], locale);

      const dialog = await screen.findByRole("dialog");
      expect(within(dialog).getByRole("heading", { name: title })).toBeVisible();
      expect(within(dialog).getByText("0500-8062")).toBeVisible();
      expect(within(dialog).getByText(explanation)).toBeVisible();
      const actionButtons = within(dialog)
        .getAllByRole("button")
        .filter((button) => button.getAttribute("name") === "error_action");
      expect(actionButtons.map((button) => button.textContent)).toEqual(labels);
      expect(within(dialog).getByRole("button", { name: closeLabel })).toBeVisible();
      expect(screen.getByRole("button", { name: warningLabel, hidden: true })).toBeVisible();
    },
  );

  it("uses the 31B-specific offset and debris guidance", async () => {
    const printer = mismatchPrinter("p1", "First", {
      serial_number: "31B123",
      print: {
        ...mismatchPrinter("p1", "First").print!,
        print_error: 83_918_988,
      },
    });
    renderCoordinator([printer]);

    const dialog = await screen.findByRole("dialog");
    expect(
      within(dialog).getByText(
        "Detected build plate offset or debris. Please align the build plate with the heatbed and remove all debris from the plate surface before continuing.",
      ),
    ).toBeVisible();
  });

  it("auto-opens once, preserves dismissal across reconnect, reopens inline, and opens a higher generation", async () => {
    const printer = mismatchPrinter("p1", "First");
    const rendered = renderCoordinator([printer]);
    const user = userEvent.setup();

    expect(await screen.findByRole("dialog")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Close mismatch dialog" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());

    rendered.rerender(
      <NextIntlClientProvider locale="en" messages={en}>
        <PrinterMismatchCoordinator printers={[{ ...printer, print: null }]}>
          <PrinterMismatchWarning printer={{ ...printer, print: null }} />
        </PrinterMismatchCoordinator>
      </NextIntlClientProvider>,
    );
    rendered.rerender(
      <NextIntlClientProvider locale="en" messages={en}>
        <PrinterMismatchCoordinator printers={[{ ...printer }]}>
          <PrinterMismatchWarning printer={{ ...printer }} />
        </PrinterMismatchCoordinator>
      </NextIntlClientProvider>,
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Review build plate mismatch for First" }));
    expect(await screen.findByRole("dialog")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Close mismatch dialog" }));

    const next = {
      ...printer,
      print: { ...printer.print!, error_generation: 10 },
    };
    rendered.rerender(
      <NextIntlClientProvider locale="en" messages={en}>
        <PrinterMismatchCoordinator printers={[next]}>
          <PrinterMismatchWarning printer={next} />
        </PrinterMismatchCoordinator>
      </NextIntlClientProvider>,
    );
    expect(await screen.findByRole("dialog")).toBeVisible();
  });

  it("does not auto-open the same generation twice after an unavailable baseline", async () => {
    const printer = mismatchPrinter("p1", "First");
    const rendered = renderCoordinator([printer]);
    expect(await screen.findByRole("dialog")).toBeVisible();

    const unavailable = { ...printer, print: null };
    rendered.rerender(
      <NextIntlClientProvider locale="en" messages={en}>
        <PrinterMismatchCoordinator printers={[unavailable]}>
          <PrinterMismatchWarning printer={unavailable} />
        </PrinterMismatchCoordinator>
      </NextIntlClientProvider>,
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());

    rendered.rerender(
      <NextIntlClientProvider locale="en" messages={en}>
        <PrinterMismatchCoordinator printers={[printer]}>
          <PrinterMismatchWarning printer={printer} />
        </PrinterMismatchCoordinator>
      </NextIntlClientProvider>,
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("moves focus into the modal and supports accessible Escape dismissal", async () => {
    const user = userEvent.setup();
    renderCoordinator([mismatchPrinter("p1", "First")]);

    const dialog = await screen.findByRole("dialog");
    await waitFor(() =>
      expect(dialog).toContainElement(document.activeElement as HTMLElement | null),
    );
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Review build plate mismatch for First" })).toBeVisible();
  });

  it("closes on authoritative clear or a different print error", async () => {
    const printer = mismatchPrinter("p1", "First");
    const rendered = renderCoordinator([printer]);
    expect(await screen.findByRole("dialog")).toBeVisible();

    const cleared = {
      ...printer,
      print: { ...printer.print!, print_error: 0 },
    };
    rendered.rerender(
      <NextIntlClientProvider locale="en" messages={en}>
        <PrinterMismatchCoordinator printers={[cleared]}>
          <PrinterMismatchWarning printer={cleared} />
        </PrinterMismatchCoordinator>
      </NextIntlClientProvider>,
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.queryByText("Build plate mismatch")).not.toBeInTheDocument();

    const otherError = {
      ...printer,
      print: { ...printer.print!, print_error: 123, error_generation: 10 },
    };
    rendered.rerender(
      <NextIntlClientProvider locale="en" messages={en}>
        <PrinterMismatchCoordinator printers={[otherError]}>
          <PrinterMismatchWarning printer={otherError} />
        </PrinterMismatchCoordinator>
      </NextIntlClientProvider>,
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("selects simultaneous mismatches in stable printer-list order", async () => {
    const first = mismatchPrinter("p1", "First");
    const second = mismatchPrinter("p2", "Second", {
      print: { ...mismatchPrinter("p2", "Second").print!, error_generation: 4 },
    });
    const user = userEvent.setup();
    renderCoordinator([second, first]);

    expect(await screen.findByRole("dialog")).toHaveTextContent("Second");
    await user.click(screen.getByRole("button", { name: "Close mismatch dialog" }));
    expect(await screen.findByRole("dialog")).toHaveTextContent("First");
  });

  it("keeps first-open selection stable under Strict Mode effect replay", async () => {
    const first = mismatchPrinter("p1", "First");
    const second = mismatchPrinter("p2", "Second", {
      print: { ...mismatchPrinter("p2", "Second").print!, error_generation: 4 },
    });

    const user = userEvent.setup();
    render(
      <NextIntlClientProvider locale="en" messages={en}>
        <StrictMode>
          <PrinterMismatchCoordinator printers={[first, second]}>
            <PrinterMismatchWarning printer={first} />
            <PrinterMismatchWarning printer={second} />
          </PrinterMismatchCoordinator>
        </StrictMode>
      </NextIntlClientProvider>,
    );

    expect(await screen.findByRole("dialog")).toHaveTextContent("First");
    await user.click(screen.getByRole("button", { name: "Close mismatch dialog" }));
    expect(await screen.findByRole("dialog")).toHaveTextContent("Second");
  });

  it.each([
    ["en", "Use the printer screen to resolve this error.", "Stop Printing"],
    ["zh", "请在打印机屏幕上处理此错误。", "停止打印"],
  ] as const)("keeps unsupported occurrences informational in %s and applies the coarse inactive veto", async (locale, guidance, stopLabel) => {
    const unsupported = mismatchPrinter("p1", "Unsupported", {
      serial_number: "26A123",
    });
    const rendered = renderCoordinator([unsupported], locale);

    let dialog = await screen.findByRole("dialog");
    expect(within(dialog).queryByRole("button", { name: stopLabel })).not.toBeInTheDocument();
    expect(within(dialog).getByText(guidance)).toBeVisible();

    const inactive = mismatchPrinter("p1", "Unsupported", { status: "IDLE" });
    rendered.rerender(
      <NextIntlClientProvider locale={locale} messages={locale === "zh" ? zh : en}>
        <PrinterMismatchCoordinator printers={[inactive]}>
          <PrinterMismatchWarning printer={inactive} />
        </PrinterMismatchCoordinator>
      </NextIntlClientProvider>,
    );
    dialog = await screen.findByRole("dialog");
    expect(within(dialog).queryByRole("button", { name: stopLabel })).not.toBeInTheDocument();
    expect(within(dialog).getByText(guidance)).toBeVisible();
  });

  it("deduplicates pending submission, sends only occurrence fields, and leaves the warning after sent", async () => {
    let resolveAction!: (value: { status: "sent" }) => void;
    vi.mocked(handlePrintError).mockImplementationOnce(
      () => new Promise((resolve) => {
        resolveAction = resolve;
      }),
    );
    const user = userEvent.setup();
    renderCoordinator([mismatchPrinter("p1", "First")]);

    const dialog = await screen.findByRole("dialog");
    const resume = within(dialog).getByRole("button", { name: "Problem Solved and Resume" });
    await user.click(resume);
    expect(handlePrintError).toHaveBeenCalledTimes(1);
    expect(
      Object.fromEntries((vi.mocked(handlePrintError).mock.calls[0][1] as FormData).entries()),
    ).toEqual({
      tenant_id: "tenant-1",
      printer_id: "p1",
      error_generation: "9",
      error_action: "resume",
    });
    for (const button of within(dialog).getAllByRole("button")) {
      expect(button).toBeDisabled();
    }
    await user.click(resume);
    expect(handlePrintError).toHaveBeenCalledTimes(1);

    resolveAction({ status: "sent" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Review build plate mismatch for First" })).toBeVisible();
  });

  it("keeps the dialog open and restores controls after a typed transport failure", async () => {
    vi.mocked(handlePrintError).mockResolvedValueOnce({
      status: "error",
      error: "Printer recovery request failed: TypeError: Network request failed",
    });
    const user = userEvent.setup();
    renderCoordinator([mismatchPrinter("p1", "First")]);

    await user.click(
      within(await screen.findByRole("dialog")).getByRole("button", {
        name: "Problem Solved and Resume",
      }),
    );
    const dialog = await screen.findByRole("dialog");
    expect(
      within(dialog).getByText(
        "Printer recovery request failed: TypeError: Network request failed",
      ),
    ).toBeVisible();
    for (const button of within(dialog).getAllByRole("button")) {
      expect(button).toBeEnabled();
    }
  });
});
