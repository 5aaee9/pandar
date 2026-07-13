use crate::{
    AmsFirmwareSwitchState, FirmwareAcknowledgement, FirmwareCommand, FirmwareControlMetadata,
    FirmwareTerminalOutcome, PrinterFirmwareModule, PrinterUpgradeState,
};
use serde_json::json;

macro_rules! assert_not_impl_any {
    ($type:ty: $trait:path) => {
        const _: fn() = || {
            trait AmbiguousIfImpl<A> {
                fn marker() {}
            }
            impl<T: ?Sized> AmbiguousIfImpl<()> for T {}
            struct Invalid;
            impl<T: ?Sized + $trait> AmbiguousIfImpl<Invalid> for T {}
            let _ = <$type as AmbiguousIfImpl<_>>::marker;
        };
    };
}

assert_not_impl_any!(FirmwareCommand: serde::Serialize);

#[test]
fn firmware_module_preserves_exact_printer_json_keys() {
    let module = PrinterFirmwareModule {
        name: "n3s/0".into(),
        software_version: Some("01.02.03.04".into()),
        software_new_version: Some("01.02.04.00".into()),
        new_version: Some("01.02.05.00".into()),
        visible: Some(false),
        product_name: Some("AMS HT".into()),
        serial_number: Some("AMS-HT-SN".into()),
        hardware_version: Some("N3S".into()),
        firmware_flag: Some(5),
    };

    let encoded = serde_json::to_value(&module).unwrap();
    assert_eq!(
        encoded,
        json!({
            "name": "n3s/0",
            "sw_ver": "01.02.03.04",
            "sw_new_ver": "01.02.04.00",
            "new_ver": "01.02.05.00",
            "visible": false,
            "product_name": "AMS HT",
            "sn": "AMS-HT-SN",
            "hw_ver": "N3S",
            "flag": 5,
        })
    );
    assert_eq!(
        serde_json::from_value::<PrinterFirmwareModule>(encoded).unwrap(),
        module
    );
}

#[test]
fn firmware_upgrade_state_preserves_scalar_and_collection_presence() {
    let present = PrinterUpgradeState {
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
        new_versions: Some(Vec::new()),
        ams_firmware: Some(AmsFirmwareSwitchState {
            firmware: Some(Vec::new()),
            current_firmware_id: Some(-1),
            current_run_firmware_id: Some(0),
            status: Some(String::new()),
        }),
    };

    let encoded = serde_json::to_value(&present).unwrap();
    assert_eq!(encoded["progress"], json!("0"));
    assert_eq!(encoded["new_ver_list"], json!([]));
    assert_eq!(encoded["mc_for_ams_firmware"]["firmware"], json!([]));
    assert_eq!(
        serde_json::from_value::<PrinterUpgradeState>(encoded).unwrap(),
        present
    );

    let absent: PrinterUpgradeState = serde_json::from_value(json!({})).unwrap();
    assert_eq!(serde_json::to_value(absent).unwrap(), json!({}));

    let absent_nested: AmsFirmwareSwitchState = serde_json::from_value(json!({})).unwrap();
    assert_eq!(absent_nested.firmware, None);
    assert_eq!(serde_json::to_value(absent_nested).unwrap(), json!({}));
}

#[test]
fn firmware_start_debug_and_metadata_never_expose_url() {
    let command = FirmwareCommand::Start {
        sequence_id: "9001".into(),
        src_id: 1,
        url: "https://user:secret@example.invalid/fw.bin?sig=SENTINEL".into(),
        module: "ota".into(),
        version: "01.02.03.04".into(),
    };

    let debug = format!("{command:?}");
    assert!(debug.contains("[redacted]"));
    assert!(!debug.contains("SENTINEL"));

    let metadata = FirmwareControlMetadata::from(&command);
    let encoded = serde_json::to_value(metadata).unwrap();
    assert_eq!(
        encoded,
        json!({
            "command": "start",
            "sequence_id": "9001",
            "src_id": 1,
            "module": "ota",
            "version": "01.02.03.04",
        })
    );
    assert!(!encoded.to_string().contains("SENTINEL"));
}

#[test]
fn firmware_printer_rejection_round_trips_without_collapsing_fields() {
    let outcome = FirmwareTerminalOutcome::Acknowledged {
        acknowledgement: FirmwareAcknowledgement {
            command: "mc_for_ams_firmware_upgrade".into(),
            sequence_id: "-77".into(),
            result: Some("fail".into()),
            error_code: Some(-42),
            reason: Some("unsupported firmware".into()),
            message: Some("printer refused the selection".into()),
        },
    };

    let encoded = serde_json::to_value(&outcome).unwrap();
    assert_eq!(encoded["acknowledgement"]["err_code"], json!(-42));
    assert_eq!(
        serde_json::from_value::<FirmwareTerminalOutcome>(encoded).unwrap(),
        outcome
    );
}
