use std::time::Duration;

use pandar_core::{PrinterFirmwareModule, PrinterFirmwareStatus, PrinterUpgradeState};
use tokio::sync::{mpsc, oneshot};

use crate::{
    machine::{
        FirmwareModulesObservation, FirmwareObservationCache, FirmwareReportContext,
        FirmwareStatusObservation, RuntimeReportContext,
        firmware_event_pause::{self, FirmwareEventKind},
    },
    protocol::agent::v1::{AgentEvent, agent_event},
};

use super::{endpoint, test_config};

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

#[tokio::test]
async fn firmware_generation_refresh_leases_serialize_per_serial_only() {
    let cache = FirmwareObservationCache::default();
    let first = cache.version_observation_lease("SERIAL1").await;
    let same_cache = cache.clone();
    let (same_acquired, same_receiver) = oneshot::channel();
    let same = tokio::spawn(async move {
        let _lease = same_cache.version_observation_lease("SERIAL1").await;
        let _ = same_acquired.send(());
    });

    assert!(
        tokio::time::timeout(Duration::from_millis(25), same_receiver)
            .await
            .is_err()
    );
    let other = tokio::time::timeout(
        Duration::from_millis(250),
        cache.version_observation_lease("SERIAL2"),
    )
    .await
    .expect("different serial remains concurrent");
    drop(other);
    drop(first);
    same.await.unwrap();
}

#[tokio::test]
async fn runtime_report_firmware_observations_emit_without_synthetic_job_reports() {
    let cache = FirmwareObservationCache::default();
    let config = test_config();
    let endpoint = endpoint("SERIAL1");
    let (sender, mut receiver) = mpsc::channel(16);
    let transition = cache
        .begin_generation(&config, endpoint.clone(), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let generation = transition.generation();
    drop(transition);
    assert_invalidated(receiver.recv().await.unwrap(), generation);

    let transport = crate::machine::mqtt::FakeMqttTransport::with_reports([
        serde_json::json!({
            "info": {
                "command": "get_version",
                "module": [{
                    "name": "ota",
                    "product_name": "X1 Carbon",
                    "sw_ver": "01.08.02.00"
                }]
            }
        }),
        serde_json::json!({
            "print": {
                "command": "push_status",
                "msg": 0,
                "cfg": "cfg-value",
                "upgrade_state": { "status": "UPGRADING", "progress": "1" }
            }
        }),
    ]);
    let task_config = config.clone();
    let task_transport = transport.clone();
    let task_endpoint = endpoint.clone();
    let task_sender = sender.clone();
    let task_cache = cache.clone();
    let task = tokio::spawn(async move {
        crate::machine::mqtt::forward_print_reports_with_firmware(
            &task_config,
            &task_transport,
            &task_endpoint,
            Duration::from_millis(10),
            &task_sender,
            &crate::machine::DeviceFeatureCache::default(),
            FirmwareReportContext {
                cache: task_cache,
                generation,
            },
        )
        .await
    });

    let modules = receiver.recv().await.unwrap();
    let agent_event::Event::PrinterFirmwareModulesSnapshot(modules) = modules.event.unwrap() else {
        panic!("expected modules snapshot before ordinary report events");
    };
    assert_eq!(modules.generation, generation);
    assert_eq!(modules.module_revision, 1);
    assert_eq!(
        modules.modules[0].software_version.as_deref(),
        Some("01.08.02.00")
    );

    let status = receiver.recv().await.unwrap();
    let agent_event::Event::PrinterFirmwareStatusSnapshot(status) = status.event.unwrap() else {
        panic!("expected firmware status without a synthetic print report");
    };
    assert_eq!(status.generation, generation);
    assert_eq!(status.status_revision, 1);
    assert_eq!(status.cfg.as_deref(), Some("cfg-value"));
    assert_eq!(
        status.upgrade_state.unwrap().status.as_deref(),
        Some("UPGRADING")
    );
    assert!(receiver.try_recv().is_err());

    let published = transport.published_commands().await;
    assert_eq!(published.len(), 2);
    assert_eq!(published[0].payload["info"]["command"], "get_version");
    assert_eq!(published[1].payload["pushing"]["command"], "pushall");
    task.abort();
}

#[tokio::test]
async fn runtime_report_reconnect_establishes_new_generation_before_new_snapshots() {
    let cache = FirmwareObservationCache::default();
    let config = test_config();
    let endpoint = endpoint("SERIAL1");
    let (sender, mut receiver) = mpsc::channel(16);
    let transition = cache
        .begin_generation(&config, endpoint.clone(), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let generation_one = transition.generation();
    drop(transition);
    assert_invalidated(receiver.recv().await.unwrap(), generation_one);

    let transport = crate::machine::mqtt::FakeMqttTransport::with_receive_failure_then_reports([
        serde_json::json!({
            "info": {
                "command": "get_version",
                "module": [{ "name": "ota", "product_name": "X1", "sw_ver": "2" }]
            }
        }),
        serde_json::json!({
            "print": { "msg": 0, "upgrade_state": { "status": "UPGRADING" } }
        }),
    ]);
    let task_config = config.clone();
    let task_transport = transport.clone();
    let task_endpoint = endpoint.clone();
    let task_sender = sender.clone();
    let task_cache = cache.clone();
    let task = tokio::spawn(async move {
        crate::machine::runtime::forward_print_reports_with_firmware_retry(
            task_config,
            task_transport,
            task_endpoint,
            Duration::from_millis(10),
            task_sender,
            Duration::from_millis(1),
            RuntimeReportContext {
                device_features: crate::machine::DeviceFeatureCache::default(),
                firmware: FirmwareReportContext {
                    cache: task_cache,
                    generation: generation_one,
                },
            },
        )
        .await
    });

    let invalidation = next_firmware_event(&mut receiver).await;
    let agent_event::Event::PrinterFirmwareInvalidated(invalidation) = invalidation.event.unwrap()
    else {
        panic!("reconnect must invalidate before snapshots");
    };
    let generation_two = invalidation.generation;
    assert!(generation_two > generation_one);

    let modules = next_firmware_event(&mut receiver).await;
    let agent_event::Event::PrinterFirmwareModulesSnapshot(modules) = modules.event.unwrap() else {
        panic!("expected new-generation modules after invalidation");
    };
    assert_eq!(modules.generation, generation_two);
    let status = next_firmware_event(&mut receiver).await;
    let agent_event::Event::PrinterFirmwareStatusSnapshot(status) = status.event.unwrap() else {
        panic!("expected new-generation status after modules");
    };
    assert_eq!(status.generation, generation_two);
    assert_eq!(
        cache.snapshot("SERIAL1").await.unwrap().generation,
        generation_two
    );
    task.abort();
}

#[tokio::test]
async fn runtime_report_idle_timeout_releases_version_observation_lease() {
    let cache = FirmwareObservationCache::default();
    let config = test_config();
    let endpoint = endpoint("SERIAL1");
    let (sender, mut receiver) = mpsc::channel(8);
    let transition = cache
        .begin_generation(&config, endpoint.clone(), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let generation = transition.generation();
    drop(transition);
    assert_invalidated(receiver.recv().await.unwrap(), generation);

    let transport = crate::machine::mqtt::FakeMqttTransport::with_timeout();
    let task_config = config.clone();
    let task_transport = transport.clone();
    let task_endpoint = endpoint.clone();
    let task_sender = sender.clone();
    let task_cache = cache.clone();
    let task = tokio::spawn(async move {
        crate::machine::mqtt::forward_print_reports_with_firmware(
            &task_config,
            &task_transport,
            &task_endpoint,
            Duration::from_millis(10),
            &task_sender,
            &crate::machine::DeviceFeatureCache::default(),
            FirmwareReportContext {
                cache: task_cache,
                generation,
            },
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(1), async {
        while transport.published_commands().await.len() < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;

    let lease = tokio::time::timeout(
        Duration::from_millis(100),
        cache.version_observation_lease("SERIAL1"),
    )
    .await
    .expect("idle report timeout must release the startup version observation lease");
    drop(lease);
    task.abort();
}

#[tokio::test]
async fn runtime_report_later_module_observation_reacquires_serial_lease() {
    let cache = FirmwareObservationCache::default();
    let config = test_config();
    let endpoint = endpoint("SERIAL1");
    let (sender, mut receiver) = mpsc::channel(8);
    let transition = cache
        .begin_generation(&config, endpoint.clone(), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let generation = transition.generation();
    drop(transition);
    assert_invalidated(receiver.recv().await.unwrap(), generation);

    let transport =
        crate::machine::mqtt::FakeMqttTransport::with_reports([version_report("startup")]);
    let task_config = config.clone();
    let task_transport = transport.clone();
    let task_endpoint = endpoint.clone();
    let task_sender = sender.clone();
    let task_cache = cache.clone();
    let task = tokio::spawn(async move {
        crate::machine::mqtt::forward_print_reports_with_firmware(
            &task_config,
            &task_transport,
            &task_endpoint,
            Duration::from_secs(1),
            &task_sender,
            &crate::machine::DeviceFeatureCache::default(),
            FirmwareReportContext {
                cache: task_cache,
                generation,
            },
        )
        .await
    });

    let first = next_firmware_event(&mut receiver).await;
    let agent_event::Event::PrinterFirmwareModulesSnapshot(first) = first.event.unwrap() else {
        panic!("expected startup modules");
    };
    assert_eq!(first.module_revision, 1);

    let other_observation = cache.version_observation_lease("SERIAL1").await;
    transport
        .push_report(version_report("long-stream-later"))
        .await;
    assert!(
        tokio::time::timeout(Duration::from_millis(50), receiver.recv())
            .await
            .is_err(),
        "later long-stream observation must wait for the same-serial coordinator"
    );
    let other = cache
        .commit_modules("SERIAL1", generation, vec![module("coordinated-other")])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(other.revision, 2);
    drop(other_observation);

    let later = next_firmware_event(&mut receiver).await;
    let agent_event::Event::PrinterFirmwareModulesSnapshot(later) = later.event.unwrap() else {
        panic!("expected later long-stream modules");
    };
    assert_eq!(later.module_revision, 3);
    assert_eq!(
        later.modules[0].software_version.as_deref(),
        Some("long-stream-later")
    );
    task.abort();
}

#[tokio::test]
async fn runtime_report_present_empty_modules_replace_prior_snapshot() {
    let cache = FirmwareObservationCache::default();
    let config = test_config();
    let endpoint = endpoint("SERIAL1");
    let (sender, mut receiver) = mpsc::channel(8);
    let transition = cache
        .begin_generation(&config, endpoint.clone(), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let generation = transition.generation();
    drop(transition);
    assert_invalidated(receiver.recv().await.unwrap(), generation);

    let transport =
        crate::machine::mqtt::FakeMqttTransport::with_reports([version_report("prior")]);
    let task_config = config.clone();
    let task_transport = transport.clone();
    let task_endpoint = endpoint.clone();
    let task_sender = sender.clone();
    let task_cache = cache.clone();
    let task = tokio::spawn(async move {
        crate::machine::mqtt::forward_print_reports_with_firmware(
            &task_config,
            &task_transport,
            &task_endpoint,
            Duration::from_secs(1),
            &task_sender,
            &crate::machine::DeviceFeatureCache::default(),
            FirmwareReportContext {
                cache: task_cache,
                generation,
            },
        )
        .await
    });

    let first = next_firmware_event(&mut receiver).await;
    let agent_event::Event::PrinterFirmwareModulesSnapshot(first) = first.event.unwrap() else {
        panic!("expected prior modules snapshot");
    };
    assert_eq!(first.module_revision, 1);

    transport
        .push_report(serde_json::json!({
            "info": { "command": "get_version", "module": [] }
        }))
        .await;
    let empty = tokio::time::timeout(
        Duration::from_millis(250),
        next_firmware_event(&mut receiver),
    )
    .await
    .expect("present-empty long-lived report must emit a replacement snapshot");
    let agent_event::Event::PrinterFirmwareModulesSnapshot(empty) = empty.event.unwrap() else {
        panic!("expected present-empty modules snapshot");
    };
    assert_eq!(empty.module_revision, 2);
    assert!(empty.modules.is_empty());
    let snapshot = cache.snapshot("SERIAL1").await.unwrap();
    assert_eq!(snapshot.module_revision, 2);
    assert_eq!(snapshot.modules, Some(Vec::new()));
    task.abort();
}

#[tokio::test]
async fn runtime_report_future_only_modules_replace_prior_snapshot() {
    let cache = FirmwareObservationCache::default();
    let config = test_config();
    let endpoint = endpoint("SERIAL1");
    let (sender, mut receiver) = mpsc::channel(8);
    let transition = cache
        .begin_generation(&config, endpoint.clone(), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let generation = transition.generation();
    drop(transition);
    assert_invalidated(receiver.recv().await.unwrap(), generation);

    let transport =
        crate::machine::mqtt::FakeMqttTransport::with_reports([version_report("prior")]);
    let task_config = config.clone();
    let task_transport = transport.clone();
    let task_endpoint = endpoint.clone();
    let task_sender = sender.clone();
    let task_cache = cache.clone();
    let task = tokio::spawn(async move {
        crate::machine::mqtt::forward_print_reports_with_firmware(
            &task_config,
            &task_transport,
            &task_endpoint,
            Duration::from_secs(1),
            &task_sender,
            &crate::machine::DeviceFeatureCache::default(),
            FirmwareReportContext {
                cache: task_cache,
                generation,
            },
        )
        .await
    });

    let first = next_firmware_event(&mut receiver).await;
    let agent_event::Event::PrinterFirmwareModulesSnapshot(first) = first.event.unwrap() else {
        panic!("expected prior modules snapshot");
    };
    assert_eq!(first.module_revision, 1);

    transport
        .push_report(serde_json::json!({
            "info": {
                "command": "get_version",
                "module": [{ "name": "future/unit", "sw_ver": "future" }]
            }
        }))
        .await;
    let future = tokio::time::timeout(
        Duration::from_millis(250),
        next_firmware_event(&mut receiver),
    )
    .await
    .expect("future-only long-lived report must emit a replacement snapshot");
    let agent_event::Event::PrinterFirmwareModulesSnapshot(future) = future.event.unwrap() else {
        panic!("expected future-only modules snapshot");
    };
    assert_eq!(future.module_revision, 2);
    assert_eq!(future.modules.len(), 1);
    assert_eq!(future.modules[0].name, "future/unit");
    assert_eq!(
        future.modules[0].software_version.as_deref(),
        Some("future")
    );
    let snapshot = cache.snapshot("SERIAL1").await.unwrap();
    assert_eq!(snapshot.module_revision, 2);
    assert_eq!(snapshot.modules.unwrap()[0].name, "future/unit");
    task.abort();
}

#[tokio::test]
async fn firmware_generation_module_event_cannot_follow_new_invalidation() {
    let cache = FirmwareObservationCache::default();
    let config = test_config();
    let endpoint = endpoint("SERIAL1");
    let (sender, mut receiver) = mpsc::channel(8);
    let transition = cache
        .begin_generation(&config, endpoint.clone(), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let generation_one = transition.generation();
    drop(transition);
    assert_invalidated(receiver.recv().await.unwrap(), generation_one);

    let mut pause = firmware_event_pause::install("SERIAL1", FirmwareEventKind::Modules);
    let transport =
        crate::machine::mqtt::FakeMqttTransport::with_reports([version_report("old-generation")]);
    let task_config = config.clone();
    let task_transport = transport.clone();
    let task_endpoint = endpoint.clone();
    let task_sender = sender.clone();
    let task_cache = cache.clone();
    let report_task = tokio::spawn(async move {
        crate::machine::mqtt::forward_print_reports_with_firmware(
            &task_config,
            &task_transport,
            &task_endpoint,
            Duration::from_secs(1),
            &task_sender,
            &crate::machine::DeviceFeatureCache::default(),
            FirmwareReportContext {
                cache: task_cache,
                generation: generation_one,
            },
        )
        .await
    });
    pause.wait_until_reached().await;

    let transition_cache = cache.clone();
    let transition_config = config.clone();
    let transition_endpoint = endpoint.clone();
    let transition_sender = sender.clone();
    let transition_task = tokio::spawn(async move {
        transition_cache
            .begin_generation(
                &transition_config,
                transition_endpoint,
                &transition_sender,
                Some(generation_one),
            )
            .await
            .unwrap()
            .unwrap()
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(50), receiver.recv())
            .await
            .is_err(),
        "generation invalidation must wait until the old module event is enqueued"
    );
    pause.release();

    let old_event = next_firmware_event(&mut receiver).await;
    let agent_event::Event::PrinterFirmwareModulesSnapshot(old_event) = old_event.event.unwrap()
    else {
        panic!("old module event must be queued before invalidation");
    };
    assert_eq!(old_event.generation, generation_one);
    let new_transition = transition_task.await.unwrap();
    let generation_two = new_transition.generation();
    drop(new_transition);
    assert_invalidated(receiver.recv().await.unwrap(), generation_two);
    assert!(receiver.try_recv().is_err());
    report_task.abort();
}

#[tokio::test]
async fn firmware_generation_status_event_cannot_follow_new_invalidation() {
    let cache = FirmwareObservationCache::default();
    let config = test_config();
    let endpoint = endpoint("SERIAL1");
    let (sender, mut receiver) = mpsc::channel(8);
    let transition = cache
        .begin_generation(&config, endpoint.clone(), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let generation_one = transition.generation();
    drop(transition);
    assert_invalidated(receiver.recv().await.unwrap(), generation_one);

    let mut pause = firmware_event_pause::install("SERIAL1", FirmwareEventKind::Status);
    let transport = crate::machine::mqtt::FakeMqttTransport::with_reports([serde_json::json!({
        "print": { "msg": 0, "upgrade_state": { "status": "OLD" } }
    })]);
    let task_config = config.clone();
    let task_transport = transport.clone();
    let task_endpoint = endpoint.clone();
    let task_sender = sender.clone();
    let task_cache = cache.clone();
    let report_task = tokio::spawn(async move {
        crate::machine::mqtt::forward_print_reports_with_firmware(
            &task_config,
            &task_transport,
            &task_endpoint,
            Duration::from_secs(1),
            &task_sender,
            &crate::machine::DeviceFeatureCache::default(),
            FirmwareReportContext {
                cache: task_cache,
                generation: generation_one,
            },
        )
        .await
    });
    pause.wait_until_reached().await;

    let transition_cache = cache.clone();
    let transition_config = config.clone();
    let transition_endpoint = endpoint.clone();
    let transition_sender = sender.clone();
    let transition_task = tokio::spawn(async move {
        transition_cache
            .begin_generation(
                &transition_config,
                transition_endpoint,
                &transition_sender,
                Some(generation_one),
            )
            .await
            .unwrap()
            .unwrap()
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(50), receiver.recv())
            .await
            .is_err(),
        "generation invalidation must wait until the old status event is enqueued"
    );
    pause.release();

    let old_event = next_firmware_event(&mut receiver).await;
    let agent_event::Event::PrinterFirmwareStatusSnapshot(old_event) = old_event.event.unwrap()
    else {
        panic!("old status event must be queued before invalidation");
    };
    assert_eq!(old_event.generation, generation_one);
    let new_transition = transition_task.await.unwrap();
    let generation_two = new_transition.generation();
    drop(new_transition);
    assert_invalidated(receiver.recv().await.unwrap(), generation_two);
    assert!(receiver.try_recv().is_err());
    report_task.abort();
}

fn assert_invalidated(event: AgentEvent, generation: u64) {
    let agent_event::Event::PrinterFirmwareInvalidated(event) = event.event.unwrap() else {
        panic!("expected firmware invalidation first");
    };
    assert_eq!(event.serial, "SERIAL1");
    assert_eq!(event.generation, generation);
}

async fn next_firmware_event(receiver: &mut mpsc::Receiver<AgentEvent>) -> AgentEvent {
    loop {
        let event = receiver.recv().await.unwrap();
        if matches!(
            event.event,
            Some(
                agent_event::Event::PrinterFirmwareModulesSnapshot(_)
                    | agent_event::Event::PrinterFirmwareStatusSnapshot(_)
                    | agent_event::Event::PrinterFirmwareInvalidated(_)
            )
        ) {
            return event;
        }
    }
}

fn module(version: &str) -> PrinterFirmwareModule {
    PrinterFirmwareModule {
        name: "ota".to_owned(),
        software_version: Some(version.to_owned()),
        software_new_version: None,
        new_version: None,
        visible: None,
        product_name: Some("X1".to_owned()),
        serial_number: None,
        hardware_version: None,
        firmware_flag: None,
    }
}

fn version_report(version: &str) -> serde_json::Value {
    serde_json::json!({
        "info": {
            "command": "get_version",
            "module": [{
                "name": "ota",
                "product_name": "X1",
                "sw_ver": version
            }]
        }
    })
}

fn status(value: &str) -> PrinterFirmwareStatus {
    PrinterFirmwareStatus {
        upgrade_state: Some(PrinterUpgradeState {
            status: Some(value.to_owned()),
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
        cfg: None,
    }
}
