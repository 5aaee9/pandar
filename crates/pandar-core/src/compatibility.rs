use serde::{Deserialize, Serialize};

use crate::PrintCalibrationMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Supported,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NozzleLayout {
    Single,
    MainAuxiliary,
    LeftRight,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct CompatibilityFeatures {
    pub chamber_temperature: Capability,
    pub drying: Capability,
    pub dual_nozzle: Capability,
    pub flow_calibration: Capability,
    pub vibration_calibration: Capability,
    pub nozzle_offset_calibration: Capability,
    pub live_controls: Capability,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct CalibrationOption {
    pub modes: Vec<PrintCalibrationMode>,
    pub default_mode: PrintCalibrationMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PrintOptionCapabilities {
    pub timelapse: bool,
    pub bed_leveling: Option<CalibrationOption>,
    pub flow_calibration: Option<CalibrationOption>,
    pub nozzle_offset_calibration: Option<CalibrationOption>,
}

impl PrintOptionCapabilities {
    fn unknown() -> Self {
        Self {
            timelapse: false,
            bed_leveling: None,
            flow_calibration: None,
            nozzle_offset_calibration: None,
        }
    }
}

impl CompatibilityFeatures {
    fn unknown() -> Self {
        Self {
            chamber_temperature: Capability::Unknown,
            drying: Capability::Unknown,
            dual_nozzle: Capability::Unknown,
            flow_calibration: Capability::Unknown,
            vibration_calibration: Capability::Unknown,
            nozzle_offset_calibration: Capability::Unknown,
            live_controls: Capability::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct DiagnosticCompatibility {
    pub normalized_model: Option<String>,
    pub external_storage: Capability,
    pub ftps_tls_1_2_cap: bool,
    pub features: CompatibilityFeatures,
    #[serde(default = "PrintOptionCapabilities::unknown")]
    pub print_options: PrintOptionCapabilities,
    #[serde(default = "unknown_capability")]
    pub chamber_fan: Capability,
    #[serde(default = "unknown_nozzle_layout")]
    pub nozzle_layout: NozzleLayout,
}

pub fn compatibility_for_model(model: Option<&str>) -> DiagnosticCompatibility {
    let normalized_model = model.and_then(normalize_model);
    let Some(key) = normalized_model.as_deref() else {
        return diagnostic(
            None,
            Capability::Unknown,
            false,
            CompatibilityFeatures::unknown(),
        );
    };
    let key = key.to_owned();

    match key.as_str() {
        "A1" | "A1_MINI" => diagnostic(
            normalized_model,
            Capability::Unsupported,
            false,
            CompatibilityFeatures {
                chamber_temperature: Capability::Unknown,
                drying: Capability::Unknown,
                dual_nozzle: Capability::Unsupported,
                flow_calibration: Capability::Supported,
                vibration_calibration: Capability::Unknown,
                nozzle_offset_calibration: Capability::Unknown,
                live_controls: Capability::Supported,
            },
        ),
        "X1C" | "A2L" => diagnostic(
            normalized_model,
            Capability::Unknown,
            false,
            CompatibilityFeatures {
                flow_calibration: Capability::Supported,
                live_controls: Capability::Supported,
                ..CompatibilityFeatures::unknown()
            },
        ),
        "X1" | "X1E" => diagnostic(
            normalized_model,
            Capability::Unknown,
            false,
            CompatibilityFeatures {
                flow_calibration: Capability::Supported,
                ..CompatibilityFeatures::unknown()
            },
        ),
        "P1S" => diagnostic(
            normalized_model,
            Capability::Unknown,
            false,
            CompatibilityFeatures {
                flow_calibration: Capability::Unsupported,
                live_controls: Capability::Supported,
                ..CompatibilityFeatures::unknown()
            },
        ),
        "P1P" => diagnostic(
            normalized_model,
            Capability::Unknown,
            false,
            CompatibilityFeatures {
                flow_calibration: Capability::Unsupported,
                ..CompatibilityFeatures::unknown()
            },
        ),
        "P2S" | "H2S" => diagnostic(
            normalized_model,
            Capability::Unknown,
            key == "P2S",
            CompatibilityFeatures {
                flow_calibration: Capability::Supported,
                nozzle_offset_calibration: Capability::Unsupported,
                live_controls: Capability::Supported,
                ..CompatibilityFeatures::unknown()
            },
        ),
        "X2D" | "H2C" | "H2D" | "H2D_PRO" => diagnostic(
            normalized_model,
            Capability::Unknown,
            key == "X2D",
            CompatibilityFeatures {
                flow_calibration: Capability::Supported,
                nozzle_offset_calibration: Capability::Supported,
                live_controls: Capability::Supported,
                ..CompatibilityFeatures::unknown()
            },
        ),
        _ => diagnostic(
            normalized_model,
            Capability::Unknown,
            false,
            CompatibilityFeatures::unknown(),
        ),
    }
}

fn diagnostic(
    normalized_model: Option<String>,
    external_storage: Capability,
    ftps_tls_1_2_cap: bool,
    mut features: CompatibilityFeatures,
) -> DiagnosticCompatibility {
    features.dual_nozzle = dual_nozzle_capability(normalized_model.as_deref());
    DiagnosticCompatibility {
        print_options: print_options_for_model(normalized_model.as_deref()),
        chamber_fan: chamber_fan_capability(normalized_model.as_deref()),
        nozzle_layout: nozzle_layout(normalized_model.as_deref()),
        normalized_model,
        external_storage,
        ftps_tls_1_2_cap,
        features,
    }
}

fn print_options_for_model(model: Option<&str>) -> PrintOptionCapabilities {
    let on_off = || CalibrationOption {
        modes: vec![PrintCalibrationMode::On, PrintCalibrationMode::Off],
        default_mode: PrintCalibrationMode::On,
    };
    let auto_on_off = |default_mode| CalibrationOption {
        modes: vec![
            PrintCalibrationMode::Auto,
            PrintCalibrationMode::On,
            PrintCalibrationMode::Off,
        ],
        default_mode,
    };
    match model {
        Some("X2D") => PrintOptionCapabilities {
            timelapse: true,
            bed_leveling: Some(auto_on_off(PrintCalibrationMode::Auto)),
            flow_calibration: Some(auto_on_off(PrintCalibrationMode::Auto)),
            nozzle_offset_calibration: Some(auto_on_off(PrintCalibrationMode::Off)),
        },
        Some("P2S" | "H2S" | "A2L") => PrintOptionCapabilities {
            timelapse: true,
            bed_leveling: Some(auto_on_off(PrintCalibrationMode::Auto)),
            flow_calibration: Some(auto_on_off(PrintCalibrationMode::Auto)),
            nozzle_offset_calibration: None,
        },
        Some("A1" | "A1_MINI" | "X1" | "X1C" | "X1E") => PrintOptionCapabilities {
            timelapse: true,
            bed_leveling: Some(on_off()),
            flow_calibration: Some(on_off()),
            nozzle_offset_calibration: None,
        },
        Some("P1P" | "P1S") => PrintOptionCapabilities {
            timelapse: true,
            bed_leveling: Some(on_off()),
            flow_calibration: None,
            nozzle_offset_calibration: None,
        },
        Some("H2C" | "H2D" | "H2D_PRO") => PrintOptionCapabilities {
            timelapse: true,
            bed_leveling: Some(auto_on_off(PrintCalibrationMode::Auto)),
            flow_calibration: Some(auto_on_off(PrintCalibrationMode::Auto)),
            nozzle_offset_calibration: Some(auto_on_off(PrintCalibrationMode::Auto)),
        },
        _ => PrintOptionCapabilities::unknown(),
    }
}

fn dual_nozzle_capability(model: Option<&str>) -> Capability {
    match model {
        Some("X2D" | "H2C" | "H2D" | "H2D_PRO") => Capability::Supported,
        Some("A1" | "A1_MINI" | "A2L" | "X1" | "X1C" | "X1E" | "P1P" | "P1S" | "P2S" | "H2S") => {
            Capability::Unsupported
        }
        _ => Capability::Unknown,
    }
}

fn chamber_fan_capability(model: Option<&str>) -> Capability {
    match model {
        Some("A1" | "A1_MINI" | "A2L" | "P1P") => Capability::Unsupported,
        Some("X1" | "X1C" | "X1E" | "P1S" | "P2S" | "X2D" | "H2C" | "H2D" | "H2D_PRO" | "H2S") => {
            Capability::Supported
        }
        _ => Capability::Unknown,
    }
}

fn nozzle_layout(model: Option<&str>) -> NozzleLayout {
    match model {
        Some("X2D") => NozzleLayout::MainAuxiliary,
        Some("H2C" | "H2D" | "H2D_PRO") => NozzleLayout::LeftRight,
        Some("A1" | "A1_MINI" | "A2L" | "X1" | "X1C" | "X1E" | "P1P" | "P1S" | "P2S" | "H2S") => {
            NozzleLayout::Single
        }
        _ => NozzleLayout::Unknown,
    }
}

const fn unknown_capability() -> Capability {
    Capability::Unknown
}

const fn unknown_nozzle_layout() -> NozzleLayout {
    NozzleLayout::Unknown
}

pub fn normalize_model(model: &str) -> Option<String> {
    let compact = model
        .trim()
        .to_ascii_uppercase()
        .replace([' ', '-', '_'], "");
    if compact.is_empty() {
        return None;
    }

    let compact = compact.strip_prefix("BAMBULAB").unwrap_or(&compact);
    let normalized = match compact {
        "N1" => "A1_MINI",
        "N2S" => "A1",
        "N7" => "P2S",
        "N6" => "X2D",
        "BLP001" => "X1C",
        "BLP002" => "X1",
        "C13" => "X1E",
        "C11" => "P1P",
        "C12" => "P1S",
        "N9" => "A2L",
        "O1C" | "O1C2" => "H2C",
        "O1D" => "H2D",
        "O1E" => "H2D_PRO",
        "O1S" => "H2S",
        "X1" | "3DPRINTERX1" => "X1",
        "X1E" => "X1E",
        "X1CARBON" | "3DPRINTERX1CARBON" => "X1C",
        "A1MINI" | "A1M" | "A1MIN" => "A1_MINI",
        "A1" => "A1",
        "P1P" | "P1S" | "A2L" | "H2C" | "H2D" | "H2S" => compact,
        "H2DPRO" => "H2D_PRO",
        "P2S" => "P2S",
        "X2D" => "X2D",
        other => other,
    };

    Some(normalized.to_owned())
}

pub fn flow_calibration_supported(model: Option<&str>) -> bool {
    compatibility_for_model(model).features.flow_calibration == Capability::Supported
}

pub fn auto_flow_calibration_supported(model: Option<&str>) -> bool {
    matches!(
        model.and_then(normalize_model).as_deref(),
        Some("P2S" | "X2D" | "A2L" | "H2C" | "H2D" | "H2D_PRO" | "H2S")
    )
}

pub fn auto_bed_leveling_supported(model: Option<&str>) -> bool {
    matches!(
        model.and_then(normalize_model).as_deref(),
        Some("P2S" | "X2D" | "A2L" | "H2C" | "H2D" | "H2D_PRO" | "H2S")
    )
}

pub fn nozzle_offset_calibration_supported(model: Option<&str>) -> bool {
    compatibility_for_model(model)
        .features
        .nozzle_offset_calibration
        == Capability::Supported
}

pub fn live_controls_supported(model: Option<&str>) -> bool {
    compatibility_for_model(model).features.live_controls == Capability::Supported
}

pub fn ftps_tls_1_2_cap(model: Option<&str>) -> bool {
    compatibility_for_model(model).ftps_tls_1_2_cap
}

pub fn brtc_emmc_upload_supported(model: Option<&str>) -> bool {
    matches!(
        model.and_then(normalize_model).as_deref(),
        Some("P2S" | "X2D" | "X1" | "X1C" | "X1E")
    )
}

pub fn studio_local_camera_supported(model: Option<&str>) -> bool {
    matches!(
        model.and_then(normalize_model).as_deref(),
        Some("A1" | "A1_MINI" | "P1S" | "A2L")
    )
}

#[cfg(test)]
mod tests;
