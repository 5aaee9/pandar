use pandar_core::{
    AmsFirmwareDescriptor, AmsFirmwareSwitchState, PrinterFirmwareStatus, PrinterFirmwareVersion,
    PrinterUpgradeState,
};
use serde_json::json;

use crate::machine::{FirmwareReportReducer, mqtt::MachineReport};

#[test]
fn firmware_reducer_deep_merges_deltas_and_replaces_arrays() {
    let mut reducer = FirmwareReportReducer::new("SERIAL1", 7);
    let first = reducer
        .observe(&MachineReport::decode(json!({
            "print": {
                "msg": 0,
                "cfg": "force-upgrade",
                "upgrade_state": {
                    "status": "DOWNLOADING",
                    "progress": "12",
                    "new_ver_list": [
                        { "name": "ota", "cur_ver": "1", "new_ver": "2" },
                        { "name": "ams/0", "cur_ver": "3", "new_ver": "4" }
                    ],
                    "mc_for_ams_firmware": {
                        "firmware": [
                            { "id": 1, "name": "stable", "version": "3" },
                            { "id": 2, "name": "beta", "version": "4" }
                        ],
                        "current_firmware_id": 1,
                        "current_run_firmware_id": 1,
                        "status": "idle"
                    }
                }
            }
        })))
        .unwrap()
        .expect("initial status");
    assert_eq!(first.revision, 1);

    let delta = reducer
        .observe(&MachineReport::decode(json!({
            "print": {
                "msg": 1,
                "upgrade_state": {
                    "progress": "13",
                    "mc_for_ams_firmware": {
                        "firmware": [{ "id": 3, "name": "replacement", "version": "5" }]
                    }
                }
            }
        })))
        .unwrap()
        .expect("changed status");

    assert_eq!(delta.serial, "SERIAL1");
    assert_eq!(delta.generation, 7);
    assert_eq!(delta.revision, 2);
    assert_eq!(delta.status.cfg.as_deref(), Some("force-upgrade"));
    let upgrade = delta.status.upgrade_state.unwrap();
    assert_eq!(upgrade.status.as_deref(), Some("DOWNLOADING"));
    assert_eq!(upgrade.progress.as_deref(), Some("13"));
    assert_eq!(
        upgrade.new_versions,
        Some(vec![
            PrinterFirmwareVersion {
                name: "ota".to_owned(),
                current_version: Some("1".to_owned()),
                new_version: Some("2".to_owned()),
            },
            PrinterFirmwareVersion {
                name: "ams/0".to_owned(),
                current_version: Some("3".to_owned()),
                new_version: Some("4".to_owned()),
            },
        ])
    );
    assert_eq!(
        upgrade.ams_firmware.unwrap().firmware,
        Some(vec![AmsFirmwareDescriptor {
            id: 3,
            name: "replacement".to_owned(),
            version: "5".to_owned(),
        }])
    );
}

#[test]
fn firmware_reducer_full_reports_preserve_absent_vs_empty_and_clear_prior_status() {
    let mut reducer = FirmwareReportReducer::new("SERIAL1", 1);
    let present_empty = reducer
        .observe(&MachineReport::decode(json!({
            "print": { "msg": 0, "upgrade_state": { "new_ver_list": [] } }
        })))
        .unwrap()
        .unwrap();
    assert_eq!(
        present_empty.status.upgrade_state.unwrap().new_versions,
        Some(Vec::new())
    );

    let absent = reducer
        .observe(&MachineReport::decode(json!({
            "print": { "upgrade_state": { "status": "" }, "cfg": "" }
        })))
        .unwrap()
        .unwrap();
    assert_eq!(absent.revision, 2);
    assert_eq!(
        absent.status,
        PrinterFirmwareStatus {
            upgrade_state: Some(PrinterUpgradeState {
                status: Some(String::new()),
                progress: None,
                message: None,
                module: None,
                error_code: None,
                new_version_state: None,
                consistency_request: None,
                force_upgrade: None,
                display_state: None,
                ota_new_version_number: None,
                ams_new_version_number: None,
                ahb_new_version_number: None,
                new_versions: None,
                ams_firmware: None,
            }),
            cfg: Some(String::new()),
        }
    );

    let cleared = reducer
        .observe(&MachineReport::decode(
            json!({ "print": { "msg": 0, "nozzle_temper": 200 } }),
        ))
        .unwrap()
        .expect("full report clears prior firmware state");
    assert_eq!(cleared.revision, 3);
    assert_eq!(
        cleared.status,
        PrinterFirmwareStatus {
            upgrade_state: None,
            cfg: None,
        }
    );
}

#[test]
fn firmware_reducer_malformed_delta_does_not_poison_next_report() {
    let mut reducer = FirmwareReportReducer::new("SERIAL1", 3);
    reducer
        .observe(&MachineReport::decode(json!({
            "print": {
                "msg": 0,
                "upgrade_state": { "status": "DOWNLOADING", "progress": "40" }
            }
        })))
        .unwrap();

    let error = reducer
        .observe(&MachineReport::decode(json!({
            "print": { "msg": 1, "upgrade_state": { "progress": 41 } }
        })))
        .unwrap_err();
    assert!(format!("{error:#}").contains("upgrade_state"));

    let recovered = reducer
        .observe(&MachineReport::decode(json!({
            "print": { "msg": 1, "upgrade_state": { "message": "still valid" } }
        })))
        .unwrap()
        .unwrap();
    let upgrade = recovered.status.upgrade_state.unwrap();
    assert_eq!(upgrade.progress.as_deref(), Some("40"));
    assert_eq!(upgrade.message.as_deref(), Some("still valid"));
}

#[test]
fn firmware_reducer_only_revises_changed_status_and_ignores_pure_info() {
    let mut reducer = FirmwareReportReducer::new("SERIAL1", 9);
    assert!(
        reducer
            .observe(&MachineReport::decode(json!({
                "info": { "command": "get_version", "module": [] }
            })))
            .unwrap()
            .is_none()
    );
    assert!(
        reducer
            .observe(&MachineReport::decode(
                json!({ "print": { "msg": 0, "nozzle_temper": 200 } })
            ))
            .unwrap()
            .is_none()
    );
    let first = reducer
        .observe(&MachineReport::decode(json!({
            "print": { "msg": 1, "upgrade_state": { "status": "UPGRADING" } }
        })))
        .unwrap()
        .unwrap();
    assert_eq!(first.revision, 1);
    assert!(
        reducer
            .observe(&MachineReport::decode(json!({
                "print": { "msg": 1, "upgrade_state": { "status": "UPGRADING" } }
            })))
            .unwrap()
            .is_none()
    );
    let second = reducer
        .observe(&MachineReport::decode(
            json!({ "print": { "msg": 1, "cfg": "new-cfg" } }),
        ))
        .unwrap()
        .unwrap();
    assert_eq!(second.revision, 2);
}

#[test]
fn firmware_reducer_retains_every_known_upgrade_field() {
    let mut reducer = FirmwareReportReducer::new("SERIAL1", 2);
    let observation = reducer
        .observe(&MachineReport::decode(json!({
            "print": {
                "msg": 0,
                "cfg": "cfg-value",
                "upgrade_state": {
                    "status": "UPGRADING",
                    "progress": "0",
                    "message": "",
                    "module": "ota",
                    "err_code": -2,
                    "new_version_state": 0,
                    "consistency_request": false,
                    "force_upgrade": false,
                    "dis_state": 0,
                    "ota_new_version_number": "01.09",
                    "ams_new_version_number": "00.01",
                    "ahb_new_version_number": "00.02",
                    "new_ver_list": [],
                    "mc_for_ams_firmware": {
                        "firmware": [],
                        "current_firmware_id": -1,
                        "current_run_firmware_id": -1,
                        "status": ""
                    }
                }
            }
        })))
        .unwrap()
        .unwrap();

    assert_eq!(observation.status.cfg.as_deref(), Some("cfg-value"));
    let upgrade = observation.status.upgrade_state.unwrap();
    assert_eq!(upgrade.progress.as_deref(), Some("0"));
    assert_eq!(upgrade.error_code, Some(-2));
    assert_eq!(upgrade.new_version_state, Some(0));
    assert_eq!(upgrade.consistency_request, Some(false));
    assert_eq!(upgrade.force_upgrade, Some(false));
    assert_eq!(upgrade.display_state, Some(0));
    assert_eq!(upgrade.ota_new_version_number.as_deref(), Some("01.09"));
    assert_eq!(upgrade.ams_new_version_number.as_deref(), Some("00.01"));
    assert_eq!(upgrade.ahb_new_version_number.as_deref(), Some("00.02"));
    assert_eq!(upgrade.new_versions, Some(Vec::new()));
    assert_eq!(
        upgrade.ams_firmware,
        Some(AmsFirmwareSwitchState {
            firmware: Some(Vec::new()),
            current_firmware_id: Some(-1),
            current_run_firmware_id: Some(-1),
            status: Some(String::new()),
        })
    );
}
