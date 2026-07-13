use prost::Message;

use super::agent::v1::{
    AgentEvent, CommandResult, FirmwareAcknowledgement, FirmwareCommandResult,
    FirmwareRefreshedModules, HubCommand, PrinterFirmwareModule, PrinterFirmwareStatus,
    PublishedWithoutAcknowledgement, RefreshPrinterMaterials, agent_event, firmware_command_result,
    hub_command,
};

#[test]
fn firmware_wire_round_trips_complete_printer_rejection() {
    let result = CommandResult {
        command_id: "control".into(),
        success: true,
        error: String::new(),
        result_json: String::new(),
        firmware_result: Some(FirmwareCommandResult {
            command_id: "control".into(),
            serial: "SERIAL".into(),
            generation: 7,
            transient_status: Some(PrinterFirmwareStatus {
                upgrade_state: None,
                cfg: Some(String::new()),
            }),
            outcome: Some(firmware_command_result::Outcome::Acknowledgement(
                FirmwareAcknowledgement {
                    command: "mc_for_ams_firmware_upgrade".into(),
                    sequence_id: "-77".into(),
                    result: Some("fail".into()),
                    error_code: Some(-42),
                    reason: Some("unsupported firmware".into()),
                    message: Some("printer refused the selection".into()),
                },
            )),
        }),
    };

    let bytes = result.encode_to_vec();
    assert_eq!(CommandResult::decode(bytes.as_slice()).unwrap(), result);
    assert!(!format!("{result:?}").contains("https://"));
}

#[test]
fn firmware_wire_refresh_keeps_generation_revision_duplicates_and_order() {
    let firmware_result = FirmwareCommandResult {
        command_id: "refresh".into(),
        serial: "SERIAL".into(),
        generation: 13,
        transient_status: None,
        outcome: Some(firmware_command_result::Outcome::RefreshedModules(
            FirmwareRefreshedModules {
                modules: vec![module("old"), module("new")],
                module_revision: 29,
            },
        )),
    };
    let bytes = firmware_result.encode_to_vec();

    assert_eq!(
        FirmwareCommandResult::decode(bytes.as_slice()).unwrap(),
        firmware_result
    );

    let published_without_ack = FirmwareCommandResult {
        command_id: "control".into(),
        serial: "SERIAL".into(),
        generation: 13,
        transient_status: None,
        outcome: Some(
            firmware_command_result::Outcome::PublishedWithoutAcknowledgement(
                PublishedWithoutAcknowledgement {},
            ),
        ),
    };
    let bytes = published_without_ack.encode_to_vec();
    assert_eq!(
        FirmwareCommandResult::decode(bytes.as_slice()).unwrap(),
        published_without_ack
    );
}

fn module(version: &str) -> PrinterFirmwareModule {
    PrinterFirmwareModule {
        name: "n3s/0".into(),
        software_version: Some(version.into()),
        software_new_version: None,
        new_version: None,
        visible: None,
        product_name: None,
        serial_number: None,
        hardware_version: None,
        firmware_flag: None,
    }
}

#[test]
fn firmware_wire_keeps_legacy_agent_and_hub_fixtures_byte_identical() {
    let agent = AgentEvent {
        agent_id: "a".into(),
        tenant_id: "t".into(),
        event_id: "e".into(),
        event: Some(agent_event::Event::CommandResult(CommandResult {
            command_id: "c".into(),
            success: true,
            error: "x".into(),
            result_json: "{}".into(),
            firmware_result: None,
        })),
    };
    assert_eq!(
        agent.encode_to_vec(),
        [
            0x0a, 0x01, b'a', 0x12, 0x01, b't', 0x1a, 0x01, b'e', 0x72, 0x0c, 0x0a, 0x01, b'c',
            0x10, 0x01, 0x1a, 0x01, b'x', 0x22, 0x02, b'{', b'}',
        ]
    );

    let hub = HubCommand {
        command_id: "c".into(),
        command: Some(hub_command::Command::RefreshPrinterMaterials(
            RefreshPrinterMaterials {
                printer_id: "p".into(),
                serial_number: "s".into(),
            },
        )),
    };
    assert_eq!(
        hub.encode_to_vec(),
        [
            0x0a, 0x01, b'c', 0x82, 0x01, 0x06, 0x0a, 0x01, b'p', 0x12, 0x01, b's',
        ]
    );
}
