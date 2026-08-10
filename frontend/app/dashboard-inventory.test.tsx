import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useState } from "react";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

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

function renderWithMessages(children: React.ReactNode, locale = "en") {
  return render(
    <NextIntlClientProvider locale={locale} messages={locale === "zh" ? zh : en}>
      <QueryClientProvider client={new QueryClient()}>
        <DashboardCameraProvider>{children}</DashboardCameraProvider>
      </QueryClientProvider>
    </NextIntlClientProvider>,
  );
}

const tenant: Tenant = {
  id: "tenant-1",
  slug: "acme",
  display_name: "Acme Labs",
  created_at: "2026-07-02T00:00:00Z",
};

const agent: Agent = {
  id: "agent-1",
  tenant_id: tenant.id,
  name: "Shop Agent",
  status: "online",
  created_at: "2026-07-02T00:00:00Z",
};

const printer: Printer = {
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

const printerWithMaterials: Printer = {
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

function CameraRouteHarness() {
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

describe("PrinterInventory", () => {
  it("renders enriched native print details in the existing card summary", () => {
    const livePrinter: Printer = {
      ...printerWithMaterials,
      status: "RUNNING",
      print: {
        task_generation: 3,
        error_generation: 0,
        hms: [],
        job_state: 0,
        gcode_state: "RUNNING",
        task_id: "task-1",
        subtask_id: "subtask-1",
        subtask_name: "Live Benchy",
        gcode_file: "/cache/plate_1.gcode.3mf",
        progress_percent: 37,
        speed_level: 2,
        remaining_time_minutes: 65,
        current_layer: 12,
        total_layers: 100,
        print_error: 0,
        printer_job_id: "native-job",
      },
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[livePrinter]} agents={[agent]} nowMs={0} />,
    );

    const status = screen.getByTestId("printer-print-status");
    expect(status).toHaveTextContent("Printing");
    expect(status).toHaveTextContent("Live Benchy");
    expect(status).toHaveTextContent("37%");
    expect(status).toHaveTextContent("Layers 12/100");
    expect(status).toHaveTextContent("Remaining 1h 5m");
    expect(screen.getByRole("article", { name: "Office A1" })).toHaveTextContent("AMS-A");
  });

  it("renders a persistent inline mismatch warning on the affected card", () => {
    const mismatchPrinter: Printer = {
      ...printer,
      status: "RUNNING",
      serial_number: "20P123",
      print: {
        task_generation: 1,
        error_generation: 9,
        hms: [],
        job_state: 0,
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
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[mismatchPrinter]} agents={[agent]} nowMs={0} />,
    );

    const card = screen.getByRole("article", { name: "Office A1" });
    expect(within(card).getByText("Build plate mismatch")).toBeVisible();
    expect(
      within(card).getByRole("button", { name: "Review build plate mismatch for Office A1" }),
    ).toBeVisible();
  });

  it("renders inventory content without the tenant subtitle or reported count", () => {
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printer]} agents={[agent]} nowMs={0} />,
    );

    expect(screen.getByRole("heading", { name: "Printer inventory" })).toBeVisible();
    expect(screen.queryByText("Acme Labs (acme)")).not.toBeInTheDocument();
    expect(screen.queryByText("1 reported")).not.toBeInTheDocument();
  });

  it("renders printers as individual machine cards", () => {
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printer]} agents={[agent]} nowMs={0} />,
    );

    const card = screen.getByRole("article", { name: "Office A1" });
    expect(card).toBeVisible();
    expect(card).toHaveTextContent("A1");
    expect(card).toHaveTextContent("SERIAL123");
    expect(card).toHaveTextContent("Shop Agent");
    expect(card).not.toHaveTextContent("Managed by");
    expect(screen.queryByText("Managed by")).not.toBeInTheDocument();
  });

  it("places the managing agent chip beside the summary status badge", () => {
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printer]} agents={[agent]} nowMs={0} />,
    );

    const chip = screen.getByText("Shop Agent").parentElement;
    expect(chip?.parentElement).toHaveTextContent("Idle");
  });

  it("opens a printer actions popover with delete", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printer]} agents={[agent]} nowMs={0} />,
    );

    await user.click(screen.getByRole("button", { name: "Actions for Office A1" }));

    expect(screen.getByRole("button", { name: "Edit printer" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Delete printer" })).toBeVisible();
  });

  it("opens an edit printer dialog from the printer actions popover", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printer]} agents={[agent]} nowMs={0} />,
    );

    await user.click(screen.getByRole("button", { name: "Actions for Office A1" }));
    await user.click(screen.getByRole("button", { name: "Edit printer" }));

    expect(screen.getByRole("dialog")).toBeVisible();
    expect(screen.getByRole("heading", { name: "Edit printer" })).toBeVisible();
    expect(screen.getByLabelText("Name")).toHaveValue("Office A1");
    expect(screen.getByLabelText("Printer IPv4 address")).toHaveAttribute("name", "host");
    expect(screen.getByLabelText("Printer IPv4 address")).not.toBeRequired();
    expect(screen.getByLabelText("Access code")).toHaveAttribute("name", "access_code");
    expect(screen.getByLabelText("Access code")).not.toBeRequired();

    const form = screen.getByRole("button", { name: "Save changes" }).closest("form");
    expect(form?.querySelector('input[name="tenant_id"]')).toHaveValue("tenant-1");
    expect(form?.querySelector('input[name="printer_id"]')).toHaveValue("printer-1");
  });

  it("opens the machine form from the empty printer state", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[]} agents={[agent]} nowMs={0} />,
    );

    expect(screen.getByText("No printers reported")).toBeVisible();

    const trigger = screen.getByRole("button", { name: "Link printer" });
    expect(trigger).toHaveAttribute("data-slot", "dialog-trigger");

    await user.click(trigger);

    const dialog = screen.getByRole("dialog", { name: "Link printer to agent" });
    const form = within(dialog)
      .getByRole("button", { name: "Link printer" })
      .closest("form")!;
    expect(form).toHaveFormValues({
      tenant_id: tenant.id,
      agent_id: agent.id,
      type: "BambuLab",
      host: "",
      name: "",
      access_code: "",
    });

    await user.type(within(dialog).getByLabelText("Access code"), "SECRET-LINK-CODE");
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    await user.click(trigger);

    expect(screen.getByLabelText("Access code")).toHaveValue("");
  });

  it("renders AMS refresh inside the printer actions popover with tenant and printer ids", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printer]} agents={[agent]} nowMs={0} />,
    );

    expect(screen.queryByRole("button", { name: "Refresh AMS" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Actions for Office A1" }));

    const button = screen.getByRole("button", { name: "Refresh AMS" });
    const form = button.closest("form");

    expect(form).not.toBeNull();
    expect(form?.querySelector('input[name="tenant_id"]')).toHaveValue("tenant-1");
    expect(form?.querySelector('input[name="printer_id"]')).toHaveValue("printer-1");
  });

  it("renders printer temperatures and controls in separate sections", () => {
    const heatingPrinter: Printer = {
      ...printer,
      nozzle_temperatures: [
        { label: "L", current_celsius: "41", target_celsius: "220" },
        { label: "R", current_celsius: "42", target_celsius: "230" },
      ],
      active_nozzle: "R",
      bed_temperature_celsius: "60",
      chamber_temperature_celsius: "32",
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[heatingPrinter]} agents={[agent]} nowMs={0} />,
    );

    const card = screen.getByRole("article", { name: "Office A1" });
    expect(card).toHaveTextContent("Nozzle");
    expect(card).toHaveTextContent("L / R");
    expect(card).toHaveTextContent("41° / 42°");
    expect(card).toHaveTextContent("Bed");
    expect(card).toHaveTextContent("60°C");
    expect(card).toHaveTextContent("Chamber");
    expect(card).toHaveTextContent("32°C");
    expect(card).toHaveTextContent("Controls");

    const cardText = card.textContent ?? "";
    expect(cardText.indexOf("Controls")).toBeGreaterThan(cardText.indexOf("Status"));
    expect(cardText.indexOf("Controls")).toBeLessThan(cardText.indexOf("Filaments"));

    const controls = screen.getByRole("group", { name: "Controls" });
    expect(controls).toHaveClass("grid-cols-2");
    expect(controls).not.toHaveClass("sm:grid-cols-1");

    const stopForm = screen.getByRole("button", { name: "Stop print" }).closest("form");
    const pauseForm = screen.getByRole("button", { name: "Pause print" }).closest("form");
    const lightForm = screen.getByRole("button", { name: "Light Off" }).closest("form");
    expect(screen.getByRole("button", { name: "View camera" })).toBeVisible();
    expect(within(card).getByRole("button", { name: "Move axes" })).toBeVisible();
    expect(stopForm?.querySelector('input[name="action"]')).toHaveValue("stop");
    expect(stopForm?.querySelector('input[name="printer_id"]')).toHaveValue("printer-1");
    expect(pauseForm?.querySelector('input[name="action"]')).toHaveValue("pause");
    expect(lightForm?.querySelector('input[name="action"]')).toHaveValue("set_chamber_light");
    expect(lightForm?.querySelector('input[name="light_on"]')).toHaveValue("true");
  });

  it("replaces pause with resume when the live print task is paused", () => {
    const pausedPrinter: Printer = {
      ...printer,
      status: "RUNNING",
      print: {
        task_generation: 3,
        error_generation: 0,
        hms: [],
        job_state: 0,
        gcode_state: "PAUSE",
        task_id: "task-1",
        subtask_id: "subtask-1",
        subtask_name: "Paused Benchy",
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

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[pausedPrinter]} agents={[agent]} nowMs={0} />,
      "zh",
    );

    const resumeButton = screen.getByRole("button", { name: "恢复打印" });
    const resumeForm = resumeButton.closest("form");
    expect(resumeForm?.querySelector('input[name="action"]')).toHaveValue("resume");
    expect(screen.queryByRole("button", { name: "暂停打印" })).not.toBeInTheDocument();
  });

  it("opens camera video using the MP4 stream route with custom controls", async () => {
    const user = userEvent.setup();
    const heatingPrinter: Printer = {
      ...printer,
      nozzle_temperatures: [{ label: null, current_celsius: "27", target_celsius: "0" }],
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[heatingPrinter]} agents={[agent]} nowMs={0} />,
    );

    await user.click(screen.getByRole("button", { name: "View camera" }));

    const video = document.querySelector("video");
    expect(video).not.toBeNull();
    expect(video?.getAttribute("aria-label")).toBe("Camera");
    expect(video).toHaveAttribute(
      "src",
      "/api/tenants/tenant-1/printers/printer-1/camera.mp4",
    );
    expect(video).not.toHaveAttribute("controls");
    expect(screen.getByRole("button", { name: "Full screen" })).toBeVisible();
  });

  it("keeps the camera stream mounted while picture in picture is active", async () => {
    const requestPictureInPicture = vi.fn().mockResolvedValue({});
    Object.defineProperty(document, "pictureInPictureEnabled", {
      configurable: true,
      value: true,
    });
    Object.defineProperty(HTMLVideoElement.prototype, "requestPictureInPicture", {
      configurable: true,
      value: requestPictureInPicture,
    });

    const user = userEvent.setup();
    const heatingPrinter: Printer = {
      ...printer,
      nozzle_temperatures: [{ label: null, current_celsius: "27", target_celsius: "0" }],
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[heatingPrinter]} agents={[agent]} nowMs={0} />,
    );

    await user.click(screen.getByRole("button", { name: "View camera" }));
    const video = document.querySelector("video");
    expect(video).not.toBeNull();

    await user.click(screen.getByRole("button", { name: "Picture in picture" }));

    await waitFor(() => expect(requestPictureInPicture).toHaveBeenCalledOnce());
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(video?.isConnected).toBe(true);
    expect(screen.getByRole("button", { name: "Light Off" })).toBeEnabled();

    act(() => video?.dispatchEvent(new Event("leavepictureinpicture")));
    await waitFor(() => expect(video?.isConnected).toBe(false));

    Reflect.deleteProperty(document, "pictureInPictureEnabled");
    Reflect.deleteProperty(HTMLVideoElement.prototype, "requestPictureInPicture");
  });

  it("keeps picture in picture mounted when the devices page unmounts", async () => {
    const requestPictureInPicture = vi.fn().mockResolvedValue({});
    Object.defineProperty(document, "pictureInPictureEnabled", {
      configurable: true,
      value: true,
    });
    Object.defineProperty(HTMLVideoElement.prototype, "requestPictureInPicture", {
      configurable: true,
      value: requestPictureInPicture,
    });

    const user = userEvent.setup();
    renderWithMessages(<CameraRouteHarness />);

    await user.click(screen.getByRole("button", { name: "View camera" }));
    const video = document.querySelector("video");
    await user.click(screen.getByRole("button", { name: "Picture in picture" }));
    await waitFor(() => expect(requestPictureInPicture).toHaveBeenCalledOnce());

    await user.click(screen.getByRole("button", { name: "Go to jobs" }));

    expect(screen.getByText("Jobs page")).toBeVisible();
    expect(screen.queryByRole("button", { name: "View camera" })).not.toBeInTheDocument();
    expect(video?.isConnected).toBe(true);

    act(() => video?.dispatchEvent(new Event("leavepictureinpicture")));
    await waitFor(() => expect(video?.isConnected).toBe(false));

    Reflect.deleteProperty(document, "pictureInPictureEnabled");
    Reflect.deleteProperty(HTMLVideoElement.prototype, "requestPictureInPicture");
  });

  it("sends explicit light-off controls when chamber light is on", () => {
    const heatingPrinter: Printer = {
      ...printer,
      nozzle_temperatures: [{ label: null, current_celsius: "27", target_celsius: "0" }],
      chamber_light_on: true,
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[heatingPrinter]} agents={[agent]} nowMs={0} />,
    );

    const lightForm = screen.getByRole("button", { name: "Light On" }).closest("form");
    expect(lightForm?.querySelector('input[name="action"]')).toHaveValue("set_chamber_light");
    expect(lightForm?.querySelector('input[name="light_on"]')).toHaveValue("false");
  });

  it("localizes the light control label in Chinese", () => {
    const heatingPrinter: Printer = {
      ...printer,
      nozzle_temperatures: [{ label: null, current_celsius: "27", target_celsius: "0" }],
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[heatingPrinter]} agents={[agent]} nowMs={0} />,
      "zh",
    );

    expect(screen.getByRole("button", { name: "灯光 已关闭" })).toBeVisible();
  });

  it("keeps the active nozzle switch in the temperature grid with nozzle details on separate lines", () => {
    const dualNozzlePrinter: Printer = {
      ...printerWithMaterials,
      nozzle_temperatures: [
        {
          label: "L",
          current_celsius: "41",
          target_celsius: "220",
          diameter_mm: "0.4",
          nozzle_type: "HH05",
        },
        {
          label: "R",
          current_celsius: "42",
          target_celsius: "230",
          diameter_mm: "0.4",
          nozzle_type: "HH05",
        },
      ],
      active_nozzle: "R",
      bed_temperature_celsius: "60",
      chamber_temperature_celsius: "32",
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[dualNozzlePrinter]} agents={[agent]} nowMs={0} />,
    );

    const switchButton = screen.getByRole("button", { name: "Switch to nozzle L" });
    const switchForm = switchButton.closest("form");
    const temperatureGrid = switchForm?.parentElement;
    expect(temperatureGrid).toHaveClass("grid-cols-2", "lg:grid-cols-4");
    expect(switchForm).not.toHaveClass("col-span-3");
    expect(switchForm).toHaveClass("h-full");
    expect(switchButton).toHaveClass("h-full");
    expect(switchForm?.querySelector('input[name="action"]')).toHaveValue("select_extruder");
    expect(switchForm?.querySelector('input[name="printer_id"]')).toHaveValue("printer-1");
    expect(switchForm?.querySelector('input[name="extruder_id"]')).toHaveValue("1");
    expect(switchButton).toHaveTextContent("L");
    expect(switchButton).toHaveTextContent("R");
    expect(switchButton).toHaveTextContent("0.4 mm");
    const diameters = within(switchButton).getAllByText("0.4 mm");
    expect(diameters).toHaveLength(2);
    for (const diameter of diameters) {
      expect(diameter.parentElement).toHaveClass("flex-col");
      expect(diameter.nextElementSibling).toHaveTextContent("HH05");
    }
    expect(within(switchButton).getByText("R").parentElement?.parentElement).toHaveClass("text-primary");
  });

  it("renders a single nozzle without a duplicate label or target temperature", () => {
    const heatingPrinter: Printer = {
      ...printer,
      nozzle_temperatures: [{ label: null, current_celsius: "27", target_celsius: "0" }],
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[heatingPrinter]} agents={[agent]} nowMs={0} />,
    );

    const card = screen.getByRole("article", { name: "Office A1" });
    expect(card).toHaveTextContent("Nozzle");
    expect(card).toHaveTextContent("27°");
    expect(card).not.toHaveTextContent("Nozzle Nozzle");
    expect(card).not.toHaveTextContent("27° / 0°");
  });

  it("opens a single-nozzle temperature menu with preset controls", async () => {
    const user = userEvent.setup();
    const heatingPrinter: Printer = {
      ...printer,
      nozzle_temperatures: [{ label: null, current_celsius: "27", target_celsius: "220" }],
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[heatingPrinter]} agents={[agent]} nowMs={0} />,
    );

    await user.click(screen.getByRole("button", { name: "Set nozzle temperature" }));

    expect(screen.getByText("Set nozzle temperature")).toBeVisible();
    expect(screen.getByText("Current 27°C")).toBeVisible();
    expect(screen.getByText("Target 220°C")).toBeVisible();
    const preset = screen.getByRole("button", { name: "220 C" });
    const form = preset.closest("form");
    expect(form?.querySelector('input[name="action"]')).toHaveValue("set_hotend_temperature");
    expect(form?.querySelector('input[name="temperature_celsius"]')).toHaveValue("220");
    expect(form?.querySelector('input[name="extruder_id"]')).toBeNull();
    expect(screen.getByPlaceholderText("Custom")).toBeVisible();
  });

  it("opens dual-nozzle temperature controls with active nozzle highlighted", async () => {
    const user = userEvent.setup();
    const dualNozzlePrinter: Printer = {
      ...printer,
      nozzle_temperatures: [
        { label: "L", current_celsius: "41", target_celsius: "220" },
        { label: "R", current_celsius: "42", target_celsius: "0" },
      ],
      active_nozzle: "R",
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[dualNozzlePrinter]} agents={[agent]} nowMs={0} />,
    );

    await user.click(screen.getByRole("button", { name: "Set nozzle temperatures" }));

    expect(screen.getByText("Set nozzle temperatures")).toBeVisible();
    const rightPanel = screen.getByText("Right temp").closest("div");
    expect(rightPanel).toHaveClass("border-primary");

    const rightOff = within(rightPanel!).getByRole("button", { name: "Off" });
    const rightForm = rightOff.closest("form");
    expect(rightForm?.querySelector('input[name="action"]')).toHaveValue("set_hotend_temperature");
    expect(rightForm?.querySelector('input[name="temperature_celsius"]')).toHaveValue("0");
    expect(rightForm?.querySelector('input[name="extruder_id"]')).toHaveValue("0");

    const leftPanel = screen.getByText("Left temp").closest("div");
    expect(within(leftPanel!).getByText("Current 41°C")).toBeVisible();
    expect(within(leftPanel!).getByText("Target 220°C")).toBeVisible();
    expect(within(leftPanel!).getAllByText(/41°C/)).toHaveLength(1);
    const leftPreset = within(leftPanel!).getByRole("button", { name: "260 C" });
    expect(leftPreset.closest("form")?.querySelector('input[name="extruder_id"]')).toHaveValue("1");

    expect(within(rightPanel!).getByText("Current 42°C")).toBeVisible();
    expect(within(rightPanel!).queryByText(/Target/)).not.toBeInTheDocument();
    expect(within(rightPanel!).getAllByText(/42°C/)).toHaveLength(1);
  });

  it("opens bed temperature controls with bed presets", async () => {
    const user = userEvent.setup();
    const heatingPrinter: Printer = {
      ...printer,
      bed_temperature_celsius: "24",
      bed_target_temperature_celsius: "75",
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[heatingPrinter]} agents={[agent]} nowMs={0} />,
    );

    await user.click(screen.getByRole("button", { name: "Set bed temperature" }));

    expect(screen.getByText("Set bed temperature")).toBeVisible();
    expect(screen.getByText("Current 24°C")).toBeVisible();
    expect(screen.getByText("Target 75°C")).toBeVisible();
    const preset = screen.getByRole("button", { name: "75 C" });
    const form = preset.closest("form");
    expect(form?.querySelector('input[name="action"]')).toHaveValue("set_bed_temperature");
    expect(form?.querySelector('input[name="temperature_celsius"]')).toHaveValue("75");
    expect(screen.getByPlaceholderText("Custom")).toBeVisible();
  });

  it("opens chamber temperature controls with chamber presets", async () => {
    const user = userEvent.setup();
    const heatingPrinter: Printer = {
      ...printer,
      chamber_temperature_celsius: "25",
      chamber_target_temperature_celsius: "45",
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[heatingPrinter]} agents={[agent]} nowMs={0} />,
    );

    await user.click(screen.getByRole("button", { name: "Set chamber temperature" }));

    expect(screen.getByText("Set chamber temperature")).toBeVisible();
    expect(screen.getByText("Current 25°C")).toBeVisible();
    expect(screen.getByText("Target 45°C")).toBeVisible();
    const preset = screen.getByRole("button", { name: "45 C" });
    const form = preset.closest("form");
    expect(form?.querySelector('input[name="action"]')).toHaveValue("set_chamber_temperature");
    expect(form?.querySelector('input[name="temperature_celsius"]')).toHaveValue("45");
    expect(screen.getByPlaceholderText("Custom")).toBeVisible();
  });

  it("hides zero bed target temperature from the card and menu", async () => {
    const user = userEvent.setup();
    const heatingPrinter: Printer = {
      ...printer,
      bed_temperature_celsius: "26",
      bed_target_temperature_celsius: "0",
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[heatingPrinter]} agents={[agent]} nowMs={0} />,
    );

    const card = screen.getByRole("article", { name: "Office A1" });
    expect(card).toHaveTextContent("Bed");
    expect(card).toHaveTextContent("26°C");
    expect(card).not.toHaveTextContent("26° / 0°");

    await user.click(screen.getByRole("button", { name: "Set bed temperature" }));

    expect(screen.getByText("Current 26°C")).toBeVisible();
    expect(screen.queryByText(/^Target /)).not.toBeInTheDocument();
  });

  it("replaces the filament summary with AMS and external slot loading details", () => {
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printerWithMaterials]} agents={[agent]} nowMs={0} />,
    );

    const card = screen.getByRole("article", { name: "Office A1" });
    expect(card).toHaveTextContent("AMS-A");
    expect(card).toHaveTextContent("1%");
    expect(card).toHaveTextContent("24.0°C");
    expect(card).toHaveTextContent("R");
    expect(card).toHaveTextContent("PLA");
    expect(card).toHaveTextContent("PETG");
    expect(card).toHaveTextContent("External");
    expect(card).toHaveTextContent("TPU");
    expect(card).not.toHaveTextContent("8 AMS trays");
  });

  it("formats active AMS slots as a unit letter and one-based tray position", () => {
    const activePrinter: Printer = {
      ...printerWithMaterials,
      materials: {
        ...printerWithMaterials.materials!,
        active_tray: {
          kind: "ams",
          ams_id: "0",
          tray_id: "2",
          global_tray_id: 2,
        },
      },
    };

    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[activePrinter]} agents={[agent]} nowMs={0} />,
    );

    const card = screen.getByRole("article", { name: "Office A1" });
    expect(card).toHaveTextContent("AMS A - 3");
    expect(card).not.toHaveTextContent("AMS 0:2");
  });

  it("opens an AMS slot popover on click with RFID, load, and unload operations", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[printerWithMaterials]} agents={[agent]} nowMs={0} />,
    );

    await user.click(
      screen.getByRole("button", { name: "AMS-A slot 2, PETG, Active, Remaining: 42%" }),
    );

    expect(screen.getByText("#FFA726")).toBeVisible();
    expect(screen.getByText("42%")).toBeVisible();
    expect(screen.getByRole("button", { name: "Re-read RFID" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Load" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Unload" })).toBeVisible();

    const rereadForm = screen.getByRole("button", { name: "Re-read RFID" }).closest("form");
    const loadForm = screen.getByRole("button", { name: "Load" }).closest("form");
    expect(rereadForm?.querySelector('input[name="global_tray_id"]')).toBeNull();
    expect(loadForm?.querySelector('input[name="global_tray_id"]')).toHaveValue("1");
    expect(loadForm?.querySelector('input[name="extruder_id"]')).toHaveValue("0");
  });

  it("renders unsupported AMS remaining estimates as unsupported with a gray progress bar", async () => {
    const user = userEvent.setup();
    const unsupportedPrinter: Printer = {
      ...printerWithMaterials,
      materials: {
        ...printerWithMaterials.materials!,
        ams_units: [
          {
            ...printerWithMaterials.materials!.ams_units[0],
            trays: [
              {
                ...printerWithMaterials.materials!.ams_units[0].trays![0],
                remaining_estimate: "-1",
              },
              printerWithMaterials.materials!.ams_units[0].trays![1],
            ],
          },
        ],
      },
    };
    renderWithMessages(
      <PrinterInventory selectedTenant={tenant} printers={[unsupportedPrinter]} agents={[agent]} nowMs={0} />,
    );

    await user.click(
      screen.getByRole("button", { name: "AMS-A slot 1, PLA, Remaining: Unsupported" }),
    );

    expect(screen.getByText("Unsupported")).toBeVisible();
    expect(screen.queryByText("-1%")).not.toBeInTheDocument();
  });

  it("uses the correct Chinese copy for external spool", () => {
    expect(zh.material.externalSpool).toContain("料盘");
    expect(zh.material.externalSpool).not.toContain("盘子");
  });
});
