export type Capability = "supported" | "unsupported" | "unknown";
export type NozzleLayout =
  | "single"
  | "main_auxiliary"
  | "left_right"
  | "unknown";

export type CalibrationOptionCapability = {
  modes: Array<0 | 1 | 2>;
  default_mode: 0 | 1 | 2;
};

export type PrinterCompatibility = {
  normalized_model: string | null;
  external_storage: Capability;
  ftps_tls_1_2_cap: boolean;
  features: {
    chamber_temperature: Capability;
    drying: Capability;
    dual_nozzle: Capability;
    flow_calibration: Capability;
    vibration_calibration: Capability;
    nozzle_offset_calibration: Capability;
    live_controls: Capability;
  };
  print_options: {
    timelapse: boolean;
    bed_leveling: CalibrationOptionCapability | null;
    flow_calibration: CalibrationOptionCapability | null;
    nozzle_offset_calibration: CalibrationOptionCapability | null;
  };
  chamber_fan: Capability;
  nozzle_layout: NozzleLayout;
};
