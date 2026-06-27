import type { Translator } from "./dashboard-runtime-helpers";

export type PrintJobForFormatting = {
  print: {
    progress_percent: number | null;
    remaining_time_minutes: number | null;
    current_layer: number | null;
    total_layers: number | null;
    last_progress_percent: number | null;
    last_layer: number | null;
  };
};

export function formatProgress(job: PrintJobForFormatting) {
  const percent = job.print.progress_percent ?? job.print.last_progress_percent;
  return percent === null ? "-" : `${percent}%`;
}

const enJobFormat: Translator = (key, values) => {
  const v = values ?? {};
  switch (key) {
    case "layersNone":
      return "Layers -";
    case "layersOpenTotal":
      return `Layers -/${v.total}`;
    case "layersOpenCurrent":
      return `Layers ${v.current}`;
    case "layersBoth":
      return `Layers ${v.current}/${v.total}`;
    case "remainingNone":
      return "Remaining -";
    case "remainingMinutes":
      return `Remaining ${v.minutes}m`;
    case "remainingHours":
      return `Remaining ${v.hours}h ${v.rest}m`;
    default:
      return key;
  }
};

export function formatLayers(job: PrintJobForFormatting, t: Translator = enJobFormat): string {
  const current = job.print.current_layer ?? job.print.last_layer;
  if (current === null && job.print.total_layers === null) {
    return t("layersNone");
  }
  if (current === null) {
    return t("layersOpenTotal", { total: job.print.total_layers ?? '-' });
  }
  if (job.print.total_layers === null) {
    return t("layersOpenCurrent", { current });
  }
  return t("layersBoth", { current, total: job.print.total_layers });
}

export function formatRemaining(minutes: number | null, t: Translator = enJobFormat): string {
  if (minutes === null) {
    return t("remainingNone");
  }
  if (minutes < 60) {
    return t("remainingMinutes", { minutes });
  }
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return t("remainingHours", { hours, rest });
}
