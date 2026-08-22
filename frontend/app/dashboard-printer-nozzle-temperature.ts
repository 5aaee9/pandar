import type { Printer } from "./dashboard-types";

export function presentNozzles(
  nozzles: NonNullable<Printer["nozzle_temperatures"]>,
) {
  const present: NonNullable<Printer["nozzle_temperatures"]> = [];
  for (const nozzle of nozzles) {
    if (nozzle.current_celsius) {
      present.push({
        ...nozzle,
        label: nozzle.label ?? String(present.length + 1),
      });
    }
  }
  return present;
}

export function formatTemperatureValue(value?: string | null, suffix = true) {
  if (!value) {
    return suffix ? "-°C" : "-°";
  }
  const parsed = Number(value);
  const text = Number.isFinite(parsed) ? `${Math.round(parsed)}` : value;
  return suffix ? `${text}°C` : `${text}°`;
}

export function hasActiveTargetTemperature(value?: string | null) {
  if (!value) {
    return false;
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0;
}
