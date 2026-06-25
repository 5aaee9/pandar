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

export function formatLayers(job: PrintJobForFormatting) {
  const current = job.print.current_layer ?? job.print.last_layer;
  if (current === null && job.print.total_layers === null) {
    return "Layers -";
  }
  if (current === null) {
    return `Layers -/${job.print.total_layers}`;
  }
  if (job.print.total_layers === null) {
    return `Layers ${current}`;
  }
  return `Layers ${current}/${job.print.total_layers}`;
}

export function formatRemaining(minutes: number | null) {
  if (minutes === null) {
    return "Remaining -";
  }
  if (minutes < 60) {
    return `Remaining ${minutes}m`;
  }
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return `Remaining ${hours}h ${rest}m`;
}
