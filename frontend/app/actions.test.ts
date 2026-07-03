import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  controlPrinter,
  duplicateJob,
  linkPrinter,
  refreshAllAgents,
  refreshPrinterMaterials,
  refreshPrinters,
  reprintJob,
  retryDispatchJob,
  retryDispatchJobs,
} from "./actions";

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

describe("job action redirects", () => {
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

  it.each([
    ["refreshPrinters", refreshPrinters, [["agent_id", "agent-1"]], "refresh_queued"],
    ["refreshAllAgents", refreshAllAgents, [["agent_id", "agent-1"]], "refresh_queued"],
    ["retryDispatchJob", retryDispatchJob, [["job_id", "job-1"]], "retry_queued"],
    ["retryDispatchJobs", retryDispatchJobs, [["job_id", "job-1"]], "retry_queued"],
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

  it("keeps recovery actions on devices by default", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("job_id", "job-1");

    await expect(retryDispatchJob(formData)).rejects.toThrow(
      "NEXT_REDIRECT:/devices?tenant=tenant-1&status=retry_queued",
    );
  });
});
