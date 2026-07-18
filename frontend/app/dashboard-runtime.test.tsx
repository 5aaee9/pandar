import { NextIntlClientProvider } from "next-intl";
import { act, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import zh from "../messages/zh.json";
import { DashboardRuntime } from "./dashboard-runtime";
import type { AuthMetadata, Tenant } from "./dashboard-types";
import type { DashboardView } from "./dashboard-shell";
import { toast } from "sonner";

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: vi.fn() }),
}));

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    warning: vi.fn(),
    error: vi.fn(),
  },
}));

const tenant: Tenant = {
  id: "t1",
  slug: "tenant-one",
  display_name: "Tenant One",
  created_at: "2026-06-30T00:00:00Z",
};

const otherTenant: Tenant = {
  id: "t2",
  slug: "tenant-two",
  display_name: "Tenant Two",
  created_at: "2026-06-30T00:00:00Z",
};

const noAuth: AuthMetadata = {
  source: "none",
  cookieName: "pandar_auth",
  provider: "none",
  signInUrl: null,
  signOutUrl: null,
};

function renderRuntime(
  auth: AuthMetadata = noAuth,
  options: {
    view?: DashboardView;
    actionStatus?: string;
    selectedCommandId?: string;
    tenants?: Tenant[];
    locale?: "en" | "zh";
  } = {},
) {
  return render(
    <NextIntlClientProvider
      locale={options.locale ?? "en"}
      messages={options.locale === "zh" ? zh : en}
    >
      <DashboardRuntime
        apiUrl="http://localhost:8080"
        view={options.view ?? "devices"}
        tenants={options.tenants ?? [tenant]}
        selectedTenant={tenant}
        initialPrinters={[]}
        agents={[]}
        initialJobs={[]}
        users={[]}
        userIdentities={[]}
        tenantTokens={[]}
        joinLinks={[]}
        auditEvents={[]}
        adminUnavailable={false}
        canManageJobs={true}
        actionStatus={options.actionStatus}
        selectedCommand={null}
        selectedCommandId={options.selectedCommandId}
        commandData={null}
        errors={[]}
        auth={auth}
      />
    </NextIntlClientProvider>,
  );
}

describe("DashboardRuntime live connection", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it("connects directly to printer events when hub auth is disabled", async () => {
    const urls: string[] = [];
    let socket: { onopen: (() => void) | null } | null = null;
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        new Response(JSON.stringify({ printers: [] }), {
          headers: { "content-type": "application/json" },
        }),
      ),
    );
    vi.stubGlobal(
      "WebSocket",
      class {
        onopen: (() => void) | null = null;

        constructor(url: string) {
          urls.push(url);
          socket = this;
        }

        close() {}
      },
    );

    renderRuntime();

    await waitFor(() => {
      expect(urls).toEqual([
        "ws://localhost:8080/api/v1/tenants/t1/printer-events",
      ]);
    });
    expect(fetch).not.toHaveBeenCalled();

    act(() => socket?.onopen?.());

    await waitFor(() =>
      expect(fetch).toHaveBeenCalledWith(
        "/api/tenants/t1/printers",
        expect.objectContaining({
          cache: "no-store",
          signal: expect.any(AbortSignal),
        }),
      ),
    );
  });

  it("preserves action status when switching tenants from jobs", () => {
    vi.stubGlobal("fetch", vi.fn());
    vi.stubGlobal(
      "WebSocket",
      class {
        close() {}
      },
    );

    renderRuntime(noAuth, {
      view: "jobs",
      actionStatus: "refresh_queued",
      selectedCommandId: "cmd1",
      tenants: [tenant, otherTenant],
    });

    expect(screen.getByRole("link", { name: "Tenant Two" })).toHaveAttribute(
      "href",
      "/jobs?tenant=t2&status=refresh_queued",
    );
  });

  it("shows a toast for printer operation command results", async () => {
    const socket = {
      current: null as {
        onmessage: ((message: { data: string }) => void) | null;
      } | null,
    };
    vi.stubGlobal("fetch", vi.fn());
    vi.stubGlobal(
      "WebSocket",
      class {
        onopen: (() => void) | null = null;
        onmessage: ((message: { data: string }) => void) | null = null;

        constructor() {
          socket.current = this;
        }

        close() {}
      },
    );

    renderRuntime();

    await waitFor(() => expect(socket.current).not.toBeNull());
    socket.current?.onmessage?.({
      data: JSON.stringify({
        type: "command_result",
        command: {
          id: "cmd1",
          tenant_id: "t1",
          agent_id: "a1",
          printer_id: "p1",
          kind: "printer_operation",
          status: "succeeded",
          payload_json: "{}",
          error: null,
          result_json: JSON.stringify({
            type: "printer_operation",
            action: "ams_reread_rfid",
            sequence_id: "20000",
            mqtt_result: "success",
          }),
          created_at: "2026-07-03T00:00:00Z",
          updated_at: "2026-07-03T00:00:01Z",
        },
      }),
    });

    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Printer control completed", {
        description: "#20000",
      }),
    );
  });

  it.each([
    ["en", "Recovery command sent; waiting for printer status confirmation"],
    ["zh", "恢复指令已发送，等待打印机状态确认"],
  ] as const)(
    "shows the dedicated sequence-zero recovery toast in %s",
    async (locale, message) => {
      const socket = {
        current: null as {
          onmessage: ((message: { data: string }) => void) | null;
        } | null,
      };
      vi.stubGlobal("fetch", vi.fn());
      vi.stubGlobal(
        "WebSocket",
        class {
          onmessage: ((message: { data: string }) => void) | null = null;

          constructor() {
            socket.current = this;
          }

          close() {}
        },
      );

      renderRuntime(noAuth, { locale });
      await waitFor(() => expect(socket.current).not.toBeNull());
      act(() => {
        socket.current?.onmessage?.({
          data: JSON.stringify({
            type: "command_result",
            command: {
              id: "cmd-recovery",
              tenant_id: "t1",
              agent_id: "a1",
              printer_id: "p1",
              kind: "printer_operation",
              status: "succeeded",
              payload_json: JSON.stringify({
                printer_id: "p1",
                serial_number: "20P123",
                operation: {
                  type: "handle_print_error",
                  error_action: "resume",
                  print_error: 83_918_929,
                  printer_job_id: "native-job",
                  sequence_id: 0,
                },
              }),
              error: null,
              result_json: JSON.stringify({ sequence_id: "0" }),
              created_at: "2026-07-10T00:00:00Z",
              updated_at: "2026-07-10T00:00:01Z",
            },
          }),
        });
      });

      await waitFor(() => expect(toast.success).toHaveBeenCalledWith(message));
      expect(toast.success).not.toHaveBeenCalledWith("Printer control completed", expect.anything());
    },
  );
});
