use std::time::{Duration, Instant};

use pandar_core::{FirmwareCatalogEntry, FirmwareCatalogTarget};
use pandar_network_plugin::firmware::{FirmwareStatusCache, firmware_catalog_json};

#[test]
fn firmware_status_renders_exact_current_modules_upgrade_state_and_cfg() {
    let now = Instant::now();
    let mut cache = FirmwareStatusCache::new(17);
    cache
        .observe_printers_at(&batch_json(Some(populated_firmware())), 17, 1, now)
        .unwrap();

    let status = cache
        .next_status_override_at("SERIAL", now)
        .expect("current status");
    let status: serde_json::Value = serde_json::from_str(&status).unwrap();
    assert_eq!(
        status,
        serde_json::json!({
            "info": {
                "command": "get_version",
                "sequence_id": "0",
                "module": [
                    {
                        "name": "ota",
                        "sw_ver": "01.02.03.04",
                        "sw_new_ver": "01.02.04.00",
                        "new_ver": "01.02.05.00",
                        "visible": false,
                        "product_name": "Printer",
                        "sn": "SERIAL",
                        "hw_ver": "AP05",
                        "flag": 5
                    },
                    {"name": "n3s/0", "sw_ver": "00.00.01.00"},
                    {"name": "n3s/0", "sw_ver": "00.00.02.00"}
                ]
            },
            "print": {
                "command": "push_status",
                "msg": 0,
                "cfg": "101",
                "upgrade_state": populated_upgrade_state()
            }
        })
    );
    assert!(status["print"]["upgrade_state"]["progress"].is_string());
}

#[test]
fn firmware_catalog_has_exact_envelope_and_filters_only_empty_urls() {
    let entries = vec![
        FirmwareCatalogEntry {
            target: FirmwareCatalogTarget::Printer,
            version: "01.02.03.04".into(),
            url: "relative/path/fw.bin".into(),
            description: "Printer release".into(),
        },
        FirmwareCatalogEntry {
            target: FirmwareCatalogTarget::Ams,
            version: "02.00.00.00".into(),
            url: "custom+scheme:value".into(),
            description: "AMS release".into(),
        },
        FirmwareCatalogEntry {
            target: FirmwareCatalogTarget::Ams,
            version: "ignored".into(),
            url: String::new(),
            description: "not selectable".into(),
        },
        FirmwareCatalogEntry {
            target: FirmwareCatalogTarget::Ams,
            version: "03.00.00.00".into(),
            url: "https://example.invalid/ams-ht".into(),
            description: "AMS HT release".into(),
        },
    ];

    let json: serde_json::Value =
        serde_json::from_str(&firmware_catalog_json("SERIAL", &entries)).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "devices": [{
                "dev_id": "SERIAL",
                "firmware": [{
                    "version": "01.02.03.04",
                    "url": "relative/path/fw.bin",
                    "description": "Printer release"
                }],
                "ams": [{"firmware": [
                    {
                        "version": "02.00.00.00",
                        "url": "custom+scheme:value",
                        "description": "AMS release"
                    },
                    {
                        "version": "03.00.00.00",
                        "url": "https://example.invalid/ams-ht",
                        "description": "AMS HT release"
                    }
                ]}]
            }]
        })
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&firmware_catalog_json("SERIAL", &[])).unwrap(),
        serde_json::json!({"devices":[{"dev_id":"SERIAL","firmware":[],"ams":[]}]})
    );
}

#[test]
fn firmware_status_emits_exact_reset_immediately_and_past_three_seconds() {
    let start = Instant::now();
    let mut cache = FirmwareStatusCache::new(7);
    cache
        .observe_printers_at(&batch_json(Some(populated_firmware())), 7, 1, start)
        .unwrap();
    cache
        .observe_printers_at(&batch_json(None), 7, 2, start + Duration::from_millis(10))
        .unwrap();

    let reset = exact_reset();
    for elapsed in [10, 2_000, 3_020] {
        let body = cache
            .next_status_override_at("SERIAL", start + Duration::from_millis(elapsed))
            .expect("reset emission");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&body).unwrap(),
            reset
        );
    }
    assert!(
        cache
            .next_status_override_at("SERIAL", start + Duration::from_millis(3_021))
            .is_none()
    );
}

#[test]
fn firmware_status_fresh_current_state_cancels_reset_repetition() {
    let start = Instant::now();
    let mut cache = FirmwareStatusCache::new(9);
    cache
        .observe_printers_at(&batch_json(Some(populated_firmware())), 9, 1, start)
        .unwrap();
    cache
        .observe_printers_at(&batch_json(None), 9, 2, start + Duration::from_millis(1))
        .unwrap();
    assert!(
        cache
            .next_status_override_at("SERIAL", start + Duration::from_millis(1))
            .is_some()
    );

    cache
        .observe_printers_at(
            &batch_json(Some(populated_firmware())),
            9,
            3,
            start + Duration::from_secs(1),
        )
        .unwrap();
    let current = cache
        .next_status_override_at("SERIAL", start + Duration::from_secs(4))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&current).unwrap()["info"]["module"][0]["name"],
        "ota"
    );
}

#[test]
fn newer_firmware_identity_marker_resets_and_rejects_late_old_generation() {
    let start = Instant::now();
    let mut cache = FirmwareStatusCache::new(11);
    cache
        .observe_printers_at(&batch_json(Some(populated_firmware())), 11, 1, start)
        .unwrap();

    cache
        .observe_printers_at(
            &batch_json(Some(marker_firmware("session-1", 6))),
            11,
            2,
            start + Duration::from_millis(1),
        )
        .unwrap();
    assert_eq!(
        status_json(&mut cache, start + Duration::from_millis(1)),
        exact_reset()
    );

    cache
        .observe_printers_at(
            &batch_json(Some(populated_firmware())),
            11,
            1,
            start + Duration::from_secs(1),
        )
        .unwrap();
    assert_eq!(
        status_json(&mut cache, start + Duration::from_millis(3_002)),
        exact_reset()
    );
    assert!(
        cache
            .next_status_override_at("SERIAL", start + Duration::from_millis(3_003))
            .is_none()
    );
}

#[test]
fn newer_firmware_identity_partial_state_follows_one_exact_reset() {
    let start = Instant::now();
    let mut cache = FirmwareStatusCache::new(12);
    cache
        .observe_printers_at(&batch_json(Some(populated_firmware())), 12, 1, start)
        .unwrap();

    let mut partial = marker_firmware("session-1", 6);
    partial["module_revision"] = serde_json::json!(1);
    partial["modules"] = serde_json::json!([]);
    cache
        .observe_printers_at(
            &batch_json(Some(partial)),
            12,
            2,
            start + Duration::from_millis(1),
        )
        .unwrap();

    assert_eq!(
        status_json(&mut cache, start + Duration::from_millis(1)),
        exact_reset()
    );
    assert_eq!(
        status_json(&mut cache, start + Duration::from_millis(2)),
        serde_json::json!({
            "info": {"command": "get_version", "sequence_id": "0", "module": []}
        })
    );
}

#[test]
fn fresh_current_after_invalidation_waits_for_one_exact_reset() {
    let start = Instant::now();
    let mut cache = FirmwareStatusCache::new(13);
    cache
        .observe_printers_at(&batch_json(Some(populated_firmware())), 13, 1, start)
        .unwrap();
    cache
        .observe_printers_at(&batch_json(None), 13, 2, start + Duration::from_millis(1))
        .unwrap();
    cache
        .observe_printers_at(
            &batch_json(Some(populated_firmware())),
            13,
            3,
            start + Duration::from_millis(2),
        )
        .unwrap();

    assert_eq!(
        status_json(&mut cache, start + Duration::from_millis(2)),
        exact_reset()
    );
    assert_eq!(
        status_json(&mut cache, start + Duration::from_millis(3))["info"]["module"][0]["name"],
        "ota"
    );
}

#[test]
fn delayed_lower_revisions_do_not_overwrite_newer_current_state() {
    let start = Instant::now();
    let mut cache = FirmwareStatusCache::new(14);
    cache
        .observe_printers_at(&batch_json(Some(populated_firmware())), 14, 1, start)
        .unwrap();

    let mut newer = populated_firmware();
    newer["module_revision"] = serde_json::json!(10);
    newer["status_revision"] = serde_json::json!(11);
    newer["modules"][0]["sw_ver"] = serde_json::json!("09.09.09.09");
    newer["cfg"] = serde_json::json!("newer-cfg");
    cache
        .observe_printers_at(
            &batch_json(Some(newer)),
            14,
            2,
            start + Duration::from_millis(1),
        )
        .unwrap();
    cache
        .observe_printers_at(
            &batch_json(Some(populated_firmware())),
            14,
            3,
            start + Duration::from_millis(2),
        )
        .unwrap();

    let status = status_json(&mut cache, start + Duration::from_millis(2));
    assert_eq!(status["info"]["module"][0]["sw_ver"], "09.09.09.09");
    assert_eq!(status["print"]["cfg"], "newer-cfg");
}

#[test]
fn delayed_unseen_session_does_not_overwrite_newer_observation() {
    let start = Instant::now();
    let mut cache = FirmwareStatusCache::new(15);
    cache
        .observe_printers_at(&batch_json(Some(populated_firmware())), 15, 1, start)
        .unwrap();

    let mut delayed = populated_firmware();
    delayed["session_id"] = serde_json::json!("session-2");
    delayed["generation"] = serde_json::json!(1);
    delayed["module_revision"] = serde_json::json!(1);
    delayed["status_revision"] = serde_json::json!(1);
    delayed["modules"][0]["sw_ver"] = serde_json::json!("06.06.06.06");
    let mut newer = populated_firmware();
    newer["session_id"] = serde_json::json!("session-3");
    newer["generation"] = serde_json::json!(1);
    newer["module_revision"] = serde_json::json!(1);
    newer["status_revision"] = serde_json::json!(1);
    newer["modules"][0]["sw_ver"] = serde_json::json!("08.08.08.08");
    cache
        .observe_printers_at(
            &batch_json(Some(newer)),
            15,
            3,
            start + Duration::from_millis(1),
        )
        .unwrap();
    assert_eq!(
        status_json(&mut cache, start + Duration::from_millis(1)),
        exact_reset()
    );
    assert_eq!(
        status_json(&mut cache, start + Duration::from_millis(2))["info"]["module"][0]["sw_ver"],
        "08.08.08.08"
    );

    cache
        .observe_printers_at(
            &batch_json(Some(delayed)),
            15,
            2,
            start + Duration::from_millis(3),
        )
        .unwrap();
    assert_eq!(
        status_json(&mut cache, start + Duration::from_millis(3))["info"]["module"][0]["sw_ver"],
        "08.08.08.08"
    );
}

#[test]
fn delayed_same_identity_observations_cannot_undo_invalidation_but_newer_equal_can_recover() {
    let start = Instant::now();
    let mut cache = FirmwareStatusCache::new(18);
    cache
        .observe_printers_at(&batch_json(Some(populated_firmware())), 18, 10, start)
        .unwrap();
    cache
        .observe_printers_at(&batch_json(None), 18, 12, start + Duration::from_millis(1))
        .unwrap();
    assert_eq!(
        status_json(&mut cache, start + Duration::from_millis(1)),
        exact_reset()
    );

    let mut lower = populated_firmware();
    lower["module_revision"] = serde_json::json!(7);
    lower["status_revision"] = serde_json::json!(8);
    cache
        .observe_printers_at(
            &batch_json(Some(lower)),
            18,
            9,
            start + Duration::from_secs(1),
        )
        .unwrap();
    cache
        .observe_printers_at(
            &batch_json(Some(populated_firmware())),
            18,
            11,
            start + Duration::from_secs(2),
        )
        .unwrap();
    assert_eq!(
        status_json(&mut cache, start + Duration::from_millis(3_002)),
        exact_reset()
    );
    assert!(
        cache
            .next_status_override_at("SERIAL", start + Duration::from_millis(3_003))
            .is_none()
    );

    cache
        .observe_printers_at(
            &batch_json(Some(populated_firmware())),
            18,
            13,
            start + Duration::from_millis(3_004),
        )
        .unwrap();
    assert_eq!(
        status_json(&mut cache, start + Duration::from_millis(3_004))["info"]["module"][0]["name"],
        "ota"
    );
}

#[test]
fn malformed_higher_sequence_does_not_block_lower_valid_typed_batch() {
    let start = Instant::now();
    let mut cache = FirmwareStatusCache::new(19);
    cache
        .observe_printers_at(&batch_json(Some(populated_firmware())), 19, 1, start)
        .unwrap();

    let mut malformed = batch_json_value(Some(populated_firmware()));
    malformed["devices"][0]["firmware"]["module_revision"] = serde_json::json!("wrong");
    assert!(
        cache
            .observe_printers_at(&malformed.to_string(), 19, 0, start)
            .is_err()
    );
    assert!(
        cache
            .observe_printers_at(
                &malformed.to_string(),
                19,
                5,
                start + Duration::from_millis(1),
            )
            .is_err()
    );

    let mut valid = populated_firmware();
    valid["session_id"] = serde_json::json!("session-2");
    valid["generation"] = serde_json::json!(1);
    valid["module_revision"] = serde_json::json!(1);
    valid["status_revision"] = serde_json::json!(1);
    valid["modules"][0]["sw_ver"] = serde_json::json!("07.07.07.07");
    cache
        .observe_printers_at(
            &batch_json(Some(valid)),
            19,
            4,
            start + Duration::from_millis(2),
        )
        .unwrap();
    assert_eq!(
        status_json(&mut cache, start + Duration::from_millis(2)),
        exact_reset()
    );
    assert_eq!(
        status_json(&mut cache, start + Duration::from_millis(3))["info"]["module"][0]["sw_ver"],
        "07.07.07.07"
    );
}

#[test]
fn never_populated_marker_does_not_fabricate_reset() {
    let start = Instant::now();
    let mut cache = FirmwareStatusCache::new(16);
    cache
        .observe_printers_at(
            &batch_json(Some(marker_firmware("session-1", 5))),
            16,
            1,
            start,
        )
        .unwrap();

    assert!(cache.next_status_override_at("SERIAL", start).is_none());
}

#[test]
fn firmware_status_rejects_malformed_typed_batch_member_without_mutation() {
    let start = Instant::now();
    let mut cache = FirmwareStatusCache::new(3);
    let malformed = batch_json_value(Some(populated_firmware()));
    let mut malformed = malformed;
    malformed["devices"][0]["firmware"]["module_revision"] = serde_json::json!("wrong");

    assert!(
        cache
            .observe_printers_at(&malformed.to_string(), 3, 1, start)
            .is_err()
    );
    assert!(cache.next_status_override_at("SERIAL", start).is_none());
}

fn populated_firmware() -> serde_json::Value {
    serde_json::json!({
        "session_id": "session-1",
        "generation": 5,
        "module_revision": 8,
        "status_revision": 9,
        "modules": [
            {
                "name": "ota", "sw_ver": "01.02.03.04", "sw_new_ver": "01.02.04.00",
                "new_ver": "01.02.05.00", "visible": false, "product_name": "Printer",
                "sn": "SERIAL", "hw_ver": "AP05", "flag": 5
            },
            {"name": "n3s/0", "sw_ver": "00.00.01.00"},
            {"name": "n3s/0", "sw_ver": "00.00.02.00"}
        ],
        "upgrade_state": populated_upgrade_state(),
        "cfg": "101"
    })
}

fn marker_firmware(session_id: &str, generation: u64) -> serde_json::Value {
    serde_json::json!({
        "session_id": session_id,
        "generation": generation,
        "module_revision": 0,
        "status_revision": 0
    })
}

fn status_json(cache: &mut FirmwareStatusCache, now: Instant) -> serde_json::Value {
    serde_json::from_str(
        &cache
            .next_status_override_at("SERIAL", now)
            .expect("firmware status override"),
    )
    .unwrap()
}

fn populated_upgrade_state() -> serde_json::Value {
    serde_json::json!({
        "status": "UPGRADING", "progress": "37", "message": "flashing", "module": "ota",
        "err_code": 12, "new_version_state": 2, "consistency_request": true,
        "force_upgrade": true, "dis_state": 3, "ota_new_version_number": "01.02.04.00",
        "ams_new_version_number": "02.00.00.00", "ahb_new_version_number": "03.00.00.00",
        "new_ver_list": [{"name":"ota","cur_ver":"1","new_ver":"2"}],
        "mc_for_ams_firmware": {
            "firmware": [{"id":4,"name":"stable","version":"02.00.00.00"}],
            "current_firmware_id": 4, "current_run_firmware_id": 3, "status": "SWITCHING"
        }
    })
}

fn exact_reset() -> serde_json::Value {
    serde_json::json!({
        "info":{"command":"get_version","sequence_id":"0","result":"fail","module":[]},
        "print":{"command":"push_status","msg":0,"cfg":"","upgrade_state":{
            "status":"","progress":"","message":"","module":"","err_code":0,
            "new_version_state":0,"consistency_request":false,"force_upgrade":false,"dis_state":0,
            "ota_new_version_number":"","ams_new_version_number":"","ahb_new_version_number":"",
            "new_ver_list":[],"mc_for_ams_firmware":{"firmware":[],"current_firmware_id":-1,
            "current_run_firmware_id":-1,"status":""}
        }}
    })
}

fn batch_json(firmware: Option<serde_json::Value>) -> String {
    batch_json_value(firmware).to_string()
}

fn batch_json_value(firmware: Option<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({"message":"success","devices":[{
        "dev_id":"SERIAL","dev_name":"Printer","name":"Printer","dev_ip":null,
        "dev_access_code":null,"dev_model_name":"N6","model":"N6","dev_online":true,
        "online":true,"task_status":"IDLE","state":"IDLE","gcode_state":"IDLE",
        "mc_percent":0,"mc_remaining_time":0,"layer_num":0,"total_layer_num":0,"task_id":null,
        "print_error":null,"job_id":null,"subtask_id":null,"gcode_file":null,"subtask_name":null,
        "hms":[],"pandar_printer_id":"printer-1","nozzle_temperatures":[],"active_nozzle":null,
        "bed_temperature_celsius":null,"bed_target_temperature_celsius":null,
        "chamber_temperature_celsius":null,"chamber_light_on":null,"materials":null,
        "firmware":firmware
    }]})
}
