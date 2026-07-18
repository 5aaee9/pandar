import { beforeEach, describe, expect, it, vi } from "vitest";

import { apiHeaders, requireAuth } from "./api-auth";
import { handlePrintError } from "./printer-recovery-actions";

vi.mock("./api-auth", () => ({
  apiHeaders: vi.fn(),
  requireAuth: vi.fn(),
}));

function validFormData() {
  const formData = new FormData();
  formData.set("tenant_id", "tenant-one");
  formData.set("printer_id", "printer-one");
  formData.set("error_action", "resume");
  formData.set("error_generation", "9");
  return formData;
}

describe("handlePrintError", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(apiHeaders).mockResolvedValue({
      "content-type": "application/json",
      authorization: "Bearer test",
    });
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        new Response(JSON.stringify({ id: "command-1", status: "sent" }), {
          status: 200,
          headers: { "content-type": "application/json" },
        }),
      ),
    );
  });

  it("authenticates, validates path fields, and sends only the semantic recovery payload", async () => {
    const formData = validFormData();
    formData.set("print_error", "83918929");
    formData.set("printer_job_id", "native-job");
    formData.set("job_state", "0");
    formData.set("task_generation", "7");
    formData.set("sequence_id", "12345");

    await expect(handlePrintError({ status: "idle" }, formData)).resolves.toEqual({
      status: "sent",
    });

    expect(requireAuth).toHaveBeenCalledTimes(1);
    expect(apiHeaders).toHaveBeenCalledWith("application/json");
    expect(fetch).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/tenants/tenant-one/printers/printer-one/controls",
      {
        method: "POST",
        headers: {
          "content-type": "application/json",
          authorization: "Bearer test",
        },
        body: JSON.stringify({
          action: "handle_print_error",
          error_action: "resume",
          error_generation: 9,
        }),
      },
    );
  });

  it("returns the backend error without redirecting", async () => {
    vi.mocked(fetch).mockResolvedValueOnce(
      new Response(JSON.stringify({ error: "printer_operation_unavailable" }), {
        status: 400,
        headers: { "content-type": "application/json" },
      }),
    );

    await expect(handlePrintError({ status: "idle" }, validFormData())).resolves.toEqual({
      status: "error",
      error: "printer_operation_unavailable",
    });
  });

  it("returns a redacted typed error when the recovery request is rejected", async () => {
    const apiUrl = "http://localhost:8080/private/control";
    const credential = "top-secret-token";
    vi.mocked(fetch).mockRejectedValueOnce(
      new Error(`request failed for ${apiUrl}`, {
        cause: new Error(`Bearer ${credential}`),
      }),
    );

    const result = await handlePrintError({ status: "idle" }, validFormData());

    expect(result.status).toBe("error");
    if (result.status !== "error") {
      throw new Error("expected error state");
    }
    expect(result.error).toContain(
      "Printer recovery request failed: Error: request failed for [redacted URL]",
    );
    expect(result.error).toContain("caused by Error: Bearer [redacted]");
    expect(result.error).not.toContain(apiUrl);
    expect(result.error).not.toContain(credential);
  });

  it("returns malformed JSON context without leaking the response body", async () => {
    const responseBody = "not-json-private-response-body";
    vi.mocked(fetch).mockResolvedValueOnce(
      new Response(responseBody, {
        status: 502,
        headers: { "content-type": "application/json" },
      }),
    );

    const result = await handlePrintError({ status: "idle" }, validFormData());

    expect(result.status).toBe("error");
    if (result.status !== "error") {
      throw new Error("expected error state");
    }
    expect(result.error).toContain(
      "http_502: Printer recovery error response decode failed: SyntaxError",
    );
    expect(result.error).not.toContain(responseBody);
    expect(result.error).not.toContain("http://localhost:8080");
  });

  it("returns a redacted body-read Error cause chain", async () => {
    const apiUrl = "http://localhost:8080/private/control";
    const credential = "top-secret-token";
    const readError = new Error(`stream terminated at ${apiUrl}`, {
      cause: new Error(`Bearer ${credential}`),
    });
    vi.mocked(fetch).mockResolvedValueOnce({
      ok: false,
      status: 503,
      text: vi.fn().mockRejectedValue(readError),
    } as unknown as Response);

    const result = await handlePrintError({ status: "idle" }, validFormData());

    expect(result.status).toBe("error");
    if (result.status !== "error") {
      throw new Error("expected error state");
    }
    expect(result.error).toContain(
      "http_503: Printer recovery error response read failed: Error: stream terminated at [redacted URL]",
    );
    expect(result.error).toContain("caused by Error: Bearer [redacted]");
    expect(result.error).not.toContain(apiUrl);
    expect(result.error).not.toContain(credential);
  });

  it.each([
    ["tenant_id", ""],
    ["tenant_id", "../jobs"],
    ["printer_id", ""],
    ["printer_id", "../jobs/00000000-0000-0000-0000-000000000001"],
    ["error_action", "pause"],
    ["error_generation", ""],
    ["error_generation", "0"],
    ["error_generation", "1.5"],
  ])("rejects invalid FormData field %s=%s at the server boundary", async (name, value) => {
    const formData = validFormData();
    formData.set(name, value);

    await expect(handlePrintError({ status: "idle" }, formData)).resolves.toEqual({
      status: "error",
      error: "invalid_printer_control",
    });
    expect(requireAuth).toHaveBeenCalledTimes(1);
    expect(fetch).not.toHaveBeenCalled();
  });
});
