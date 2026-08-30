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
        "N1", "N2S", "BL-P001", "BL-P002", "C13", "N7", "N6", "N9", "O1C", "O1C2", "O1D", "O1E",
        "O1S",
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
        Capability::Unsupported
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
            features: CompatibilityFeatures::unknown(),
            print_options: PrintOptionCapabilities::unknown(),
            chamber_fan: Capability::Unknown,
            nozzle_layout: NozzleLayout::Unknown,
        }
    );
}

#[test]
fn projected_ui_capabilities_match_the_model_matrix() {
    let x2d = compatibility_for_model(Some("N6"));
    assert_eq!(x2d.normalized_model.as_deref(), Some("X2D"));
    assert_eq!(x2d.features.dual_nozzle, Capability::Supported);
    assert_eq!(x2d.nozzle_layout, NozzleLayout::MainAuxiliary);
    assert_eq!(
        x2d.print_options.nozzle_offset_calibration,
        Some(CalibrationOption {
            modes: vec![
                PrintCalibrationMode::Auto,
                PrintCalibrationMode::On,
                PrintCalibrationMode::Off,
            ],
            default_mode: PrintCalibrationMode::Off,
        })
    );

    for model in ["O1C2", "H2D", "O1E"] {
        assert_eq!(
            compatibility_for_model(Some(model)).features.dual_nozzle,
            Capability::Supported,
            "{model}"
        );
    }
    for model in ["A1", "A1 Mini", "A2L", "P1P"] {
        assert_eq!(
            compatibility_for_model(Some(model)).chamber_fan,
            Capability::Unsupported,
            "{model}"
        );
    }
}

#[test]
fn print_option_projection_covers_every_verified_model_family() {
    let on_off = || CalibrationOption {
        modes: vec![PrintCalibrationMode::On, PrintCalibrationMode::Off],
        default_mode: PrintCalibrationMode::On,
    };
    let auto = |default_mode| CalibrationOption {
        modes: vec![
            PrintCalibrationMode::Auto,
            PrintCalibrationMode::On,
            PrintCalibrationMode::Off,
        ],
        default_mode,
    };
    let cases = [
        (
            &["N6", "X2D", "Bambu Lab X2D"][..],
            PrintOptionCapabilities {
                timelapse: true,
                bed_leveling: Some(auto(PrintCalibrationMode::Auto)),
                flow_calibration: Some(auto(PrintCalibrationMode::Auto)),
                nozzle_offset_calibration: Some(auto(PrintCalibrationMode::Off)),
            },
        ),
        (
            &["N7", "P2S", "O1S", "H2S", "N9", "A2L"][..],
            PrintOptionCapabilities {
                timelapse: true,
                bed_leveling: Some(auto(PrintCalibrationMode::Auto)),
                flow_calibration: Some(auto(PrintCalibrationMode::Auto)),
                nozzle_offset_calibration: None,
            },
        ),
        (
            &[
                "N1", "A1 Mini", "N2S", "A1", "BL-P001", "X1C", "BL-P002", "X1", "C13", "X1E",
            ][..],
            PrintOptionCapabilities {
                timelapse: true,
                bed_leveling: Some(on_off()),
                flow_calibration: Some(on_off()),
                nozzle_offset_calibration: None,
            },
        ),
        (
            &["C11", "P1P", "C12", "P1S"][..],
            PrintOptionCapabilities {
                timelapse: true,
                bed_leveling: Some(on_off()),
                flow_calibration: None,
                nozzle_offset_calibration: None,
            },
        ),
        (
            &["O1C", "O1C2", "H2C", "O1D", "H2D", "O1E", "H2D Pro"][..],
            PrintOptionCapabilities {
                timelapse: true,
                bed_leveling: Some(auto(PrintCalibrationMode::Auto)),
                flow_calibration: Some(auto(PrintCalibrationMode::Auto)),
                nozzle_offset_calibration: Some(auto(PrintCalibrationMode::Auto)),
            },
        ),
    ];

    for (models, expected) in cases {
        for model in models {
            assert_eq!(
                compatibility_for_model(Some(model)).print_options,
                expected,
                "{model}"
            );
        }
    }
}

#[test]
fn compatibility_requires_all_current_capabilities() {
    let current = serde_json::to_value(compatibility_for_model(Some("A1"))).unwrap();
    for field in ["print_options", "chamber_fan", "nozzle_layout"] {
        let mut missing = current.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<DiagnosticCompatibility>(missing).is_err());
    }
}

#[test]
fn compatibility_json_omits_clear_data_fallback_capability() {
    let json = serde_json::to_string(&compatibility_for_model(Some("A1 Mini"))).unwrap();

    assert!(!json.contains("ftps_clear_data_fallback"));
}

#[test]
fn live_controls_are_supported_only_for_verified_models() {
    for model in [
        "A1", "A1 Mini", "X1C", "BL-P001", "P1S", "C12", "N7", "N6", "A2L", "N9", "O1C2", "O1D",
        "O1E", "O1S",
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

#[test]
fn studio_local_camera_is_supported_only_for_the_verified_models() {
    for model in ["A1", "N2S", "A1 Mini", "N1", "P1S", "C12", "A2L", "N9"] {
        assert!(studio_local_camera_supported(Some(model)), "{model}");
    }
    for model in ["P1P", "C11", "X1C", "N6", "O1C2", "Mystery Model"] {
        assert!(!studio_local_camera_supported(Some(model)), "{model}");
    }
    assert!(!studio_local_camera_supported(None));
}
