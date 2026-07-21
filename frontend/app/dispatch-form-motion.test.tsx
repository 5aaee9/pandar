import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import { DispatchForm } from "./dispatch-form";

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

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

const printers = [
  {
    id: "printer-1",
    name: "Printer One",
    serial_number: "SN1",
    model: "N6",
    materials: null,
  },
  {
    id: "printer-2",
    name: "Printer Two",
    serial_number: "SN2",
    model: "A1",
    materials: null,
  },
];

const metadata = {
  display_name: "project",
  default_plate_id: 1,
  warnings: [],
  plates: [1, 2].map((plateId) => ({
    plate_id: plateId,
    name: `Plate ${plateId}`,
    estimated_time_seconds: null,
    filament_weight_grams: null,
    object_count: 1,
    objects: [`object-${plateId}`],
    filaments: [
      {
        filament_id: String(plateId),
        tray_info_idx: `GFA0${plateId}`,
        nozzle_id: 0,
        filament_type: "PLA",
        color: "#000000",
        used_grams: 1,
        used_meters: 1,
      },
    ],
    has_thumbnail: false,
  })),
};

describe("DispatchForm motion boundaries", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("mounts motion roots only when metadata unlocks controls", async () => {
    const user = userEvent.setup();
    const preview = deferred<Response>();
    vi.stubGlobal("fetch", vi.fn(() => preview.promise));
    const { container } = renderDispatchForm({
      selectedTenant: { id: "tenant-1" },
      printers,
    });

    expect(container.querySelectorAll('[data-motion="dispatch-unlocked"]')).toHaveLength(0);

    await user.upload(
      container.querySelector('input[type="file"]') as HTMLInputElement,
      new File(["3mf"], "project.3mf", { type: "model/3mf" }),
    );
    expect(container.querySelectorAll('[data-motion="dispatch-unlocked"]')).toHaveLength(0);

    preview.resolve(Response.json({ metadata }));

    await waitFor(() => {
      expect(container.querySelectorAll('[data-motion="dispatch-unlocked"]')).toHaveLength(2);
    });
  });

  it("keeps the material motion wrapper stable across keyed editor resets", async () => {
    const user = userEvent.setup();
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => Response.json({ metadata })),
    );
    const { container } = renderDispatchForm({
      selectedTenant: { id: "tenant-1" },
      printers,
    });

    await user.upload(
      container.querySelector('input[type="file"]') as HTMLInputElement,
      new File(["3mf"], "project.3mf", { type: "model/3mf" }),
    );

    const wrapper = await waitFor(() => {
      const element = Array.from(
        container.querySelectorAll<HTMLElement>('[data-motion="dispatch-unlocked"]'),
      ).find((candidate) => candidate.querySelector("fieldset"));
      expect(element).toBeInstanceOf(HTMLDivElement);
      return element as HTMLDivElement;
    });

    let editor = wrapper.querySelector("fieldset");
    expect(editor).toBeInstanceOf(HTMLFieldSetElement);

    await user.selectOptions(
      container.querySelector('select[name="plate_id"]') as HTMLSelectElement,
      "2",
    );
    await waitFor(() => expect(wrapper.querySelector("fieldset")).not.toBe(editor));
    expect(wrapper).toBe(container.querySelector('[data-motion="dispatch-unlocked"]:has(fieldset)'));
    editor = wrapper.querySelector("fieldset");

    await user.selectOptions(
      container.querySelector('select[name="printer_id"]') as HTMLSelectElement,
      "printer-2",
    );
    await waitFor(() => expect(wrapper.querySelector("fieldset")).not.toBe(editor));
    expect(wrapper).toBe(container.querySelector('[data-motion="dispatch-unlocked"]:has(fieldset)'));
    editor = wrapper.querySelector("fieldset");

    await user.click(container.querySelector('input[type="checkbox"]') as HTMLInputElement);
    await waitFor(() => expect(wrapper.querySelector("fieldset")).not.toBe(editor));
    expect(wrapper).toBe(container.querySelector('[data-motion="dispatch-unlocked"]:has(fieldset)'));
  });
});
