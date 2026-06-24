use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Supported,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
                flow_calibration: Capability::Unknown,
                vibration_calibration: Capability::Unknown,
                nozzle_offset_calibration: Capability::Unknown,
                live_controls: Capability::Supported,
            },
        },
        "P2S" | "X2D" => DiagnosticCompatibility {
            normalized_model,
            external_storage: Capability::Unknown,
            ftps_tls_1_2_cap: true,
            ftps_clear_data_fallback: false,
            features: CompatibilityFeatures {
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

    let normalized = match compact.as_str() {
        "N7" => "P2S",
        "N6" => "X2D",
        "A1MINI" | "A1M" | "A1MIN" | "BAMBULABA1MINI" => "A1_MINI",
        "A1" | "BAMBULABA1" => "A1",
        "P2S" => "P2S",
        "X2D" => "X2D",
        other => other,
    };

    Some(normalized.to_owned())
}

pub fn flow_calibration_supported(model: Option<&str>) -> bool {
    compatibility_for_model(model).features.flow_calibration == Capability::Supported
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn normalizes_aliases_and_a1_mini_spellings() {
        assert_eq!(normalize_model("N7").as_deref(), Some("P2S"));
        assert_eq!(normalize_model("N6").as_deref(), Some("X2D"));
        assert_eq!(normalize_model("A1 Mini").as_deref(), Some("A1_MINI"));
        assert_eq!(
            normalize_model("bambu lab a1 mini").as_deref(),
            Some("A1_MINI")
        );
        assert_eq!(normalize_model(" a1-mini ").as_deref(), Some("A1_MINI"));
        assert_eq!(normalize_model(" ").as_deref(), None);
    }

    #[test]
    fn matrix_covers_ftps_storage_and_unknown_defaults() {
        assert!(ftps_tls_1_2_cap(Some("N7")));
        assert!(ftps_tls_1_2_cap(Some("X2D")));
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
            Capability::Unknown
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

        assert_eq!(
            value,
            json!({
                "normalized_model": null,
                "external_storage": "unknown",
                "ftps_tls_1_2_cap": false,
                "ftps_clear_data_fallback": false,
                "features": {
                    "chamber_temperature": "unknown",
                    "drying": "unknown",
                    "dual_nozzle": "unknown",
                    "flow_calibration": "unknown",
                    "vibration_calibration": "unknown",
                    "nozzle_offset_calibration": "unknown",
                    "live_controls": "unknown"
                }
            })
        );
    }

    #[test]
    fn live_controls_are_supported_only_for_known_phase_27_models() {
        assert!(live_controls_supported(Some("A1")));
        assert!(live_controls_supported(Some("A1 Mini")));
        assert!(live_controls_supported(Some("N7")));
        assert!(live_controls_supported(Some("N6")));
        assert!(!live_controls_supported(None));
        assert!(!live_controls_supported(Some("Mystery Model")));
    }

    #[test]
    fn compatibility_serializes_live_controls_capability() {
        let value = serde_json::to_value(compatibility_for_model(Some("A1 Mini"))).unwrap();
        assert_eq!(value["normalized_model"], "A1_MINI");
        assert_eq!(value["features"]["live_controls"], "supported");
    }
}
