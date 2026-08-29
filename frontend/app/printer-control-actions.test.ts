import { beforeEach, describe, expect, it, vi } from "vitest";

import { controlPrinter, deletePrinter } from "./actions";

const redirectMock = vi.hoisted(() =>
  vi.fn((url: string) => {
    throw new Error(`NEXT_REDIRECT:${url}`);
  }),
);
const refreshMock = vi.hoisted(() => vi.fn());

vi.mock("next/cache", () => ({
  refresh: refreshMock,
}));

vi.mock("next/navigation", () => ({
  redirect: redirectMock,
}));

vi.mock("./api-auth", () => ({
  requireAuth: vi.fn(async () => undefined),
  apiHeaders: vi.fn(async () => ({ "content-type": "application/json" })),
}));

describe("controlPrinter axis operations", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify({ id: "command-1" }), {
            status: 200,
            headers: { "content-type": "application/json" },
          }),
      ),
    );
  });

  it.each([
    ["x", "-10", 3000],
    ["y", "1", 3000],
    ["z", "10", 900],
  ] as const)(
    "posts a single %s movement with the Studio feedrate",
    async (axis, deltaMm, feedrateMmPerMin) => {
      const formData = new FormData();
      formData.set("tenant_id", "tenant-1");
      formData.set("printer_id", "printer-1");
      formData.set("action", "move_axes");
      formData.set("axis", axis);
      formData.set("delta_mm", deltaMm);
      formData.set("feedrate_mm_per_min", String(feedrateMmPerMin));

      await expect(controlPrinter(null, formData)).resolves.toEqual({
        ok: true,
      });

      const init = vi.mocked(fetch).mock.calls[0][1] as RequestInit;
      expect(JSON.parse(String(init.body))).toEqual({
        action: "move_axes",
        movements: [{ axis, delta_mm: Number(deltaMm) }],
        feedrate_mm_per_min: feedrateMmPerMin,
      });
    },
  );

  it("posts full-axis Home with an explicit empty axis list", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("printer_id", "printer-1");
    formData.set("action", "home");

    await expect(controlPrinter(null, formData)).resolves.toEqual({ ok: true });

    const init = vi.mocked(fetch).mock.calls[0][1] as RequestInit;
    expect(JSON.parse(String(init.body))).toEqual({
      action: "home",
      axes: [],
    });
  });
});

describe("controlPrinter AMS operations", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify({ id: "command-1" }), {
            status: 200,
            headers: { "content-type": "application/json" },
          }),
      ),
    );
  });

  it("posts AMS slot operation details to the printer controls API", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("printer_id", "printer-1");
    formData.set("action", "ams_load_filament");
    formData.set("ams_id", "0");
    formData.set("slot_id", "1");
    formData.set("global_tray_id", "1");
    formData.set("extruder_id", "0");
    formData.set("external_id", "");

    await expect(controlPrinter(null, formData)).resolves.toEqual({ ok: true });

    expect(fetch).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/tenants/tenant-1/printers/printer-1/controls",
      expect.objectContaining({ method: "POST" }),
    );
    const init = vi.mocked(fetch).mock.calls[0][1] as RequestInit;
    const body = JSON.parse(String(init.body)) as Record<string, unknown>;
    expect(body).toMatchObject({
      action: "ams_load_filament",
      ams_id: 0,
      slot_id: 1,
      global_tray_id: 1,
      extruder_id: 0,
    });
    expect(body.speed_mode).toBeUndefined();
    expect(body.external_id).toBeUndefined();
  });

  it("posts hotend temperature details to the printer controls API", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("printer_id", "printer-1");
    formData.set("action", "set_hotend_temperature");
    formData.set("temperature_celsius", "220");
    formData.set("extruder_id", "1");

    await expect(controlPrinter(null, formData)).resolves.toEqual({ ok: true });

    const init = vi.mocked(fetch).mock.calls[0][1] as RequestInit;
    const body = JSON.parse(String(init.body)) as Record<string, unknown>;
    expect(body).toMatchObject({
      action: "set_hotend_temperature",
      temperature_celsius: 220,
      extruder_id: 1,
    });
    expect(body.wait).toBeUndefined();
  });

  it("posts fan speed details to the printer controls API", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("printer_id", "printer-1");
    formData.set("action", "set_fan_speed");
    formData.set("fan_index", "2");
    formData.set("speed_percent", "50");
    formData.set("airduct", "true");

    await expect(controlPrinter(null, formData)).resolves.toEqual({ ok: true });

    const init = vi.mocked(fetch).mock.calls[0][1] as RequestInit;
    const body = JSON.parse(String(init.body)) as Record<string, unknown>;
    expect(body).toMatchObject({
      action: "set_fan_speed",
      fan_index: 2,
      speed_percent: 50,
      airduct: true,
    });
  });

  it("posts bed temperature details to the printer controls API", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("printer_id", "printer-1");
    formData.set("action", "set_bed_temperature");
    formData.set("temperature_celsius", "75");

    await expect(controlPrinter(null, formData)).resolves.toEqual({ ok: true });

    const init = vi.mocked(fetch).mock.calls[0][1] as RequestInit;
    const body = JSON.parse(String(init.body)) as Record<string, unknown>;
    expect(body).toMatchObject({
      action: "set_bed_temperature",
      temperature_celsius: 75,
    });
  });

  it("posts chamber temperature details to the printer controls API", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("printer_id", "printer-1");
    formData.set("action", "set_chamber_temperature");
    formData.set("temperature_celsius", "45");

    await expect(controlPrinter(null, formData)).resolves.toEqual({ ok: true });

    const init = vi.mocked(fetch).mock.calls[0][1] as RequestInit;
    const body = JSON.parse(String(init.body)) as Record<string, unknown>;
    expect(body).toMatchObject({
      action: "set_chamber_temperature",
      temperature_celsius: 45,
    });
  });

  it("posts chamber light target state to the printer controls API", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("printer_id", "printer-1");
    formData.set("action", "set_chamber_light");
    formData.set("light_on", "true");

    await expect(controlPrinter(null, formData)).resolves.toEqual({ ok: true });

    const init = vi.mocked(fetch).mock.calls[0][1] as RequestInit;
    const body = JSON.parse(String(init.body)) as Record<string, unknown>;
    expect(body).toMatchObject({
      action: "set_chamber_light",
      light_on: true,
    });
  });

  it("returns the hub error code when the control command is rejected", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify({ error: "agent_not_connected" }), {
            status: 409,
            headers: { "content-type": "application/json" },
          }),
      ),
    );
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("printer_id", "printer-1");
    formData.set("action", "pause");

    await expect(controlPrinter(null, formData)).resolves.toEqual({
      ok: false,
      error: "agent_not_connected",
    });
  });
});

describe("deletePrinter", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify({ id: "printer-1" }), {
            status: 200,
            headers: { "content-type": "application/json" },
          }),
      ),
    );
  });

  it("deletes the printer through the API and returns success", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("printer_id", "printer-1");

    await expect(deletePrinter(null, formData)).resolves.toEqual({ ok: true });

    expect(fetch).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/tenants/tenant-1/printers/printer-1",
      expect.objectContaining({ method: "DELETE" }),
    );
  });
});
