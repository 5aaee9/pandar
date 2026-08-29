import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { PrinterInventory } from "./dashboard-inventory";
import type { Printer } from "./dashboard-types";
import {
  CameraRouteHarness,
  agent,
  printer,
  renderWithMessages,
  tenant,
} from "./dashboard-inventory.test.context";

describe("PrinterInventory", () => {
  it("renders printer temperatures and controls in separate sections", async () => {
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
    await waitFor(() => expect(card).toHaveTextContent("Nozzle"));
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

  it("replaces pause with resume when the live print task is paused", async () => {
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

    const resumeButton = await screen.findByRole("button", { name: "恢复打印" });
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

    await user.click(await screen.findByRole("button", { name: "View camera" }));

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

    await user.click(await screen.findByRole("button", { name: "View camera" }));
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

    await user.click(await screen.findByRole("button", { name: "View camera" }));
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
});
