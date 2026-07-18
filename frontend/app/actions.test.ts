import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  controlPrinter,
  createMobileTicket,
  createTenantToken,
  deletePrinter,
  duplicateJob,
  linkPrinter,
  refreshPrinterMaterials,
  refreshPrinters,
  reprintJob,
  revokeTenantToken,
  rotateTenantToken,
  retryDispatchJob,
  retryDispatchJobs,
  updatePrinter,
} from "./actions";

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

describe("linkPrinter", () => {
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

  it("posts type, host, access code, and optional name without serial or model", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("agent_id", "agent-1");
    formData.set("type", "BambuLab");
    formData.set("host", "192.0.2.10");
    formData.set("access_code", "SECRET-LINK-CODE");
    formData.set("name", "Office X1C");

    await expect(linkPrinter(formData)).rejects.toThrow(
      "NEXT_REDIRECT:/agents?tenant=tenant-1&command=command-1",
    );

    expect(fetch).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/tenants/tenant-1/agents/agent-1/link-printer",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          type: "BambuLab",
          host: "192.0.2.10",
          access_code: "SECRET-LINK-CODE",
          name: "Office X1C",
        }),
      }),
    );
    const init = vi.mocked(fetch).mock.calls[0][1] as RequestInit;
    const body = JSON.parse(String(init.body)) as Record<string, unknown>;
    expect(body.serial_number).toBeUndefined();
    expect(body.model).toBeUndefined();
  });
});

describe("createMobileTicket", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({
              ticket: "pandar_plugin_ticket_abc",
              redirect_url: "zip.iptables.pandar.android:/auth/callback",
            }),
            {
              status: 201,
              headers: { "content-type": "application/json" },
            },
          ),
      ),
    );
  });

  it("creates a mobile login ticket and redirects back to Android", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("redirect_url", "zip.iptables.pandar.android:/auth/callback");

    await expect(createMobileTicket(formData)).rejects.toThrow(
      "NEXT_REDIRECT:zip.iptables.pandar.android:/auth/callback?ticket=pandar_plugin_ticket_abc&redirect_url=zip.iptables.pandar.android%3A%2Fauth%2Fcallback",
    );

    expect(fetch).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/tenants/tenant-1/mobile/login-tickets",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          redirect_url: "zip.iptables.pandar.android:/auth/callback",
        }),
      }),
    );
  });
});

describe("updatePrinter", () => {
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

  it("patches printer connection details and redirects to devices", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("printer_id", "printer-1");
    formData.set("host", "192.0.2.11");
    formData.set("access_code", "UPDATED-LINK-CODE");
    formData.set("name", "Office A1 Updated");

    await expect(updatePrinter(formData)).rejects.toThrow(
      "NEXT_REDIRECT:/devices?tenant=tenant-1&status=printer_updated",
    );

    expect(fetch).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/tenants/tenant-1/printers/printer-1",
      expect.objectContaining({
        method: "PATCH",
        body: JSON.stringify({
          host: "192.0.2.11",
          access_code: "UPDATED-LINK-CODE",
          name: "Office A1 Updated",
        }),
      }),
    );
  });
});

describe("refreshPrinterMaterials", () => {
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

  it("posts refresh printer materials to the API and redirects to devices", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("printer_id", "printer-1");

    await expect(refreshPrinterMaterials(formData)).rejects.toThrow(
      "NEXT_REDIRECT:/devices?tenant=tenant-1&status=materials_refresh_queued",
    );

    expect(fetch).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/tenants/tenant-1/printers/printer-1/materials:refresh",
      expect.objectContaining({ method: "POST" }),
    );
  });

  it("rejects a printer ID that could normalize into a jobs endpoint", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("printer_id", "../jobs/00000000-0000-0000-0000-000000000001");

    await expect(refreshPrinterMaterials(formData)).rejects.toThrow(
      "printer_id must be a valid ID",
    );
    expect(fetch).not.toHaveBeenCalled();
  });
});

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

      await expect(controlPrinter(formData)).rejects.toThrow(
        "NEXT_REDIRECT:/devices?tenant=tenant-1&status=printer_control_queued",
      );

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

    await expect(controlPrinter(formData)).rejects.toThrow(
      "NEXT_REDIRECT:/devices?tenant=tenant-1&status=printer_control_queued",
    );

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

    await expect(controlPrinter(formData)).rejects.toThrow(
      "NEXT_REDIRECT:/devices?tenant=tenant-1&status=printer_control_queued",
    );

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

    await expect(controlPrinter(formData)).rejects.toThrow(
      "NEXT_REDIRECT:/devices?tenant=tenant-1&status=printer_control_queued",
    );

    const init = vi.mocked(fetch).mock.calls[0][1] as RequestInit;
    const body = JSON.parse(String(init.body)) as Record<string, unknown>;
    expect(body).toMatchObject({
      action: "set_hotend_temperature",
      temperature_celsius: 220,
      extruder_id: 1,
    });
    expect(body.wait).toBeUndefined();
  });

  it("posts bed temperature details to the printer controls API", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("printer_id", "printer-1");
    formData.set("action", "set_bed_temperature");
    formData.set("temperature_celsius", "75");

    await expect(controlPrinter(formData)).rejects.toThrow(
      "NEXT_REDIRECT:/devices?tenant=tenant-1&status=printer_control_queued",
    );

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

    await expect(controlPrinter(formData)).rejects.toThrow(
      "NEXT_REDIRECT:/devices?tenant=tenant-1&status=printer_control_queued",
    );

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

    await expect(controlPrinter(formData)).rejects.toThrow(
      "NEXT_REDIRECT:/devices?tenant=tenant-1&status=printer_control_queued",
    );

    const init = vi.mocked(fetch).mock.calls[0][1] as RequestInit;
    const body = JSON.parse(String(init.body)) as Record<string, unknown>;
    expect(body).toMatchObject({
      action: "set_chamber_light",
      light_on: true,
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

  it("deletes the printer through the API and redirects to devices", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("printer_id", "printer-1");

    await expect(deletePrinter(formData)).rejects.toThrow(
      "NEXT_REDIRECT:/devices?tenant=tenant-1&status=printer_deleted",
    );

    expect(fetch).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/tenants/tenant-1/printers/printer-1",
      expect.objectContaining({ method: "DELETE" }),
    );
  });
});

describe("job action redirects", () => {
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
    [
      "refreshPrinters",
      refreshPrinters,
      [["agent_id", "agent-1"]],
      "refresh_queued",
    ],
    [
      "retryDispatchJob",
      retryDispatchJob,
      [["job_id", "job-1"]],
      "retry_queued",
    ],
    [
      "retryDispatchJobs",
      retryDispatchJobs,
      [["job_id", "job-1"]],
      "retry_queued",
    ],
    ["reprintJob", reprintJob, [["job_id", "job-1"]], "reprint_queued"],
    ["duplicateJob", duplicateJob, [["job_id", "job-1"]], "duplicate_queued"],
    [
      "controlPrinter",
      controlPrinter,
      [
        ["printer_id", "printer-1"],
        ["action", "pause"],
      ],
      "printer_control_queued",
    ],
  ] as const)(
    "redirects %s back to jobs when submitted from jobs",
    async (_name, action, fields, status) => {
      const formData = new FormData();
      formData.set("tenant_id", "tenant-1");
      formData.set("return_to", "jobs");
      for (const [name, value] of fields) {
        formData.append(name, value);
      }

      await expect(action(formData)).rejects.toThrow(
        `NEXT_REDIRECT:/jobs?tenant=tenant-1&status=${status}`,
      );
    },
  );

  it("redirects agent refresh back to Agents", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("agent_id", "agent-1");
    formData.set("return_to", "agents");

    await expect(refreshPrinters(formData)).rejects.toThrow(
      "NEXT_REDIRECT:/agents?tenant=tenant-1&status=refresh_queued",
    );
  });

  it("keeps recovery actions on devices by default", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("job_id", "job-1");

    await expect(retryDispatchJob(formData)).rejects.toThrow(
      "NEXT_REDIRECT:/devices?tenant=tenant-1&status=retry_queued",
    );
  });
});

describe("revokeTenantToken", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify({ tenant_token: { id: "token-1" } }), {
            status: 200,
            headers: { "content-type": "application/json" },
          }),
      ),
    );
  });

  it("returns to token management after revoking a token", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("token_id", "token-1");
    formData.set("return_to", "settings");

    await expect(revokeTenantToken(formData)).rejects.toThrow(
      "NEXT_REDIRECT:/settings?tenant=tenant-1&status=tenant_token_revoked",
    );
    expect(fetch).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/tenants/tenant-1/tenant-tokens/token-1",
      {
        method: "DELETE",
        headers: { "content-type": "application/json" },
      },
    );
  });
});

describe("createTenantToken", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("posts normalized token fields, returns the one-time secret, and refreshes token management", async () => {
    const tenantToken = {
      id: "token-created",
      tenant_id: "tenant-1",
      name: "Studio automation",
      scopes: ["plugin:studio", "agent:register"],
      created_by_user_id: null,
      created_at: "2026-07-17T01:00:00Z",
      last_used_at: null,
      expires_at: "2026-12-31T00:00:00Z",
      revoked_at: null,
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({
              tenant_token: tenantToken,
              token: "pandar_tenant_created-secret",
            }),
            {
              status: 201,
              headers: { "content-type": "application/json" },
            },
          ),
      ),
    );
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("name", "Studio automation");
    formData.set("scopes", " plugin:studio, agent:register, ");
    formData.set("expires_at", "2026-12-31T00:00:00Z");

    await expect(createTenantToken(null, formData)).resolves.toEqual({
      ok: true,
      kind: "tenant_token",
      operation: "created",
      tenantToken,
      token: "pandar_tenant_created-secret",
    });
    expect(fetch).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/tenants/tenant-1/tenant-tokens",
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          name: "Studio automation",
          scopes: ["plugin:studio", "agent:register"],
          expires_at: "2026-12-31T00:00:00Z",
        }),
      },
    );
    expect(refreshMock).toHaveBeenCalledTimes(1);
  });

  it("returns the API error without refreshing token management", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify({ error: "invalid_scope" }), {
            status: 400,
            headers: { "content-type": "application/json" },
          }),
      ),
    );
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("name", "Broken token");
    formData.set("scopes", "unknown:scope");

    await expect(createTenantToken(null, formData)).resolves.toEqual({
      ok: false,
      error: "invalid_scope",
    });
    expect(refreshMock).not.toHaveBeenCalled();
  });
});

describe("rotateTenantToken", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("posts the replacement expiration, returns the one-time secret, and refreshes token management", async () => {
    const tenantToken = {
      id: "token-rotated",
      tenant_id: "tenant-1",
      name: "Studio automation",
      scopes: ["plugin:studio"],
      created_by_user_id: null,
      created_at: "2026-07-17T02:00:00Z",
      last_used_at: null,
      expires_at: "2027-01-01T00:00:00Z",
      revoked_at: null,
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({
              tenant_token: tenantToken,
              token: "pandar_tenant_rotated-secret",
            }),
            {
              status: 201,
              headers: { "content-type": "application/json" },
            },
          ),
      ),
    );
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("token_id", "token-old");
    formData.set("expires_at", "2027-01-01T00:00:00Z");

    await expect(rotateTenantToken(null, formData)).resolves.toEqual({
      ok: true,
      kind: "tenant_token",
      operation: "rotated",
      tenantToken,
      token: "pandar_tenant_rotated-secret",
    });
    expect(fetch).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/tenants/tenant-1/tenant-tokens/token-old/rotate",
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ expires_at: "2027-01-01T00:00:00Z" }),
      },
    );
    expect(refreshMock).toHaveBeenCalledTimes(1);
  });

  it("returns the API error without refreshing token management", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify({ error: "invalid_expires_at" }), {
            status: 400,
            headers: { "content-type": "application/json" },
          }),
      ),
    );
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("token_id", "token-old");
    formData.set("expires_at", "not-a-date");

    await expect(rotateTenantToken(null, formData)).resolves.toEqual({
      ok: false,
      error: "invalid_expires_at",
    });
    expect(refreshMock).not.toHaveBeenCalled();
  });
});
