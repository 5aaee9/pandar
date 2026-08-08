export type PrinterCoolingMode =
  | "cooling"
  | "heating"
  | "exhaust"
  | "full_cooling";

export type PrinterCoolingFanKind =
  | "hotend"
  | "part_cooling"
  | "auxiliary"
  | "chamber"
  | "hotend_second"
  | "controller"
  | "inner_loop"
  | "auxiliary_second";

export type PrinterCoolingSystem = {
  mode?: PrinterCoolingMode | null;
  fans: Array<{
    kind: PrinterCoolingFanKind;
    speed_percent: number;
  }>;
};
