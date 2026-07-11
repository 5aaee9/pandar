import { NextIntlClientProvider } from "next-intl";
import { act, renderHook } from "@testing-library/react";
import type { PropsWithChildren } from "react";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";

import en from "../messages/en.json";
import type {
  AuthMetadata,
  Job,
  Printer,
  PrinterEvent,
  Tenant,
} from "./dashboard-types";
import type { PrinterPrintState } from "./printer-live-types";
import { useDashboardRuntimeEvents } from "./use-dashboard-runtime-events";
import { toast } from "sonner";

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    warning: vi.fn(),
    error: vi.fn(),
  },
}));

const tenant: Tenant = {
  id: "tenant-1",
  slug: "tenant-one",
  display_name: "Tenant One",
  created_at: "2026-07-01T00:00:00Z",
};

const noAuth: AuthMetadata = {
  source: "none",
  cookieName: "pandar_auth",
  provider: "none",
  signInUrl: null,
  signOutUrl: null,
};

const printState: PrinterPrintState = {
  task_generation: 1,
  error_generation: 1,
  hms: [],
  job_state: 0,
  gcode_state: "RUNNING",
  task_id: "task-1",
  subtask_id: "subtask-1",
  subtask_name: "Cube",
  gcode_file: "/data/Metadata/plate_1.gcode",
  progress_percent: 25,
  remaining_time_minutes: 12,
  current_layer: 2,
  total_layers: 8,
  print_error: 83_918_929,
  printer_job_id: "native-job-1",
};

function printer(
  id: string,
  overrides: Partial<Printer> = {},
): Printer {
  return {
    id,
    tenant_id: tenant.id,
    agent_id: "agent-1",
    serial_number: `serial-${id}`,
    name: `Printer ${id}`,
    model: "P1S",
    status: "RUNNING",
    last_seen_at: "2026-07-10T00:00:00Z",
    created_at: "2026-07-01T00:00:00Z",
    nozzle_temperatures: [],
    active_nozzle: null,
    bed_temperature_celsius: null,
    bed_target_temperature_celsius: null,
    chamber_temperature_celsius: null,
    chamber_light_on: null,
    materials: null,
    state_revision: 1,
    print: printState,
    ...overrides,
  };
}

function legacyPrinter(
  id: string,
  overrides: Partial<Printer> = {},
): Printer {
  return printer(id, {
    state_revision: undefined,
    print: undefined,
    ...overrides,
  });
}

function materials(observedAt: string) {
  return {
    ams_units: [],
    external_spools: [],
    active_tray: null,
    observed_at: observedAt,
  };
}

function job(id: string, status = "queued"): Job {
  return {
    id,
    printer_id: "printer-1",
    agent_id: "agent-1",
    artifact_id: "artifact-1",
    command_id: "command-1",
    status,
    error: null,
    created_at: "2026-07-10T00:00:00Z",
    updated_at: "2026-07-10T00:00:00Z",
    print: {
      status: "pending",
      printer_state: null,
      progress_percent: null,
      remaining_time_minutes: null,
      current_layer: null,
      total_layers: null,
      active_file: null,
      last_progress_percent: null,
      last_layer: null,
      error: null,
      started_at: null,
      finished_at: null,
      updated_at: null,
    },
    command: { id: "command-1", kind: "start_print", status: "queued" },
    artifact: {
      id: "artifact-1",
      tenant_id: tenant.id,
      filename: "cube.3mf",
      content_type: "application/octet-stream",
      size_bytes: 123,
      metadata: null,
      created_at: "2026-07-10T00:00:00Z",
    },
    material: {
      ams_mapping: null,
      ams_mapping2: null,
      filament_usage: [],
    },
  };
}

function listResponse(printers: Printer[], status = 200): Response {
  return new Response(JSON.stringify({ printers }), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function abortableResponse() {
  const result = deferred<Response>();
  let signal: AbortSignal | undefined;
  const fetch = vi.fn((_input: RequestInfo | URL, init?: RequestInit) => {
    signal = init?.signal ?? undefined;
    signal?.addEventListener("abort", () => {
      result.reject(new DOMException("aborted", "AbortError"));
    });
    return result.promise;
  });
  return { fetch, result, signal: () => signal };
}

class FakeWebSocket {
  static instances: FakeWebSocket[] = [];

  readonly url: string;
  onopen: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent<string>) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  onclose: ((event: CloseEvent) => void) | null = null;
  close = vi.fn();

  constructor(url: string | URL) {
    this.url = String(url);
    FakeWebSocket.instances.push(this);
  }

  open() {
    this.onopen?.(new Event("open"));
  }

  message(event: PrinterEvent) {
    this.onmessage?.(
      new MessageEvent("message", { data: JSON.stringify(event) }),
    );
  }

  serverClose() {
    this.onclose?.(new CloseEvent("close"));
  }
}

function wrapper({ children }: PropsWithChildren) {
  return (
    <NextIntlClientProvider locale="en" messages={en}>
      {children}
    </NextIntlClientProvider>
  );
}

function renderRuntimeEvents(
  options: {
    initialPrinters?: Printer[];
    initialJobs?: Job[];
    auth?: AuthMetadata;
  } = {},
) {
  const initialPrinters = options.initialPrinters ?? [];
  const initialJobs = options.initialJobs ?? [];
  return renderHook(
    () =>
      useDashboardRuntimeEvents({
        apiUrl: "http://localhost:8080",
        auth: options.auth ?? noAuth,
        selectedTenant: tenant,
        initialPrinters,
        initialJobs,
      }),
    { wrapper },
  );
}

async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
}

describe("useDashboardRuntimeEvents", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-10T00:00:00Z"));
    FakeWebSocket.instances = [];
    vi.stubGlobal("WebSocket", FakeWebSocket);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it("opens the socket before fetching and keeps typed job and command paths active", async () => {
    const baseline = deferred<Response>();
    const fetchMock = vi.fn(() => baseline.promise);
    vi.stubGlobal("fetch", fetchMock);
    const initialJob = job("job-1");
    const { result } = renderRuntimeEvents({ initialJobs: [initialJob] });

    expect(FakeWebSocket.instances).toHaveLength(1);
    expect(fetchMock).not.toHaveBeenCalled();

    const socket = FakeWebSocket.instances[0]!;
    act(() => socket.open());

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/tenants/tenant-1/printers",
      expect.objectContaining({
        cache: "no-store",
        signal: expect.any(AbortSignal),
      }),
    );
    expect(result.current.liveState).toBe("connecting");

    const progressed = job("job-1", "dispatched");
    act(() => {
      socket.message({ type: "job_progress", job: progressed });
      socket.message({
        type: "command_result",
        command: {
          id: "command-result-1",
          tenant_id: tenant.id,
          agent_id: "agent-1",
          printer_id: "printer-1",
          kind: "printer_operation",
          status: "succeeded",
          payload_json: "{}",
          error: null,
          result_json: JSON.stringify({ sequence_id: "20000" }),
          created_at: "2026-07-10T00:00:00Z",
          updated_at: "2026-07-10T00:00:01Z",
        },
      });
    });

    expect(result.current.jobs[0]?.status).toBe("dispatched");
    expect(toast.success).toHaveBeenCalledWith(
      "Printer control completed",
      { description: "#20000" },
    );

    act(() => {
      socket.message({
        type: "command_result",
        command: {
          id: "recovery-result-1",
          tenant_id: tenant.id,
          agent_id: "agent-1",
          printer_id: "printer-1",
          kind: "printer_operation",
          status: "succeeded",
          payload_json: JSON.stringify({
            printer_id: "printer-1",
            serial_number: "20P123",
            operation: {
              type: "handle_print_error",
              error_action: "ignore",
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
      });
    });
    expect(toast.success).toHaveBeenLastCalledWith(
      "Recovery command sent; waiting for printer status confirmation",
    );

    baseline.resolve(listResponse([]));
    await flush();

    expect(result.current.liveState).toBe("live");
  });

  it("does not let resume triggers reconcile before the socket opens", async () => {
    const fetchMock = vi.fn().mockResolvedValue(listResponse([]));
    vi.stubGlobal("fetch", fetchMock);
    const { result } = renderRuntimeEvents();

    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: "visible",
    });
    act(() => {
      document.dispatchEvent(new Event("visibilitychange"));
      window.dispatchEvent(new PageTransitionEvent("pageshow"));
    });
    await flush();

    expect(FakeWebSocket.instances).toHaveLength(1);
    expect(fetchMock).not.toHaveBeenCalled();
    expect(result.current.liveState).toBe("connecting");
  });

  it("uses a resume trigger to accelerate a pending reconnect", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(new Response("unavailable", { status: 503 })),
    );
    renderRuntimeEvents();

    act(() => FakeWebSocket.instances[0]!.open());
    await flush();
    expect(FakeWebSocket.instances).toHaveLength(1);

    act(() => window.dispatchEvent(new PageTransitionEvent("pageshow")));

    expect(FakeWebSocket.instances).toHaveLength(2);
  });

  it("whole-replaces REST inventory then replays higher revisions and newer materials", async () => {
    const baseline = deferred<Response>();
    vi.stubGlobal("fetch", vi.fn(() => baseline.promise));
    const { result } = renderRuntimeEvents({
      initialPrinters: [printer("deleted"), printer("kept")],
    });
    const socket = FakeWebSocket.instances[0]!;

    act(() => {
      socket.open();
      socket.message({
        type: "printer_snapshot",
        printer: printer("kept", {
          state_revision: 3,
          print: { ...printState, progress_percent: 30 },
          materials: materials("2026-07-10T00:00:01Z"),
        }),
      });
      socket.message({
        type: "printer_snapshot",
        printer: printer("kept", {
          state_revision: 2,
          print: { ...printState, progress_percent: 20 },
          materials: materials("2026-07-10T00:00:02Z"),
        }),
      });
    });
    baseline.resolve(
      listResponse([
        printer("kept", {
          state_revision: 2,
          print: { ...printState, progress_percent: 10 },
        }),
      ]),
    );
    await flush();

    expect(result.current.printers).toHaveLength(1);
    expect(result.current.printers[0]).toMatchObject({
      id: "kept",
      state_revision: 3,
      print: { progress_percent: 30 },
      materials: { observed_at: "2026-07-10T00:00:02Z" },
    });
  });

  it("ignores an abort-resistant stale baseline that resolves after reconnect", async () => {
    const stale = deferred<Response>();
    let staleSignal: AbortSignal | undefined;
    const fetchMock = vi
      .fn()
      .mockImplementationOnce((_input: RequestInfo | URL, init?: RequestInit) => {
        staleSignal = init?.signal ?? undefined;
        return stale.promise;
      })
      .mockResolvedValueOnce(
        listResponse([printer("current", { state_revision: 5 })]),
      );
    vi.stubGlobal("fetch", fetchMock);
    const { result } = renderRuntimeEvents();
    const staleSocket = FakeWebSocket.instances[0]!;

    act(() => {
      staleSocket.open();
      staleSocket.serverClose();
    });

    expect(staleSignal?.aborted).toBe(true);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_000);
    });
    expect(FakeWebSocket.instances).toHaveLength(2);

    act(() => FakeWebSocket.instances[1]!.open());
    await flush();

    expect(result.current.liveState).toBe("live");
    expect(result.current.printers[0]).toMatchObject({
      id: "current",
      state_revision: 5,
    });

    stale.resolve(
      listResponse([printer("stale", { state_revision: 99 })]),
    );
    await flush();

    expect(result.current.liveState).toBe("live");
    expect(result.current.printers).toHaveLength(1);
    expect(result.current.printers[0]).toMatchObject({
      id: "current",
      state_revision: 5,
    });
  });

  it("uses a serialized 30-second start cadence and coalesces immediate triggers", async () => {
    const inProgress = deferred<Response>();
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(listResponse([printer("p1")]))
      .mockImplementationOnce(() => inProgress.promise)
      .mockResolvedValue(listResponse([printer("p1")]));
    vi.stubGlobal("fetch", fetchMock);
    const { result } = renderRuntimeEvents();
    act(() => FakeWebSocket.instances[0]!.open());
    await flush();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(29_999);
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(fetchMock).toHaveBeenCalledTimes(2);

    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: "visible",
    });
    act(() => {
      document.dispatchEvent(new Event("visibilitychange"));
      window.dispatchEvent(new PageTransitionEvent("pageshow"));
    });
    expect(fetchMock).toHaveBeenCalledTimes(2);

    inProgress.resolve(listResponse([printer("p1", { state_revision: 2 })]));
    await flush();
    expect(fetchMock).toHaveBeenCalledTimes(3);
    expect(result.current.liveState).toBe("live");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(29_999);
    });
    expect(fetchMock).toHaveBeenCalledTimes(3);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(fetchMock).toHaveBeenCalledTimes(4);
  });

  it("keeps the ten-second deadline armed through the response body", async () => {
    const timeoutCause = new Error("response body aborted", {
      cause: new Error("deadline elapsed"),
    });
    let bodyController!: ReadableStreamDefaultController<Uint8Array>;
    let signal: AbortSignal | undefined;
    const response = new Response(
      new ReadableStream<Uint8Array>({
        start(controller) {
          bodyController = controller;
        },
      }),
      { headers: { "content-type": "application/json" } },
    );
    const textSpy = vi.spyOn(response, "text");
    vi.stubGlobal(
      "fetch",
      vi.fn((_input: RequestInfo | URL, init?: RequestInit) => {
        signal = init?.signal ?? undefined;
        signal?.addEventListener("abort", () => bodyController.error(timeoutCause));
        return Promise.resolve(response);
      }),
    );
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const { result } = renderRuntimeEvents({
      initialPrinters: [printer("p1", { state_revision: 4 })],
    });
    const socket = FakeWebSocket.instances[0]!;
    act(() => socket.open());
    await flush();

    expect(textSpy).toHaveBeenCalledOnce();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(9_999);
    });
    expect(signal?.aborted).toBe(false);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    await flush();

    expect(signal?.aborted).toBe(true);
    expect(socket.close).toHaveBeenCalledOnce();
    expect(result.current.liveState).toBe("unavailable");
    expect(result.current.printers[0]?.state_revision).toBeUndefined();
    expect(result.current.printers[0]?.print).toBeUndefined();
    expect(errorSpy).toHaveBeenCalledWith(
      "Printer state reconciliation failed",
      timeoutCause,
    );
  });

  it("fails closed and retains the malformed JSON decode cause", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response("{malformed", {
          headers: { "content-type": "application/json" },
        }),
      ),
    );
    const decodeCause = new SyntaxError("malformed printer list");
    const parse = vi.spyOn(JSON, "parse").mockImplementationOnce(() => {
      throw decodeCause;
    });
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const { result } = renderRuntimeEvents({
      initialPrinters: [printer("p1", { state_revision: 4 })],
    });
    const socket = FakeWebSocket.instances[0]!;

    act(() => socket.open());
    await flush();

    expect(parse).toHaveBeenCalledWith("{malformed");
    expect(socket.close).toHaveBeenCalledOnce();
    expect(result.current.liveState).toBe("unavailable");
    expect(result.current.printers[0]?.state_revision).toBeUndefined();
    expect(result.current.printers[0]?.print).toBeUndefined();
    expect(errorSpy).toHaveBeenCalledWith(
      "Printer state reconciliation failed",
      decodeCause,
    );
  });

  it("retains the full printer-list network error cause chain", async () => {
    const lowLevel = new Error("connection reset");
    const networkCause = new Error("printer list unavailable", {
      cause: lowLevel,
    });
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(networkCause));
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const { result } = renderRuntimeEvents();

    act(() => FakeWebSocket.instances[0]!.open());
    await flush();

    expect(result.current.liveState).toBe("unavailable");
    expect(networkCause.cause).toBe(lowLevel);
    expect(errorSpy).toHaveBeenCalledWith(
      "Printer state reconciliation failed",
      networkCause,
    );
  });

  it("retains ticket-fetch errors without creating a socket", async () => {
    const ticketCause = new Error("ticket request unavailable", {
      cause: new Error("connection reset"),
    });
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(ticketCause));
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const { result } = renderRuntimeEvents({
      auth: { ...noAuth, source: "request_cookie", provider: "clerk" },
    });
    await flush();

    expect(FakeWebSocket.instances).toHaveLength(0);
    expect(result.current.liveState).toBe("disconnected");
    expect(errorSpy).toHaveBeenCalledWith(
      "Printer event connection failed",
      ticketCause,
    );
  });

  it("retains WebSocket construction errors", async () => {
    const constructionCause = new Error("WebSocket construction failed", {
      cause: new Error("invalid socket configuration"),
    });
    vi.stubGlobal(
      "WebSocket",
      class {
        constructor() {
          throw constructionCause;
        }
      },
    );
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const { result } = renderRuntimeEvents();
    await flush();

    expect(result.current.liveState).toBe("disconnected");
    expect(errorSpy).toHaveBeenCalledWith(
      "Printer event connection failed",
      constructionCause,
    );
  });

  it("retries immediately without restoring stale enriched initial state", async () => {
    const retriedBaseline = deferred<Response>();
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(new Response("unavailable", { status: 503 }))
      .mockImplementationOnce(() => retriedBaseline.promise);
    vi.stubGlobal("fetch", fetchMock);
    const { result } = renderRuntimeEvents({
      initialPrinters: [printer("p1", { state_revision: 4 })],
    });

    act(() => FakeWebSocket.instances[0]!.open());
    await flush();
    expect(result.current.liveState).toBe("unavailable");
    expect(result.current.printers[0]?.state_revision).toBeUndefined();

    act(() => result.current.retry());

    expect(FakeWebSocket.instances).toHaveLength(2);
    expect(result.current.printers[0]?.state_revision).toBeUndefined();
    act(() => FakeWebSocket.instances[1]!.open());
    expect(result.current.printers[0]?.state_revision).toBeUndefined();

    retriedBaseline.resolve(
      listResponse([printer("p1", { state_revision: 5 })]),
    );
    await flush();
    expect(result.current.liveState).toBe("live");
    expect(result.current.printers[0]?.state_revision).toBe(5);
  });

  it("advances retry backoff when sockets open but baselines keep failing", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(new Response("unavailable", { status: 503 })),
    );
    renderRuntimeEvents();

    act(() => FakeWebSocket.instances[0]!.open());
    await flush();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_000);
    });
    expect(FakeWebSocket.instances).toHaveLength(2);

    act(() => FakeWebSocket.instances[1]!.open());
    await flush();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_000);
    });
    expect(FakeWebSocket.instances).toHaveLength(2);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_000);
    });
    expect(FakeWebSocket.instances).toHaveLength(3);
  });

  it("rejects a decoded baseline whose monotonic deadline elapsed before apply", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => listResponse([printer("too-late", { state_revision: 8 })])),
    );
    const { result } = renderRuntimeEvents({
      initialPrinters: [printer("safe", { state_revision: 3 })],
    });
    let decoded = false;
    const parse = JSON.parse.bind(JSON);
    vi.spyOn(JSON, "parse").mockImplementation((text: string) => {
      decoded = true;
      return parse(text) as unknown;
    });
    vi.spyOn(performance, "now").mockImplementation(() =>
      decoded ? 10_001 : 0,
    );

    act(() => FakeWebSocket.instances[0]!.open());
    await flush();

    expect(result.current.liveState).toBe("unavailable");
    expect(result.current.printers.map(({ id }) => id)).toEqual(["safe"]);
    expect(result.current.printers[0]?.state_revision).toBeUndefined();
  });

  it("checks the monotonic deadline again at the state-apply boundary", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => listResponse([printer("too-late", { state_revision: 8 })])),
    );
    const { result } = renderRuntimeEvents({
      initialPrinters: [printer("safe", { state_revision: 3 })],
    });
    let decoded = false;
    let applyDelayed = false;
    const parse = JSON.parse.bind(JSON);
    vi.spyOn(JSON, "parse").mockImplementation((text: string) => {
      decoded = true;
      queueMicrotask(() => {
        applyDelayed = true;
      });
      return parse(text) as unknown;
    });
    vi.spyOn(performance, "now").mockImplementation(() => {
      if (applyDelayed) {
        return 10_001;
      }
      return decoded ? 1 : 0;
    });

    act(() => FakeWebSocket.instances[0]!.open());
    await flush();

    expect(result.current.liveState).toBe("unavailable");
    expect(result.current.printers.map(({ id }) => id)).toEqual(["safe"]);
    expect(result.current.printers[0]?.state_revision).toBeUndefined();
  });

  it("ignores same-tenant prop baselines after live state owns the inventory", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        listResponse([printer("p1", { state_revision: 5 })]),
      )
      .mockResolvedValueOnce(new Response("unavailable", { status: 503 }));
    vi.stubGlobal("fetch", fetchMock);
    const initialJobs: Job[] = [];
    const rendered = renderHook(
      ({ initialPrinters }: { initialPrinters: Printer[] }) =>
        useDashboardRuntimeEvents({
          apiUrl: "http://localhost:8080",
          auth: noAuth,
          selectedTenant: tenant,
          initialPrinters,
          initialJobs,
        }),
      {
        initialProps: {
          initialPrinters: [printer("p1", { state_revision: 4 })],
        },
        wrapper,
      },
    );

    act(() => FakeWebSocket.instances[0]!.open());
    await flush();
    expect(rendered.result.current.printers[0]?.state_revision).toBe(5);

    rendered.rerender({
      initialPrinters: [printer("p1", { state_revision: 2 })],
    });
    expect(rendered.result.current.printers[0]?.state_revision).toBe(5);

    act(() => window.dispatchEvent(new PageTransitionEvent("pageshow")));
    await flush();
    expect(rendered.result.current.liveState).toBe("unavailable");
    expect(
      rendered.result.current.printers[0]?.state_revision,
    ).toBeUndefined();

    rendered.rerender({
      initialPrinters: [printer("p1", { state_revision: 4 })],
    });
    expect(
      rendered.result.current.printers[0]?.state_revision,
    ).toBeUndefined();
  });

  it("refreshes same-tenant jobs while preserving known live progress", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(listResponse([])));
    const initialPrinters: Printer[] = [];
    const rendered = renderHook(
      ({ initialJobs }: { initialJobs: Job[] }) =>
        useDashboardRuntimeEvents({
          apiUrl: "http://localhost:8080",
          auth: noAuth,
          selectedTenant: tenant,
          initialPrinters,
          initialJobs,
        }),
      {
        initialProps: {
          initialJobs: [job("live"), job("removed")],
        },
        wrapper,
      },
    );
    const socket = FakeWebSocket.instances[0]!;
    act(() => socket.open());
    await flush();

    act(() =>
      socket.message({
        type: "job_progress",
        job: job("live", "dispatched"),
      }),
    );
    expect(rendered.result.current.jobs.find(({ id }) => id === "live")?.status).toBe(
      "dispatched",
    );

    rendered.rerender({
      initialJobs: [job("live"), job("added")],
    });

    expect(rendered.result.current.jobs.map(({ id }) => id)).toEqual([
      "live",
      "added",
    ]);
    expect(rendered.result.current.jobs[0]?.status).toBe("dispatched");
  });

  it("preserves event-only jobs that arrive during a same-tenant refresh", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(listResponse([])));
    const initialPrinters: Printer[] = [];
    const rendered = renderHook(
      ({ initialJobs }: { initialJobs: Job[] }) =>
        useDashboardRuntimeEvents({
          apiUrl: "http://localhost:8080",
          auth: noAuth,
          selectedTenant: tenant,
          initialPrinters,
          initialJobs,
        }),
      {
        initialProps: {
          initialJobs: [job("retained"), job("removed")],
        },
        wrapper,
      },
    );
    const refreshedJobs = [job("added"), job("retained")];
    const socket = FakeWebSocket.instances[0]!;
    act(() => socket.open());
    await flush();

    act(() => {
      socket.message({
        type: "job_progress",
        job: job("event-only", "dispatched"),
      });
      socket.message({
        type: "job_progress",
        job: job("retained", "dispatched"),
      });
    });
    rendered.rerender({ initialJobs: refreshedJobs });

    expect(rendered.result.current.jobs.map(({ id }) => id)).toEqual([
      "event-only",
      "added",
      "retained",
    ]);
    expect(
      rendered.result.current.jobs.find(({ id }) => id === "event-only")
        ?.status,
    ).toBe("dispatched");
    expect(
      rendered.result.current.jobs.find(({ id }) => id === "retained")
        ?.status,
    ).toBe("dispatched");
    expect(
      rendered.result.current.jobs.find(({ id }) => id === "removed"),
    ).toBeUndefined();
  });

  it("confirms an unknown printer once and replays its still-higher event", async () => {
    const baseline = deferred<Response>();
    const fetchMock = vi
      .fn()
      .mockImplementationOnce(() => baseline.promise)
      .mockResolvedValueOnce(
        listResponse([printer("new", { state_revision: 2 })]),
      );
    vi.stubGlobal("fetch", fetchMock);
    const { result } = renderRuntimeEvents();
    const socket = FakeWebSocket.instances[0]!;

    act(() => {
      socket.open();
      socket.message({
        type: "printer_snapshot",
        printer: printer("new", {
          state_revision: 3,
          print: { ...printState, progress_percent: 60 },
        }),
      });
    });
    baseline.resolve(listResponse([]));
    await flush();

    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(result.current.printers[0]).toMatchObject({
      id: "new",
      state_revision: 3,
      print: { progress_percent: 60 },
    });
  });

  it("discards all still-unknown events after one confirmation fetch", async () => {
    const baseline = deferred<Response>();
    const fetchMock = vi
      .fn()
      .mockImplementationOnce(() => baseline.promise)
      .mockResolvedValueOnce(listResponse([]));
    vi.stubGlobal("fetch", fetchMock);
    const { result } = renderRuntimeEvents();
    const socket = FakeWebSocket.instances[0]!;

    act(() => {
      socket.open();
      socket.message({ type: "printer_snapshot", printer: printer("missing-1") });
      socket.message({ type: "printer_snapshot", printer: printer("missing-2") });
    });
    baseline.resolve(listResponse([]));
    await flush();

    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(result.current.printers).toEqual([]);
  });

  it("caps a late baseline plus stuck unknown confirmation within twenty seconds", async () => {
    const baseline = deferred<Response>();
    const confirmation = abortableResponse();
    const fetchMock = vi
      .fn()
      .mockImplementationOnce(() => baseline.promise)
      .mockImplementationOnce(confirmation.fetch);
    vi.stubGlobal("fetch", fetchMock);
    const { result } = renderRuntimeEvents();
    const socket = FakeWebSocket.instances[0]!;

    act(() => {
      socket.open();
      socket.message({ type: "printer_snapshot", printer: printer("unknown") });
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(9_999);
      baseline.resolve(listResponse([]));
    });
    await flush();
    expect(fetchMock).toHaveBeenCalledTimes(2);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(9_999);
    });
    expect(confirmation.signal()?.aborted).toBe(false);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    await flush();

    expect(confirmation.signal()?.aborted).toBe(true);
    expect(result.current.liveState).toBe("unavailable");
  });

  it("repairs silently missed state within the active-page forty-second bound", async () => {
    const periodic = deferred<Response>();
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        listResponse([
          printer("p1", {
            state_revision: 4,
            print: { ...printState, print_error: 83_918_929 },
          }),
        ]),
      )
      .mockImplementationOnce(() => periodic.promise);
    vi.stubGlobal("fetch", fetchMock);
    const { result } = renderRuntimeEvents();
    act(() => FakeWebSocket.instances[0]!.open());
    await flush();
    expect(result.current.printers[0]?.print?.print_error).toBe(83_918_929);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(39_999);
      periodic.resolve(
        listResponse([
          printer("p1", {
            state_revision: 5,
            status: "IDLE",
            print: {
              ...printState,
              error_generation: 2,
              gcode_state: "IDLE",
              print_error: 0,
            },
          }),
        ]),
      );
    });
    await flush();

    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(result.current.liveState).toBe("live");
    expect(result.current.printers[0]).toMatchObject({
      state_revision: 5,
      status: "IDLE",
      print: { error_generation: 2, print_error: 0 },
    });
  });

  it("repairs immediately on pageshow and visibility restoration", async () => {
    const visibilityBaseline = deferred<Response>();
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(listResponse([]))
      .mockImplementationOnce(() => visibilityBaseline.promise)
      .mockResolvedValue(listResponse([]));
    vi.stubGlobal("fetch", fetchMock);
    renderRuntimeEvents();
    act(() => FakeWebSocket.instances[0]!.open());
    await flush();
    expect(fetchMock).toHaveBeenCalledTimes(1);

    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: "hidden",
    });
    act(() => document.dispatchEvent(new Event("visibilitychange")));
    await flush();
    expect(fetchMock).toHaveBeenCalledTimes(1);

    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: "visible",
    });
    act(() => document.dispatchEvent(new Event("visibilitychange")));
    expect(fetchMock).toHaveBeenCalledTimes(2);
    visibilityBaseline.resolve(listResponse([]));
    await flush();
    expect(fetchMock).toHaveBeenCalledTimes(2);

    act(() => window.dispatchEvent(new PageTransitionEvent("pageshow")));
    await flush();
    expect(fetchMock).toHaveBeenCalledTimes(3);
  });

  it("whole-replaces with a legacy baseline and keeps later legacy events coarse", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(listResponse([legacyPrinter("legacy")])),
    );
    const { result } = renderRuntimeEvents({
      initialPrinters: [printer("legacy", { state_revision: 9 })],
    });
    const socket = FakeWebSocket.instances[0]!;
    act(() => socket.open());
    await flush();

    expect(result.current.printers[0]?.state_revision).toBeUndefined();
    expect(result.current.printers[0]?.print).toBeUndefined();

    act(() =>
      socket.message({
        type: "printer_snapshot",
        printer: legacyPrinter("legacy", {
          status: "OFFLINE",
          materials: materials("2026-07-10T00:00:03Z"),
        }),
      }),
    );

    expect(result.current.printers[0]).toMatchObject({
      status: "OFFLINE",
      materials: { observed_at: "2026-07-10T00:00:03Z" },
    });
    expect(result.current.printers[0]?.state_revision).toBeUndefined();
    expect(result.current.printers[0]?.print).toBeUndefined();
  });

  it("aborts work, timers, and socket callbacks during cleanup", async () => {
    const pending = abortableResponse();
    vi.stubGlobal("fetch", pending.fetch);
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const rendered = renderRuntimeEvents();
    const socket = FakeWebSocket.instances[0]!;
    act(() => socket.open());

    rendered.unmount();

    expect(pending.signal()?.aborted).toBe(true);
    expect(socket.close).toHaveBeenCalledOnce();
    expect(socket.onopen).toBeNull();
    expect(socket.onmessage).toBeNull();
    expect(socket.onerror).toBeNull();
    expect(socket.onclose).toBeNull();
    await act(async () => {
      await vi.runAllTimersAsync();
    });
    expect(FakeWebSocket.instances).toHaveLength(1);
    expect(errorSpy).not.toHaveBeenCalled();
  });
});
