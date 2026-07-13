use pandar_core::FirmwareCommand;
use tokio::sync::mpsc;

use super::{endpoint, test_config};
use crate::machine::{
    BambuPrinterEndpoint, FirmwareExecuteRequest, FirmwareMachineGateway, FirmwareObservationCache,
    FirmwarePrepareRequest,
};

#[test]
fn runtime_gateway_implements_independent_firmware_gateway() {
    fn assert_gateway<T: FirmwareMachineGateway>() {}
    assert_gateway::<crate::machine::runtime::RuntimeBambuMachineGateway>();
}

#[tokio::test]
async fn firmware_control_reservation_is_epoch_generation_bound_and_busy_in_flight() {
    let cache = FirmwareObservationCache::default();
    let (sender, _receiver) = mpsc::channel(8);
    let transition = cache
        .begin_generation(&test_config(), endpoint("SERIAL1"), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let generation = transition.generation();
    drop(transition);

    let prepared = cache
        .prepare_firmware_control(FirmwarePrepareRequest {
            command_id: "command-a".into(),
            serial: "SERIAL1".into(),
            expected_generation: generation,
            session_epoch: 41,
        })
        .await
        .unwrap();
    assert_eq!(prepared.command_id, "command-a");
    assert_eq!(prepared.generation, generation);
    assert_eq!(
        cache
            .snapshot("SERIAL1")
            .await
            .unwrap()
            .reservation
            .unwrap()
            .session_epoch,
        41
    );

    let execution = cache
        .claim_firmware_execute(&FirmwareExecuteRequest {
            command_id: "command-a".into(),
            serial: "SERIAL1".into(),
            expected_generation: generation,
            session_epoch: 41,
            command: FirmwareCommand::UpgradeConfirm {
                sequence_id: "9001".into(),
                src_id: 1,
            },
        })
        .await
        .unwrap();
    let busy = cache
        .prepare_firmware_control(FirmwarePrepareRequest {
            command_id: "command-b".into(),
            serial: "SERIAL1".into(),
            expected_generation: generation,
            session_epoch: 41,
        })
        .await
        .unwrap_err();
    assert!(format!("{busy:#}").contains("busy"));

    let publish = execution.publish_transition().await.unwrap();
    assert_eq!(publish.endpoint().serial, "SERIAL1");
    drop(publish);
    drop(execution);
    cache
        .prepare_firmware_control(FirmwarePrepareRequest {
            command_id: "command-b".into(),
            serial: "SERIAL1".into(),
            expected_generation: generation,
            session_epoch: 42,
        })
        .await
        .unwrap();

    let wrong_epoch = cache
        .claim_firmware_execute(&FirmwareExecuteRequest {
            command_id: "command-b".into(),
            serial: "SERIAL1".into(),
            expected_generation: generation,
            session_epoch: 43,
            command: FirmwareCommand::UpgradeConfirm {
                sequence_id: "9002".into(),
                src_id: 1,
            },
        })
        .await
        .unwrap_err();
    assert!(format!("{wrong_epoch:#}").contains("session"));
    cache.cancel_firmware_session(42).await;
    assert!(
        cache
            .snapshot("SERIAL1")
            .await
            .unwrap()
            .reservation
            .is_none()
    );
}

#[tokio::test]
async fn firmware_control_preparation_expires_after_one_second() {
    let cache = FirmwareObservationCache::default();
    let (sender, _receiver) = mpsc::channel(8);
    let transition = cache
        .begin_generation(&test_config(), endpoint("SERIAL1"), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let generation = transition.generation();
    drop(transition);
    cache
        .prepare_firmware_control(FirmwarePrepareRequest {
            command_id: "expiring".into(),
            serial: "SERIAL1".into(),
            expected_generation: generation,
            session_epoch: 5,
        })
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;
    assert!(
        cache.raw_reservation_for_test("SERIAL1").await.is_some(),
        "reservation expiry must be lazy and owned by cache boundaries"
    );
    assert!(
        cache
            .snapshot("SERIAL1")
            .await
            .unwrap()
            .reservation
            .is_none()
    );
    assert!(cache.raw_reservation_for_test("SERIAL1").await.is_none());
}

#[tokio::test]
async fn ending_session_epoch_rejects_late_prepare_and_claim() {
    let cache = FirmwareObservationCache::default();
    let (sender, _receiver) = mpsc::channel(8);
    let transition = cache
        .begin_generation(&test_config(), endpoint("SERIAL1"), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let generation = transition.generation();
    drop(transition);
    cache
        .prepare_firmware_control(FirmwarePrepareRequest {
            command_id: "old-command".into(),
            serial: "SERIAL1".into(),
            expected_generation: generation,
            session_epoch: 90,
        })
        .await
        .unwrap();
    cache.cancel_firmware_session(90).await;

    let prepare_error = cache
        .prepare_firmware_control(FirmwarePrepareRequest {
            command_id: "late-command".into(),
            serial: "SERIAL1".into(),
            expected_generation: generation,
            session_epoch: 90,
        })
        .await
        .unwrap_err();
    let claim_error = cache
        .claim_firmware_execute(&FirmwareExecuteRequest {
            command_id: "old-command".into(),
            serial: "SERIAL1".into(),
            expected_generation: generation,
            session_epoch: 90,
            command: FirmwareCommand::UpgradeConfirm {
                sequence_id: "late".into(),
                src_id: 1,
            },
        })
        .await
        .unwrap_err();

    assert!(format!("{prepare_error:#}").contains("ended reverse session"));
    assert!(format!("{claim_error:#}").contains("ended reverse session"));
    cache
        .prepare_firmware_control(FirmwarePrepareRequest {
            command_id: "new-command".into(),
            serial: "SERIAL1".into(),
            expected_generation: generation,
            session_epoch: 91,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn exact_firmware_claim_is_atomic() {
    let cache = FirmwareObservationCache::default();
    let (sender, _receiver) = mpsc::channel(8);
    let transition = cache
        .begin_generation(&test_config(), endpoint("SERIAL1"), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let generation = transition.generation();
    drop(transition);
    cache
        .prepare_firmware_control(FirmwarePrepareRequest {
            command_id: "atomic".into(),
            serial: "SERIAL1".into(),
            expected_generation: generation,
            session_epoch: 100,
        })
        .await
        .unwrap();
    let request = FirmwareExecuteRequest {
        command_id: "atomic".into(),
        serial: "SERIAL1".into(),
        expected_generation: generation,
        session_epoch: 100,
        command: FirmwareCommand::UpgradeConfirm {
            sequence_id: "atomic".into(),
            src_id: 1,
        },
    };

    let (first, second) = tokio::join!(
        cache.claim_firmware_execute(&request),
        cache.claim_firmware_execute(&request)
    );

    assert_ne!(first.is_ok(), second.is_ok());
    let error = first.err().or_else(|| second.err()).unwrap();
    assert!(format!("{error:#}").contains("busy"));
}

#[tokio::test]
async fn stale_generation_is_rejected_at_prepare_and_rechecked_before_publish() {
    let cache = FirmwareObservationCache::default();
    let (sender, _receiver) = mpsc::channel(8);
    let transition = cache
        .begin_generation(&test_config(), endpoint("SERIAL1"), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let old_generation = transition.generation();
    drop(transition);
    cache
        .prepare_firmware_control(FirmwarePrepareRequest {
            command_id: "old-generation".into(),
            serial: "SERIAL1".into(),
            expected_generation: old_generation,
            session_epoch: 110,
        })
        .await
        .unwrap();
    let execution = cache
        .claim_firmware_execute(&FirmwareExecuteRequest {
            command_id: "old-generation".into(),
            serial: "SERIAL1".into(),
            expected_generation: old_generation,
            session_epoch: 110,
            command: FirmwareCommand::UpgradeConfirm {
                sequence_id: "old".into(),
                src_id: 1,
            },
        })
        .await
        .unwrap();
    let replacement = cache
        .begin_generation(
            &test_config(),
            BambuPrinterEndpoint {
                host: "192.0.2.20".into(),
                ..endpoint("SERIAL1")
            },
            &sender,
            Some(old_generation),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(replacement.generation(), old_generation + 1);
    drop(replacement);

    let publish_error = match execution.publish_transition().await {
        Ok(_) => panic!("stale execution unexpectedly reached publish transition"),
        Err(error) => error,
    };
    let prepare_error = cache
        .prepare_firmware_control(FirmwarePrepareRequest {
            command_id: "stale".into(),
            serial: "SERIAL1".into(),
            expected_generation: old_generation,
            session_epoch: 111,
        })
        .await
        .unwrap_err();

    assert!(format!("{publish_error:#}").contains("no longer current"));
    assert!(format!("{prepare_error:#}").contains("stale firmware generation"));
}
