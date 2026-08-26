use std::fmt::Debug;

use prost::Message;

use crate::agent::v1::{
    AgentCapability, AgentEvent, AmsFirmwareDescriptorList, AmsFirmwareSwitchState, CommandResult,
    ExecuteFirmwareControl, FirmwareAcknowledgement, FirmwareCommand, FirmwareCommandResult,
    FirmwareConsistencyConfirm, FirmwarePrepared, FirmwarePublished, FirmwareRefreshedModules,
    FirmwareStart, FirmwareSwitchAmsFirmware, FirmwareUpgradeConfirm, HubCommand,
    PrepareFirmwareControl, PrinterFirmwareInvalidated, PrinterFirmwareModule,
    PrinterFirmwareModulesSnapshot, PrinterFirmwareStatus, PrinterFirmwareStatusSnapshot,
    PrinterFirmwareVersionList, PrinterUpgradeState, PublishedWithoutAcknowledgement,
    RefreshFirmwareVersion, RefreshPrinterMaterials, agent_event, firmware_command,
    firmware_command_result, hub_command,
};

mod studio_print;

fn assert_round_trip<T>(value: T)
where
    T: Message + Default + PartialEq + Debug,
{
    let bytes = value.encode_to_vec();
    assert_eq!(T::decode(bytes.as_slice()).unwrap(), value);
}

fn module(version: &str) -> PrinterFirmwareModule {
    PrinterFirmwareModule {
        name: "n3s/0".into(),
        software_version: Some(version.into()),
        software_new_version: Some(String::new()),
        new_version: Some("01.02.05.00".into()),
        visible: Some(false),
        product_name: Some("AMS HT".into()),
        serial_number: Some("AMS-HT-SN".into()),
        hardware_version: Some("N3S".into()),
        firmware_flag: Some(5),
    }
}

fn event(event: agent_event::Event) -> AgentEvent {
    AgentEvent {
        agent_id: String::new(),
        tenant_id: String::new(),
        event_id: String::new(),
        event: Some(event),
    }
}

#[test]
fn firmware_wire_round_trips_order_presence_and_exact_event_tags() {
    assert_eq!(AgentCapability::FirmwareControl as i32, 5);
    assert_eq!(AgentCapability::StudioLocalCamera as i32, 7);

    let modules = event(agent_event::Event::PrinterFirmwareModulesSnapshot(
        PrinterFirmwareModulesSnapshot {
            serial: "SERIAL".into(),
            generation: 7,
            module_revision: 11,
            modules: vec![module("01.00.00.00"), module("02.00.00.00")],
        },
    ));
    assert_eq!(&modules.encode_to_vec()[..2], &[0x92, 0x01]);
    assert_round_trip(modules);

    let status = event(agent_event::Event::PrinterFirmwareStatusSnapshot(
        PrinterFirmwareStatusSnapshot {
            serial: "SERIAL".into(),
            generation: 7,
            status_revision: 12,
            upgrade_state: Some(PrinterUpgradeState {
                status: Some(String::new()),
                progress: Some("0".into()),
                message: Some(String::new()),
                module: Some(String::new()),
                error_code: Some(0),
                new_version_state: Some(0),
                consistency_request: Some(false),
                force_upgrade: Some(false),
                display_state: Some(0),
                ota_new_version_number: Some(String::new()),
                ams_new_version_number: Some(String::new()),
                ahb_new_version_number: Some(String::new()),
                new_versions: Some(PrinterFirmwareVersionList { versions: vec![] }),
                ams_firmware: Some(AmsFirmwareSwitchState {
                    firmware: Some(AmsFirmwareDescriptorList { firmware: vec![] }),
                    current_firmware_id: Some(-2),
                    current_run_firmware_id: Some(-1),
                    status: Some(String::new()),
                }),
            }),
            cfg: Some(String::new()),
        },
    ));
    assert_eq!(&status.encode_to_vec()[..2], &[0x9a, 0x01]);
    assert_round_trip(status);

    let invalidated = event(agent_event::Event::PrinterFirmwareInvalidated(
        PrinterFirmwareInvalidated {
            serial: "SERIAL".into(),
            generation: 8,
        },
    ));
    assert_eq!(&invalidated.encode_to_vec()[..2], &[0xa2, 0x01]);
    assert_round_trip(invalidated);

    let prepared = event(agent_event::Event::FirmwarePrepared(FirmwarePrepared {
        command_id: "prepare".into(),
        serial: "SERIAL".into(),
        generation: 8,
    }));
    assert_eq!(&prepared.encode_to_vec()[..2], &[0xaa, 0x01]);
    assert_round_trip(prepared);

    let published = event(agent_event::Event::FirmwarePublished(FirmwarePublished {
        command_id: "publish".into(),
        serial: "SERIAL".into(),
        generation: 8,
    }));
    assert_eq!(&published.encode_to_vec()[..2], &[0xb2, 0x01]);
    assert_round_trip(published);
}

fn hub_command(command: hub_command::Command) -> HubCommand {
    HubCommand {
        command_id: String::new(),
        command: Some(command),
    }
}

#[test]
fn firmware_wire_round_trips_all_commands_and_exact_hub_tags() {
    let refresh = hub_command(hub_command::Command::RefreshFirmwareVersion(
        RefreshFirmwareVersion {
            serial: "SERIAL".into(),
            sequence_id: "101".into(),
            expected_generation: 7,
        },
    ));
    assert_eq!(&refresh.encode_to_vec()[..2], &[0x92, 0x01]);
    assert_round_trip(refresh);

    let prepare = hub_command(hub_command::Command::PrepareFirmwareControl(
        PrepareFirmwareControl {
            command_id: "command-1".into(),
            serial: "SERIAL".into(),
            expected_generation: 7,
        },
    ));
    assert_eq!(&prepare.encode_to_vec()[..2], &[0x9a, 0x01]);
    assert_round_trip(prepare);

    let variants = [
        firmware_command::Command::UpgradeConfirm(FirmwareUpgradeConfirm {}),
        firmware_command::Command::ConsistencyConfirm(FirmwareConsistencyConfirm {}),
        firmware_command::Command::Start(FirmwareStart {
            url: "https://example.invalid/firmware.bin".into(),
            module: "ota".into(),
            version: "01.02.03.04".into(),
        }),
        firmware_command::Command::SwitchAmsFirmware(FirmwareSwitchAmsFirmware { id: -7 }),
    ];
    for variant in variants {
        let execute = hub_command(hub_command::Command::ExecuteFirmwareControl(
            ExecuteFirmwareControl {
                command_id: "command-1".into(),
                serial: "SERIAL".into(),
                expected_generation: 7,
                command: Some(FirmwareCommand {
                    sequence_id: "101".into(),
                    src_id: -1,
                    command: Some(variant),
                }),
            },
        ));
        assert_eq!(&execute.encode_to_vec()[..2], &[0xa2, 0x01]);
        assert_round_trip(execute);
    }
}

#[test]
fn firmware_wire_round_trips_refresh_acknowledgement_and_unknown_outcomes() {
    let refreshed = FirmwareCommandResult {
        command_id: "refresh".into(),
        serial: "SERIAL".into(),
        generation: 9,
        transient_status: None,
        outcome: Some(firmware_command_result::Outcome::RefreshedModules(
            FirmwareRefreshedModules {
                modules: vec![module("01.00.00.00"), module("02.00.00.00")],
                module_revision: 31,
            },
        )),
    };
    assert_round_trip(refreshed);

    let acknowledged = FirmwareCommandResult {
        command_id: "control".into(),
        serial: "SERIAL".into(),
        generation: 9,
        transient_status: Some(PrinterFirmwareStatus {
            upgrade_state: None,
            cfg: Some(String::new()),
        }),
        outcome: Some(firmware_command_result::Outcome::Acknowledgement(
            FirmwareAcknowledgement {
                command: "mc_for_ams_firmware_upgrade".into(),
                sequence_id: "101".into(),
                result: Some("fail".into()),
                error_code: Some(-42),
                reason: Some("unsupported".into()),
                message: Some("printer rejected selection".into()),
            },
        )),
    };
    assert!(!format!("{acknowledged:?}").contains("https://"));
    assert_round_trip(acknowledged);

    assert_round_trip(FirmwareCommandResult {
        command_id: "control".into(),
        serial: "SERIAL".into(),
        generation: 9,
        transient_status: None,
        outcome: Some(
            firmware_command_result::Outcome::PublishedWithoutAcknowledgement(
                PublishedWithoutAcknowledgement {},
            ),
        ),
    });
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

fn sparse_module(version: &str) -> PrinterFirmwareModule {
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
                modules: vec![sparse_module("old"), sparse_module("new")],
                module_revision: 29,
            },
        )),
    };
    let bytes = firmware_result.encode_to_vec();

    assert_eq!(
        FirmwareCommandResult::decode(bytes.as_slice()).unwrap(),
        firmware_result
    );
}
