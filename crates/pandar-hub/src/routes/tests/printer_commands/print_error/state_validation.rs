use super::*;

#[tokio::test]
async fn tenant_recovery_revalidates_every_authoritative_error_and_state_guard() {
    let cases = [
        ("cleared", RecoveryMutation::PrintError(None)),
        (
            "different-error",
            RecoveryMutation::PrintError(Some(BUILD_PLATE_MISMATCH as i32 + 1)),
        ),
        (
            "generation",
            RecoveryMutation::ErrorGeneration(ERROR_GENERATION as i64 - 1),
        ),
        (
            "task-marker",
            RecoveryMutation::ErrorTaskGeneration(Some(ERROR_GENERATION as i64 - 1)),
        ),
        (
            "session-marker",
            RecoveryMutation::ErrorSession(Some("other-session")),
        ),
        ("receive-marker", RecoveryMutation::ErrorReceivedAt(None)),
        ("native-missing", RecoveryMutation::GcodeState(None)),
        (
            "native-unknown",
            RecoveryMutation::GcodeState(Some("UNKNOWN")),
        ),
        ("native-idle", RecoveryMutation::GcodeState(Some("IDLE"))),
        (
            "native-finish",
            RecoveryMutation::GcodeState(Some("FINISH")),
        ),
        (
            "native-failed",
            RecoveryMutation::GcodeState(Some("FAILED")),
        ),
        ("coarse-idle", RecoveryMutation::CoarseState("IDLE")),
        ("coarse-offline", RecoveryMutation::CoarseState("offline")),
        ("coarse-failed", RecoveryMutation::CoarseState("FAILED")),
        ("job-attr-missing", RecoveryMutation::JobAttr(None)),
        ("job-state-unsafe", RecoveryMutation::JobAttr(Some(0x20))),
    ];

    for (case, mutation) in cases {
        let fixture = RecoveryFixture::new(
            &format!("guard-{case}"),
            "20P123456789",
            [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
        )
        .await;
        mutate_printer(&fixture, mutation).await;

        let (status, body) = fixture.request("resume", ERROR_GENERATION).await;

        assert_unavailable(status, body);
        assert_eq!(
            fixture.state.commands().count().await.unwrap(),
            0,
            "guard case {case} persisted a command"
        );
    }

    let fixture = RecoveryFixture::new(
        "guard-stale-client-generation",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;
    let (status, body) = fixture.request("resume", ERROR_GENERATION - 1).await;
    assert_unavailable(status, body);
    assert_eq!(fixture.state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn tenant_recovery_accepts_only_the_four_native_active_states() {
    for native_state in ["PREPARE", "SLICING", "RUNNING", "PAUSE"] {
        let fixture = RecoveryFixture::new(
            &format!("native-state-{}", native_state.to_ascii_lowercase()),
            "20P123456789",
            [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
        )
        .await;
        mutate_printer(&fixture, RecoveryMutation::GcodeState(Some(native_state))).await;

        let (status, body) = fixture.request("resume", ERROR_GENERATION).await;

        assert_eq!(status, StatusCode::OK, "{native_state}: {body}");
    }
}

#[tokio::test]
async fn tenant_recovery_derives_job_state_bits_and_stop_does_not_use_the_guard() {
    for (slug, action, job_attr) in [
        ("job-state-zero", "ignore", Some(0x00)),
        ("job-state-one", "resume", Some(0x1f)),
        ("stop-job-state-unknown", "stop", None),
        ("stop-job-state-unsafe", "stop", Some(0xf0)),
    ] {
        let fixture = RecoveryFixture::new(
            slug,
            "20P123456789",
            [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
        )
        .await;
        mutate_printer(&fixture, RecoveryMutation::JobAttr(job_attr)).await;

        let (status, body) = fixture.request(action, ERROR_GENERATION).await;

        assert_eq!(status, StatusCode::OK, "{slug}: {body}");
    }
}

#[tokio::test]
async fn tenant_recovery_preserves_explicit_empty_and_unknown_job_ids_as_empty() {
    for (slug, job_id) in [("empty-job", Some("")), ("unknown-job", None)] {
        let fixture = RecoveryFixture::new(
            slug,
            "20P123456789",
            [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
        )
        .await;
        mutate_printer(&fixture, RecoveryMutation::PrinterJobId(job_id)).await;

        let (status, body) = fixture.request("stop", ERROR_GENERATION).await;

        assert_eq!(status, StatusCode::OK, "{slug}: {body}");
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
            PrinterOperationKind::HandlePrintError { printer_job_id, .. } if printer_job_id.is_empty()
        ));
    }
}

#[tokio::test]
async fn tenant_recovery_rejects_offline_or_replaced_persisted_agent_session() {
    let offline = RecoveryFixture::new(
        "agent-offline",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;
    offline
        .state
        .agents()
        .mark_offline_if_current(
            offline.tenant_id,
            offline.agent_id,
            &offline.session_id,
            &pandar_core::created_at_now(),
        )
        .await
        .unwrap();
    let (status, body) = offline.request("resume", ERROR_GENERATION).await;
    assert_unavailable(status, body);

    let replaced = RecoveryFixture::new(
        "agent-replaced",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;
    replaced
        .state
        .agents()
        .claim_online_session(
            replaced.tenant_id,
            replaced.agent_id,
            &SessionToken::new().persisted_id(),
            "replacement",
            &pandar_core::created_at_now(),
        )
        .await
        .unwrap();
    let (status, body) = replaced.request("resume", ERROR_GENERATION).await;
    assert_unavailable(status, body);
}

#[tokio::test]
async fn tenant_recovery_revalidates_printer_state_after_the_route_owner_read() {
    let fixture = RecoveryFixture::new(
        "tenant-native-state-race",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;
    let pause =
        crate::repositories::printer_operation_ownership_pause::install(&fixture.printer_id);
    let request = tokio::spawn({
        let app = fixture.app.clone();
        let uri = fixture.uri.clone();
        let token = fixture.token.clone();
        async move {
            request_as(
                app,
                Method::POST,
                &uri,
                Some(recovery_body("resume", ERROR_GENERATION)),
                &token,
            )
            .await
        }
    });
    let resume = pause.wait_until_reached().await.unwrap();
    mutate_printer(&fixture, RecoveryMutation::PrintError(None)).await;
    resume.send(()).unwrap();

    let (status, body) = request.await.unwrap();

    assert_unavailable(status, body);
    assert_eq!(fixture.state.commands().count().await.unwrap(), 0);
}
