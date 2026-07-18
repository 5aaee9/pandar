import { render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { describe, expect, it } from "vitest";

import en from "../messages/en.json";
import zh from "../messages/zh.json";
import { PrinterPrintStatus } from "./printer-print-status";
import type { PrinterPrintState } from "./printer-live-types";

const basePrint: PrinterPrintState = {
  task_generation: 3,
  error_generation: 0,
  hms: [],
  job_state: 0,
  gcode_state: "RUNNING",
  task_id: "task-1",
  subtask_id: "subtask-1",
  subtask_name: "Benchy",
  gcode_file: "/cache/plate_1.gcode.3mf",
  progress_percent: 42,
  remaining_time_minutes: 65,
  current_layer: 12,
  total_layers: 100,
  print_error: 0,
  printer_job_id: "job-1",
};

function renderStatus(
  print: PrinterPrintState | null,
  coarseStatus = "running",
  locale: "en" | "zh" = "en",
) {
  return render(
    <NextIntlClientProvider locale={locale} messages={locale === "zh" ? zh : en}>
      <PrinterPrintStatus coarseStatus={coarseStatus} print={print} />
    </NextIntlClientProvider>,
  );
}

describe("PrinterPrintStatus", () => {
  it("uses subtask, device-path basename, then localized unknown-task precedence", () => {
    const rendered = renderStatus(basePrint);
    expect(screen.getByText("Benchy")).toBeVisible();

    rendered.rerender(
      <NextIntlClientProvider locale="en" messages={en}>
        <PrinterPrintStatus
          coarseStatus="running"
          print={{
            ...basePrint,
            subtask_name: "  ",
            gcode_file: "C:\\printer\\cache\\dragon.gcode.3mf",
          }}
        />
      </NextIntlClientProvider>,
    );
    expect(screen.getByText("dragon.gcode.3mf")).toBeVisible();
    expect(screen.queryByText(/printer|cache/)).not.toBeInTheDocument();

    rendered.rerender(
      <NextIntlClientProvider locale="zh" messages={zh}>
        <PrinterPrintStatus
          coarseStatus="running"
          print={{ ...basePrint, subtask_name: null, gcode_file: "" }}
        />
      </NextIntlClientProvider>,
    );
    expect(screen.getByText("未知打印任务")).toBeVisible();
  });

  it.each([
    ["en", "Printing", "Layers 12/100", "Remaining 1h 5m"],
    ["zh", "打印中", "层数 12/100", "剩余 1 小时 5 分钟"],
  ] as const)(
    "renders clamped live progress and localized details in %s",
    (locale, stateLabel, layers, remaining) => {
      renderStatus({ ...basePrint, progress_percent: 124 }, "running", locale);

      expect(screen.getByText(stateLabel)).toBeVisible();
      expect(screen.getByText("100%")).toBeVisible();
      expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "100");
      expect(screen.getByText(layers)).toBeVisible();
      expect(screen.getByText(remaining)).toBeVisible();
    },
  );

  it.each([
    ["en", "RUNNING", "Printing"],
    ["en", "PRINTING", "Printing"],
    ["en", "PAUSE", "Paused"],
    ["en", "PAUSED", "Paused"],
    ["en", "FINISH", "Finished"],
    ["zh", "RUNNING", "打印中"],
    ["zh", "PRINTING", "打印中"],
    ["zh", "PAUSE", "已暂停"],
    ["zh", "PAUSED", "已暂停"],
    ["zh", "FINISH", "已完成"],
  ] as const)("renders the %s %s display state as %s", (locale, gcodeState, label) => {
    renderStatus({ ...basePrint, gcode_state: gcodeState }, "running", locale);
    expect(screen.getByText(label)).toBeVisible();
    expect(screen.getByText("42%")).toBeVisible();
  });

  it.each([
    ["en", "PREPARE", "Preparing", "Layers -", "Remaining -"],
    ["en", "SLICING", "Slicing", "Layers -", "Remaining -"],
    ["zh", "PREPARE", "准备中", "层数 -", "剩余 -"],
    ["zh", "SLICING", "切片中", "层数 -", "剩余 -"],
  ] as const)("keeps the task but suppresses stale numeric details for %s %s", (locale, gcodeState, label, layers, remaining) => {
    renderStatus({ ...basePrint, gcode_state: gcodeState }, "running", locale);

    expect(screen.getByText(label)).toBeVisible();
    expect(screen.getByText("Benchy")).toBeVisible();
    expect(screen.getByText("-")).toBeVisible();
    expect(screen.getByText(layers)).toBeVisible();
    expect(screen.getByText(remaining)).toBeVisible();
    expect(screen.queryByText("42%")).not.toBeInTheDocument();
    expect(screen.queryByText("Layers 12/100")).not.toBeInTheDocument();
    expect(screen.queryByText("Remaining 1h 5m")).not.toBeInTheDocument();
  });

  it("renders useful one-sided layer values", () => {
    const rendered = renderStatus({ ...basePrint, total_layers: null });
    expect(screen.getByText("Layers 12")).toBeVisible();

    rendered.rerender(
      <NextIntlClientProvider locale="en" messages={en}>
        <PrinterPrintStatus
          coarseStatus="running"
          print={{ ...basePrint, current_layer: null, total_layers: 100 }}
        />
      </NextIntlClientProvider>,
    );
    expect(screen.getByText("Layers -/100")).toBeVisible();
  });

  it.each([
    ["IDLE", "Idle"],
    ["OFFLINE", "Offline"],
    ["FAILED", "Failed"],
  ])(
    "lets coarse %s veto stale live and finished content",
    (coarseStatus, displayLabel) => {
      renderStatus(
        { ...basePrint, gcode_state: "FINISH", progress_percent: 100 },
        coarseStatus,
      );

      expect(screen.getByText(displayLabel)).toBeVisible();
      expect(screen.queryByText("Finished")).not.toBeInTheDocument();
      expect(screen.queryByText("Benchy")).not.toBeInTheDocument();
      expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
    },
  );

  it("falls back to the existing coarse status when enriched state is absent or unknown", () => {
    const rendered = renderStatus(null, "READY");
    expect(screen.getByText("Ready")).toBeVisible();

    rendered.rerender(
      <NextIntlClientProvider locale="en" messages={en}>
        <PrinterPrintStatus
          coarseStatus="RUNNING"
          print={{ ...basePrint, gcode_state: null, progress_percent: null }}
        />
      </NextIntlClientProvider>,
    );
    expect(screen.getByText("Running")).toBeVisible();
    expect(screen.queryByText("0%")).not.toBeInTheDocument();
  });

  it("does not widen the exact display aliases with case normalization", () => {
    renderStatus({ ...basePrint, gcode_state: "printing" }, "RUNNING");

    expect(screen.getByText("Running")).toBeVisible();
    expect(screen.queryByText("Printing")).not.toBeInTheDocument();
    expect(screen.queryByText("Benchy")).not.toBeInTheDocument();
  });
});
