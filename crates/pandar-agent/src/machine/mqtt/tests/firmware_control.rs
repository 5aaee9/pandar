use pandar_core::{FirmwareAcknowledgement, FirmwareCommand};

use super::super::firmware::{
    FirmwareMqttCommand, firmware_command_payload, firmware_mqtt_options,
    parse_firmware_acknowledgement,
};
use crate::machine::BambuPrinterEndpoint;

#[test]
fn firmware_control_payloads_preserve_exact_studio_fields() {
    let cases = [
        (
            FirmwareMqttCommand::get_version("70001"),
            br#"{"info":{"command":"get_version","sequence_id":"70001"}}"#.as_slice(),
        ),
        (
            firmware_command_payload(&FirmwareCommand::UpgradeConfirm {
                sequence_id: "70002".into(),
                src_id: 17,
            }),
            br#"{"upgrade":{"command":"upgrade_confirm","sequence_id":"70002","src_id":17}}"#,
        ),
        (
            firmware_command_payload(&FirmwareCommand::ConsistencyConfirm {
                sequence_id: "70003".into(),
                src_id: -9,
            }),
            br#"{"upgrade":{"command":"consistency_confirm","sequence_id":"70003","src_id":-9}}"#,
        ),
        (
            firmware_command_payload(&FirmwareCommand::Start {
                sequence_id: "70004".into(),
                src_id: 1,
                url: "https://user:secret@example.invalid/fw.bin?sig=UNIQUE-URL-SENTINEL".into(),
                module: "ota/submodule".into(),
                version: "01.02.03.04-beta".into(),
            }),
            br#"{"upgrade":{"command":"start","sequence_id":"70004","src_id":1,"url":"https://user:secret@example.invalid/fw.bin?sig=UNIQUE-URL-SENTINEL","module":"ota/submodule","version":"01.02.03.04-beta"}}"#,
        ),
        (
            firmware_command_payload(&FirmwareCommand::SwitchAmsFirmware {
                sequence_id: "70005".into(),
                src_id: 3,
                id: -7,
            }),
            br#"{"upgrade":{"command":"mc_for_ams_firmware_upgrade","sequence_id":"70005","src_id":3,"id":-7}}"#,
        ),
    ];

    for (command, expected) in cases {
        assert_eq!(command.payload_bytes(), expected);
    }
}

#[test]
fn firmware_start_debug_output_redacts_the_url() {
    let command = firmware_command_payload(&FirmwareCommand::Start {
        sequence_id: "70006".into(),
        src_id: 1,
        url: "https://example.invalid/fw.bin?sig=UNIQUE-URL-SENTINEL".into(),
        module: "ota".into(),
        version: "01.02.03.04".into(),
    });

    assert!(!format!("{command:?}").contains("UNIQUE-URL-SENTINEL"));
}

#[test]
fn firmware_mqtt_acknowledgement_preserves_all_typed_fields() {
    let report = serde_json::json!({
        "upgrade": {
            "command": "mc_for_ams_firmware_upgrade",
            "sequence_id": "88001",
            "result": "fail",
            "err_code": -42,
            "reason": "unsupported",
            "message": "printer rejected selection",
            "unknown_sibling": true
        }
    });

    let acknowledgement =
        parse_firmware_acknowledgement(&report, "mc_for_ams_firmware_upgrade", "88001")
            .unwrap()
            .unwrap();

    assert_eq!(
        acknowledgement,
        FirmwareAcknowledgement {
            command: "mc_for_ams_firmware_upgrade".into(),
            sequence_id: "88001".into(),
            result: Some("fail".into()),
            error_code: Some(-42),
            reason: Some("unsupported".into()),
            message: Some("printer rejected selection".into()),
        }
    );
    assert!(
        parse_firmware_acknowledgement(&report, "upgrade_confirm", "88001")
            .unwrap()
            .is_none()
    );
    assert!(
        parse_firmware_acknowledgement(
            &report,
            "mc_for_ams_firmware_upgrade",
            "different-sequence",
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn firmware_mqtt_options_are_clean_bounded_and_unique() {
    let endpoint = BambuPrinterEndpoint {
        host: "192.0.2.10".into(),
        serial: "SERIAL with unsafe separators and a component that is intentionally very long"
            .into(),
        access_code: "secret".into(),
        model: None,
        name: None,
    };

    let first = firmware_mqtt_options(&endpoint);
    let second = firmware_mqtt_options(&endpoint);

    assert!(first.clean_session());
    assert_eq!(first.max_packet_size(), 256 * 1024);
    assert!(
        first
            .client_id()
            .starts_with("pandar-agent-fw-SERIAL-with-unsafe-sep-")
    );
    assert!(first.client_id().len() <= 80);
    assert_ne!(first.client_id(), second.client_id());
}
