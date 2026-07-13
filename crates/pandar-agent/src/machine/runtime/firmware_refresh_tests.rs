use std::{sync::Arc, time::Duration};

use tokio::{
    io::AsyncWriteExt,
    net::TcpListener,
    sync::{Barrier, oneshot},
    time::timeout,
};

use crate::machine::{FirmwareObservationCache, FirmwareRefreshRequest};

use super::firmware_refresh::refresh_firmware_version_with_connector;

mod support;
use support::*;

#[tokio::test]
async fn firmware_refresh_real_tcp_preserves_future_modules_without_model_gate() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        accept_subscribed_session(&mut stream).await;
        let command = read_acked_command(&mut stream).await;
        assert_eq!(command["info"]["sequence_id"], "future-modules");
        let payload = serde_json::to_vec(&serde_json::json!({
            "info": {
                "command": "get_version",
                "sequence_id": "future-modules",
                "module": [
                    {
                        "name": "future/unit",
                        "sw_ver": "9.9.9",
                        "sw_new_ver": "10.0.0",
                        "new_ver": "10.1.0",
                        "visible": false,
                        "product_name": "Future Unit",
                        "sn": "FUTURE-SN",
                        "hw_ver": "F00",
                        "flag": 7
                    },
                    { "name": "ota", "sw_ver": "1.2.3" }
                ]
            }
        }))
        .unwrap();
        write_publish(&mut stream, REPORT_TOPIC, &payload).await;
        expect_disconnect(&mut stream).await;
    });
    let cache = seeded_cache("SERIAL").await;
    let connector = LoopbackConnector::new(address);

    let mut delivery = refresh_firmware_version_with_connector(
        &cache,
        FirmwareRefreshRequest {
            serial: "SERIAL".into(),
            sequence_id: "future-modules".into(),
            expected_generation: 1,
        },
        Duration::from_secs(1),
        &connector,
    )
    .await
    .unwrap();
    let observation = delivery.take_observation();

    assert_eq!(observation.modules.len(), 2);
    assert_eq!(observation.modules[0].name, "future/unit");
    assert_eq!(
        observation.modules[0].software_version.as_deref(),
        Some("9.9.9")
    );
    assert_eq!(
        observation.modules[0].software_new_version.as_deref(),
        Some("10.0.0")
    );
    assert_eq!(
        observation.modules[0].new_version.as_deref(),
        Some("10.1.0")
    );
    assert_eq!(observation.modules[0].visible, Some(false));
    assert_eq!(
        observation.modules[0].product_name.as_deref(),
        Some("Future Unit")
    );
    assert_eq!(
        observation.modules[0].serial_number.as_deref(),
        Some("FUTURE-SN")
    );
    assert_eq!(
        observation.modules[0].hardware_version.as_deref(),
        Some("F00")
    );
    assert_eq!(observation.modules[0].firmware_flag, Some(7));
    assert_eq!(observation.modules[1].name, "ota");
    assert_eq!(observation.modules[1].product_name, None);
    broker.await.unwrap();
}

#[tokio::test]
async fn production_refresh_uses_three_fresh_subscribed_sessions_and_does_not_cache_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let broker = tokio::spawn(async move {
        let mut identities = Vec::new();
        for attempt in 0..3 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let connect = read_packet(&mut stream).await;
            assert_eq!(connect.header >> 4, 1);
            assert_ne!(connect.body[7] & 0x02, 0, "attempt {attempt} is not clean");
            identities.push(mqtt_string(&connect.body, 10).0);
            stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
            let subscribe = read_packet(&mut stream).await;
            assert_eq!(subscribe.header >> 4, 8);
            let packet_id = u16::from_be_bytes([subscribe.body[0], subscribe.body[1]]);
            assert!(
                timeout(Duration::from_millis(30), read_packet(&mut stream))
                    .await
                    .is_err(),
                "attempt {attempt} published before SUBACK"
            );
            stream
                .write_all(&[0x90, 0x03, (packet_id >> 8) as u8, packet_id as u8, 0x01])
                .await
                .unwrap();
            let publish = read_packet(&mut stream).await;
            assert_eq!(publish.header >> 4, 3);
            drop(stream);
        }
        identities
    });
    let cache = seeded_cache("SERIAL").await;
    let connector = LoopbackConnector::new(address);

    let error = refresh_firmware_version_with_connector(
        &cache,
        FirmwareRefreshRequest {
            serial: "SERIAL".into(),
            sequence_id: "refresh-failure".into(),
            expected_generation: 1,
        },
        Duration::from_millis(100),
        &connector,
    )
    .await
    .unwrap_err();

    assert!(format!("{error:#}").contains("attempt 3/3"));
    assert_eq!(connector.option_packet_sizes().await, [256 * 1024; 3]);
    let identities = broker.await.unwrap();
    assert_eq!(identities.len(), 3);
    assert_eq!(
        identities
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        3
    );
    let snapshot = cache.snapshot("SERIAL").await.unwrap();
    assert_eq!(snapshot.modules, None);
    assert_eq!(snapshot.module_revision, 0);
}

#[tokio::test]
async fn same_serial_refreshes_are_serial_and_each_returns_fresh_modules_in_revision_order() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (first_published, wait_first_published) = oneshot::channel();
    let (release_first, wait_release_first) = oneshot::channel();
    let broker = tokio::spawn(async move {
        let (mut first, _) = listener.accept().await.unwrap();
        let first_id = accept_subscribed_session(&mut first).await;
        let first_command = read_acked_command(&mut first).await;
        assert_eq!(first_command["info"]["sequence_id"], "same-serial-first");
        first_published.send(()).unwrap();
        wait_release_first.await.unwrap();
        send_version_report(&mut first, "same-serial-first", "01.00.00.00").await;
        expect_disconnect(&mut first).await;

        let (mut second, _) = listener.accept().await.unwrap();
        let second_id = accept_subscribed_session(&mut second).await;
        let second_command = read_acked_command(&mut second).await;
        assert_eq!(second_command["info"]["sequence_id"], "same-serial-second");
        send_version_report(&mut second, "same-serial-second", "02.00.00.00").await;
        expect_disconnect(&mut second).await;
        [first_id, second_id]
    });
    let cache = seeded_cache("SERIAL").await;
    let connector = LoopbackConnector::new(address);
    let first = tokio::spawn({
        let cache = cache.clone();
        let connector = connector.clone();
        async move {
            refresh_firmware_version_with_connector(
                &cache,
                FirmwareRefreshRequest {
                    serial: "SERIAL".into(),
                    sequence_id: "same-serial-first".into(),
                    expected_generation: 1,
                },
                Duration::from_secs(1),
                &connector,
            )
            .await
        }
    });
    wait_first_published.await.unwrap();
    let second = tokio::spawn({
        let cache = cache.clone();
        let connector = connector.clone();
        async move {
            refresh_firmware_version_with_connector(
                &cache,
                FirmwareRefreshRequest {
                    serial: "SERIAL".into(),
                    sequence_id: "same-serial-second".into(),
                    expected_generation: 1,
                },
                Duration::from_secs(1),
                &connector,
            )
            .await
        }
    });
    tokio::task::yield_now().await;
    assert_eq!(connector.option_packet_sizes().await.len(), 1);
    release_first.send(()).unwrap();

    let mut first_delivery = first.await.unwrap().unwrap();
    let first = first_delivery.take_observation();
    drop(first_delivery);
    let mut second_delivery = second.await.unwrap().unwrap();
    let second = second_delivery.take_observation();

    assert_eq!(first.revision, 1);
    assert_eq!(
        first.modules[0].software_version.as_deref(),
        Some("01.00.00.00")
    );
    assert_eq!(second.revision, 2);
    assert_eq!(
        second.modules[0].software_version.as_deref(),
        Some("02.00.00.00")
    );
    let client_ids = broker.await.unwrap();
    assert_ne!(client_ids[0], client_ids[1]);
}

#[tokio::test]
async fn different_serial_refreshes_reach_the_broker_concurrently() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let broker = tokio::spawn({
        let barrier = Arc::clone(&barrier);
        async move {
            let mut tasks = Vec::new();
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let barrier = Arc::clone(&barrier);
                tasks.push(tokio::spawn(async move {
                    let client_id = accept_subscribed_session(&mut stream).await;
                    let command = read_acked_command(&mut stream).await;
                    let sequence_id = command["info"]["sequence_id"].as_str().unwrap().to_owned();
                    barrier.wait().await;
                    send_version_report(&mut stream, &sequence_id, &format!("{sequence_id}-v"))
                        .await;
                    expect_disconnect(&mut stream).await;
                    client_id
                }));
            }
            let mut ids = Vec::new();
            for task in tasks {
                ids.push(task.await.unwrap());
            }
            ids
        }
    });
    let cache = FirmwareObservationCache::default();
    seed_cache_entry(&cache, "SERIAL-A").await;
    seed_cache_entry(&cache, "SERIAL-B").await;
    let connector = LoopbackConnector::new(address);
    let refreshes = ["SERIAL-A", "SERIAL-B"].map(|serial| {
        let cache = cache.clone();
        let connector = connector.clone();
        tokio::spawn(async move {
            refresh_firmware_version_with_connector(
                &cache,
                FirmwareRefreshRequest {
                    serial: serial.into(),
                    sequence_id: serial.into(),
                    expected_generation: 1,
                },
                Duration::from_secs(1),
                &connector,
            )
            .await
        })
    });

    timeout(Duration::from_secs(1), barrier.wait())
        .await
        .expect("different serials must both publish before either completes");

    let [first, second] = refreshes;
    let (first, second) = tokio::join!(first, second);
    let mut first_delivery = first.unwrap().unwrap();
    let mut second_delivery = second.unwrap().unwrap();
    let first = first_delivery.take_observation();
    let second = second_delivery.take_observation();
    assert_eq!(first.revision, 1);
    assert_eq!(second.revision, 1);
    assert_eq!(
        first.modules[0].software_version.as_deref(),
        Some("SERIAL-A-v")
    );
    assert_eq!(
        second.modules[0].software_version.as_deref(),
        Some("SERIAL-B-v")
    );
    let client_ids = broker.await.unwrap();
    assert_eq!(client_ids.len(), 2);
    assert_ne!(client_ids[0], client_ids[1]);
}
