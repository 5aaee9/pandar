import type { PrinterCompatibility } from "./printer-compatibility";

const features: PrinterCompatibility["features"] = {
  chamber_temperature: "unknown",
  drying: "unknown",
  dual_nozzle: "unknown",
  flow_calibration: "unknown",
  vibration_calibration: "unknown",
  nozzle_offset_calibration: "unknown",
  live_controls: "unknown",
};

export function printerCompatibility(
  profile: "x2d" | "a1" | "a2l" | "unknown",
): PrinterCompatibility {
  if (profile === "x2d") {
    return {
      normalized_model: "X2D",
      external_storage: "unknown",
      ftps_tls_1_2_cap: true,
      features: {
        ...features,
        dual_nozzle: "supported",
        flow_calibration: "supported",
        nozzle_offset_calibration: "supported",
        live_controls: "supported",
      },
      print_options: {
        timelapse: true,
        bed_leveling: { modes: [2, 1, 0], default_mode: 2 },
        flow_calibration: { modes: [2, 1, 0], default_mode: 2 },
        nozzle_offset_calibration: { modes: [2, 1, 0], default_mode: 0 },
      },
      chamber_fan: "supported",
      nozzle_layout: "main_auxiliary",
    };
  }
  if (profile === "a1") {
    return {
      normalized_model: "A1",
      external_storage: "unsupported",
      ftps_tls_1_2_cap: false,
      features: {
        ...features,
        dual_nozzle: "unsupported",
        flow_calibration: "supported",
        live_controls: "supported",
      },
      print_options: {
        timelapse: true,
        bed_leveling: { modes: [1, 0], default_mode: 1 },
        flow_calibration: { modes: [1, 0], default_mode: 1 },
        nozzle_offset_calibration: null,
      },
      chamber_fan: "unsupported",
      nozzle_layout: "single",
    };
  }
  if (profile === "a2l") {
    return {
      normalized_model: "A2L",
      external_storage: "unknown",
      ftps_tls_1_2_cap: false,
      features: {
        ...features,
        dual_nozzle: "unsupported",
        flow_calibration: "supported",
        live_controls: "supported",
      },
      print_options: {
        timelapse: true,
        bed_leveling: { modes: [2, 1, 0], default_mode: 2 },
        flow_calibration: { modes: [2, 1, 0], default_mode: 2 },
        nozzle_offset_calibration: null,
      },
      chamber_fan: "unsupported",
      nozzle_layout: "single",
    };
  }
  return {
    normalized_model: null,
    external_storage: "unknown",
    ftps_tls_1_2_cap: false,
    features,
    print_options: {
      timelapse: false,
      bed_leveling: null,
      flow_calibration: null,
      nozzle_offset_calibration: null,
    },
    chamber_fan: "unknown",
    nozzle_layout: "unknown",
  };
}
