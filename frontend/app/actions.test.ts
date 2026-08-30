import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  createMobileTicket,
  discoverPrinters,
  linkPrinter,
  refreshPrinterMaterials,
  updatePrinter,
} from "./actions";
import { requireAuth } from "./api-auth";

const redirectMock = vi.hoisted(() =>
  vi.fn((url: string) => {
    throw new Error(`NEXT_REDIRECT:${url}`);
  }),
);
const refreshMock = vi.hoisted(() => vi.fn());

const commandResponse = {
  id: "command-1",
  tenant_id: "tenant-1",
  agent_id: "agent-1",
  printer_id: null,
  kind: "printer_operation",
  status: "queued",
  payload_json: "{}",
  error: null,
  result_json: null,
  created_at: "created",
  updated_at: "updated",
};

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

describe("discoverPrinters", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify(commandResponse), {
            status: 200,
            headers: { "content-type": "application/json" },
          }),
      ),
    );
  });

  it("redirects discovery commands to the agents page command view", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("agent_id", "agent-1");
    formData.set("timeout_seconds", "9");

    await expect(discoverPrinters(formData)).rejects.toThrow(
      "NEXT_REDIRECT:/agents?command=command-1",
    );
    expect(fetch).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/tenants/tenant-1/agents/agent-1/discover-printers",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ timeout_seconds: 9 }),
      }),
    );
  });
});

describe("linkPrinter", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify(commandResponse), {
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

    await expect(linkPrinter(null, formData)).resolves.toEqual({
      ok: true,
      commandId: "command-1",
    });

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

  it("returns the hub error code when the dispatch is rejected", async () => {
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
    formData.set("agent_id", "agent-1");
    formData.set("type", "BambuLab");
    formData.set("host", "192.0.2.10");
    formData.set("access_code", "SECRET-LINK-CODE");

    await expect(linkPrinter(null, formData)).resolves.toEqual({
      ok: false,
      error: "agent_not_connected",
    });
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
              expires_at: "2026-08-31T00:00:00Z",
              redirect_url: "zip.iptables.pandar.android://auth/callback",
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
    formData.set("redirect_url", "zip.iptables.pandar.android://auth/callback");
    formData.set("code_challenge", "challenge");
    formData.set("state", "state");

    await expect(createMobileTicket(formData)).rejects.toThrow(
      "NEXT_REDIRECT:zip.iptables.pandar.android://auth/callback?ticket=pandar_plugin_ticket_abc&redirect_url=zip.iptables.pandar.android%3A%2F%2Fauth%2Fcallback&state=state",
    );

    expect(requireAuth).toHaveBeenCalledTimes(1);
    expect(fetch).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/tenants/tenant-1/mobile/login-tickets",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          redirect_url: "zip.iptables.pandar.android://auth/callback",
          code_challenge: "challenge",
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

  it("patches printer connection details and returns success", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("printer_id", "printer-1");
    formData.set("host", "192.0.2.11");
    formData.set("access_code", "UPDATED-LINK-CODE");
    formData.set("name", "Office A1 Updated");

    await expect(updatePrinter(null, formData)).resolves.toEqual({ ok: true });

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
          new Response(JSON.stringify(commandResponse), {
            status: 200,
            headers: { "content-type": "application/json" },
          }),
      ),
    );
  });

  it("posts refresh printer materials to the API and returns success", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("printer_id", "printer-1");

    await expect(refreshPrinterMaterials(null, formData)).resolves.toEqual({
      ok: true,
    });

    expect(fetch).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/tenants/tenant-1/printers/printer-1/materials:refresh",
      expect.objectContaining({ method: "POST" }),
    );
  });

  it("rejects a printer ID that could normalize into a jobs endpoint", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("printer_id", "../jobs/00000000-0000-0000-0000-000000000001");

    await expect(refreshPrinterMaterials(null, formData)).rejects.toThrow(
      "printer_id must be a valid ID",
    );
    expect(fetch).not.toHaveBeenCalled();
  });
});
