import type { Severity } from "./dashboard-attention";
import type { LiveState, Translator } from "./dashboard-runtime-helpers";

export const PILL_TONES: Record<Severity, string> = {
  success: "border-emerald-200 bg-emerald-50 text-emerald-800",
  warning: "border-amber-200 bg-amber-50 text-amber-800",
  critical: "border-red-200 bg-red-50 text-red-800",
  info: "border-slate-200 bg-slate-100 text-slate-700",
};

const TONES: Record<
  Severity,
  {
    border: string;
    surface: string;
    ink: string;
    sub: string;
    separator: string;
  }
> = {
  critical: {
    border: "border-red-200 dark:border-red-900/60",
    surface: "bg-red-50 dark:bg-red-950/30",
    ink: "text-red-900 dark:text-red-50",
    sub: "text-red-800 dark:text-red-200/80",
    separator: "lg:before:bg-red-200 dark:lg:before:bg-red-900/60",
  },
  warning: {
    border: "border-amber-200 dark:border-amber-900/60",
    surface: "bg-amber-50 dark:bg-amber-950/30",
    ink: "text-amber-900 dark:text-amber-50",
    sub: "text-amber-800 dark:text-amber-200/80",
    separator: "lg:before:bg-amber-200 dark:lg:before:bg-amber-900/60",
  },
  success: {
    border: "border-emerald-200 dark:border-emerald-900/60",
    surface: "bg-emerald-50 dark:bg-emerald-950/30",
    ink: "text-emerald-900 dark:text-emerald-50",
    sub: "text-emerald-800 dark:text-emerald-200/80",
    separator: "lg:before:bg-emerald-200 dark:lg:before:bg-emerald-900/60",
  },
  info: {
    border: "border-slate-200 dark:border-border",
    surface: "bg-white dark:bg-card",
    ink: "text-slate-900 dark:text-foreground",
    sub: "text-slate-600 dark:text-muted-foreground",
    separator: "lg:before:bg-slate-200 dark:lg:before:bg-border",
  },
};

type Verdict = {
  title: string;
  detail: string;
  severity: Severity;
  tone: {
    border: string;
    surface: string;
    ink: string;
    sub: string;
    separator: string;
  };
};

const enVerdict: Translator = (key, values) => {
  const v = values ?? {};
  switch (key) {
    case "noFleet.title":
      return "No fleet configured";
    case "noFleet.detail":
      return "Connect an agent to start monitoring your printers.";
    case "liveUnavailable.title":
      return "Live updates unavailable";
    case "liveUnavailable.detail":
      return "Reconnecting - showing the last known state.";
    case "liveDisconnected.title":
      return "Live updates disconnected";
    case "liveDisconnected.detail":
      return "Reconnecting - showing the last known state.";
    case "nominal.title":
      return "All systems nominal";
    case "nominal.detail":
      return "No exceptions across the fleet.";
    case "needAttention.title": {
      const count = (v.count as number) ?? 0;
      return `${count} ${count === 1 ? "item" : "items"} need attention`;
    }
    case "needAttention.detailCritical":
      return "Failures detected - review below.";
    case "needAttention.detailOther":
      return "Review the items below.";
    default:
      return key;
  }
};

export function computeVerdict(
  args: {
    attentionCount: number;
    topSeverity: Severity | null;
    liveState: LiveState;
    fleetEmpty: boolean;
  },
  t: Translator = enVerdict,
): Verdict {
  const { attentionCount, topSeverity, liveState, fleetEmpty } = args;

  if (fleetEmpty) {
    return {
      title: t("noFleet.title"),
      detail: t("noFleet.detail"),
      severity: "info",
      tone: TONES.info,
    };
  }

  if (liveState === "unavailable" || liveState === "error") {
    return {
      title: t("liveUnavailable.title"),
      detail: t("liveUnavailable.detail"),
      severity: "warning",
      tone: TONES.warning,
    };
  }
  if (liveState === "disconnected") {
    return {
      title: t("liveDisconnected.title"),
      detail: t("liveDisconnected.detail"),
      severity: "warning",
      tone: TONES.warning,
    };
  }

  if (attentionCount === 0) {
    return {
      title: t("nominal.title"),
      detail: t("nominal.detail"),
      severity: "success",
      tone: TONES.success,
    };
  }

  const severity = topSeverity ?? "warning";
  return {
    title: t("needAttention.title", { count: attentionCount }),
    detail:
      severity === "critical"
        ? t("needAttention.detailCritical")
        : t("needAttention.detailOther"),
    severity,
    tone: severity === "critical" ? TONES.critical : TONES.warning,
  };
}
