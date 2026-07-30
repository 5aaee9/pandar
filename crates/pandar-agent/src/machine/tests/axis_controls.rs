use super::*;
use pandar_core::BambuDeviceFeature;

use crate::{
    machine::{
        mqtt::{MachineReport, decode_mqtt_report_payload, device_feature_observation},
        operations::{device_feature_dispatch_pause, operate_printer_with_feature_selection},
    },
    protocol::agent::v1::{AgentEvent, agent_event},
};

fn axis_gateway(
    mqtt: FakeMqttTransport,
) -> ConfiguredBambuMachineGateway<FakeMqttTransport, FakeMachineFileTransfer> {
    axis_gateway_for_serial(mqtt, "SERIAL1")
}

fn axis_gateway_for_serial(
    mqtt: FakeMqttTransport,
    serial: &str,
) -> ConfiguredBambuMachineGateway<FakeMqttTransport, FakeMachineFileTransfer> {
    ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint(serial), mqtt, FakeMachineFileTransfer::default())],
        Duration::from_secs(1),
        TransferModeCache::default(),
    )
}

async fn published_print_param(operation: PrinterOperation) -> String {
    let mqtt = FakeMqttTransport::default();
    axis_gateway(mqtt.clone())
        .operate_printer("SERIAL1", operation)
        .await
        .unwrap();
    let published = mqtt.published_commands().await;
    assert_eq!(published.len(), 1);
    published[0].payload["print"]["param"]
        .as_str()
        .unwrap()
        .to_owned()
}

struct AxisRuntimeFixture {
    gateway: tokio::sync::Mutex<
        ConfiguredBambuMachineGateway<FakeMqttTransport, FakeMachineFileTransfer>,
    >,
    mqtt: FakeMqttTransport,
    cache: DeviceFeatureCache,
    serial: String,
    current_sender: tokio::sync::Mutex<Option<mpsc::Sender<AgentEvent>>>,
    receiver: tokio::sync::Mutex<mpsc::Receiver<AgentEvent>>,
}

impl AxisRuntimeFixture {
    fn new(mqtt: FakeMqttTransport) -> Self {
        Self::for_serial(mqtt, "SERIAL1")
    }

    fn for_serial(mqtt: FakeMqttTransport, serial: &str) -> Self {
        let (sender, receiver) = mpsc::channel(16);
        Self {
            gateway: tokio::sync::Mutex::new(axis_gateway_for_serial(mqtt.clone(), serial)),
            mqtt,
            cache: DeviceFeatureCache::default(),
            serial: serial.to_owned(),
            current_sender: tokio::sync::Mutex::new(Some(sender)),
            receiver: tokio::sync::Mutex::new(receiver),
        }
    }

    async fn operate(
        &self,
        operation: PrinterOperation,
    ) -> anyhow::Result<PrinterOperationDispatchResult> {
        operate_printer_with_feature_selection(
            &test_config(),
            &self.gateway,
            &self.cache,
            &self.current_sender,
            &self.serial,
            operation,
        )
        .await
    }

    async fn next_feature_event_bits(&self) -> Option<u64> {
        let event = self.receiver.lock().await.recv().await.unwrap();
        let Some(agent_event::Event::PrinterDeviceFeaturesSnapshot(snapshot)) = event.event else {
            panic!("expected device-feature convergence event, got {event:?}");
        };
        assert_eq!(snapshot.serial, "SERIAL1");
        snapshot
            .device_features
            .map(|features| features.bambu_fun_bits)
    }
}

fn feature_report(fun: &str) -> Value {
    decode_mqtt_report_payload(format!(r#"{{"print":{{"fun":"{fun}"}}}}"#).as_bytes()).unwrap()
}

async fn ingest_feature_report(cache: &DeviceFeatureCache, fun: &str) {
    let report = MachineReport::decode(feature_report(fun));
    let observed = device_feature_observation("SERIAL1", report.snapshot().unwrap())
        .unwrap()
        .unwrap();
    cache.update("SERIAL1", observed).await;
}

#[tokio::test]
async fn axis_controls_legacy_home_preserves_requested_axis_order() {
    assert_eq!(
        published_print_param(PrinterOperation::Home {
            axes: vec![PrinterAxis::Z, PrinterAxis::X, PrinterAxis::Y],
            required_feature: None,
        })
        .await,
        "G28 Z X Y"
    );
    assert_eq!(
        published_print_param(PrinterOperation::Home {
            axes: vec![PrinterAxis::X],
            required_feature: None,
        })
        .await,
        "G28 X"
    );
    assert_eq!(
        published_print_param(PrinterOperation::Home {
            axes: Vec::new(),
            required_feature: None,
        })
        .await,
        "G28"
    );
}

#[tokio::test]
async fn axis_controls_legacy_move_uses_studio_seven_line_g1_envelope() {
    for (operation, expected_move) in [
        (
            PrinterOperation::MoveAxes {
                x_mm: Some(1.0),
                y_mm: None,
                z_mm: None,
                feedrate_mm_per_min: Some(3000.0),
                required_feature: None,
            },
            "G1 X1 F3000",
        ),
        (
            PrinterOperation::MoveAxes {
                x_mm: None,
                y_mm: Some(-10.0),
                z_mm: None,
                feedrate_mm_per_min: Some(3000.0),
                required_feature: None,
            },
            "G1 Y-10 F3000",
        ),
        (
            PrinterOperation::MoveAxes {
                x_mm: None,
                y_mm: None,
                z_mm: Some(10.0),
                feedrate_mm_per_min: Some(900.0),
                required_feature: None,
            },
            "G1 Z10 F900",
        ),
        (
            PrinterOperation::MoveAxes {
                x_mm: Some(0.123456789),
                y_mm: None,
                z_mm: None,
                feedrate_mm_per_min: None,
                required_feature: None,
            },
            "G1 X0.123456789",
        ),
    ] {
        assert_eq!(
            published_print_param(operation).await,
            [
                "M211 S",
                "M211 X1 Y1 Z1",
                "M1002 push_ref_mode",
                "G91",
                expected_move,
                "M1002 pop_ref_mode",
                "M211 R",
            ]
            .join("\n")
        );
    }
}

#[tokio::test]
async fn axis_controls_modern_home_uses_exact_back_to_center_payload() {
    let fixture = AxisRuntimeFixture::new(FakeMqttTransport::default());
    fixture
        .cache
        .update(
            "SERIAL1",
            BambuDeviceFeatures::from_bits(1_u64 << BambuDeviceFeature::MqttHoming.bit()),
        )
        .await;

    fixture
        .operate(PrinterOperation::Home {
            axes: Vec::new(),
            required_feature: Some(BambuDeviceFeature::MqttHoming),
        })
        .await
        .unwrap();

    let published = fixture.mqtt.published_commands().await;
    assert_eq!(published.len(), 1);
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published[0].payload,
        serde_json::json!({
            "print": {
                "command": "back_to_center",
                "sequence_id": sequence_id,
            }
        })
    );
}

#[tokio::test]
async fn axis_controls_modern_move_preserves_signed_xyz_direction_and_mode() {
    for (axis, delta, expected_axis, expected_direction, expected_mode) in [
        (PrinterAxis::X, 1.0, "X", 1, 0),
        (PrinterAxis::Y, -10.0, "Y", -1, 1),
        (PrinterAxis::Z, 10.0, "Z", 1, 1),
    ] {
        let fixture = AxisRuntimeFixture::new(FakeMqttTransport::default());
        fixture
            .cache
            .update(
                "SERIAL1",
                BambuDeviceFeatures::from_bits(1_u64 << BambuDeviceFeature::MqttAxisControl.bit()),
            )
            .await;
        let (x_mm, y_mm, z_mm) = match axis {
            PrinterAxis::X => (Some(delta), None, None),
            PrinterAxis::Y => (None, Some(delta), None),
            PrinterAxis::Z => (None, None, Some(delta)),
        };

        fixture
            .operate(PrinterOperation::MoveAxes {
                x_mm,
                y_mm,
                z_mm,
                feedrate_mm_per_min: None,
                required_feature: Some(BambuDeviceFeature::MqttAxisControl),
            })
            .await
            .unwrap();

        let published = fixture.mqtt.published_commands().await;
        assert_eq!(published.len(), 1);
        let sequence_id = dynamic_sequence_id(&published[0].payload);
        assert_eq!(
            published[0].payload,
            serde_json::json!({
                "print": {
                    "command": "xyz_ctrl",
                    "axis": expected_axis,
                    "dir": expected_direction,
                    "mode": expected_mode,
                    "sequence_id": sequence_id,
                }
            })
        );
    }
}

#[tokio::test]
async fn axis_controls_cached_missing_feature_fails_closed_and_reemits_exact_bitmap() {
    let fixture = AxisRuntimeFixture::new(FakeMqttTransport::default());
    let missing = BambuDeviceFeatures::from_bits(0x8000_0000_0000_0020);
    fixture.cache.update("SERIAL1", missing).await;

    let error = fixture
        .operate(PrinterOperation::Home {
            axes: Vec::new(),
            required_feature: Some(BambuDeviceFeature::MqttHoming),
        })
        .await
        .unwrap_err();

    let error = format!("{error:#}");
    assert!(error.contains("SERIAL1"), "{error}");
    assert!(error.contains("8000000000000020"), "{error}");
    assert!(error.contains("32"), "{error}");
    assert!(fixture.mqtt.published_commands().await.is_empty());
    assert_eq!(
        fixture.next_feature_event_bits().await,
        Some(missing.bits())
    );
}

#[tokio::test]
async fn axis_controls_cold_zero_probe_fails_closed_and_emits_zero() {
    let fixture = AxisRuntimeFixture::new(FakeMqttTransport::with_reports([feature_report("0")]));

    let error = fixture
        .operate(PrinterOperation::Home {
            axes: Vec::new(),
            required_feature: Some(BambuDeviceFeature::MqttHoming),
        })
        .await
        .unwrap_err();

    let error = format!("{error:#}");
    assert!(error.contains("SERIAL1"), "{error}");
    assert!(error.contains("0"), "{error}");
    let published = fixture.mqtt.published_commands().await;
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].payload["pushing"]["command"], "pushall");
    assert_eq!(fixture.next_feature_event_bits().await, Some(0));
}

#[tokio::test]
async fn axis_controls_cold_nonzero_missing_bit_preserves_exact_observation() {
    let exact = 0x8000_0001_0000_0020;
    let fixture = AxisRuntimeFixture::new(FakeMqttTransport::with_reports([feature_report(
        "8000000100000020",
    )]));

    let error = fixture
        .operate(PrinterOperation::MoveAxes {
            x_mm: Some(1.0),
            y_mm: None,
            z_mm: None,
            feedrate_mm_per_min: None,
            required_feature: Some(BambuDeviceFeature::MqttAxisControl),
        })
        .await
        .unwrap_err();

    let error = format!("{error:#}");
    assert!(error.contains("8000000100000020"), "{error}");
    let published = fixture.mqtt.published_commands().await;
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].payload["pushing"]["command"], "pushall");
    assert_eq!(fixture.next_feature_event_bits().await, Some(exact));
}

#[tokio::test]
async fn axis_controls_invalid_missing_and_timed_out_probes_invalidate_without_operation_publish() {
    for mqtt in [
        FakeMqttTransport::with_reports([serde_json::json!({"print": {"fun": false}})]),
        FakeMqttTransport::with_reports([serde_json::json!({
            "print": {"gcode_state": "RUNNING"}
        })]),
        FakeMqttTransport::with_timeout(),
    ] {
        let fixture = AxisRuntimeFixture::new(mqtt);
        let error = fixture
            .operate(PrinterOperation::Home {
                axes: Vec::new(),
                required_feature: Some(BambuDeviceFeature::MqttHoming),
            })
            .await
            .unwrap_err();

        let error = format!("{error:#}");
        assert!(error.contains("SERIAL1"), "{error}");
        let published = fixture.mqtt.published_commands().await;
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].payload["pushing"]["command"], "pushall");
        assert_eq!(fixture.cache.get("SERIAL1").await, None);
        assert_eq!(fixture.next_feature_event_bits().await, None);
    }
}

#[tokio::test]
async fn axis_controls_requirement_free_operation_never_probes_or_downgrades_from_cache_state() {
    let fixture = AxisRuntimeFixture::new(FakeMqttTransport::with_timeout());

    fixture
        .operate(PrinterOperation::Home {
            axes: vec![PrinterAxis::X],
            required_feature: None,
        })
        .await
        .unwrap();

    let published = fixture.mqtt.published_commands().await;
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].payload["print"]["command"], "gcode_line");
    assert_eq!(published[0].payload["print"]["param"], "G28 X");
    assert!(fixture.receiver.lock().await.try_recv().is_err());
}

#[tokio::test]
async fn axis_controls_cold_supported_probe_publishes_pushall_before_modern_operation() {
    let supported = "8000004100000020";
    let fixture =
        AxisRuntimeFixture::new(FakeMqttTransport::with_reports([feature_report(supported)]));

    fixture
        .operate(PrinterOperation::Home {
            axes: Vec::new(),
            required_feature: Some(BambuDeviceFeature::MqttHoming),
        })
        .await
        .unwrap();

    let published = fixture.mqtt.published_commands().await;
    assert_eq!(published.len(), 2);
    assert_eq!(published[0].payload["pushing"]["command"], "pushall");
    assert_eq!(published[1].payload["print"]["command"], "back_to_center");
    assert_eq!(
        fixture.next_feature_event_bits().await,
        Some(0x8000_0041_0000_0020)
    );
}

#[tokio::test]
async fn axis_controls_shared_cache_converges_from_supported_to_exact_zero() {
    let fixture = AxisRuntimeFixture::new(FakeMqttTransport::default());
    ingest_feature_report(&fixture.cache, "8000004100000020").await;

    fixture
        .operate(PrinterOperation::Home {
            axes: Vec::new(),
            required_feature: Some(BambuDeviceFeature::MqttHoming),
        })
        .await
        .unwrap();
    fixture
        .operate(PrinterOperation::MoveAxes {
            x_mm: None,
            y_mm: Some(-10.0),
            z_mm: None,
            feedrate_mm_per_min: None,
            required_feature: Some(BambuDeviceFeature::MqttAxisControl),
        })
        .await
        .unwrap();
    assert_eq!(fixture.mqtt.published_commands().await.len(), 2);

    ingest_feature_report(&fixture.cache, "0").await;
    fixture
        .operate(PrinterOperation::Home {
            axes: Vec::new(),
            required_feature: Some(BambuDeviceFeature::MqttHoming),
        })
        .await
        .unwrap_err();

    let published = fixture.mqtt.published_commands().await;
    assert_eq!(published.len(), 2);
    assert_eq!(published[0].payload["print"]["command"], "back_to_center");
    assert_eq!(published[1].payload["print"]["command"], "xyz_ctrl");
    assert_eq!(fixture.next_feature_event_bits().await, Some(0));
}

#[tokio::test]
async fn axis_controls_required_publish_is_linearized_with_feature_writers() {
    const SERIAL: &str = "FEATURE-LEASE-RACE";
    let fixture = std::sync::Arc::new(AxisRuntimeFixture::for_serial(
        FakeMqttTransport::with_operation_reports(),
        SERIAL,
    ));
    let supported = BambuDeviceFeatures::from_bits(1_u64 << BambuDeviceFeature::MqttHoming.bit());
    fixture.cache.update(SERIAL, supported).await;

    let mut before_final_read = device_feature_dispatch_pause::install(
        SERIAL,
        device_feature_dispatch_pause::Phase::BeforeFinalLease,
    );
    let operation_fixture = fixture.clone();
    let operation = tokio::spawn(async move {
        operation_fixture
            .operate(PrinterOperation::Home {
                axes: Vec::new(),
                required_feature: Some(BambuDeviceFeature::MqttHoming),
            })
            .await
    });
    before_final_read.wait_until_reached().await;

    fixture
        .cache
        .update(SERIAL, BambuDeviceFeatures::default())
        .await;
    before_final_read.resume();

    let error = operation.await.unwrap().unwrap_err();
    assert!(format!("{error:#}").contains("missing required feature bit 32"));
    assert!(fixture.mqtt.published_commands().await.is_empty());

    fixture.cache.update(SERIAL, supported).await;
    let mut after_final_read = device_feature_dispatch_pause::install(
        SERIAL,
        device_feature_dispatch_pause::Phase::AfterFinalReadBeforePublish,
    );
    let operation_fixture = fixture.clone();
    let operation = tokio::spawn(async move {
        operation_fixture
            .operate(PrinterOperation::Home {
                axes: Vec::new(),
                required_feature: Some(BambuDeviceFeature::MqttHoming),
            })
            .await
    });
    after_final_read.wait_until_reached().await;

    let mut writer_waiting =
        crate::machine::device_feature_transition_pause::observe_waiting(SERIAL);
    let cache = fixture.cache.clone();
    let invalidation = tokio::spawn(async move { cache.invalidate(SERIAL).await });
    writer_waiting.wait_until_reached().await;
    assert!(!invalidation.is_finished());

    after_final_read.resume();
    operation.await.unwrap().unwrap();
    invalidation.await.unwrap();

    let published = fixture.mqtt.published_commands().await;
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].payload["print"]["command"], "back_to_center");
    assert_eq!(fixture.cache.get(SERIAL).await, None);
}

#[tokio::test]
async fn axis_controls_requirement_free_operation_does_not_take_feature_lease() {
    let fixture = AxisRuntimeFixture::new(FakeMqttTransport::with_operation_reports());
    let lease = fixture.cache.transition_lease("SERIAL1").await;

    tokio::time::timeout(
        Duration::from_secs(1),
        fixture.operate(PrinterOperation::Home {
            axes: vec![PrinterAxis::X],
            required_feature: None,
        }),
    )
    .await
    .expect("legacy operation must not wait for the feature lease")
    .unwrap();
    drop(lease);

    let published = fixture.mqtt.published_commands().await;
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].payload["print"]["command"], "gcode_line");
    assert_eq!(published[0].payload["print"]["param"], "G28 X");
}
