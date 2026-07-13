use super::fixture::FirmwareFixture;
use super::support::*;
use super::*;

#[tokio::test]
async fn firmware_redaction_typed_result_scrubs_sibling_live_command_url() {
    let mut fixture = FirmwareFixture::new("firmware-redaction-sibling-result-url").await;
    let first = fixture.prepare(start_metadata()).await;
    let second = fixture.prepare(second_start_metadata()).await;
    let first_waiter = fixture
        .start_execute(&first.prepared_token, start_command())
        .await;
    let second_waiter = fixture
        .start_execute(&second.prepared_token, second_start_command())
        .await;

    fixture
        .event(control_result_event(
            second.command_id,
            &fixture.serial,
            firmware_command_result::Outcome::Acknowledgement(FirmwareAcknowledgement {
                command: "start".to_owned(),
                sequence_id: "second-sequence".to_owned(),
                result: Some("fail".to_owned()),
                error_code: Some(9),
                reason: Some(format!("sibling package rejected {URL_SENTINEL}")),
                message: Some(format!("sibling package unavailable {URL_SENTINEL}")),
            }),
        ))
        .await;

    let result = second_waiter.await.unwrap();
    assert!(!first_waiter.is_finished());
    let readback = serde_json::to_string(&(
        result,
        fixture.command(second.command_id).await,
        fixture.command(first.command_id).await,
    ))
    .unwrap();
    assert!(
        !readback.contains(URL_SENTINEL),
        "leaked sibling URL: {readback}"
    );
    assert!(readback.contains("[redacted]"));

    fixture
        .event(agent_event::Event::CommandResult(CommandResult {
            command_id: first.command_id.to_string(),
            success: false,
            error: "test cleanup before publish".to_owned(),
            result_json: String::new(),
            firmware_result: None,
        }))
        .await;
    assert_eq!(
        first_waiter.await.unwrap().phase,
        FirmwareExecutePhase::PrePublishFailure
    );
}

#[tokio::test]
async fn firmware_redaction_negative_ack_scrubs_sibling_live_command_url() {
    let mut fixture = FirmwareFixture::new("firmware-redaction-sibling-ack-url").await;
    let first = fixture.prepare(start_metadata()).await;
    let second = fixture.prepare(second_start_metadata()).await;
    let first_waiter = fixture
        .start_execute(&first.prepared_token, start_command())
        .await;
    let second_waiter = fixture
        .start_execute(&second.prepared_token, second_start_command())
        .await;

    fixture
        .event(agent_event::Event::CommandAck(CommandAck {
            command_id: second.command_id.to_string(),
            accepted: false,
            error: format!("sibling package rejected {URL_SENTINEL}"),
        }))
        .await;

    let result = second_waiter.await.unwrap();
    assert!(!first_waiter.is_finished());
    let readback = serde_json::to_string(&(
        result,
        fixture.command(second.command_id).await,
        fixture.command(first.command_id).await,
    ))
    .unwrap();
    assert!(
        !readback.contains(URL_SENTINEL),
        "leaked sibling URL: {readback}"
    );
    assert!(readback.contains("[redacted]"));

    fixture
        .event(agent_event::Event::CommandResult(CommandResult {
            command_id: first.command_id.to_string(),
            success: false,
            error: "test cleanup before publish".to_owned(),
            result_json: String::new(),
            firmware_result: None,
        }))
        .await;
    assert_eq!(
        first_waiter.await.unwrap().phase,
        FirmwareExecutePhase::PrePublishFailure
    );
}

#[tokio::test]
async fn firmware_redaction_overlapping_sibling_urls_are_order_independent() {
    const SHORT_URL: &str = "https://overlap.invalid/fw";
    const LONG_URL: &str = "https://overlap.invalid/fw-OVERLAP-SECRET?token=OVERLAP-QUERY";

    let mut fixture = FirmwareFixture::new("firmware-redaction-overlapping-live-urls").await;
    let first = fixture.prepare(start_metadata()).await;
    let second = fixture.prepare(start_metadata()).await;
    let storage_order = fixture
        .state
        .sessions()
        .pending_firmware_command_ids_in_storage_order();
    let (short, long) = if storage_order
        .iter()
        .position(|command_id| *command_id == first.command_id)
        .unwrap()
        < storage_order
            .iter()
            .position(|command_id| *command_id == second.command_id)
            .unwrap()
    {
        (first, second)
    } else {
        (second, first)
    };
    let command = |url: &str| FirmwareCommand::Start {
        sequence_id: "studio-sequence".to_owned(),
        src_id: 1,
        url: url.to_owned(),
        module: "ota".to_owned(),
        version: "01.02.03".to_owned(),
    };
    let short_waiter = fixture
        .start_execute(&short.prepared_token, command(SHORT_URL))
        .await;
    let long_waiter = fixture
        .start_execute(&long.prepared_token, command(LONG_URL))
        .await;

    fixture
        .event(control_result_event(
            long.command_id,
            &fixture.serial,
            firmware_command_result::Outcome::Acknowledgement(FirmwareAcknowledgement {
                command: "start".to_owned(),
                sequence_id: "studio-sequence".to_owned(),
                result: Some("fail".to_owned()),
                error_code: Some(9),
                reason: Some(format!("overlapping full URL {LONG_URL}")),
                message: Some(
                    "overlapping path /fw-OVERLAP-SECRET query token=OVERLAP-QUERY".to_owned(),
                ),
            }),
        ))
        .await;

    let result = long_waiter.await.unwrap();
    assert!(!short_waiter.is_finished());
    let readback = serde_json::to_string(&(
        result,
        fixture.command(long.command_id).await,
        fixture.command(short.command_id).await,
    ))
    .unwrap();
    for forbidden in [
        LONG_URL,
        "/fw-OVERLAP-SECRET",
        "OVERLAP-SECRET",
        "token=OVERLAP-QUERY",
        "OVERLAP-QUERY",
    ] {
        assert!(
            !readback.contains(forbidden),
            "leaked overlapping URL component {forbidden}: {readback}"
        );
    }

    fixture
        .event(agent_event::Event::CommandResult(CommandResult {
            command_id: short.command_id.to_string(),
            success: false,
            error: "test cleanup before publish".to_owned(),
            result_json: String::new(),
            firmware_result: None,
        }))
        .await;
    assert_eq!(
        short_waiter.await.unwrap().phase,
        FirmwareExecutePhase::PrePublishFailure
    );

    let mut module = module_with_version("ota", "01.02.03");
    module.name = format!("{SHORT_URL} {LONG_URL}");
    fixture
        .event(agent_event::Event::PrinterFirmwareModulesSnapshot(
            PrinterFirmwareModulesSnapshot {
                serial: fixture.serial.clone(),
                generation: GENERATION,
                module_revision: 1,
                modules: vec![module],
            },
        ))
        .await;
    let readback = serde_json::to_string(
        &fixture
            .state
            .printers()
            .get_with_live_status_for_tenant(fixture.tenant_id, &fixture.printer_id)
            .await
            .unwrap()
            .unwrap()
            .firmware,
    )
    .unwrap();
    for forbidden in [
        LONG_URL,
        "/fw-OVERLAP-SECRET",
        "OVERLAP-SECRET",
        "token=OVERLAP-QUERY",
        "OVERLAP-QUERY",
    ] {
        assert!(
            !readback.contains(forbidden),
            "completed overlapping URL leaked {forbidden}: {readback}"
        );
    }
}

#[tokio::test]
async fn firmware_redaction_scrubs_query_secret_echo_without_full_url() {
    let logs = CapturedLogs::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.writer())
        .with_ansi(false)
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);
    let mut fixture = FirmwareFixture::new("firmware-redaction-query-fragment").await;
    let prepared = fixture.prepare(start_metadata()).await;
    let waiter = fixture
        .start_execute(&prepared.prepared_token, start_command())
        .await;
    fixture
        .event(agent_event::Event::CommandResult(CommandResult {
            command_id: prepared.command_id.to_string(),
            success: false,
            error: "signature=FIRMWARE-URL-SENTINEL".to_owned(),
            result_json: String::new(),
            firmware_result: None,
        }))
        .await;

    let result = waiter.await.unwrap();
    let readback =
        serde_json::to_string(&(result, fixture.command(prepared.command_id).await)).unwrap();
    drop(guard);
    assert!(!readback.contains("FIRMWARE-URL-SENTINEL"));
    assert!(!logs.to_string().contains("FIRMWARE-URL-SENTINEL"));
}

#[tokio::test]
async fn firmware_redaction_scrubs_full_url_before_generic_key_redaction() {
    let mut fixture = FirmwareFixture::new("firmware-redaction-full-ticket-url").await;
    let prepared = fixture.prepare(start_metadata()).await;
    let waiter = fixture
        .start_execute(&prepared.prepared_token, ticket_start_command())
        .await;
    fixture
        .event(agent_event::Event::CommandResult(CommandResult {
            command_id: prepared.command_id.to_string(),
            success: false,
            error: format!("publish rejected {TICKET_URL_SENTINEL}"),
            result_json: String::new(),
            firmware_result: None,
        }))
        .await;

    let result = waiter.await.unwrap();
    let readback =
        serde_json::to_string(&(result, fixture.command(prepared.command_id).await)).unwrap();
    assert!(!readback.contains("FIRMWARE-PATH-SENTINEL"));
    assert!(!readback.contains("FIRMWARE-TICKET-SENTINEL"));
}
