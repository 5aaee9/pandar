use super::*;
use crate::repositories::{PrinterSnapshotUpsert, printer_operation_ownership_pause};

#[tokio::test]
async fn plugin_print_error_rejects_same_serial_reassignment_between_owner_read_and_insert() {
    let fixture = operation_fixture("plugin-native-owner-race").await;
    let replacement_agent = fixture
        .state
        .agents()
        .create(fixture.tenant_id, "replacement-agent")
        .await
        .unwrap();
    let (wake_sender, _) = mpsc::channel(1);
    let (old_command_sender, mut old_command_receiver) = mpsc::channel(1);
    register_session(
        &fixture,
        wake_sender,
        old_command_sender,
        [AgentCapability::HandlePrintError],
    )
    .await;
    let (replacement_command_sender, mut replacement_command_receiver) = mpsc::channel(1);
    register_session_for_agent(
        &fixture,
        replacement_agent.id,
        mpsc::channel(1).0,
        replacement_command_sender,
        [AgentCapability::HandlePrintError],
    )
    .await;

    let pause = printer_operation_ownership_pause::install(&fixture.printer_id);
    let request = tokio::spawn({
        let app = fixture.app.clone();
        let uri = fixture.uri.clone();
        let token = fixture.token.clone();
        async move { request_as(app, Method::POST, &uri, Some(native_body("resume")), &token).await }
    });
    let resume = pause.wait_until_reached().await.unwrap();

    fixture
        .state
        .printers()
        .upsert_snapshot(
            fixture.tenant_id,
            replacement_agent.id,
            PrinterSnapshotUpsert {
                serial_number: format!("serial-{}", fixture.printer_id),
                host: Some("192.0.2.20".to_owned()),
                access_code: None,
                name: "Reassigned Printer".to_owned(),
                model: Some("A1".to_owned()),
                status: "IDLE".to_owned(),
                observed_at: "2026-07-10T00:00:00Z".to_owned(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: None,
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_light_on: None,
            },
        )
        .await
        .unwrap();
    resume.send(()).unwrap();

    let (status, body) = request.await.unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        decode::<OperationErrorResponse>(body).error,
        "printer_operation_unavailable"
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
    assert!(old_command_receiver.try_recv().is_err());
    assert!(replacement_command_receiver.try_recv().is_err());
}
