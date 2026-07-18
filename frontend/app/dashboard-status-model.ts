import type { Severity } from "./dashboard-attention";
import type { LiveState, Translator } from "./dashboard-runtime-helpers";

export const PILL_TONES: Record<Severity, string> = {
  success: "border-success/40 bg-success/10 text-success",
  warning: "border-warning/50 bg-warning/10 text-warning",
  critical: "border-destructive/40 bg-destructive/10 text-destructive",
  info: "border-border bg-muted text-muted-foreground",
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
    border: "border-destructive/40",
    surface: "bg-destructive/10",
    ink: "text-destructive",
    sub: "text-foreground/80",
    separator: "lg:before:bg-destructive/30",
  },
  warning: {
    border: "border-warning/50",
    surface: "bg-warning/10",
    ink: "text-warning",
    sub: "text-foreground/80",
    separator: "lg:before:bg-warning/40",
  },
  success: {
    border: "border-success/40",
    surface: "bg-success/10",
    ink: "text-success",
    sub: "text-foreground/80",
    separator: "lg:before:bg-success/30",
  },
  info: {
    border: "border-border",
    surface: "bg-card",
    ink: "text-foreground",
    sub: "text-muted-foreground",
    separator: "lg:before:bg-border",
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
