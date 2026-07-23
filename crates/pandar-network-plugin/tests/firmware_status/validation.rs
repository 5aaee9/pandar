use super::support::*;

pub(crate) fn malformed_higher_sequence_does_not_block_lower_valid_typed_batch() {
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

pub(crate) fn never_populated_marker_does_not_fabricate_reset() {
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

pub(crate) fn firmware_status_rejects_malformed_typed_batch_member_without_mutation() {
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
