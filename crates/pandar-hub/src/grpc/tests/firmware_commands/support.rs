use super::*;

pub(super) fn audit_actor() -> AuditActor {
    AuditActor::plugin_token(None, "firmware-plugin", vec!["plugin"])
}

pub(super) fn start_metadata() -> FirmwareControlMetadata {
    FirmwareControlMetadata::Start {
        sequence_id: "studio-sequence".to_owned(),
        src_id: 1,
        module: "ota".to_owned(),
        version: "01.02.03".to_owned(),
    }
}

pub(super) fn start_command() -> FirmwareCommand {
    FirmwareCommand::Start {
        sequence_id: "studio-sequence".to_owned(),
        src_id: 1,
        url: URL_SENTINEL.to_owned(),
        module: "ota".to_owned(),
        version: "01.02.03".to_owned(),
    }
}

pub(super) fn second_start_metadata() -> FirmwareControlMetadata {
    FirmwareControlMetadata::Start {
        sequence_id: "second-sequence".to_owned(),
        src_id: 2,
        module: "ota".to_owned(),
        version: "02.03.04".to_owned(),
    }
}

pub(super) fn second_start_command() -> FirmwareCommand {
    FirmwareCommand::Start {
        sequence_id: "second-sequence".to_owned(),
        src_id: 2,
        url: SECOND_URL_SENTINEL.to_owned(),
        module: "ota".to_owned(),
        version: "02.03.04".to_owned(),
    }
}

pub(super) fn ticket_start_command() -> FirmwareCommand {
    FirmwareCommand::Start {
        sequence_id: "studio-sequence".to_owned(),
        src_id: 1,
        url: TICKET_URL_SENTINEL.to_owned(),
        module: "ota".to_owned(),
        version: "01.02.03".to_owned(),
    }
}

pub(super) fn upgrade_metadata(sequence_id: &str) -> FirmwareControlMetadata {
    FirmwareControlMetadata::UpgradeConfirm {
        sequence_id: sequence_id.to_owned(),
        src_id: 1,
    }
}

pub(super) fn upgrade_command(sequence_id: &str) -> FirmwareCommand {
    FirmwareCommand::UpgradeConfirm {
        sequence_id: sequence_id.to_owned(),
        src_id: 1,
    }
}

pub(super) fn control_result_event(
    command_id: CommandId,
    serial: &str,
    outcome: firmware_command_result::Outcome,
) -> agent_event::Event {
    control_result_event_inner(command_id, serial, None, outcome)
}

pub(super) fn control_result_event_with_status(
    command_id: CommandId,
    serial: &str,
    transient_status: PrinterFirmwareStatus,
    outcome: firmware_command_result::Outcome,
) -> agent_event::Event {
    control_result_event_inner(command_id, serial, Some(transient_status), outcome)
}

pub(super) fn control_result_event_inner(
    command_id: CommandId,
    serial: &str,
    transient_status: Option<PrinterFirmwareStatus>,
    outcome: firmware_command_result::Outcome,
) -> agent_event::Event {
    agent_event::Event::CommandResult(CommandResult {
        command_id: command_id.to_string(),
        success: true,
        error: String::new(),
        result_json: String::new(),
        firmware_result: Some(FirmwareCommandResult {
            command_id: command_id.to_string(),
            serial: serial.to_owned(),
            generation: GENERATION,
            transient_status,
            outcome: Some(outcome),
        }),
    })
}

pub(super) fn leaking_value(surface: &str) -> String {
    format!("{surface}:{URL_SENTINEL}:user:secret:/main.bin:FIRMWARE-URL-SENTINEL")
}

pub(super) fn leaking_module() -> PrinterFirmwareModule {
    PrinterFirmwareModule {
        name: leaking_value("module-name"),
        software_version: Some(leaking_value("software-version")),
        software_new_version: Some(leaking_value("software-new-version")),
        new_version: Some(leaking_value("new-version")),
        visible: Some(true),
        product_name: Some(leaking_value("product-name")),
        serial_number: Some(leaking_value("serial-number")),
        hardware_version: Some(leaking_value("hardware-version")),
        firmware_flag: Some(17),
    }
}

pub(super) fn module_with_version(name: &str, version: &str) -> PrinterFirmwareModule {
    PrinterFirmwareModule {
        name: name.to_owned(),
        software_version: Some(version.to_owned()),
        software_new_version: None,
        new_version: None,
        visible: None,
        product_name: None,
        serial_number: None,
        hardware_version: None,
        firmware_flag: None,
    }
}

pub(super) fn leaking_upgrade_state() -> PrinterUpgradeState {
    PrinterUpgradeState {
        status: Some(leaking_value("status")),
        progress: Some(leaking_value("progress")),
        message: Some(leaking_value("message")),
        module: Some(leaking_value("module")),
        error_code: Some(23),
        new_version_state: Some(29),
        consistency_request: Some(true),
        force_upgrade: Some(false),
        display_state: Some(31),
        ota_new_version_number: Some(leaking_value("ota-new-version")),
        ams_new_version_number: Some(leaking_value("ams-new-version")),
        ahb_new_version_number: Some(leaking_value("ahb-new-version")),
        new_versions: Some(PrinterFirmwareVersionList {
            versions: vec![PrinterFirmwareVersion {
                name: leaking_value("version-name"),
                current_version: Some(leaking_value("current-version")),
                new_version: Some(leaking_value("version-new-version")),
            }],
        }),
        ams_firmware: Some(AmsFirmwareSwitchState {
            firmware: Some(AmsFirmwareDescriptorList {
                firmware: vec![AmsFirmwareDescriptor {
                    id: 41,
                    name: leaking_value("ams-name"),
                    version: leaking_value("ams-version"),
                }],
            }),
            current_firmware_id: Some(42),
            current_run_firmware_id: Some(43),
            status: Some(leaking_value("ams-status")),
        }),
    }
}
