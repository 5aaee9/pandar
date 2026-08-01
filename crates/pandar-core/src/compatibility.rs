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
    pub ftps_clear_data_fallback: bool,
    pub features: CompatibilityFeatures,
}

pub fn compatibility_for_model(model: Option<&str>) -> DiagnosticCompatibility {
    let normalized_model = model.and_then(normalize_model);
    let Some(key) = normalized_model.as_deref() else {
        return DiagnosticCompatibility {
            normalized_model: None,
            external_storage: Capability::Unknown,
            ftps_tls_1_2_cap: false,
            ftps_clear_data_fallback: false,
            features: CompatibilityFeatures::unknown(),
        };
    };
    let key = key.to_owned();

    match key.as_str() {
        "A1" | "A1_MINI" => DiagnosticCompatibility {
            normalized_model,
            external_storage: Capability::Unsupported,
            ftps_tls_1_2_cap: false,
            ftps_clear_data_fallback: true,
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
            ftps_clear_data_fallback: false,
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
            ftps_clear_data_fallback: false,
            features: CompatibilityFeatures {
                flow_calibration: Capability::Supported,
                ..CompatibilityFeatures::unknown()
            },
        },
        "P1S" => DiagnosticCompatibility {
            normalized_model,
            external_storage: Capability::Unknown,
            ftps_tls_1_2_cap: false,
            ftps_clear_data_fallback: false,
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
            ftps_clear_data_fallback: false,
            features: CompatibilityFeatures {
                flow_calibration: Capability::Unsupported,
                ..CompatibilityFeatures::unknown()
            },
        },
        "P2S" | "H2S" => DiagnosticCompatibility {
            normalized_model,
            external_storage: Capability::Unknown,
            ftps_tls_1_2_cap: key == "P2S",
            ftps_clear_data_fallback: false,
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
            ftps_clear_data_fallback: false,
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
            ftps_clear_data_fallback: false,
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

pub fn ftps_clear_data_fallback(model: Option<&str>) -> bool {
    compatibility_for_model(model).ftps_clear_data_fallback
}

pub fn brtc_emmc_upload_supported(model: Option<&str>) -> bool {
    matches!(
        model.and_then(normalize_model).as_deref(),
        Some("P2S" | "X2D" | "X1" | "X1C" | "X1E")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_aliases_and_friendly_model_names() {
        for (model, expected) in [
            ("N1", "A1_MINI"),
            ("N2S", "A1"),
            ("N7", "P2S"),
            ("N6", "X2D"),
            ("BL-P001", "X1C"),
            ("BL-P002", "X1"),
            ("C13", "X1E"),
            ("C11", "P1P"),
            ("C12", "P1S"),
            ("N9", "A2L"),
            ("O1C", "H2C"),
            ("O1C2", "H2C"),
            ("O1D", "H2D"),
            ("O1E", "H2D_PRO"),
            ("O1S", "H2S"),
            ("Bambu Lab H2D", "H2D"),
            ("Bambu Lab H2C", "H2C"),
            ("Bambu Lab H2S", "H2S"),
            ("Bambu Lab A2L", "A2L"),
            ("Bambu Lab P1P", "P1P"),
            ("Bambu Lab P1S", "P1S"),
            ("X1 Carbon", "X1C"),
            ("3DPrinter-X1-Carbon", "X1C"),
            ("Bambu Lab X1E", "X1E"),
            ("Bambu Lab P2S", "P2S"),
            ("Bambu Lab X2D", "X2D"),
            ("A1 Mini", "A1_MINI"),
            ("bambu lab a1 mini", "A1_MINI"),
            (" a1-mini ", "A1_MINI"),
        ] {
            assert_eq!(normalize_model(model).as_deref(), Some(expected), "{model}");
        }
        assert_eq!(normalize_model(" ").as_deref(), None);
    }

    #[test]
    fn flow_calibration_matrix_matches_studio_resources() {
        for model in [
            "N1", "N2S", "BL-P001", "BL-P002", "C13", "N7", "N6", "N9", "O1C", "O1C2", "O1D",
            "O1E", "O1S",
        ] {
            assert!(flow_calibration_supported(Some(model)), "{model}");
        }
        for model in ["C11", "C12"] {
            assert!(!flow_calibration_supported(Some(model)), "{model}");
        }
        for model in ["N7", "N6", "N9", "O1C", "O1C2", "O1D", "O1E", "O1S"] {
            assert!(auto_flow_calibration_supported(Some(model)), "{model}");
        }
        for model in ["N1", "N2S", "BL-P001", "BL-P002", "C13", "C11", "C12"] {
            assert!(!auto_flow_calibration_supported(Some(model)), "{model}");
        }
    }
    #[test]
    fn bed_and_nozzle_calibration_matrix_matches_studio_resources() {
        for model in ["N7", "N6", "H2D"] {
            assert!(auto_bed_leveling_supported(Some(model)), "{model}");
        }
        for model in ["P1S", "A1", "Mystery Model"] {
            assert!(!auto_bed_leveling_supported(Some(model)), "{model}");
        }
        assert!(!auto_bed_leveling_supported(None));

        for model in ["N6", "H2D"] {
            assert!(nozzle_offset_calibration_supported(Some(model)), "{model}");
        }
        for model in ["N7", "P1S", "A1", "Mystery Model"] {
            assert!(!nozzle_offset_calibration_supported(Some(model)), "{model}");
        }
        assert!(!nozzle_offset_calibration_supported(None));
    }

    #[test]
    fn matrix_covers_ftps_storage_and_unknown_defaults() {
        assert!(ftps_tls_1_2_cap(Some("N7")));
        assert!(ftps_tls_1_2_cap(Some("X2D")));
        assert!(brtc_emmc_upload_supported(Some("N6")));
        assert!(brtc_emmc_upload_supported(Some("P2S")));
        assert!(brtc_emmc_upload_supported(Some("X1 Carbon")));
        assert!(brtc_emmc_upload_supported(Some("X1E")));
        assert!(!brtc_emmc_upload_supported(Some("A1 Mini")));
        assert!(ftps_clear_data_fallback(Some("A1")));
        assert!(ftps_clear_data_fallback(Some("A1 Mini")));
        assert_eq!(
            compatibility_for_model(Some("A1 Mini")).external_storage,
            Capability::Unsupported
        );
        assert_eq!(
            compatibility_for_model(Some("A1 Mini"))
                .features
                .flow_calibration,
            Capability::Supported
        );
        assert_eq!(
            compatibility_for_model(Some("A1 Mini"))
                .features
                .vibration_calibration,
            Capability::Unknown
        );
        assert_eq!(
            compatibility_for_model(Some("A1 Mini"))
                .features
                .nozzle_offset_calibration,
            Capability::Unknown
        );
        assert_eq!(
            compatibility_for_model(Some("P2S")).features.dual_nozzle,
            Capability::Unknown
        );

        let unknown = compatibility_for_model(Some("Mystery Model"));
        assert_eq!(unknown.features.flow_calibration, Capability::Unknown);
        assert_eq!(unknown.external_storage, Capability::Unknown);
        assert!(!unknown.ftps_tls_1_2_cap);
    }

    #[test]
    fn absent_model_serializes_null_and_unknown_features() {
        let value = serde_json::to_value(compatibility_for_model(None)).unwrap();
        let decoded: DiagnosticCompatibility = Deserialize::deserialize(value).unwrap();

        assert_eq!(
            decoded,
            DiagnosticCompatibility {
                normalized_model: None,
                external_storage: Capability::Unknown,
                ftps_tls_1_2_cap: false,
                ftps_clear_data_fallback: false,
                features: CompatibilityFeatures::unknown(),
            }
        );
    }

    #[test]
    fn live_controls_are_supported_only_for_verified_models() {
        for model in [
            "A1", "A1 Mini", "X1C", "BL-P001", "P1S", "C12", "N7", "N6", "A2L", "N9", "O1C2",
            "O1D", "O1E", "O1S",
        ] {
            assert!(live_controls_supported(Some(model)), "{model}");
        }
        assert!(!live_controls_supported(Some("BL-P002")));
        assert!(!live_controls_supported(Some("C11")));
        assert!(!live_controls_supported(Some("C13")));
        assert!(!live_controls_supported(None));
        assert!(!live_controls_supported(Some("Mystery Model")));
    }

    #[test]
    fn compatibility_serializes_live_controls_capability() {
        let value = serde_json::to_value(compatibility_for_model(Some("A1 Mini"))).unwrap();
        let decoded: DiagnosticCompatibility = Deserialize::deserialize(value).unwrap();

        assert_eq!(decoded.normalized_model.as_deref(), Some("A1_MINI"));
        assert_eq!(decoded.features.live_controls, Capability::Supported);
    }
}
