use super::support::*;

pub(crate) fn firmware_status_renders_exact_current_modules_upgrade_state_and_cfg() {
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

pub(crate) fn firmware_catalog_has_exact_envelope_and_filters_only_empty_urls() {
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

pub(crate) fn firmware_status_emits_exact_reset_immediately_and_past_three_seconds() {
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

pub(crate) fn firmware_status_fresh_current_state_cancels_reset_repetition() {
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
