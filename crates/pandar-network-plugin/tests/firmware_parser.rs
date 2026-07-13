use pandar_network_plugin::firmware::{
    PLUGIN_JSON_BODY_LIMIT, StudioFirmwareCommand, StudioFirmwareParse, parse_studio_firmware,
};
use serde::Serialize;

#[test]
fn firmware_parser_accepts_all_commands_and_preserves_exact_fields() {
    let cases = [
        (
            r#"{"upgrade":{"command":"upgrade_confirm","sequence_id":"0009","src_id":-7},"future":true}"#,
            StudioFirmwareCommand::UpgradeConfirm {
                sequence_id: "0009".into(),
                src_id: -7,
            },
        ),
        (
            r#"{"upgrade":{"command":"consistency_confirm","sequence_id":"abc","src_id":9223372036854775807,"future":1}}"#,
            StudioFirmwareCommand::ConsistencyConfirm {
                sequence_id: "abc".into(),
                src_id: i64::MAX,
            },
        ),
        (
            r#"{"upgrade":{"command":"start","sequence_id":"42","src_id":1,"url":"https://example.invalid/fw.bin?sig=exact","module":"n3s/0","version":"01.02.03.04","future":null}}"#,
            StudioFirmwareCommand::Start {
                sequence_id: "42".into(),
                src_id: 1,
                url: "https://example.invalid/fw.bin?sig=exact".into(),
                module: "n3s/0".into(),
                version: "01.02.03.04".into(),
            },
        ),
        (
            r#"{"upgrade":{"command":"mc_for_ams_firmware_upgrade","sequence_id":"ams","src_id":3,"id":-2147483648}}"#,
            StudioFirmwareCommand::McForAmsFirmwareUpgrade {
                sequence_id: "ams".into(),
                src_id: 3,
                id: i32::MIN,
            },
        ),
    ];

    for (message, expected) in cases {
        assert_eq!(
            parse_studio_firmware(message),
            StudioFirmwareParse::Firmware(expected)
        );
    }
}

#[test]
fn firmware_parser_preserves_absent_upgrade_as_not_firmware() {
    for message in [
        r#"{}"#,
        r#"{"print":{"command":"stop"}}"#,
        r#"{"pushing":{"command":"pushall","sequence_id":"1"}}"#,
        r#"{"upgrade":"#,
        r#"["upgrade"]"#,
    ] {
        assert_eq!(
            parse_studio_firmware(message),
            StudioFirmwareParse::NotFirmware
        );
    }
}

#[test]
fn firmware_parser_rejects_present_invalid_upgrade_shapes() {
    let invalid = [
        r#"{"upgrade":null}"#,
        r#"{"upgrade":[]}"#,
        r#"{"upgrade":"upgrade_confirm"}"#,
        r#"{"upgrade":{}}"#,
        r#"{"upgrade":{"command":"future","sequence_id":"1","src_id":1}}"#,
        r#"{"upgrade":{"command":"upgrade_confirm","src_id":1}}"#,
        r#"{"upgrade":{"command":"upgrade_confirm","sequence_id":1,"src_id":1}}"#,
        r#"{"upgrade":{"command":"upgrade_confirm","sequence_id":"1","src_id":"1"}}"#,
        r#"{"upgrade":{"command":"start","sequence_id":"1","src_id":1,"url":"","module":"ota","version":"1"}}"#,
        r#"{"upgrade":{"command":"start","sequence_id":"1","src_id":1,"url":"https://example.invalid/fw","module":"","version":"1"}}"#,
        r#"{"upgrade":{"command":"start","sequence_id":"1","src_id":1,"url":"https://example.invalid/fw","module":"ota","version":""}}"#,
        r#"{"upgrade":{"command":"mc_for_ams_firmware_upgrade","sequence_id":"1","src_id":1,"id":2147483648}}"#,
        r#"{"upgrade":{"command":"mc_for_ams_firmware_upgrade","sequence_id":"1","src_id":1,"id":-2147483649}}"#,
    ];

    for message in invalid {
        assert_eq!(
            parse_studio_firmware(message),
            StudioFirmwareParse::InvalidFirmware,
            "accepted invalid firmware input: {message}"
        );
    }
}

#[test]
fn firmware_parser_rejects_duplicate_top_level_upgrade_keys() {
    for message in [
        r#"{"upgrade":{"command":"upgrade_confirm","sequence_id":"first","src_id":1},"upgrade":{"command":"consistency_confirm","sequence_id":"second","src_id":2}}"#,
        r#"{"upgrade":{"command":"upgrade_confirm","sequence_id":"first","src_id":1},"upgrade":null}"#,
    ] {
        assert_eq!(
            parse_studio_firmware(message),
            StudioFirmwareParse::InvalidFirmware,
            "duplicate upgrade key was not classified as invalid: {message}"
        );
    }
}

#[test]
fn firmware_parser_debug_redacts_start_url_and_query() {
    let command = StudioFirmwareCommand::Start {
        sequence_id: "9001".into(),
        src_id: 1,
        url: "https://user:secret@example.invalid/fw.bin?sig=SENTINEL".into(),
        module: "ota".into(),
        version: "01.02.03.04".into(),
    };
    let diagnostic = format!("{command:?}");

    assert!(!diagnostic.contains("SENTINEL"));
    assert!(!diagnostic.contains("user:secret"));
    assert!(diagnostic.contains("[redacted]"));
}

#[test]
fn firmware_parser_enforces_shared_body_limit_for_every_string_position() {
    for field in ["sequence_id", "url", "module", "version"] {
        let inside = start_message_at_size(field, PLUGIN_JSON_BODY_LIMIT);
        assert_eq!(inside.len(), PLUGIN_JSON_BODY_LIMIT);
        assert!(matches!(
            parse_studio_firmware(&inside),
            StudioFirmwareParse::Firmware(_)
        ));

        let outside = start_message_at_size(field, PLUGIN_JSON_BODY_LIMIT + 1);
        assert_eq!(outside.len(), PLUGIN_JSON_BODY_LIMIT + 1);
        assert_eq!(
            parse_studio_firmware(&outside),
            StudioFirmwareParse::InvalidFirmware
        );
    }
}

#[test]
fn firmware_parser_keeps_valid_oversized_non_firmware_distinct_from_present_upgrade() {
    let non_firmware = oversized_non_firmware_message();
    assert!(non_firmware.len() > PLUGIN_JSON_BODY_LIMIT);
    assert_eq!(
        parse_studio_firmware(&non_firmware),
        StudioFirmwareParse::NotFirmware
    );

    let present_upgrade = format!(
        r#"{{"upgrade":{{"command":"upgrade_confirm","sequence_id":"{}","src_id":1}}}}"#,
        "x".repeat(PLUGIN_JSON_BODY_LIMIT)
    );
    assert!(present_upgrade.len() > PLUGIN_JSON_BODY_LIMIT);
    assert_eq!(
        parse_studio_firmware(&present_upgrade),
        StudioFirmwareParse::InvalidFirmware
    );
}

#[derive(Serialize)]
struct NonFirmwareMessage {
    print: NonFirmwareStatus,
    padding: String,
}

#[derive(Serialize)]
struct NonFirmwareStatus {
    command: &'static str,
}

fn oversized_non_firmware_message() -> String {
    serde_json::to_string(&NonFirmwareMessage {
        print: NonFirmwareStatus {
            command: "push_status",
        },
        padding: "x".repeat(PLUGIN_JSON_BODY_LIMIT),
    })
    .unwrap()
}

fn start_message_at_size(field: &str, size: usize) -> String {
    let mut value = serde_json::json!({
        "upgrade": {
            "command": "start",
            "sequence_id": "s",
            "src_id": 1,
            "url": "u",
            "module": "m",
            "version": "v"
        }
    });
    let base = serde_json::to_string(&value).unwrap();
    let fill = size - base.len() + 1;
    value["upgrade"][field] = serde_json::Value::String("x".repeat(fill));
    serde_json::to_string(&value).unwrap()
}
