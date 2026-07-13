use pandar_core::PrinterFirmwareModule;
use serde_json::json;

use super::super::{
    FakeMqttTransport, parse_firmware_refresh_modules, parse_firmware_version_observation,
    parse_snapshot_report, print_report_from_report, refresh_printer_with_firmware,
};
use super::endpoint;

#[test]
fn firmware_observation_preserves_order_duplicates_and_all_module_fields() {
    let report = json!({
        "info": {
            "command": "get_version",
            "sequence_id": "17",
            "module": [
                {
                    "name": "ota",
                    "sw_ver": "01.08.02.00",
                    "sw_new_ver": "01.09.00.00",
                    "new_ver": "01.09.01.00",
                    "visible": false,
                    "product_name": " X1 Carbon ",
                    "sn": "00M09A",
                    "hw_ver": "AP05",
                    "flag": 3
                },
                { "name": "ams/0", "sw_ver": "00.00.06.44", "hw_ver": "AMS08", "flag": 1 },
                { "name": "n3f/0", "sw_ver": "01.00.00.31", "visible": true },
                { "name": "n3s/0", "sw_ver": "01.00.00.22", "new_ver": "01.00.00.23" },
                { "name": "future/unit", "sw_ver": "9.9.9" },
                { "name": "ams/0", "sw_ver": "duplicate" }
            ]
        }
    });

    let observation = parse_firmware_version_observation(&report)
        .unwrap()
        .expect("get_version observation");

    assert_eq!(observation.model, "X1 Carbon");
    assert_eq!(
        observation.modules,
        vec![
            PrinterFirmwareModule {
                name: "ota".to_owned(),
                software_version: Some("01.08.02.00".to_owned()),
                software_new_version: Some("01.09.00.00".to_owned()),
                new_version: Some("01.09.01.00".to_owned()),
                visible: Some(false),
                product_name: Some(" X1 Carbon ".to_owned()),
                serial_number: Some("00M09A".to_owned()),
                hardware_version: Some("AP05".to_owned()),
                firmware_flag: Some(3),
            },
            module("ams/0", "00.00.06.44", Some("AMS08"), Some(1)),
            PrinterFirmwareModule {
                name: "n3f/0".to_owned(),
                software_version: Some("01.00.00.31".to_owned()),
                software_new_version: None,
                new_version: None,
                visible: Some(true),
                product_name: None,
                serial_number: None,
                hardware_version: None,
                firmware_flag: None,
            },
            PrinterFirmwareModule {
                name: "n3s/0".to_owned(),
                software_version: Some("01.00.00.22".to_owned()),
                software_new_version: None,
                new_version: Some("01.00.00.23".to_owned()),
                visible: None,
                product_name: None,
                serial_number: None,
                hardware_version: None,
                firmware_flag: None,
            },
            module("future/unit", "9.9.9", None, None),
            module("ams/0", "duplicate", None, None),
        ]
    );
}

#[test]
fn firmware_observation_malformed_fields_do_not_discard_sibling_telemetry() {
    let report = json!({
        "info": {
            "command": "get_version",
            "module": [{ "name": "ota", "product_name": "X1", "flag": "invalid" }]
        },
        "print": {
            "task_id": "job-9",
            "gcode_state": "RUNNING",
            "nozzle_temper": 210.5
        }
    });

    let error = parse_firmware_version_observation(&report).unwrap_err();
    assert!(format!("{error:#}").contains("firmware get_version"));
    assert_eq!(
        print_report_from_report(&endpoint(), &report)
            .job_id
            .as_deref(),
        Some("job-9")
    );
    assert!(parse_snapshot_report(&report).is_some());
}

#[test]
fn firmware_observation_ignores_non_version_reports() {
    assert!(
        parse_firmware_version_observation(&json!({
            "print": { "upgrade_state": { "status": "DOWNLOADING" } }
        }))
        .unwrap()
        .is_none()
    );
}

#[test]
fn firmware_observation_rejects_empty_module_name_without_discarding_sibling_telemetry() {
    let report = json!({
        "info": {
            "command": "get_version",
            "module": [
                { "name": "ota", "product_name": "X1", "sw_ver": "1" },
                { "name": "", "sw_ver": "empty-name" }
            ]
        },
        "print": {
            "task_id": "job-empty-name",
            "gcode_state": "RUNNING",
            "nozzle_temper": 210.5
        }
    });

    let error = parse_firmware_version_observation(&report).unwrap_err();
    assert!(format!("{error:#}").contains("non-empty name"));
    assert_eq!(
        print_report_from_report(&endpoint(), &report)
            .job_id
            .as_deref(),
        Some("job-empty-name")
    );
    assert!(parse_snapshot_report(&report).is_some());
}

#[test]
fn firmware_observation_preserves_opaque_whitespace_and_future_module_names() {
    let report = json!({
        "info": {
            "command": "get_version",
            "module": [
                { "name": "ota", "product_name": "X1", "sw_ver": "1" },
                { "name": "   ", "sw_ver": "whitespace-name" },
                { "name": "future/unit", "sw_ver": "future-name" },
                { "name": "future/unit", "sw_ver": "duplicate-name" }
            ]
        }
    });

    let observation = parse_firmware_version_observation(&report)
        .unwrap()
        .unwrap();
    assert_eq!(observation.model, "X1");
    assert_eq!(
        observation
            .modules
            .iter()
            .map(|module| (module.name.as_str(), module.software_version.as_deref()))
            .collect::<Vec<_>>(),
        [
            ("ota", Some("1")),
            ("   ", Some("whitespace-name")),
            ("future/unit", Some("future-name")),
            ("future/unit", Some("duplicate-name")),
        ]
    );
}

#[test]
fn firmware_refresh_parser_accepts_no_ota_and_ota_without_product_name() {
    for report in [
        json!({
            "info": {
                "command": "get_version",
                "module": [
                    { "name": "future/unit", "sw_ver": "9.9.9", "hw_ver": "F00", "flag": 7 }
                ]
            }
        }),
        json!({
            "info": {
                "command": "get_version",
                "module": [
                    { "name": "ota", "sw_ver": "1.2.3" },
                    { "name": "future/ams-ht", "new_ver": "4.5.6", "visible": false }
                ]
            }
        }),
    ] {
        let modules = parse_firmware_refresh_modules(&report)
            .unwrap()
            .expect("typed get_version modules");
        assert!(!modules.is_empty());
    }
}

#[tokio::test]
async fn firmware_observation_model_discovery_returns_modules_from_the_same_query() {
    let transport = FakeMqttTransport::with_reports([
        json!({
            "info": {
                "command": "get_version",
                "module": [
                    { "name": "ota", "product_name": "X1 Carbon", "sw_ver": "1" },
                    { "name": "ams/0", "sw_ver": "2" }
                ]
            }
        }),
        json!({ "print": { "state": "IDLE", "nozzle_temper": 25 } }),
    ]);

    let (refresh, firmware) =
        refresh_printer_with_firmware(&transport, &endpoint(), std::time::Duration::from_secs(1))
            .await
            .unwrap();

    assert_eq!(refresh.snapshot.model.as_deref(), Some("X1 Carbon"));
    assert_eq!(firmware.model, "X1 Carbon");
    assert_eq!(
        firmware
            .modules
            .iter()
            .map(|module| module.name.as_str())
            .collect::<Vec<_>>(),
        ["ota", "ams/0"]
    );
    let published = transport.published_commands().await;
    assert_eq!(published.len(), 2);
    assert_eq!(published[0].payload["info"]["command"], "get_version");
    assert_eq!(published[1].payload["pushing"]["command"], "pushall");
}

fn module(
    name: &str,
    software_version: &str,
    hardware_version: Option<&str>,
    firmware_flag: Option<i32>,
) -> PrinterFirmwareModule {
    PrinterFirmwareModule {
        name: name.to_owned(),
        software_version: Some(software_version.to_owned()),
        software_new_version: None,
        new_version: None,
        visible: None,
        product_name: None,
        serial_number: None,
        hardware_version: hardware_version.map(str::to_owned),
        firmware_flag,
    }
}
