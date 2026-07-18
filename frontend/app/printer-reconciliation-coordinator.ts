import type { AuthMetadata, Job, Printer, PrinterEvent } from "./dashboard-types";
import { createDashboardRuntimeEventHandler, printerOfflineNotifications, type CommandResultTranslator } from "./dashboard-runtime-event-notifications";
import type { LiveState, RuntimeNotification } from "./dashboard-runtime-helpers";
import { fetchAuthoritativePrinters, printerEventConnectionUrl, requestPrinterEventTicket } from "./printer-event-transport";
import { clearEnrichedPrinterState, replayPrinterEvents, replacePrinterInventory } from "./printer-reconciliation";

const retryDelays = [1_000, 2_000, 5_000, 10_000];
const reconciliationIntervalMs = 30_000;
const reconciliationDeadlineMs = 10_000;

type CoordinatorOptions = {
  apiUrl: string;
  authSource: AuthMetadata["source"];
  tenantId: string;
  translateCommandResult: CommandResultTranslator;
  getPrinters: () => Printer[];
  getJobs: () => Job[];
  setPrinters: (printers: Printer[]) => void;
  applyJobProgress: (job: Job) => void;
  setLiveState: (state: LiveState) => void;
  setLastEventAt: (timestamp: string) => void;
  addNotification: (notification: RuntimeNotification) => void;
};

type Cycle = { socket: WebSocket; buffer: Printer[]; pending: boolean; token: number };
type BaselineAttempt = { printers: Printer[]; token: number; deadline: number; controller: AbortController };

export function startPrinterReconciliationCoordinator(
  options: CoordinatorOptions,
): PrinterReconciliationCoordinator {
  let stopped = false;
  let socket: WebSocket | null = null;
  let socketOpened = false;
  let bootstrapComplete = false;
  let activeCycle: Cycle | null = null;
  let activeAbort: AbortController | null = null;
  let connectionAbort: AbortController | null = null;
  let retryTimer: ReturnType<typeof setTimeout> | null = null;
  let cadenceTimer: ReturnType<typeof setTimeout> | null = null;
  let ownerToken = 0;
  let connectionToken = 0;
  let failures = 0;
  let outage = 0;
  let notifiedOutage = -1;
  const handleEvent = createDashboardRuntimeEventHandler({
    getPrinterBuffer: () => activeCycle?.buffer ?? null,
    triggerPrinterResync: (printer) => triggerCycle([printer]),
    getPrinters: options.getPrinters,
    getJobs: options.getJobs,
    setPrinters: options.setPrinters,
    applyJobProgress: options.applyJobProgress,
    setLastEventAt: options.setLastEventAt,
    addNotification: options.addNotification,
    translateCommandResult: options.translateCommandResult,
  });

  function clearSocketCallbacks(candidate: WebSocket) {
    candidate.onopen = null;
    candidate.onmessage = null;
    candidate.onerror = null;
    candidate.onclose = null;
  }

  function closeSocket(candidate: WebSocket) {
    clearSocketCallbacks(candidate);
    candidate.close();
  }

  function owns(cycle: Cycle, token: number) {
    return !stopped && activeCycle === cycle && socket === cycle.socket && ownerToken === token;
  }

  function canApply(cycle: Cycle, attempt: BaselineAttempt) {
    if (!owns(cycle, attempt.token)) {
      return false;
    }
    if (performance.now() >= attempt.deadline) {
      attempt.controller.abort();
      throw new Error("printer list deadline exceeded before apply");
    }
    return true;
  }

  function invalidateCycle() {
    ownerToken += 1;
    activeAbort?.abort();
    activeAbort = null;
    activeCycle = null;
    if (cadenceTimer) {
      clearTimeout(cadenceTimer);
      cadenceTimer = null;
    }
  }

  function addPrinterNotifications(previous: Printer[], current: Printer[]) {
    for (const notification of printerOfflineNotifications(previous, current, new Date().toISOString())) {
      options.addNotification(notification);
    }
  }

  function scheduleRetry(unavailable: boolean) {
    if (stopped) {
      return;
    }
    const delay = retryDelays[Math.min(failures - 1, retryDelays.length - 1)]!;
    options.setLiveState(
      unavailable || failures >= 3 ? "unavailable" : "disconnected",
    );
    if (notifiedOutage !== outage) {
      notifiedOutage = outage;
      options.addNotification({
        key: `live:${options.tenantId}:disconnected:${outage}`,
        titleKey: { namespace: "runtime.notification", key: "liveTitle" },
        detailKey: {
          namespace: "runtime.notification",
          key:
            unavailable || failures >= 3
              ? "liveRetryingUnavailable"
              : "liveDisconnectedRetrying",
        },
        timestamp: new Date().toISOString(),
      });
    }
    if (retryTimer) {
      clearTimeout(retryTimer);
    }
    retryTimer = setTimeout(() => void connect(), delay);
  }

  function failCycle(cycle: Cycle) {
    if (activeCycle !== cycle) {
      return;
    }
    invalidateCycle();
    options.setPrinters(clearEnrichedPrinterState(options.getPrinters()));
    const failedSocket = socket;
    socket = null;
    socketOpened = false;
    bootstrapComplete = false;
    if (failedSocket) {
      closeSocket(failedSocket);
    }
    failures += 1;
    scheduleRetry(true);
  }

  async function fetchBaseline(cycle: Cycle) {
    const controller = new AbortController();
    const token = ++ownerToken;
    cycle.token = token;
    activeAbort = controller;
    const deadline = performance.now() + reconciliationDeadlineMs;
    const timeout = setTimeout(
      () => controller.abort(),
      reconciliationDeadlineMs,
    );
    try {
      const printers = await fetchAuthoritativePrinters(
        options.tenantId,
        controller,
        deadline,
      );
      return owns(cycle, token)
        ? { printers, token, deadline, controller }
        : null;
    } finally {
      clearTimeout(timeout);
      if (activeAbort === controller) {
        activeAbort = null;
      }
    }
  }

  function finishCycle(cycle: Cycle, printers: Printer[], previous: Printer[]) {
    if (!owns(cycle, cycle.token)) {
      return;
    }
    options.setPrinters(printers);
    addPrinterNotifications(previous, printers);
    if (!bootstrapComplete) {
      bootstrapComplete = true;
      failures = 0;
      outage += 1;
    }
    const rerun = cycle.pending;
    activeCycle = null;
    if (rerun) {
      triggerCycle();
    } else {
      options.setLiveState("live");
    }
  }

  async function runCycle(cycle: Cycle) {
    const previous = options.getPrinters();
    try {
      const fetched = await fetchBaseline(cycle);
      if (!fetched || !owns(cycle, fetched.token)) {
        return;
      }
      let finalAttempt = fetched;
      let baseline = replacePrinterInventory(fetched.printers);
      if (!canApply(cycle, fetched)) {
        return;
      }
      options.setPrinters(baseline);
      let replayed = replayPrinterEvents(baseline, cycle.buffer);
      if (!canApply(cycle, fetched)) {
        return;
      }
      options.setPrinters(replayed.printers);

      if (replayed.resync) {
        const confirmed = await fetchBaseline(cycle);
        if (!confirmed || !owns(cycle, confirmed.token)) {
          return;
        }
        finalAttempt = confirmed;
        baseline = replacePrinterInventory(confirmed.printers);
        if (!canApply(cycle, confirmed)) {
          return;
        }
        options.setPrinters(baseline);
        replayed = replayPrinterEvents(baseline, cycle.buffer);
        if (!canApply(cycle, confirmed)) {
          return;
        }
      }
      if (!canApply(cycle, finalAttempt)) {
        return;
      }
      finishCycle(cycle, replayed.printers, previous);
    } catch (error) {
      if (activeCycle !== cycle) {
        return;
      }
      console.error("Printer state reconciliation failed", error);
      failCycle(cycle);
    }
  }

  function triggerCycle(seed: Printer[] = []) {
    if (stopped || socket === null || !socketOpened) {
      return;
    }
    if (activeCycle) {
      activeCycle.buffer.push(...seed);
      activeCycle.pending = true;
      return;
    }
    if (cadenceTimer) {
      clearTimeout(cadenceTimer);
    }
    cadenceTimer = setTimeout(triggerCycle, reconciliationIntervalMs);
    const cycle: Cycle = {
      socket,
      buffer: [...seed],
      pending: false,
      token: 0,
    };
    activeCycle = cycle;
    options.setLiveState("connecting");
    void runCycle(cycle);
  }

  async function connect() {
    if (stopped) {
      return;
    }
    options.setLiveState("connecting");
    const token = ++connectionToken;
    const controller = new AbortController();
    connectionAbort = controller;
    try {
      const ticket =
        options.authSource === "none"
          ? null
          : await requestPrinterEventTicket(
              options.tenantId,
              controller.signal,
            );
      if (stopped || token !== connectionToken) {
        return;
      }
      connectionAbort = null;
      const candidate = new WebSocket(
        printerEventConnectionUrl(options.apiUrl, options.tenantId, ticket),
      );
      socket = candidate;
      socketOpened = false;
      bootstrapComplete = false;
      candidate.onopen = () => {
        if (stopped || socket !== candidate) {
          return;
        }
        socketOpened = true;
        triggerCycle();
      };
      candidate.onmessage = (message) => {
        if (!stopped && socket === candidate) {
          handleEvent(JSON.parse(message.data as string) as PrinterEvent);
        }
      };
      candidate.onerror = () => candidate.close();
      candidate.onclose = () => {
        if (stopped || socket !== candidate) {
          return;
        }
        clearSocketCallbacks(candidate);
        socket = null;
        socketOpened = false;
        bootstrapComplete = false;
        invalidateCycle();
        failures += 1;
        scheduleRetry(false);
      };
    } catch (error) {
      if (stopped || token !== connectionToken) {
        return;
      }
      console.error("Printer event connection failed", error);
      connectionAbort = null;
      failures += 1;
      scheduleRetry(false);
    }
  }

  function onVisibilityChange() {
    if (document.visibilityState === "visible") {
      triggerImmediateCycle();
    }
  }

  function onPageShow() {
    triggerImmediateCycle();
  }

  function triggerImmediateCycle() {
    if (stopped) {
      return;
    }
    if (socket === null) {
      if (connectionAbort === null && retryTimer) {
        clearTimeout(retryTimer);
        retryTimer = null;
        void connect();
      }
      return;
    }
    if (socketOpened) {
      triggerCycle();
    }
  }

  document.addEventListener("visibilitychange", onVisibilityChange);
  window.addEventListener("pageshow", onPageShow);
  void connect();

  function stop() {
    stopped = true;
    disconnect();
    document.removeEventListener("visibilitychange", onVisibilityChange);
    window.removeEventListener("pageshow", onPageShow);
  }

  function retry() {
    if (stopped) {
      return;
    }
    disconnect();
    failures = 0;
    void connect();
  }

  function disconnect() {
    connectionToken += 1;
    connectionAbort?.abort();
    connectionAbort = null;
    invalidateCycle();
    if (retryTimer) {
      clearTimeout(retryTimer);
      retryTimer = null;
    }
    if (socket) {
      closeSocket(socket);
      socket = null;
    }
    socketOpened = false;
    bootstrapComplete = false;
  }

  return { retry, stop };
}

export type PrinterReconciliationCoordinator = {
  retry: () => void;
  stop: () => void;
};
