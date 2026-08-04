use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Supported,
    Unsupported,
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
}

pub fn compatibility_for_model(model: Option<&str>) -> DiagnosticCompatibility {
    let normalized_model = model.and_then(normalize_model);
    let Some(key) = normalized_model.as_deref() else {
        return DiagnosticCompatibility {
            normalized_model: None,
            external_storage: Capability::Unknown,
            ftps_tls_1_2_cap: false,
            features: CompatibilityFeatures::unknown(),
        };
    };
    let key = key.to_owned();

    match key.as_str() {
        "A1" | "A1_MINI" => DiagnosticCompatibility {
            normalized_model,
            external_storage: Capability::Unsupported,
            ftps_tls_1_2_cap: false,
            features: CompatibilityFeatures {
                chamber_temperature: Capability::Unknown,
                drying: Capability::Unknown,
                dual_nozzle: Capability::Unsupported,
                flow_calibration: Capability::Supported,
                vibration_calibration: Capability::Unknown,
                nozzle_offset_calibration: Capability::Unknown,
                live_controls: Capability::Supported,
            },
        },
        "X1C" | "A2L" => DiagnosticCompatibility {
            normalized_model,
            external_storage: Capability::Unknown,
            ftps_tls_1_2_cap: false,
            features: CompatibilityFeatures {
                flow_calibration: Capability::Supported,
                live_controls: Capability::Supported,
                ..CompatibilityFeatures::unknown()
            },
        },
        "X1" | "X1E" => DiagnosticCompatibility {
            normalized_model,
            external_storage: Capability::Unknown,
            ftps_tls_1_2_cap: false,
            features: CompatibilityFeatures {
                flow_calibration: Capability::Supported,
                ..CompatibilityFeatures::unknown()
            },
        },
        "P1S" => DiagnosticCompatibility {
            normalized_model,
            external_storage: Capability::Unknown,
            ftps_tls_1_2_cap: false,
            features: CompatibilityFeatures {
                flow_calibration: Capability::Unsupported,
                live_controls: Capability::Supported,
                ..CompatibilityFeatures::unknown()
            },
        },
        "P1P" => DiagnosticCompatibility {
            normalized_model,
            external_storage: Capability::Unknown,
            ftps_tls_1_2_cap: false,
            features: CompatibilityFeatures {
                flow_calibration: Capability::Unsupported,
                ..CompatibilityFeatures::unknown()
            },
        },
        "P2S" | "H2S" => DiagnosticCompatibility {
            normalized_model,
            external_storage: Capability::Unknown,
            ftps_tls_1_2_cap: key == "P2S",
            features: CompatibilityFeatures {
                flow_calibration: Capability::Supported,
                nozzle_offset_calibration: Capability::Unsupported,
                live_controls: Capability::Supported,
                ..CompatibilityFeatures::unknown()
            },
        },
        "X2D" | "H2C" | "H2D" | "H2D_PRO" => DiagnosticCompatibility {
            normalized_model,
            external_storage: Capability::Unknown,
            ftps_tls_1_2_cap: key == "X2D",
            features: CompatibilityFeatures {
                flow_calibration: Capability::Supported,
                nozzle_offset_calibration: Capability::Supported,
                live_controls: Capability::Supported,
                ..CompatibilityFeatures::unknown()
            },
        },
        _ => DiagnosticCompatibility {
            normalized_model,
            external_storage: Capability::Unknown,
            ftps_tls_1_2_cap: false,
            features: CompatibilityFeatures::unknown(),
        },
    }
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
