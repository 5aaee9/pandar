use super::*;

#[tokio::test]
async fn plugin_print_error_rejects_present_null_ordinary_field_before_insert() {
    let fixture = operation_fixture("plugin-native-null-ordinary").await;
    let (wake_sender, _) = mpsc::channel(1);
    let (command_sender, _command_receiver) = mpsc::channel(1);
    register_session(
        &fixture,
        wake_sender,
        command_sender,
        [AgentCapability::HandlePrintError],
    )
    .await;
    let mut body = native_body("resume");
    body.as_object_mut()
        .unwrap()
        .insert("speed_mode".to_owned(), Value::Null);

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
        decode::<OperationErrorResponse>(body).error,
        "invalid_printer_control"
    );
    assert_eq!(fixture.state.commands().count().await.unwrap(), 0);
    assert!(
        fixture
            .state
            .audit_events()
            .list_for_tenant(fixture.tenant_id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn plugin_ordinary_operation_rejects_present_null_native_field_before_insert() {
    let fixture = operation_fixture("plugin-ordinary-null-native").await;

    let (status, body) = request_as(
        fixture.app.clone(),
        Method::POST,
        &fixture.uri,
        Some(serde_json::json!({
            "action": "pause",
            "error_action": null
        })),
        &fixture.token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        decode::<OperationErrorResponse>(body).error,
        "invalid_printer_control"
    );
    assert_eq!(fixture.state.commands().count().await.unwrap(), 0);
    assert!(
        fixture
            .state
            .audit_events()
            .list_for_tenant(fixture.tenant_id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn plugin_print_error_rejects_missing_extra_cross_and_invalid_fields_before_insert() {
    let fixture = operation_fixture("plugin-native-invalid").await;
    let (wake_sender, _) = mpsc::channel(1);
    let (command_sender, _command_receiver) = mpsc::channel(8);
    register_session(
        &fixture,
        wake_sender,
        command_sender,
        [AgentCapability::HandlePrintError],
    )
    .await;
    let mut bodies = vec![
        serde_json::json!({
            "action": "handle_print_error",
            "error_action": "resume",
            "print_error": 0,
            "printer_job_id": "job-7",
            "sequence_id": 20_042
        }),
        serde_json::json!({
            "action": "handle_print_error",
            "error_action": "resume",
            "print_error": -1,
            "printer_job_id": "job-7",
            "sequence_id": 20_042
        }),
        serde_json::json!({
            "action": "handle_print_error",
            "error_action": "resume",
            "print_error": i32::MAX as u64 + 1,
            "printer_job_id": "job-7",
            "sequence_id": 20_042
        }),
        serde_json::json!({
            "action": "handle_print_error",
            "error_action": "unknown",
            "print_error": 83_918_929,
            "printer_job_id": "job-7",
            "sequence_id": 20_042
        }),
        serde_json::json!({
            "action": "handle_print_error",
            "error_action": "resume",
            "print_error": 83_918_929,
            "printer_job_id": "job-7",
            "sequence_id": 20_042,
            "unexpected": true
        }),
        serde_json::json!({
            "action": "handle_print_error",
            "error_action": "resume",
            "print_error": 83_918_929,
            "printer_job_id": "job-7",
            "sequence_id": 20_042,
            "speed_mode": 2
        }),
        serde_json::json!({
            "action": "pause",
            "error_action": "resume"
        }),
        serde_json::json!({
            "action": "handle_print_error",
            "error_action": "resume",
            "print_error": 83_918_929,
            "printer_job_id": "job-7",
            "sequence_id": 20_042,
            "axes": []
        }),
        serde_json::json!({
            "action": "handle_print_error",
            "error_action": "resume",
            "print_error": 83_918_929,
            "printer_job_id": "job-7",
            "sequence_id": 20_042,
            "movements": []
        }),
    ];
    for missing in [
        "error_action",
        "print_error",
        "printer_job_id",
        "sequence_id",
    ] {
        let mut body = native_body("resume");
        body.as_object_mut().unwrap().remove(missing);
        bodies.push(body);
    }

    for body in bodies {
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
            decode::<OperationErrorResponse>(body).error,
            "invalid_printer_control"
        );
    }
    assert_eq!(fixture.state.commands().count().await.unwrap(), 0);
}
