use super::*;

#[tokio::test]
async fn printer_operation_duplicate_move_axis_rejects_ack_without_dispatch() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(1);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        move_axes_operation_command_with_movements(
            command_id.clone(),
            "SERIAL1",
            vec![
                AxisMovement {
                    axis: Axis::X as i32,
                    delta_mm: 10.0,
                },
                AxisMovement {
                    axis: Axis::X as i32,
                    delta_mm: 12.0,
                },
            ],
            3000,
        ),
    )
    .await
    .unwrap();
    drop(sender);

    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandAck(ack) => {
            assert_eq!(ack.command_id, command_id);
            assert!(!ack.accepted);
            assert!(ack.error.contains("duplicate axis"));
        }
        other => panic!("expected command ack, got {other:?}"),
    }
    assert!(receiver.recv().await.is_none());
    assert!(gateway.operations().await.is_empty());
}

#[tokio::test]
async fn printer_operation_invalid_move_bounds_reject_ack_without_dispatch() {
    for (command, expected_error) in [
        (
            move_axes_operation_command_with_movements(
                uuid::Uuid::new_v4().to_string(),
                "SERIAL1",
                vec![AxisMovement {
                    axis: Axis::X as i32,
                    delta_mm: 0.0,
                }],
                3000,
            ),
            "delta_mm",
        ),
        (
            move_axes_operation_command_with_movements(
                uuid::Uuid::new_v4().to_string(),
                "SERIAL1",
                vec![AxisMovement {
                    axis: Axis::X as i32,
                    delta_mm: 51.0,
                }],
                3000,
            ),
            "delta_mm",
        ),
        (
            move_axes_operation_command_with_movements(
                uuid::Uuid::new_v4().to_string(),
                "SERIAL1",
                vec![AxisMovement {
                    axis: Axis::X as i32,
                    delta_mm: f64::NAN,
                }],
                3000,
            ),
            "delta_mm",
        ),
        (
            move_axes_operation_command_with_movements(
                uuid::Uuid::new_v4().to_string(),
                "SERIAL1",
                vec![AxisMovement {
                    axis: Axis::X as i32,
                    delta_mm: 5.0,
                }],
                12_001,
            ),
            "feedrate",
        ),
    ] {
        let config = test_config();
        let command_id = command.command_id.clone();
        let gateway = OperationGateway::default();
        let (sender, mut receiver) = mpsc::channel(1);

        handle_command_with_gateway(&config, &gateway, &sender, command)
            .await
            .unwrap();
        drop(sender);

        match receiver.recv().await.unwrap().event.unwrap() {
            agent_event::Event::CommandAck(ack) => {
                assert_eq!(ack.command_id, command_id);
                assert!(!ack.accepted);
                assert!(ack.error.contains(expected_error), "{}", ack.error);
            }
            other => panic!("expected command ack, got {other:?}"),
        }
        assert!(receiver.recv().await.is_none());
        assert!(gateway.operations().await.is_empty());
    }
}

#[tokio::test]
async fn printer_operation_required_features_reject_unknown_duplicate_and_mismatched_semantics() {
    let home = || printer_operation::Operation::Home(HomeOperation { axes: Vec::new() });
    let modern_move = || {
        printer_operation::Operation::MoveAxes(MoveAxesOperation {
            movements: vec![AxisMovement {
                axis: Axis::X as i32,
                delta_mm: 1.0,
            }],
            feedrate_mm_per_min: 0,
        })
    };
    let cases = vec![
        (
            vec![DeviceFeature::Unspecified as i32],
            home(),
            "required device feature",
        ),
        (vec![999], home(), "required device feature"),
        (
            vec![DeviceFeature::BambuMqttHoming as i32],
            modern_move(),
            "required device feature",
        ),
        (
            vec![DeviceFeature::BambuMqttAxisControl as i32],
            home(),
            "required device feature",
        ),
        (
            vec![
                DeviceFeature::BambuMqttHoming as i32,
                DeviceFeature::BambuMqttHoming as i32,
            ],
            home(),
            "required device feature",
        ),
        (
            vec![DeviceFeature::BambuMqttHoming as i32],
            printer_operation::Operation::Pause(PauseOperation {}),
            "required device feature",
        ),
        (
            vec![DeviceFeature::BambuMqttHoming as i32],
            printer_operation::Operation::SetHotendTemperature(SetHotendTemperatureOperation {
                temperature_celsius: 200,
                wait: false,
                extruder_id: None,
            }),
            "required device feature",
        ),
        (
            vec![DeviceFeature::BambuMqttHoming as i32],
            printer_operation::Operation::AmsRereadRfid(AmsRereadRfidOperation {
                ams_id: 0,
                slot_id: 0,
            }),
            "required device feature",
        ),
        (
            vec![DeviceFeature::BambuMqttHoming as i32],
            printer_operation::Operation::Home(HomeOperation {
                axes: vec![Axis::X as i32],
            }),
            "required device feature",
        ),
        (
            vec![DeviceFeature::BambuMqttAxisControl as i32],
            printer_operation::Operation::MoveAxes(MoveAxesOperation {
                movements: vec![AxisMovement {
                    axis: Axis::X as i32,
                    delta_mm: 2.0,
                }],
                feedrate_mm_per_min: 0,
            }),
            "required device feature",
        ),
        (
            vec![DeviceFeature::BambuMqttAxisControl as i32],
            printer_operation::Operation::MoveAxes(MoveAxesOperation {
                movements: vec![
                    AxisMovement {
                        axis: Axis::X as i32,
                        delta_mm: 1.0,
                    },
                    AxisMovement {
                        axis: Axis::Y as i32,
                        delta_mm: 1.0,
                    },
                ],
                feedrate_mm_per_min: 0,
            }),
            "required device feature",
        ),
        (
            vec![DeviceFeature::BambuMqttAxisControl as i32],
            printer_operation::Operation::MoveAxes(MoveAxesOperation {
                movements: vec![AxisMovement {
                    axis: Axis::X as i32,
                    delta_mm: 10.0,
                }],
                feedrate_mm_per_min: 3000,
            }),
            "required device feature",
        ),
    ];

    for (required_features, operation, expected_error) in cases {
        let config = test_config();
        let command_id = uuid::Uuid::new_v4().to_string();
        let gateway = OperationGateway::default();
        let (sender, mut receiver) = mpsc::channel(2);
        let command = printer_operation_command_with_required_features(
            command_id.clone(),
            "SERIAL1",
            required_features,
            Some(operation),
        );

        handle_command_with_gateway(&config, &gateway, &sender, command)
            .await
            .unwrap();
        drop(sender);

        match receiver.recv().await.unwrap().event.unwrap() {
            agent_event::Event::CommandAck(ack) => {
                assert_eq!(ack.command_id, command_id);
                assert!(!ack.accepted, "unexpected accepted requirement: {ack:?}");
                assert!(ack.error.contains(expected_error), "{}", ack.error);
            }
            other => panic!("expected rejected command ack, got {other:?}"),
        }
        assert!(receiver.recv().await.is_none());
        assert!(gateway.operations().await.is_empty());
    }
}
