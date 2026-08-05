import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import { DispatchForm } from "./dispatch-form";
import type { Job } from "./dashboard-types";

function createTestQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
}

function renderDispatchForm(props: Parameters<typeof DispatchForm>[0]) {
  return render(
    <QueryClientProvider client={createTestQueryClient()}>
      <NextIntlClientProvider locale="en" messages={en}>
        <DispatchForm {...props} />
      </NextIntlClientProvider>
    </QueryClientProvider>,
  );
}

describe("DispatchForm", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("matches Bambu Studio print option modes and defaults", async () => {
    const user = userEvent.setup();
    const { container } = renderDispatchForm({
      selectedTenant: { id: "tenant-1" },
      printers: [{ id: "printer-1", name: "Printer One", serial_number: "SN1", model: "N6", materials: null }],
    });

    expect(screen.getByRole("region", { name: "Print options" })).toBeVisible();
    const timelapse = within(screenGroup(container, "Timelapse"));
    const timelapseOn = timelapse.getByRole("radio", { name: "On" });
    expect(timelapseOn).toBeChecked();
    expect(timelapseOn.nextElementSibling?.querySelector("svg")).toBeInTheDocument();
    expect(
      timelapse.getByRole("radio", { name: "Off" }).nextElementSibling?.querySelector("svg"),
    ).not.toBeInTheDocument();

    const bedLeveling = within(screenGroup(container, "Auto bed leveling"));
    expect(bedLeveling.getByRole("radio", { name: "Auto" })).toBeChecked();

    const flowCalibration = within(
      screenGroup(container, "Flow dynamics calibration"),
    );
    expect(flowCalibration.getByRole("radio", { name: "Auto" })).toBeChecked();

    const nozzleOffset = within(
      screenGroup(container, "Nozzle offset calibration"),
    );
    expect(nozzleOffset.getByRole("radio", { name: "Off" })).toBeChecked();
    expect(container.querySelector('input[name="bed_leveling"]')).toHaveValue("false");
    expect(container.querySelector('input[name="flow_cali"]')).toHaveValue("false");
    expect(container.querySelector('input[type="checkbox"]')).toBeChecked();

    const form = container.querySelector("form") as HTMLFormElement;
    let formData = new FormData(form);
    expect(formData.get("timelapse")).toBe("true");
    expect(formData.get("bed_leveling")).toBe("false");
    expect(formData.get("auto_bed_leveling")).toBe("2");
    expect(formData.get("flow_cali")).toBe("false");
    expect(formData.get("auto_flow_cali")).toBe("2");
    expect(formData.get("auto_offset_cali")).toBe("0");

    expect(formData.get("use_ams")).toBe("true");
    await user.click(flowCalibration.getByRole("radio", { name: "On" }));
    await user.click(timelapse.getByRole("radio", { name: "Off" }));
    formData = new FormData(form);
    expect(formData.get("flow_cali")).toBe("true");
    expect(formData.get("auto_flow_cali")).toBe("1");
    expect(formData.get("timelapse")).toBe("false");
  });
  it("resets modes when the selected printer profile changes", async () => {
    const user = userEvent.setup();
    const { container } = renderDispatchForm({
      selectedTenant: { id: "tenant-1" },
      printers: [
        {
          id: "x2d",
          name: "X2D",
          serial_number: "SN-X2D",
          model: "N6",
          materials: null,
        },
        {
          id: "a1",
          name: "A1",
          serial_number: "SN-A1",
          model: "A1",
          materials: null,
        },
      ],
    });

    expect(
      within(screenGroup(container, "Flow dynamics calibration")).getByRole(
        "radio",
        { name: "Auto" },
      ),
    ).toBeChecked();

    await user.selectOptions(
      container.querySelector('select[name="printer_id"]') as HTMLSelectElement,
      "a1",
    );

    const flow = within(screenGroup(container, "Flow dynamics calibration"));
    expect(flow.queryByRole("radio", { name: "Auto" })).not.toBeInTheDocument();
    expect(flow.getByRole("radio", { name: "On" })).toBeChecked();
    expect(
      Array.from(container.querySelectorAll("fieldset")).some(
        (field) => field.querySelector("legend")?.textContent === "Nozzle offset calibration",
      ),
    ).toBe(false);

    const formData = new FormData(container.querySelector("form") as HTMLFormElement);
    expect(formData.get("bed_leveling")).toBe("true");
    expect(formData.get("auto_bed_leveling")).toBe("1");
    expect(formData.get("flow_cali")).toBe("true");
    expect(formData.get("auto_flow_cali")).toBe("1");
    expect(formData.get("auto_offset_cali")).toBe("0");
  });

  it("submits conservative safe-off values for an unknown printer model", () => {
    const { container } = renderDispatchForm({
      selectedTenant: { id: "tenant-1" },
      printers: [
        {
          id: "unknown",
          name: "Unknown",
          serial_number: "SN-UNKNOWN",
          model: null,
          materials: null,
        },
      ],
    });

    expect(container.querySelectorAll("fieldset")).toHaveLength(0);
    expect(container.querySelector("section")).not.toBeInTheDocument();
    const formData = new FormData(container.querySelector("form") as HTMLFormElement);
    expect(formData.get("timelapse")).toBe("false");
    expect(formData.get("bed_leveling")).toBe("false");
    expect(formData.get("auto_bed_leveling")).toBe("0");
    expect(formData.get("flow_cali")).toBe("false");
    expect(formData.get("auto_flow_cali")).toBe("0");
    expect(formData.get("auto_offset_cali")).toBe("0");
  });

  it("redirects dispatch results to jobs", async () => {
    const user = userEvent.setup();
    const onRedirect = vi.fn();
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        new Response(JSON.stringify({}), {
          status: 200,
          headers: { "content-type": "application/json" },
        }),
      ),
    );

    const { container } = renderDispatchForm({
      selectedTenant: { id: "tenant-1" },
      printers: [{ id: "printer-1", name: "Printer One", serial_number: "SN1", model: "N6", materials: null }],
      onRedirect,
    });
    const fileInput = container.querySelector('input[type="file"]');
    expect(fileInput).toBeInstanceOf(HTMLInputElement);
    expect(container.querySelector('[name="plate_id"]')).toBeNull();

    await user.upload(
      fileInput as HTMLInputElement,
      new File(["3mf"], "benchy.3mf", { type: "model/3mf" }),
    );
    await waitFor(() =>
      expect(container.querySelector('input[name="plate_id"]')).toHaveValue(1),
    );
    const form = container.querySelector("form");
    expect(form).toBeInstanceOf(HTMLFormElement);
    fireEvent.submit(form as HTMLFormElement);

    await waitFor(() =>
      expect(onRedirect).toHaveBeenCalledWith("/jobs?status=job_created"),
    );
  });

  it("reprints a selected artifact while keeping printer, plate, mapping, and options editable", async () => {
    const user = userEvent.setup();
    const onRedirect = vi.fn();
    const fetchMock = vi.fn(
      async (_input: RequestInfo | URL, _init?: RequestInit) =>
        Response.json({}, { status: 201 }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const sourceJob = {
      id: "job-1",
      printer_id: "printer-2",
      artifact: {
        filename: "benchy.3mf",
        content_type: "model/3mf",
        size_bytes: 42,
        metadata: {
          display_name: "Benchy",
          default_plate_id: 2,
          warnings: [],
          plates: [{
            plate_id: 2,
            name: "Plate 2",
            estimated_time_seconds: null,
            filament_weight_grams: null,
            object_count: 1,
            objects: ["benchy"],
            filaments: [{
              filament_id: "GFA00",
              tray_info_idx: "GFA00",
              nozzle_id: 0,
              filament_type: "PLA",
              color: "#00FF00",
              used_grams: 10,
              used_meters: 3,
            }],
            has_thumbnail: false,
          }],
        },
      },
    } as unknown as Job;

    const { container } = renderDispatchForm({
      selectedTenant: { id: "tenant-1" },
      onRedirect,
      sourceJob,
      printers: [
        { id: "printer-1", name: "Printer One", serial_number: "SN1", model: "N6", materials: null },
        {
          id: "printer-2",
          name: "Printer Two",
          serial_number: "SN2",
          model: "N6",
          materials: {
            filament_switch_installed: true,
            observed_at: "2026-07-18T00:00:00Z",
            active_tray: null,
            ams_units: [{
              unit_id: "0",
              toolhead: "R",
              trays: [{
                tray_id: "0",
                global_tray_id: 0,
                type: "PLA",
                color: "00FF00",
                exists: true,
              }],
            }],
            external_spools: [],
          },
        },
      ],
    });

    expect(container.querySelector('input[type="file"]')).toBeNull();
    expect(screen.getByText("benchy.3mf")).toBeVisible();
    expect(container.querySelector('select[name="printer_id"]')).toHaveValue("printer-2");
    expect(container.querySelector('select[name="plate_id"]')).toHaveValue("2");
    expect(container.querySelector('input[name="ams_mapping"]')).toHaveValue("[0]");
    expect(screen.getByRole("button", { name: "Reprint" })).toBeEnabled();

    await user.selectOptions(
      container.querySelector('select[name="printer_id"]') as HTMLSelectElement,
      "printer-1",
    );
    await user.selectOptions(
      container.querySelector('select[name="printer_id"]') as HTMLSelectElement,
      "printer-2",
    );
    await user.click(screen.getByRole("button", { name: "Reprint" }));

    await waitFor(() => expect(fetchMock).toHaveBeenCalledOnce());
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe("/api/tenants/tenant-1/jobs/job-1/reprint");
    expect(init).toMatchObject({
      method: "POST",
      headers: { "content-type": "application/json" },
    });
    expect(JSON.parse(String(init?.body))).toMatchObject({
      printer_id: "printer-2",
      plate_id: 2,
      use_ams: true,
      auto_bed_leveling: 2,
      auto_flow_cali: 2,
      auto_offset_cali: 0,
      timelapse: true,
      ams_mapping: [0],
      ams_mapping2: [{ ams_id: 0, slot_id: 0 }],
    });
    expect(onRedirect).toHaveBeenCalledWith(
      "/jobs?status=reprint_queued",
    );
  });

  it("offers parsed plate choices only after metadata is ready", async () => {
    const user = userEvent.setup();
    let resolvePreview!: (response: Response) => void;
    const previewResponse = new Promise<Response>((resolve) => {
      resolvePreview = resolve;
    });
    vi.stubGlobal("fetch", vi.fn(() => previewResponse));

    const { container, getByText } = renderDispatchForm({
      selectedTenant: { id: "tenant-1" },
      printers: [{ id: "printer-1", name: "Printer One", serial_number: "SN1", model: "N6", materials: null }],
    });

    expect(container.querySelector('[name="plate_id"]')).toBeNull();
    expect(container.querySelector('button[type="submit"]')).toBeDisabled();

    await user.upload(
      container.querySelector('input[type="file"]') as HTMLInputElement,
      new File(["3mf"], "project.3mf", { type: "model/3mf" }),
    );

    expect(container.querySelector('[name="plate_id"]')).toBeNull();
    expect(container.querySelector('button[type="submit"]')).toBeDisabled();

    resolvePreview(
      Response.json({
        metadata: {
          display_name: "project",
          default_plate_id: 7,
          warnings: [],
          plates: [
            {
              plate_id: 3,
              name: "Bracket",
              estimated_time_seconds: null,
              filament_weight_grams: null,
              object_count: 1,
              objects: ["bracket"],
              filaments: [],
              has_thumbnail: false,
            },
            {
              plate_id: 7,
              name: "Cover",
              estimated_time_seconds: null,
              filament_weight_grams: null,
              object_count: 1,
              objects: ["cover"],
              filaments: [],
              has_thumbnail: false,
            },
          ],
        },
      }),
    );

    const plateSelect = await waitFor(() => {
      const select = container.querySelector('select[name="plate_id"]');
      expect(select).toBeInstanceOf(HTMLSelectElement);
      return select;
    });
    expect(plateSelect).toHaveValue("7");
    expect(plateSelect).toHaveTextContent("3 · Bracket");
    expect(plateSelect).toHaveTextContent("7 · Cover");
    expect(getByText("cover")).toBeInTheDocument();
    expect(container.querySelector('button[type="submit"]')).toBeEnabled();

    await user.selectOptions(plateSelect as HTMLSelectElement, "3");
    expect(plateSelect).toHaveValue("3");
    expect(getByText("bracket")).toBeInTheDocument();
    expect(new FormData(container.querySelector("form") as HTMLFormElement).get("plate_id"))
      .toBe("3");
  });

  it("shows Studio-style routed material pickers with colors and allows valid overrides", async () => {
    const user = userEvent.setup();
    const onRedirect = vi.fn();
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        if (String(input).includes("artifact-metadata-preview")) {
          return Response.json({
            metadata: {
              display_name: "benchy",
              default_plate_id: 1,
              warnings: [],
              plates: [{
                plate_id: 1,
                name: "Plate 1",
                estimated_time_seconds: null,
                filament_weight_grams: null,
                object_count: 1,
                objects: ["benchy"],
                filaments: [
                  {
                    filament_id: "1",
                    tray_info_idx: "GFA00",
                    nozzle_id: 1,
                    filament_type: "PLA",
                    color: "#000000",
                    used_grams: 10,
                    used_meters: 3,
                  },
                  {
                    filament_id: "2",
                    tray_info_idx: "GFA01",
                    nozzle_id: 0,
                    filament_type: "PLA",
                    color: "#FF0000",
                    used_grams: 2,
                    used_meters: 1,
                  },
                ],
                has_thumbnail: false,
              }],
            },
          });
        }
        return Response.json({}, { status: 201 });
      }),
    );

    const { container, findByRole, getByRole, getByText } = renderDispatchForm({
      selectedTenant: { id: "tenant-1" },
      onRedirect,
      printers: [{
            id: "printer-1",
            name: "Printer One",
            serial_number: "SN1",
            model: "Bambu Lab X2D",
            materials: {
              filament_switch_installed: false,
              observed_at: "2026-07-15T00:00:00Z",
              active_tray: null,
              ams_units: [
                {
                  unit_id: "0",
                  trays: [{
                    tray_id: "0",
                    global_tray_id: 0,
                    type: "PLA",
                    color: "FF0000",
                    exists: true,
                  }],
                },
                {
                  unit_id: "1",
                  trays: [{
                    tray_id: "0",
                    global_tray_id: 4,
                    type: "PLA",
                    color: "000000",
                    exists: true,
                  }],
                },
                {
                  unit_id: "128",
                  unit_kind: "ams_ht",
                  toolhead: "R",
                  trays: [{
                    tray_id: "0",
                    type: "PETG",
                    color: "00FF00",
                    exists: true,
                  }],
                },
              ],
              external_spools: [
                {
                  external_id: "254",
                  tray_id: "0",
                  type: "PLA",
                  color: "00000000",
                  exists: true,
                },
                {
                  external_id: "255",
                  tray_id: "1",
                  type: "ABS",
                  color: "0000FF",
                  exists: true,
                }],
            },
          }],
    });

    await user.upload(
      container.querySelector('input[type="file"]') as HTMLInputElement,
      new File(["3mf"], "benchy.3mf", { type: "model/3mf" }),
    );

    const mainMapping = await findByRole("button", { name: "Map PLA (1)" });
    const auxiliaryMapping = getByRole("button", { name: "Map PLA (2)" });
    expect(getByText("Main nozzle")).toBeInTheDocument();
    expect(getByText("Auxiliary nozzle")).toBeInTheDocument();
    expect(mainMapping).toHaveTextContent("B1");
    expect(auxiliaryMapping).toHaveTextContent("A1");
    expect(mainMapping.querySelector('[style*="background-color"]'))
      .toHaveStyle({ backgroundColor: "#000000" });
    expect(container.querySelector('input[name="ams_mapping"]')).toHaveValue("[4,0]");
    expect(container.querySelector('input[name="ams_mapping2"]')).toHaveValue(
      '[{"ams_id":1,"slot_id":0},{"ams_id":0,"slot_id":0}]',
    );
    expect(container.querySelector('input[name="ams_mapping_info"]')).toHaveValue(
      '[{"ams":4,"filamentType":"PLA","filamentId":"GFA00","nozzleId":1,"sourceColor":"#000000FF","targetColor":"#000000FF"},{"ams":0,"filamentType":"PLA","filamentId":"GFA01","nozzleId":0,"sourceColor":"#FF0000FF","targetColor":"#FF0000FF"}]',
    );

    await user.click(auxiliaryMapping);
    expect(await findByRole("heading", {
      name: "Select the filament installed on the Auxiliary nozzle.",
    })).toBeInTheDocument();
    expect(getByText("Left AMS")).toBeInTheDocument();
    expect(getByText("Right AMS")).toBeInTheDocument();
    expect(getByText("AMS(2)")).toBeInTheDocument();
    expect(getByText("AMS(1)")).toBeInTheDocument();
    expect(getByRole("button", {
      name: /B1, PLA,.*This source cannot feed the selected nozzle\./,
    })).toHaveAttribute("aria-disabled", "true");

    const transparentExternal = getByRole("button", {
      name: /Ext-L, PLA,.*This source cannot feed the selected nozzle\./,
    });
    expect(transparentExternal.querySelector('[style*="background-color"]'))
      .toHaveStyle({ backgroundColor: "#00000000" });
    expect(getByText("AMS HT (1)")).toBeInTheDocument();
    expect(getByRole("button", {
      name: /HT-A, PETG,.*The installed AMS material type does not match the project material\./,
    })).toHaveAttribute("aria-disabled", "true");
    await user.click(getByRole("button", { name: "Unmapped" }));
    await waitFor(() => {
      expect(container.querySelector('input[name="material_mapping_valid"]'))
        .toHaveValue("false");
    });
    expect(getByRole("button", { name: "Dispatch print job" })).toBeDisabled();
    expect(getByText(
      "Select a compatible filament source for every required material before dispatch.",
    )).toBeInTheDocument();
    await user.click(auxiliaryMapping);
    await user.click(getByRole("button", { name: "Ext-R, ABS" }));
    expect(auxiliaryMapping).toHaveTextContent("Ext-R");
    expect(container.querySelector('input[name="ams_mapping"]')).toHaveValue("[4,-1]");
    expect(container.querySelector('input[name="ams_mapping2"]')).toHaveValue(
      '[{"ams_id":1,"slot_id":0},{"ams_id":255,"slot_id":0}]',
    );
    expect(container.querySelector('input[name="material_mapping_valid"]'))
      .toHaveValue("true");
    expect(container.querySelector('input[name="external_material_mismatch"]'))
      .toHaveValue("true");
    expect(container.querySelector('input[name="material_mapping_uses_ams"]'))
      .toHaveValue("true");
    expect(getByText(
      "The selected external spool has a different material type. Pandar will ask for confirmation before dispatch.",
    )).toBeInTheDocument();

    await user.click(getByRole("checkbox", { name: "Use AMS" }));
    expect(getByRole("checkbox", { name: "Use AMS" })).not.toBeChecked();
    await waitFor(() => {
      expect(container.querySelector('input[name="material_mapping_uses_ams"]'))
        .toHaveValue("false");
      expect(container.querySelector('input[name="ams_mapping2"]')).toHaveValue(
        '[{"ams_id":254,"slot_id":0},{"ams_id":255,"slot_id":0}]',
      );
    });

    fireEvent.submit(container.querySelector("form") as HTMLFormElement);
    const mismatchDialog = await screen.findByRole("dialog", {
      name: "Dispatch with material mismatch?",
    });
    expect(mismatchDialog).toHaveTextContent(
      "The external spool material type does not match the project. Dispatch anyway?",
    );
    await user.click(
      within(mismatchDialog).getByRole("button", { name: "Dispatch print job" }),
    );
    await waitFor(() => expect(onRedirect).toHaveBeenCalled());
    const upload = vi.mocked(fetch).mock.calls.at(-1)?.[1]?.body;
    expect(upload).toBeInstanceOf(FormData);
    expect((upload as FormData).get("use_ams")).toBe("false");
    expect((upload as FormData).get("material_mapping_uses_ams")).toBeNull();
  });
});
function screenGroup(container: HTMLElement, name: string) {
  const group = Array.from(container.querySelectorAll("fieldset")).find(
    (field) => field.querySelector("legend")?.textContent === name,
  );
  expect(group).toBeInstanceOf(HTMLFieldSetElement);
  return group as HTMLFieldSetElement;
}
