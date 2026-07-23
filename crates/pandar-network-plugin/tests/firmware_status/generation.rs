use super::support::*;

pub(crate) fn newer_firmware_identity_marker_resets_and_rejects_late_old_generation() {
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

pub(crate) fn newer_firmware_identity_partial_state_follows_one_exact_reset() {
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

pub(crate) fn fresh_current_after_invalidation_waits_for_one_exact_reset() {
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

pub(crate) fn delayed_lower_revisions_do_not_overwrite_newer_current_state() {
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

pub(crate) fn delayed_unseen_session_does_not_overwrite_newer_observation() {
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

pub(crate) fn delayed_same_identity_observations_cannot_undo_invalidation_but_newer_equal_can_recover()
 {
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
