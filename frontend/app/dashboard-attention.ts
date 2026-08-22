import type { Agent, Job, Printer } from "./dashboard-types";

export type Severity = "critical" | "warning" | "success" | "info";

export const OFFLINE_PRINTER_STATUSES = new Set(["offline", "problem"]);
const ONLINE_AGENT_STATUSES = new Set(["online"]);
const HEALTHY_AGENT_STATUSES = new Set(["online", "connecting"]);
const TERMINAL_JOB_STATUSES = new Set([
  "stalled",
  "completed",
  "failed",
  "cancelled",
]);

const SEVERITY_RANK: Record<Severity, number> = {
  critical: 0,
  warning: 1,
  info: 2,
  success: 3,
};

export type Health = {
  printersTotal: number;
  printersOnline: number;
  agentsTotal: number;
  agentsConnected: number;
  jobsActive: number;
  jobsFailed: number;
};

export function computeHealth(
  agents: Agent[],
  printers: Printer[],
  jobs: Job[],
): Health {
  return {
    printersTotal: printers.length,
    printersOnline: printers.filter(
      (printer) => !OFFLINE_PRINTER_STATUSES.has(printer.status.toLowerCase()),
    ).length,
    agentsTotal: agents.length,
    agentsConnected: agents.filter((agent) =>
      ONLINE_AGENT_STATUSES.has(agent.status.toLowerCase()),
    ).length,
    jobsActive: jobs.filter(isJobActive).length,
    jobsFailed: jobs.filter(isJobFailed).length,
  };
}

function isJobActive(job: Job): boolean {
  return (
    !TERMINAL_JOB_STATUSES.has(job.status.toLowerCase()) &&
    !TERMINAL_JOB_STATUSES.has(job.print.status.toLowerCase())
  );
}

export function isJobStalled(job: Job): boolean {
  return job.print.status.toLowerCase() === "stalled";
}

function isJobFailed(job: Job): boolean {
  return (
    job.status.toLowerCase() === "failed" ||
    job.print.status.toLowerCase() === "failed"
  );
}

export type AttentionReason =
  | "agent_unhealthy"
  | "printer_offline"
  | "job_print_failed"
  | "job_dispatch_failed"
  | "job_stalled";

export type TextKey = {
  namespace: string;
  key: string;
  values?: Record<string, string | number>;
};

export type AttentionItem = {
  id: string;
  agentId: string;
  agentName: string;
  severity: Severity;
  kind: "agent" | "printer" | "job";
  reason: AttentionReason;
  title: string;
  label: string;
  detailKey?: TextKey;
  titleKey: TextKey;
  labelKey: TextKey;
  mono: string;
  sectionId: string;
  ageMs: number | null;
};

export function computeAttention(args: {
  agents: Agent[];
  printers: Printer[];
  jobs: Job[];
  nowMs: number;
}): AttentionItem[] {
  const { agents, printers, jobs } = args;
  const items: AttentionItem[] = [];

  for (const agent of agents) {
    if (!HEALTHY_AGENT_STATUSES.has(agent.status.toLowerCase())) {
      items.push({
        id: `agent:${agent.id}`,
        agentId: agent.id,
        agentName: agent.name,
        severity: statusSeverity(agent.status),
        kind: "agent",
        reason: "agent_unhealthy",
        title: `Agent ${prettifyToken(agent.status)}`,
        label: `${agent.name} is ${agent.status || "offline"}`,
        titleKey: {
          namespace: "attention.agent",
          key: "title",
          values: { status: prettifyToken(agent.status) },
        },
        labelKey: {
          namespace: "attention.agent",
          key: "label",
          values: { name: agent.name, status: agent.status || "offline" },
        },
        mono: agent.id,
        sectionId: "printers",
        ageMs: null,
      });
    }
  }

  for (const printer of printers) {
    if (OFFLINE_PRINTER_STATUSES.has(printer.status.toLowerCase())) {
      items.push({
        id: `printer:${printer.id}`,
        agentId: printer.agent_id,
        agentName: agentName(agents, printer.agent_id),
        severity: statusSeverity(printer.status),
        kind: "printer",
        reason: "printer_offline",
        title: `Printer ${prettifyToken(printer.status)}`,
        label: `${printer.name} is ${printer.status}`,
        titleKey: {
          namespace: "attention.printer",
          key: "title",
          values: { status: prettifyToken(printer.status) },
        },
        labelKey: {
          namespace: "attention.printer",
          key: "label",
          values: { name: printer.name, status: printer.status },
        },
        mono: printer.serial_number,
        sectionId: "printers",
        ageMs: null,
      });
    }
  }

  for (const job of jobs) {
    const artifactFilename = job.artifact.filename;
    if (isJobFailed(job)) {
      const physical = job.print.status.toLowerCase() === "failed";
      const failureReason = physical ? job.print.error : job.error;
      items.push({
        id: `job:${job.id}:failed`,
        agentId: job.agent_id,
        agentName: agentName(agents, job.agent_id),
        severity: statusSeverity(physical ? job.print.status : job.status),
        kind: "job",
        reason: physical ? "job_print_failed" : "job_dispatch_failed",
        title: physical ? "Print failed" : "Dispatch failed",
        label: artifactFilename,
        ...(failureReason
          ? {
              detailKey: {
                namespace: "attention",
                key: "failureReason",
                values: { reason: failureReason },
              },
            }
          : {}),
        titleKey: {
          namespace: physical
            ? "attention.jobPrintFailed"
            : "attention.jobDispatchFailed",
          key: "title",
        },
        labelKey: {
          namespace: "job",
          key: "filename",
          values: { filename: artifactFilename },
        },
        mono: job.id,
        sectionId: "recovery",
        ageMs: null,
      });
    } else if (isJobStalled(job)) {
      items.push({
        id: `job:${job.id}:stale`,
        agentId: job.agent_id,
        agentName: agentName(agents, job.agent_id),
        severity: "warning",
        kind: "job",
        reason: "job_stalled",
        title: "Job stalled",
        label: `${artifactFilename} · did not start within 15 minutes`,
        titleKey: { namespace: "attention.jobStalled", key: "title" },
        labelKey: {
          namespace: "attention.jobStalled",
          key: "label",
          values: { filename: artifactFilename },
        },
        mono: job.id,
        sectionId: "jobs",
        ageMs: null,
      });
    }
  }

  return items.sort((a, b) => {
    if (a.agentName !== b.agentName)
      return a.agentName.localeCompare(b.agentName);
    return SEVERITY_RANK[a.severity] - SEVERITY_RANK[b.severity];
  });
}

export function topSeverityOf(items: AttentionItem[]): Severity | null {
  let top: Severity | null = null;
  for (const item of items) {
    if (top === null || SEVERITY_RANK[item.severity] < SEVERITY_RANK[top]) {
      top = item.severity;
    }
  }
  return top;
}

function agentName(agents: Agent[], id: string): string {
  return agents.find((agent) => agent.id === id)?.name ?? "Unknown agent";
}

const STATUS_SEVERITY: Array<{ severity: Severity; tokens: string[] }> = [
  {
    severity: "success",
    tokens: [
      "online",
      "ok",
      "succeeded",
      "completed",
      "running",
      "printing",
      "ready",
    ],
  },
  {
    severity: "warning",
    tokens: [
      "warning",
      "queued",
      "sent",
      "acknowledged",
      "connecting",
      "problem",
      "degraded",
      "stalled",
      "pending",
    ],
  },
  {
    severity: "critical",
    tokens: ["failed", "offline", "unavailable", "error", "down"],
  },
];

const STATUS_SEVERITY_LOOKUP = STATUS_SEVERITY.map((group) => ({
  severity: group.severity,
  tokens: new Set(group.tokens),
}));

function statusSeverity(value: string): Severity {
  const normalized = value.toLowerCase();
  for (const group of STATUS_SEVERITY_LOOKUP) {
    if (group.tokens.has(normalized)) {
      return group.severity;
    }
  }
  return "info";
}

export type TokenTranslator = (token: string) => string | undefined;

export function prettifyToken(
  value: string,
  tokenTranslator?: TokenTranslator,
): string {
  const translated = tokenTranslator?.(value.toLowerCase());
  if (translated) {
    return translated;
  }
  const cleaned = value.replace(/[_-]+/g, " ").trim();
  return cleaned.length
    ? cleaned.charAt(0).toUpperCase() + cleaned.slice(1)
    : value;
}

export function statusMeta(
  value: string,
  tokenTranslator?: TokenTranslator,
): { severity: Severity; label: string } {
  return {
    severity: statusSeverity(value),
    label: prettifyToken(value, tokenTranslator),
  };
}
