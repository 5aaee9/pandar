use std::time::Duration;

use pandar_core::BambuDeviceFeatures;
use tokio::{sync::mpsc, time::timeout};

use super::*;
use crate::{machine::DeviceFeatureCache, protocol::agent::v1::agent_event};

const HIGH_BITS: u64 = 0x8000_0041_0000_0020;

fn report_from_mqtt_bytes(payload: &[u8]) -> MachineReport {
    let value = decode_mqtt_report_payload(payload).expect("valid MQTT-shaped JSON bytes");
    let report = MachineReport::decode(value);
    assert!(
        report.snapshot().is_some(),
        "snapshot report should retain sibling telemetry"
    );
    report
}

fn feature_report(fun: &str) -> serde_json::Value {
    decode_mqtt_report_payload(format!(r#"{{"print":{{"fun":"{fun}"}}}}"#).as_bytes())
        .expect("valid MQTT-shaped JSON bytes")
}

fn test_config() -> AgentConfig {
    AgentConfig {
        hub_grpc_url: "http://hub.internal:50051".to_owned(),
        hub_api_url: None,
        agent_name: "garage".to_owned(),
        agent_id: "agent-id".to_owned(),
        tenant_id: "tenant-id".to_owned(),
        agent_credential: "pandar_ac_test".to_owned(),
        agent_version: "9.8.7".to_owned(),
        printers: "[]".to_owned(),
        artifact_root: ".".into(),
    }
}

#[test]
fn device_features_parser_preserves_presence_and_sibling_telemetry() {
    let valid = report_from_mqtt_bytes(
        br#"{"print":{"fun":"8000004100000020","gcode_state":"RUNNING","bed_temper":60}}"#,
    );
    let observed = device_feature_observation("SERIAL-1", valid.snapshot().unwrap())
        .unwrap()
        .expect("print.fun is present");
    assert_eq!(observed.bits(), HIGH_BITS);
    let snapshot = snapshot_from_parsed_report(&endpoint(), valid.snapshot());
    assert_eq!(snapshot.state.as_deref(), Some("RUNNING"));
    assert_eq!(snapshot.bed_temperature_celsius.as_deref(), Some("60"));

    for payload in [
        br#"{"print":{"fun":false,"gcode_state":"RUNNING","bed_temper":60}}"#.as_slice(),
        br#"{"print":{"fun":null,"gcode_state":"RUNNING","bed_temper":60}}"#.as_slice(),
        br#"{"print":{"fun":"not-hex","gcode_state":"RUNNING","bed_temper":60}}"#.as_slice(),
    ] {
        let report = report_from_mqtt_bytes(payload);
        let snapshot = snapshot_from_parsed_report(&endpoint(), report.snapshot());
        assert_eq!(snapshot.state.as_deref(), Some("RUNNING"));
        assert_eq!(snapshot.bed_temperature_celsius.as_deref(), Some("60"));

        let error = device_feature_observation("SERIAL-1", report.snapshot().unwrap()).unwrap_err();
        let error = format!("{error:#}");
        assert!(error.contains("SERIAL-1"), "{error}");
        assert!(error.contains("print.fun"), "{error}");
        assert!(
            error.contains("expected a hexadecimal string")
                || error.contains("non-hexadecimal characters"),
            "{error}"
        );
    }
}

#[test]
fn device_features_parser_does_not_unicode_trim_fun() {
    let report = report_from_mqtt_bytes(
        "{\"print\":{\"fun\":\"\u{00a0}8000004100000020\u{00a0}\",\"gcode_state\":\"RUNNING\",\"bed_temper\":60}}"
            .as_bytes(),
    );

    let error = device_feature_observation("SERIAL-1", report.snapshot().unwrap()).unwrap_err();
    let error = format!("{error:#}");
    assert!(error.contains("SERIAL-1"), "{error}");
    assert!(error.contains("print.fun"), "{error}");
    assert!(
        error.contains("non-hexadecimal characters")
            || error.contains("exceeds 16 hexadecimal digits"),
        "{error}"
    );
}

#[test]
fn device_features_refresh_logs_invalid_fun_and_preserves_sibling_telemetry() {
    let transport = FakeMqttTransport::with_reports([
        get_version_report("X1 Carbon"),
        decode_mqtt_report_payload(
            br#"{"print":{"fun":"not-hex","gcode_state":"RUNNING","bed_temper":60}}"#,
        )
        .unwrap(),
    ]);

    let (logs, snapshot) = crate::test_tracing::capture_logs(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                refresh_printer(&transport, &endpoint(), Duration::from_secs(1))
                    .await
                    .unwrap()
                    .snapshot
            })
    });

    assert_eq!(snapshot.state.as_deref(), Some("RUNNING"));
    assert_eq!(snapshot.bed_temperature_celsius.as_deref(), Some("60"));
    assert_eq!(snapshot.device_features, None);
    let logs = logs.contents();
    assert!(logs.contains("01S00EXAMPLE"), "{logs}");
    assert!(logs.contains("print.fun"), "{logs}");
    assert!(logs.contains("non-hexadecimal characters"), "{logs}");
}

#[tokio::test]
async fn device_features_cache_preserves_unknown_invalidation_and_valid_zero() {
    let cache = DeviceFeatureCache::default();
    let serial = "SERIAL-1";
    let high_bits = BambuDeviceFeatures::from_bits(HIGH_BITS);

    assert_eq!(cache.get(serial).await, None);
    cache.update(serial, high_bits).await;
    assert_eq!(cache.get(serial).await, Some(high_bits));
    cache.invalidate(serial).await;
    assert_eq!(cache.get(serial).await, None);
    cache.update(serial, BambuDeviceFeatures::default()).await;
    assert_eq!(cache.get(serial).await.unwrap().bits(), 0);
}

#[tokio::test]
async fn device_features_probe_pushes_all_skips_unrelated_and_updates_shared_cache() {
    let transport = FakeMqttTransport::with_reports([
        decode_mqtt_report_payload(br#"{"print":{"gcode_state":"RUNNING"}}"#).unwrap(),
        feature_report("8000004100000020"),
    ]);
    let cache = DeviceFeatureCache::default();
    let endpoint = endpoint();

    let observed = probe_device_features(&transport, &endpoint, Duration::from_secs(1), &cache)
        .await
        .unwrap();

    assert_eq!(observed.bits(), HIGH_BITS);
    assert_eq!(cache.get(&endpoint.serial).await, Some(observed));
    assert_eq!(
        transport.subscriptions().await,
        ["device/01S00EXAMPLE/report".to_owned()]
    );
    let published = transport.published_commands().await;
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].topic, "device/01S00EXAMPLE/request");
    assert_eq!(
        published[0].payload["pushing"]["command"],
        serde_json::json!("pushall")
    );
}

#[tokio::test]
async fn device_features_probe_rejects_present_invalid_fun_with_context() {
    for reports in [
        vec![
            decode_mqtt_report_payload(br#"{"print":{"gcode_state":"RUNNING"}}"#).unwrap(),
            decode_mqtt_report_payload(br#"{"print":{"fun":false,"gcode_state":"RUNNING"}}"#)
                .unwrap(),
        ],
        vec![
            decode_mqtt_report_payload(br#"{"print":{"fun":null,"gcode_state":"RUNNING"}}"#)
                .unwrap(),
        ],
    ] {
        let transport = FakeMqttTransport::with_reports(reports);
        let cache = DeviceFeatureCache::default();

        let error = probe_device_features(&transport, &endpoint(), Duration::from_secs(1), &cache)
            .await
            .unwrap_err();
        let error = format!("{error:#}");
        assert!(error.contains("01S00EXAMPLE"), "{error}");
        assert!(error.contains("print.fun"), "{error}");
        assert!(error.contains("expected a hexadecimal string"), "{error}");
        assert_eq!(cache.get("01S00EXAMPLE").await, None);
    }
}

#[tokio::test]
async fn device_features_invalid_continuous_report_keeps_cached_value() {
    let report = decode_mqtt_report_payload(
        br#"{"print":{"fun":false,"gcode_state":"RUNNING","bed_temper":60}}"#,
    )
    .unwrap();
    let transport = FakeMqttTransport::with_reports([report]);
    let cache = DeviceFeatureCache::default();
    cache
        .update("01S00EXAMPLE", BambuDeviceFeatures::from_bits(HIGH_BITS))
        .await;
    let (sender, mut receiver) = mpsc::channel(4);
    let task = tokio::spawn({
        let cache = cache.clone();
        async move {
            forward_print_reports(
                &test_config(),
                &transport,
                &endpoint(),
                Duration::from_millis(10),
                &sender,
                &cache,
            )
            .await
        }
    });

    let _ = receiver.recv().await;
    assert_eq!(cache.get("01S00EXAMPLE").await.unwrap().bits(), HIGH_BITS);
    task.abort();
}

#[tokio::test]
async fn device_features_probe_timeout_leaves_cache_unknown() {
    let transport = FakeMqttTransport::with_timeout();
    let cache = DeviceFeatureCache::default();

    let error = probe_device_features(&transport, &endpoint(), Duration::from_millis(1), &cache)
        .await
        .unwrap_err();

    let error = format!("{error:#}");
    assert!(error.contains("01S00EXAMPLE"), "{error}");
    assert!(error.contains("device/01S00EXAMPLE/report"), "{error}");
    assert_eq!(cache.get("01S00EXAMPLE").await, None);
}

#[tokio::test]
async fn device_features_fun_only_report_emits_only_feature_snapshot() {
    let transport = FakeMqttTransport::with_reports([feature_report("8000004100000020")]);
    let cache = DeviceFeatureCache::default();
    let (sender, mut receiver) = mpsc::channel(4);
    let task = tokio::spawn({
        let transport = transport.clone();
        let cache = cache.clone();
        async move {
            forward_print_reports(
                &test_config(),
                &transport,
                &endpoint(),
                Duration::from_millis(10),
                &sender,
                &cache,
            )
            .await
        }
    });

    let event = receiver.recv().await.unwrap();
    let Some(agent_event::Event::PrinterDeviceFeaturesSnapshot(snapshot)) = event.event else {
        panic!("expected feature-only snapshot, got {event:?}");
    };
    assert_eq!(snapshot.serial, "01S00EXAMPLE");
    assert_eq!(
        snapshot.device_features.unwrap().bambu_fun_bits,
        Some(HIGH_BITS)
    );
    assert_eq!(cache.get("01S00EXAMPLE").await.unwrap().bits(), HIGH_BITS);

    let offline = timeout(Duration::from_millis(50), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_offline_snapshot(offline);
    task.abort();
}

#[tokio::test]
async fn device_features_temperature_report_precedes_separate_offline_transition() {
    let report = decode_mqtt_report_payload(
        br#"{"print":{"fun":"8000004100000020","gcode_state":"RUNNING","bed_temper":60}}"#,
    )
    .unwrap();
    let transport = FakeMqttTransport::with_reports([report]);
    let cache = DeviceFeatureCache::default();
    let (sender, mut receiver) = mpsc::channel(4);
    let task = tokio::spawn({
        let cache = cache.clone();
        async move {
            forward_print_reports(
                &test_config(),
                &transport,
                &endpoint(),
                Duration::from_millis(10),
                &sender,
                &cache,
            )
            .await
        }
    });

    let snapshot = loop {
        let event = receiver.recv().await.unwrap();
        match event.event {
            Some(agent_event::Event::PrinterSnapshot(snapshot)) => break snapshot,
            Some(agent_event::Event::PrinterDeviceFeaturesSnapshot(snapshot)) => {
                panic!("unexpected duplicate feature-only snapshot: {snapshot:?}")
            }
            _ => {}
        }
    };
    let features = snapshot
        .device_features
        .as_ref()
        .expect("full snapshot carries observed features");
    assert_eq!(features.bambu_fun_bits, Some(HIGH_BITS));
    assert_eq!(snapshot.state, "RUNNING");
    assert_eq!(snapshot.bed_temperature_celsius, "60");

    let offline = timeout(Duration::from_millis(50), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_offline_snapshot(offline);
    task.abort();
}

fn assert_offline_snapshot(event: crate::protocol::agent::v1::AgentEvent) {
    let Some(agent_event::Event::PrinterSnapshot(snapshot)) = event.event else {
        panic!("expected offline printer snapshot, got {event:?}");
    };
    assert_eq!(snapshot.state, "offline");
    assert!(!snapshot.telemetry_authoritative);
    assert!(snapshot.device_features.is_none());
}
