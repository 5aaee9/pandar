use super::*;

#[tokio::test]
async fn firmware_redaction_allows_more_than_64_distinct_start_urls_without_leaking() {
    let mut fixture = FirmwareFixture::new("firmware-redaction-many-urls").await;

    for index in 0..65 {
        let sequence_id = format!("studio-sequence-{index}");
        let version = format!("01.02.{index:02}");
        let url = format!(
            "https://user:secret@firmware.invalid/main-{index}.bin?signature=FIRMWARE-URL-SENTINEL-{index}"
        );
        let prepared = fixture
            .prepare(FirmwareControlMetadata::Start {
                sequence_id: sequence_id.clone(),
                src_id: 1,
                module: "ota".to_owned(),
                version: version.clone(),
            })
            .await;
        let waiter = fixture
            .start_execute(
                &prepared.prepared_token,
                FirmwareCommand::Start {
                    sequence_id,
                    src_id: 1,
                    url: url.clone(),
                    module: "ota".to_owned(),
                    version,
                },
            )
            .await;
        fixture
            .event(agent_event::Event::CommandResult(CommandResult {
                command_id: prepared.command_id.to_string(),
                success: false,
                error: format!("publish rejected url={url}"),
                result_json: String::new(),
                firmware_result: None,
            }))
            .await;
        assert_eq!(
            waiter.await.unwrap().phase,
            FirmwareExecutePhase::PrePublishFailure
        );
        let durable = serde_json::to_string(&fixture.command(prepared.command_id).await).unwrap();
        assert!(!durable.contains(&url));
        assert!(durable.contains("[redacted]"));
    }

    let first_url =
        "https://user:secret@firmware.invalid/main-0.bin?signature=FIRMWARE-URL-SENTINEL-0";
    let last_url =
        "https://user:secret@firmware.invalid/main-64.bin?signature=FIRMWARE-URL-SENTINEL-64";
    let mut module = module_with_version("ota", "01.02.64");
    module.name = format!("{first_url} {last_url}");
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
    let firmware = fixture
        .state
        .printers()
        .get_with_live_status_for_tenant(fixture.tenant_id, &fixture.printer_id)
        .await
        .unwrap()
        .unwrap()
        .firmware;
    let readback = serde_json::to_string(&firmware).unwrap();
    assert!(!readback.contains(first_url));
    assert!(!readback.contains(last_url));
    assert!(readback.contains("[redacted]"));
}

#[tokio::test]
async fn firmware_redaction_deduplicates_and_retains_across_generation_and_session_cancellation() {
    let mut generation = FirmwareFixture::new("firmware-redaction-generation-cleanup").await;
    let first = generation.prepare(start_metadata()).await;
    let first_identity = generation
        .state
        .sessions()
        .firmware_token_locator(&first.prepared_token)
        .unwrap();
    let first_waiter = generation
        .start_execute(&first.prepared_token, start_command())
        .await;
    generation
        .event(agent_event::Event::CommandResult(CommandResult {
            command_id: first.command_id.to_string(),
            success: false,
            error: "publish was not attempted".to_owned(),
            result_json: String::new(),
            firmware_result: None,
        }))
        .await;
    assert_eq!(
        first_waiter.await.unwrap().phase,
        FirmwareExecutePhase::PrePublishFailure
    );

    let second = generation.prepare(start_metadata()).await;
    let second_waiter = generation
        .start_execute(&second.prepared_token, start_command())
        .await;
    generation
        .event(agent_event::Event::CommandResult(CommandResult {
            command_id: second.command_id.to_string(),
            success: false,
            error: "publish was not attempted".to_owned(),
            result_json: String::new(),
            firmware_result: None,
        }))
        .await;
    assert_eq!(
        second_waiter.await.unwrap().phase,
        FirmwareExecutePhase::PrePublishFailure
    );
    assert_eq!(
        generation
            .state
            .sessions()
            .retained_firmware_redaction_url_count(&first_identity),
        1
    );
    generation
        .event(agent_event::Event::PrinterFirmwareInvalidated(
            PrinterFirmwareInvalidated {
                serial: generation.serial.clone(),
                generation: GENERATION + 1,
            },
        ))
        .await;
    assert_eq!(
        generation
            .state
            .sessions()
            .retained_firmware_redaction_url_count(&first_identity),
        1
    );
    let mut next_generation_identity = first_identity.clone();
    next_generation_identity.generation = GENERATION + 1;
    generation
        .state
        .sessions()
        .retain_firmware_redaction_url_for_tests(&next_generation_identity, URL_SENTINEL)
        .unwrap();
    assert_eq!(
        generation
            .state
            .sessions()
            .retained_firmware_redaction_url_count(&next_generation_identity),
        1
    );

    let mut session = FirmwareFixture::new("firmware-redaction-session-cleanup").await;
    let prepared = session.prepare(start_metadata()).await;
    let identity = session
        .state
        .sessions()
        .firmware_token_locator(&prepared.prepared_token)
        .unwrap();
    session
        .state
        .sessions()
        .retain_firmware_redaction_url_for_tests(&identity, URL_SENTINEL)
        .unwrap();
    let lease = session
        .state
        .sessions()
        .transition_lease_for_session(session.agent_id, session.token)
        .await;
    let cancelled = session
        .state
        .sessions()
        .cancel_firmware_session_under_transition(session.agent_id, session.token);
    drop(lease);
    crate::firmware_control::finish_cancelled_commands(
        &session.state,
        cancelled,
        "test session cleanup",
    )
    .await;
    assert_eq!(
        session
            .state
            .sessions()
            .retained_firmware_redaction_url_count(&identity),
        1
    );
}

#[tokio::test]
async fn firmware_redaction_scope_isolates_other_tenants_and_serials() {
    const OPAQUE_URL: &str = "https://firmware.invalid/FIRMWARE-OPAQUE-REDACTION-SCOPE.bin";
    let mut fixture = FirmwareFixture::new("firmware-redaction-scope-isolation").await;
    let prepared = fixture.prepare(start_metadata()).await;
    let identity = fixture
        .state
        .sessions()
        .firmware_token_locator(&prepared.prepared_token)
        .unwrap();
    fixture
        .state
        .sessions()
        .retain_firmware_redaction_url_for_tests(&identity, OPAQUE_URL)
        .unwrap();

    let mut later_identity = identity.clone();
    later_identity.agent_id = AgentId::new();
    later_identity.session_token = SessionToken::new();
    later_identity.generation += 1;
    assert_eq!(
        fixture
            .state
            .sessions()
            .redact_firmware_text_under_transition(&later_identity, OPAQUE_URL),
        "[redacted]"
    );

    let mut other_serial = later_identity.clone();
    other_serial.serial.push_str("-other");
    assert!(
        fixture
            .state
            .sessions()
            .redact_firmware_text_under_transition(&other_serial, OPAQUE_URL)
            .contains("FIRMWARE-OPAQUE-REDACTION-SCOPE")
    );

    let mut other_tenant = later_identity;
    other_tenant.tenant_id = TenantId::new();
    assert!(
        fixture
            .state
            .sessions()
            .redact_firmware_text_under_transition(&other_tenant, OPAQUE_URL)
            .contains("FIRMWARE-OPAQUE-REDACTION-SCOPE")
    );
}
