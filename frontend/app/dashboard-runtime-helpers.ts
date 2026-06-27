import type { AuthMetadata, Job, Printer } from "./dashboard-types";
import { formatDate as formatDateDefault } from "./dashboard-ui";

export type Translator = (key: string, values?: Record<string, string | number>) => string;

type DateFmt = (value: string) => string;

export type LiveState =
  | "idle"
  | "connecting"
  | "live"
  | "disconnected"
  | "unavailable"
  | "error";

export type TextKey = {
  namespace: string;
  key: string;
  values?: Record<string, string | number>;
};

export type RuntimeNotification = {
  key: string;
  titleKey: TextKey;
  detailKey: TextKey;
  timestamp: string;
};

const enLiveState: Record<LiveState, string> = {
  live: "Connected",
  connecting: "Connecting",
  disconnected: "Reconnecting",
  idle: "Idle",
  unavailable: "Unavailable",
  error: "Unavailable",
};

export function formatLiveState(state: LiveState, t: Translator = (k) => enLiveState[state]): string {
  switch (state) {
    case "live":
      return t("live");
    case "connecting":
      return t("connecting");
    case "disconnected":
      return t("disconnected");
    case "idle":
      return t("idle");
    case "unavailable":
    case "error":
      return t("unavailable");
  }
}

export function mergePrinter(printers: Printer[], printer: Printer) {
  return printers.some((current) => current.id === printer.id)
    ? printers.map((current) => (current.id === printer.id ? printer : current))
    : [printer, ...printers];
}

export function mergeJob(jobs: Job[], job: Job) {
  return jobs.some((current) => current.id === job.id)
    ? jobs.map((current) => (current.id === job.id ? job : current))
    : [job, ...jobs];
}

export function printerEventWebSocketUrl(
  apiUrl: string,
  tenantId: string,
  ticket: string,
) {
  const base = new URL(apiUrl);
  const basePath = base.pathname.replace(/\/$/, "");
  const url = new URL(
    `${basePath}/api/v1/tenants/${encodeURIComponent(tenantId)}/printer-events`,
    base,
  );
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  url.searchParams.set("ticket", ticket);
  return url.toString();
}

const enAuthSource: Record<AuthMetadata["source"], string> = {
  request_cookie: "Request cookie",
  app_auth_bearer_token: "App bearer token",
  app_api_token: "App API token",
  none: "No auth",
};

export function formatAuthSource(
  source: AuthMetadata["source"],
  t: Translator = (k) => enAuthSource[source],
): string {
  return t(source);
}

const enMaterial: Translator = (key, values) => {
  const v = values ?? {};
  switch (key) {
    case "noMaterial":
      return "No material state";
    case "awaitingReport":
      return "Awaiting printer report";
    case "externalSpool":
      return "External spool";
    case "amsSlot":
      return `AMS ${v.ams}:${v.tray}`;
    case "noActiveTray":
      return "No active tray";
    case "amsSummary":
      return `${v.trays} AMS tray${v.trays === 1 ? "" : "s"}, ${v.external} external`;
    case "activeDetail":
      return `${v.active} · ${v.observed}`;
    case "externalSlot":
      return `external ${v.tray}`;
    case "usageRow":
      return `${v.index}: ${v.slot} ${v.type}`;
    case "amsMapping":
      return `ams_mapping ${v.count}`;
    case "amsMapping2":
      return `ams_mapping2 ${v.count}`;
    case "noMapping":
      return "No material mapping";
    case "noMetadata":
      return "No slicer metadata";
    case "plate":
      return `plate ${v.id}`;
    case "plateNone":
      return "plate -";
    case "noObjects":
      return "no objects";
    case "noFilament":
      return "no filament";
    case "artifactSummary":
      return `${v.name} · ${v.plate} · ${v.objects} · ${v.filament}`;
    default:
      return key;
  }
};

export function formatPrinterMaterials(
  printer: Printer,
  t: Translator = enMaterial,
  formatDate: DateFmt = formatDateDefault,
) {
  const materials = printer.materials;
  if (!materials) {
    return { summary: t("noMaterial"), detail: t("awaitingReport") };
  }
  const amsTrays = materials.ams_units.reduce(
    (count, unit) =>
      count + (unit.trays?.filter((tray) => tray.exists !== false).length ?? 0),
    0,
  );
  const external = materials.external_spools.filter(
    (spool) => spool.exists !== false,
  ).length;
  const active = materials.active_tray
    ? materials.active_tray.kind === "external"
      ? t("externalSpool")
      : t("amsSlot", { ams: materials.active_tray.ams_id ?? "-", tray: materials.active_tray.tray_id ?? "-" })
    : t("noActiveTray");
  return {
    summary: t("amsSummary", { trays: amsTrays, external }),
    detail: t("activeDetail", { active, observed: formatDate(materials.observed_at) }),
  };
}

export function formatJobMaterial(job: Job, t: Translator = enMaterial): string {
  const usage = job.material.filament_usage;
  if (usage.length > 0) {
    return usage
      .map((row) => {
        const slot =
          row.external_id !== null
            ? t("externalSlot", { tray: row.tray_id ?? "-" })
            : t("amsSlot", { ams: row.ams_id ?? "-", tray: row.tray_id ?? "-" });
        return t("usageRow", { index: row.slot_index, slot, type: row.filament_type ?? row.filament_id ?? "" }).trim();
      })
      .join(", ");
  }
  const mappings = [
    job.material.ams_mapping
      ? t("amsMapping", { count: job.material.ams_mapping.length })
      : null,
    job.material.ams_mapping2
      ? t("amsMapping2", { count: job.material.ams_mapping2.length })
      : null,
  ].filter(Boolean);
  return mappings.length > 0 ? mappings.join(", ") : t("noMapping");
}

export function formatArtifactMetadata(
  job: Job,
  t: Translator = enMaterial,
  formatDate: DateFmt = formatDateDefault,
): string {
  const metadata = job.artifact.metadata;
  if (!metadata) {
    return t("noMetadata");
  }

  const plate =
    metadata.plates.find(
      (candidate) => candidate.plate_id === metadata.default_plate_id,
    ) ?? metadata.plates[0];
  const plateLabel = metadata.default_plate_id
    ? t("plate", { id: metadata.default_plate_id })
    : t("plateNone");
  const objects = plate?.objects.length
    ? plate.objects.join(", ")
    : t("noObjects");
  const filament =
    plate?.filaments
      .map((row) => row.filament_type ?? row.filament_id)
      .filter(Boolean)
      .join(", ") || t("noFilament");

  return t("artifactSummary", { name: metadata.display_name, plate: plateLabel, objects, filament });
}

const enRecoveryState: Record<string, string> = {
  printing: "Printing now",
  completed: "Print completed",
  failed: "Print failed",
  cancelled: "Print cancelled",
  waitingAgent: "Waiting for the agent to come back online",
  fileFailed: "Could not send the file to the printer",
  mqttFailed: "Printer did not accept the start command",
  queueFailed: "Could not queue the job at the hub",
  waitingStart: "Waiting for the print to start",
};

export function jobRecoveryStateKey(job: Job): string {
  const dispatch = job.status.toLowerCase();
  const command = job.command.status.toLowerCase();
  const physical = job.print.status.toLowerCase();
  const message = `${job.error ?? ""} ${job.print.error ?? ""}`.toLowerCase();

  if (physical === "running") {
    return "printing";
  }
  if (physical === "completed") {
    return "completed";
  }
  if (physical === "failed") {
    return "failed";
  }
  if (physical === "cancelled") {
    return "cancelled";
  }
  if (dispatch === "queued" || command === "queued") {
    return "waitingAgent";
  }
  if (
    message.includes("upload") ||
    message.includes("transfer") ||
    message.includes("sftp") ||
    message.includes("file")
  ) {
    return "fileFailed";
  }
  if (message.includes("mqtt") || message.includes("publish")) {
    return "mqttFailed";
  }
  if (dispatch === "failed" || command === "failed") {
    return "queueFailed";
  }
  return "waitingStart";
}

export function formatJobRecoveryState(job: Job, t: Translator = (k) => enRecoveryState[k]): string {
  return t(jobRecoveryStateKey(job));
}
