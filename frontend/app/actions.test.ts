import { beforeEach, describe, expect, it, vi } from "vitest";

import { linkPrinter, refreshPrinterMaterials } from "./actions";

const redirectMock = vi.hoisted(() =>
  vi.fn((url: string) => {
    throw new Error(`NEXT_REDIRECT:${url}`);
  }),
);

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
      vi.fn(async () =>
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

describe("refreshPrinterMaterials", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
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
});
