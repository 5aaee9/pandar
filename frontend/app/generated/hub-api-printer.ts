// Generated from contracts/hub-client.openapi.json. Do not edit.

export type Capability = "supported" | "unsupported" | "unknown";

export type NozzleLayout =
  | "single"
  | "main_auxiliary"
  | "left_right"
  | "unknown";

export type CoolingMode = "cooling" | "heating" | "exhaust" | "full_cooling";

export type CoolingFanKind =
  | "hotend"
  | "part_cooling"
  | "auxiliary"
  | "chamber"
  | "hotend_second"
  | "controller"
  | "inner_loop"
  | "auxiliary_second";

export type PrinterNozzleTemperature = {
  label?: string | null;
  current_celsius?: string | null;
  target_celsius?: string | null;
  diameter_mm?: string | null;
  nozzle_type?: string | null;
};

export type CalibrationOption = {
  modes: Array<0 | 1 | 2>;
  default_mode: 0 | 1 | 2;
};

export type CompatibilityFeatures = {
  chamber_temperature: Capability;
  drying: Capability;
  dual_nozzle: Capability;
  flow_calibration: Capability;
  vibration_calibration: Capability;
  nozzle_offset_calibration: Capability;
  live_controls: Capability;
};

export type PrintOptionCapabilities = {
  timelapse: boolean;
  bed_leveling: CalibrationOption | null;
  flow_calibration: CalibrationOption | null;
  nozzle_offset_calibration: CalibrationOption | null;
};

export type PrinterCompatibility = {
  normalized_model: string | null;
  external_storage: Capability;
  ftps_tls_1_2_cap: boolean;
  features: CompatibilityFeatures;
  print_options: PrintOptionCapabilities;
  chamber_fan: Capability;
  nozzle_layout: NozzleLayout;
};

export type CoolingFan = {
  kind: CoolingFanKind;
  speed_percent: number;
};

export type CoolingSystem = {
  mode?: CoolingMode | null;
  fans: Array<CoolingFan>;
};

export type AmsTray = {
  tray_id?: string;
  type?: string | null;
  color?: string | null;
  multi_color?: Array<string> | null;
  filament_id?: string | null;
  setting_id?: string | null;
  name?: string | null;
  remaining_estimate?: string | number | null;
  k_value?: string | number | null;
  toolhead?: string | null;
  global_tray_id?: number | null;
  exists?: boolean | null;
};

export type AmsUnit = {
  unit_id?: string;
  unit_kind?: string | null;
  humidity?: string | number | null;
  humidity_level?: string | number | null;
  temperature_celsius?: string | number | null;
  dry_status?: string | number | null;
  dry_time_minutes?: string | number | null;
  toolhead?: string | null;
  trays?: Array<AmsTray>;
};

export type ExternalSpool = {
  external_id?: string;
  tray_id?: string;
  type?: string | null;
  color?: string | null;
  multi_color?: Array<string> | null;
  filament_id?: string | null;
  setting_id?: string | null;
  name?: string | null;
  remaining_estimate?: string | number | null;
  k_value?: string | number | null;
  toolhead?: string | null;
  global_tray_id?: number | null;
  exists?: boolean | null;
};

export type ActiveTray = {
  kind?: string;
  ams_id?: string | null;
  tray_id?: string | null;
  global_tray_id?: number | null;
  external_id?: string | null;
};

export type PrinterMaterials = {
  filament_switch_installed?: boolean | null;
  cfg?: string | null;
  aux?: string | null;
  stat?: string | null;
  ams_units: Array<AmsUnit>;
  external_spools: Array<ExternalSpool>;
  active_tray: ActiveTray | null;
  observed_at: string;
};

export type NozzleInfo = {
  id: number;
  diameter: number;
  type: string;
  stat?: number | null;
  fila_id?: string | null;
  wear?: number | null;
  p_t?: number | null;
  color_m?: string | null;
};

export type NozzleRack = {
  exist?: number | null;
  state?: number | null;
  src_id?: number | null;
  tar_id?: number | null;
  info: Array<NozzleInfo>;
};

export type NozzleHolder = {
  stat?: number | null;
  pos?: number | null;
  info?: number | null;
};

export type NozzleSystem = {
  nozzle: NozzleRack;
  holder?: NozzleHolder | null;
};

export type Hms = {
  attr: number;
  code: number;
};

export type PrinterPrint = {
  task_generation: number;
  error_generation: number;
  hms: Array<Hms>;
  job_state: number | null;
  gcode_state: string | null;
  task_id: string | null;
  subtask_id: string | null;
  subtask_name: string | null;
  gcode_file: string | null;
  progress_percent: number | null;
  speed_level: number | null;
  remaining_time_minutes: number | null;
  current_layer: number | null;
  total_layers: number | null;
  print_error: number | null;
  printer_job_id: string | null;
};

export type Printer = {
  id: string;
  tenant_id: string;
  agent_id: string;
  serial_number: string;
  name: string;
  model: string | null;
  compatibility: PrinterCompatibility;
  status: string;
  last_seen_at: string;
  created_at: string;
  nozzle_temperatures?: Array<PrinterNozzleTemperature>;
  active_nozzle?: string | null;
  bed_temperature_celsius?: string | null;
  bed_target_temperature_celsius?: string | null;
  chamber_temperature_celsius?: string | null;
  chamber_target_temperature_celsius?: string | null;
  chamber_light_on?: boolean | null;
  cooling_system?: CoolingSystem | null;
  materials: PrinterMaterials | null;
  nozzle_system?: NozzleSystem | null;
  state_revision?: number;
  print?: PrinterPrint | null;
};

export type PrinterList = {
  printers: Array<Printer>;
};
