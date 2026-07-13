use pandar_core::{
    FirmwareAcknowledgement, PrinterFirmwareModule, PrinterFirmwareState, PrinterFirmwareStatus,
    PrinterUpgradeState,
};
use serde::Serialize;

#[derive(Serialize)]
struct FirmwareEnvelope<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    info: Option<VersionReport<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    print: Option<UpgradeReport<'a>>,
}

#[derive(Serialize)]
struct VersionReport<'a> {
    command: &'static str,
    sequence_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<&'static str>,
    module: &'a [PrinterFirmwareModule],
}

#[derive(Serialize)]
struct UpgradeReport<'a> {
    command: &'static str,
    msg: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    cfg: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    upgrade_state: Option<&'a PrinterUpgradeState>,
}

#[derive(Serialize)]
struct AcknowledgementEnvelope<'a> {
    upgrade: &'a FirmwareAcknowledgement,
    #[serde(skip_serializing_if = "Option::is_none")]
    print: Option<UpgradeReport<'a>>,
}

pub(crate) fn current_firmware_json(state: &PrinterFirmwareState) -> Option<String> {
    let info = state.modules.as_deref().map(|modules| VersionReport {
        command: "get_version",
        sequence_id: "0",
        result: None,
        module: modules,
    });
    let print = (state.upgrade_state.is_some() || state.cfg.is_some()).then_some(UpgradeReport {
        command: "push_status",
        msg: 0,
        cfg: state.cfg.as_deref(),
        upgrade_state: state.upgrade_state.as_ref(),
    });
    (info.is_some() || print.is_some()).then(|| serialize(&FirmwareEnvelope { info, print }))
}

pub(crate) fn firmware_reset_json() -> String {
    let modules = Vec::new();
    let state = reset_upgrade_state();
    serialize(&FirmwareEnvelope {
        info: Some(VersionReport {
            command: "get_version",
            sequence_id: "0",
            result: Some("fail"),
            module: &modules,
        }),
        print: Some(UpgradeReport {
            command: "push_status",
            msg: 0,
            cfg: Some(""),
            upgrade_state: Some(&state),
        }),
    })
}

pub(crate) fn firmware_refresh_success_json(
    sequence_id: &str,
    modules: &[PrinterFirmwareModule],
) -> String {
    serialize(&FirmwareEnvelope {
        info: Some(VersionReport {
            command: "get_version",
            sequence_id,
            result: None,
            module: modules,
        }),
        print: None,
    })
}

pub(crate) fn firmware_refresh_failure_json(sequence_id: &str) -> String {
    let modules = Vec::new();
    serialize(&FirmwareEnvelope {
        info: Some(VersionReport {
            command: "get_version",
            sequence_id,
            result: Some("fail"),
            module: &modules,
        }),
        print: None,
    })
}

pub(crate) fn acknowledgement_callback_json(
    acknowledgement: &FirmwareAcknowledgement,
    status: Option<&PrinterFirmwareStatus>,
) -> String {
    let print = status.and_then(|status| {
        (status.upgrade_state.is_some() || status.cfg.is_some()).then_some(UpgradeReport {
            command: "push_status",
            msg: 1,
            cfg: status.cfg.as_deref(),
            upgrade_state: status.upgrade_state.as_ref(),
        })
    });
    serialize(&AcknowledgementEnvelope {
        upgrade: acknowledgement,
        print,
    })
}

fn reset_upgrade_state() -> PrinterUpgradeState {
    PrinterUpgradeState {
        status: Some(String::new()),
        progress: Some(String::new()),
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
        new_versions: Some(Vec::new()),
        ams_firmware: Some(pandar_core::AmsFirmwareSwitchState {
            firmware: Some(Vec::new()),
            current_firmware_id: Some(-1),
            current_run_firmware_id: Some(-1),
            status: Some(String::new()),
        }),
    }
}

fn serialize(value: &impl Serialize) -> String {
    serde_json::to_string(value).expect("typed Studio firmware response is serializable")
}
