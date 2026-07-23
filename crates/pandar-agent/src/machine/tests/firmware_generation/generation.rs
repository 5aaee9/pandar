use super::*;

#[tokio::test]
async fn firmware_generation_endpoint_replacement_emits_invalidation_first() {
    let cache = FirmwareObservationCache::default();
    let (sender, mut receiver) = mpsc::channel(8);
    let mut old_endpoint = endpoint("SERIAL1");
    old_endpoint.host = "192.0.2.1".to_owned();

    let first = cache
        .begin_generation(&test_config(), old_endpoint, &sender, None)
        .await
        .unwrap()
        .expect("initial generation");
    let generation_one = first.generation();
    assert_invalidated(receiver.recv().await.unwrap(), generation_one);
    drop(first);

    let mut new_endpoint = endpoint("SERIAL1");
    new_endpoint.host = "192.0.2.2".to_owned();
    let second = cache
        .begin_generation(
            &test_config(),
            new_endpoint.clone(),
            &sender,
            Some(generation_one),
        )
        .await
        .unwrap()
        .expect("current producer may replace generation");
    let generation_two = second.generation();
    assert!(generation_two > generation_one);
    assert_invalidated(receiver.recv().await.unwrap(), generation_two);
    drop(second);

    let snapshot = cache.snapshot("SERIAL1").await.unwrap();
    assert_eq!(snapshot.endpoint, new_endpoint);
    assert_eq!(snapshot.generation, generation_two);
    assert_eq!(snapshot.module_revision, 0);
    assert_eq!(snapshot.status_revision, 0);
    assert!(snapshot.modules.is_none());
    assert!(snapshot.status.is_none());
    assert!(snapshot.reservation.is_none());
}

#[tokio::test]
async fn firmware_generation_rejects_late_old_and_lower_or_equal_revisions() {
    let cache = FirmwareObservationCache::default();
    let (sender, mut receiver) = mpsc::channel(8);
    let first = cache
        .begin_generation(&test_config(), endpoint("SERIAL1"), &sender, None)
        .await
        .unwrap()
        .expect("initial generation");
    let generation_one = first.generation();
    drop(first);
    receiver.recv().await.unwrap();

    assert!(
        cache
            .apply_modules_for_test(FirmwareModulesObservation {
                serial: "SERIAL1".to_owned(),
                generation: generation_one,
                revision: 1,
                modules: vec![module("old")],
            })
            .await
    );

    let second = cache
        .begin_generation(
            &test_config(),
            endpoint("SERIAL1"),
            &sender,
            Some(generation_one),
        )
        .await
        .unwrap()
        .unwrap();
    let generation_two = second.generation();
    drop(second);
    receiver.recv().await.unwrap();

    assert!(
        !cache
            .apply_modules_for_test(FirmwareModulesObservation {
                serial: "SERIAL1".to_owned(),
                generation: generation_one,
                revision: 2,
                modules: vec![module("late-before-new")],
            })
            .await
    );
    assert!(
        cache
            .apply_modules_for_test(FirmwareModulesObservation {
                serial: "SERIAL1".to_owned(),
                generation: generation_two,
                revision: 1,
                modules: vec![module("new")],
            })
            .await
    );
    assert!(
        !cache
            .apply_modules_for_test(FirmwareModulesObservation {
                serial: "SERIAL1".to_owned(),
                generation: generation_one,
                revision: 3,
                modules: vec![module("late-after-new")],
            })
            .await
    );
    for revision in [0, 1] {
        assert!(
            !cache
                .apply_modules_for_test(FirmwareModulesObservation {
                    serial: "SERIAL1".to_owned(),
                    generation: generation_two,
                    revision,
                    modules: vec![module("not-newer")],
                })
                .await
        );
    }

    assert!(
        cache
            .apply_status_for_test(FirmwareStatusObservation {
                serial: "SERIAL1".to_owned(),
                generation: generation_two,
                revision: 2,
                status: status("new-status"),
            })
            .await
    );
    for (generation, revision) in [
        (generation_one, 99),
        (generation_two, 1),
        (generation_two, 2),
    ] {
        assert!(
            !cache
                .apply_status_for_test(FirmwareStatusObservation {
                    serial: "SERIAL1".to_owned(),
                    generation,
                    revision,
                    status: status("stale-status"),
                })
                .await
        );
    }

    let snapshot = cache.snapshot("SERIAL1").await.unwrap();
    assert_eq!(
        snapshot.modules.unwrap()[0].software_version.as_deref(),
        Some("new")
    );
    assert_eq!(
        snapshot
            .status
            .unwrap()
            .upgrade_state
            .unwrap()
            .status
            .as_deref(),
        Some("new-status")
    );
}
