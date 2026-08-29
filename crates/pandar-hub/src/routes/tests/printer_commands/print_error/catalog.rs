use super::*;

#[tokio::test]
async fn tenant_recovery_catalog_is_exactly_six_families_and_three_actions() {
    for family in ["093", "094", "20P", "22E", "239", "31B"] {
        for (action, expected) in [
            ("resume", PrintErrorAction::Resume),
            ("ignore", PrintErrorAction::Ignore),
            ("stop", PrintErrorAction::Stop),
        ] {
            let slug = format!("catalog-{}-{action}", family.to_ascii_lowercase());
            let fixture = RecoveryFixture::new(
                &slug,
                &format!("{family}123456789"),
                [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
            )
            .await;

            let (status, body) = fixture.request(action, ERROR_GENERATION).await;

            assert_eq!(status, StatusCode::OK, "{family} {action}: {body}");
            let response = decode::<CommandResponse>(body);
            let command = fixture
                .state
                .commands()
                .get_for_tenant(fixture.tenant_id, CommandId::parse(&response.id).unwrap())
                .await
                .unwrap()
                .unwrap();
            let payload: PrinterOperationPayload =
                serde_json::from_str(&command.payload_json).unwrap();
            assert!(matches!(
                payload.operation,
                PrinterOperationKind::HandlePrintError {
                    error_action,
                    print_error: BUILD_PLATE_MISMATCH,
                    sequence_id: 0,
                    ..
                } if error_action == expected
            ));
        }
    }

    for serial in ["26A123456789", "XYZ123456789", "20"] {
        let slug = format!("catalog-miss-{}", serial.to_ascii_lowercase());
        let fixture = RecoveryFixture::new(
            &slug,
            serial,
            [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
        )
        .await;

        let (status, body) = fixture.request("stop", ERROR_GENERATION).await;

        assert_unavailable(status, body);
        assert_eq!(fixture.state.commands().count().await.unwrap(), 0);
    }
}

#[tokio::test]
async fn tenant_recovery_uses_native_build_plate_marker_actions_and_error_code() {
    for (action, expected) in [
        ("ignore", PrintErrorAction::Ignore),
        ("resume", PrintErrorAction::Resume),
    ] {
        let fixture = RecoveryFixture::new(
            &format!("marker-{action}"),
            "20P123456789",
            [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
        )
        .await;
        mutate_printer(
            &fixture,
            RecoveryMutation::PrintError(Some(BUILD_PLATE_MARKER_NOT_DETECTED as i32)),
        )
        .await;

        let (status, body) = fixture.request(action, ERROR_GENERATION).await;

        assert_eq!(status, StatusCode::OK, "{action}: {body}");
        let response = decode::<CommandResponse>(body);
        let command = fixture
            .state
            .commands()
            .get_for_tenant(fixture.tenant_id, CommandId::parse(&response.id).unwrap())
            .await
            .unwrap()
            .unwrap();
        let payload: PrinterOperationPayload = serde_json::from_str(&command.payload_json).unwrap();
        assert!(matches!(
            payload.operation,
            PrinterOperationKind::HandlePrintError {
                error_action,
                print_error: BUILD_PLATE_MARKER_NOT_DETECTED,
                sequence_id: 0,
                ..
            } if error_action == expected
        ));
    }

    let fixture = RecoveryFixture::new(
        "marker-stop-rejected",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;
    mutate_printer(
        &fixture,
        RecoveryMutation::PrintError(Some(BUILD_PLATE_MARKER_NOT_DETECTED as i32)),
    )
    .await;

    let (status, body) = fixture.request("stop", ERROR_GENERATION).await;

    assert_unavailable(status, body);
    assert_eq!(fixture.state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn tenant_recovery_preserves_additional_server_owned_plate_error() {
    let fixture = RecoveryFixture::new(
        "plate-offset-ignore",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;
    mutate_printer(
        &fixture,
        RecoveryMutation::PrintError(Some(BUILD_PLATE_OFFSET as i32)),
    )
    .await;

    let (status, body) = fixture.request("ignore", ERROR_GENERATION).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let response = decode::<CommandResponse>(body);
    let command = fixture
        .state
        .commands()
        .get_for_tenant(fixture.tenant_id, CommandId::parse(&response.id).unwrap())
        .await
        .unwrap()
        .unwrap();
    let payload: PrinterOperationPayload = serde_json::from_str(&command.payload_json).unwrap();
    assert!(matches!(
        payload.operation,
        PrinterOperationKind::HandlePrintError {
            error_action: PrintErrorAction::Ignore,
            print_error: BUILD_PLATE_OFFSET,
            sequence_id: 0,
            ..
        }
    ));
}

#[tokio::test]
async fn tenant_recovery_parser_rejects_transport_state_and_cross_operation_fields() {
    let fixture = RecoveryFixture::new(
        "tenant-native-parser",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;
    let invalid = [
        serde_json::json!({"error_action":"resume","error_generation":9}),
        serde_json::json!({"action":null,"error_action":"resume","error_generation":9}),
        serde_json::json!({"action":"handle_print_error","error_generation":9}),
        serde_json::json!({"action":"handle_print_error","error_action":null,"error_generation":9}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume"}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":null}),
        serde_json::json!({"action":"handle_print_error","error_action":"retry","error_generation":9}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":-1}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":9,"print_error":BUILD_PLATE_MISMATCH}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":9,"printer_job_id":"forged"}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":9,"job_id":"forged"}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":9,"sequence_id":0}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":9,"job_attr":0}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":9,"job_state":0}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":9,"task_generation":9}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":9,"speed_mode":1}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":9,"unexpected":true}),
        serde_json::json!({"action":"pause","error_generation":9}),
    ];

    for body in invalid {
        let (status, body) = request_as(
            fixture.app.clone(),
            Method::POST,
            &fixture.uri,
            Some(body),
            &fixture.token,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            decode::<ErrorResponse>(body).error,
            "invalid_printer_control"
        );
    }
    assert_eq!(fixture.state.commands().count().await.unwrap(), 0);
}
