import { NextIntlClientProvider } from "next-intl";
import { fireEvent, render, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import { DispatchForm } from "./dispatch-form";

describe("DispatchForm", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
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

    const { container } = render(
      <NextIntlClientProvider locale="en" messages={en}>
        <DispatchForm
          selectedTenant={{ id: "tenant-1" }}
          printers={[{ id: "printer-1", name: "Printer One", serial_number: "SN1", materials: null }]}
          onRedirect={onRedirect}
        />
      </NextIntlClientProvider>,
    );
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
      expect(onRedirect).toHaveBeenCalledWith("/jobs?tenant=tenant-1&status=job_created"),
    );
  });

  it("offers parsed plate choices only after metadata is ready", async () => {
    const user = userEvent.setup();
    let resolvePreview!: (response: Response) => void;
    const previewResponse = new Promise<Response>((resolve) => {
      resolvePreview = resolve;
    });
    vi.stubGlobal("fetch", vi.fn(() => previewResponse));

    const { container, getByText } = render(
      <NextIntlClientProvider locale="en" messages={en}>
        <DispatchForm
          selectedTenant={{ id: "tenant-1" }}
          printers={[{ id: "printer-1", name: "Printer One", serial_number: "SN1", materials: null }]}
        />
      </NextIntlClientProvider>,
    );

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

  it("maps uploaded project materials to the selected printer AMS and allows overrides", async () => {
    const user = userEvent.setup();
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        if (String(input).includes("artifact-metadata-preview")) {
          return Response.json({
            metadata: {
              display_name: "benchy",
              default_plate_id: 1,
              warnings: [],
              plates: [
                {
                  plate_id: 1,
                  name: "Plate 1",
                  estimated_time_seconds: null,
                  filament_weight_grams: null,
                  object_count: 1,
                  objects: ["benchy"],
                  filaments: [
                    {
                      filament_id: "1",
                      filament_type: "PLA",
                      color: "#ff0000",
                      used_grams: 10,
                      used_meters: 3,
                    },
                    {
                      filament_id: "2",
                      filament_type: "ABS",
                      color: "#111111",
                      used_grams: 2,
                      used_meters: 1,
                    },
                  ],
                  has_thumbnail: false,
                },
              ],
            },
          });
        }
        return Response.json({}, { status: 201 });
      }),
    );

    const { container } = render(
      <NextIntlClientProvider locale="en" messages={en}>
        <DispatchForm
          selectedTenant={{ id: "tenant-1" }}
          printers={[
            {
              id: "printer-1",
              name: "Printer One",
              serial_number: "SN1",
              materials: {
                observed_at: "2026-07-15T00:00:00Z",
                active_tray: null,
                external_spools: [],
                ams_units: [
                  {
                    unit_id: "0",
                    trays: [
                      {
                        tray_id: "0",
                        global_tray_id: 0,
                        type: "PLA",
                        color: "FF0000",
                        exists: true,
                      },
                    ],
                  },
                ],
              },
            },
          ]}
        />
      </NextIntlClientProvider>,
    );

    await user.upload(
      container.querySelector('input[type="file"]') as HTMLInputElement,
      new File(["3mf"], "benchy.3mf", { type: "model/3mf" }),
    );

    const plaMapping = await waitFor(() =>
      container.querySelector('select[aria-label="Map PLA (1)"]'),
    );
    expect(plaMapping).toHaveValue("0:0");
    expect(container.querySelector('select[aria-label="Map ABS (2)"]')).toHaveValue("");
    expect(container.querySelector('input[name="ams_mapping"]')).toHaveValue("[0,-1]");
    expect(container.querySelector('input[name="ams_mapping2"]')).toHaveValue(
      '[{"ams_id":0,"slot_id":0},{"ams_id":255,"slot_id":255}]',
    );

    await user.selectOptions(
      container.querySelector('select[aria-label="Map ABS (2)"]') as HTMLSelectElement,
      "0:0",
    );

    expect(container.querySelector('input[name="ams_mapping"]')).toHaveValue("[0,0]");
  });
});
