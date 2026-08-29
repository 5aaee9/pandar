use super::*;

#[tokio::test]
async fn agent_replacement_waits_until_web_recovery_is_persisted_and_enqueued() {
    let mut fixture = RecoveryFixture::new(
        "tenant-native-lease-race",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;
    let mut pause = crate::repositories::current_transaction_pause::install(&fixture.session_id);
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
    pause.wait_until_reached().await;

    let (lease_acquired_sender, mut lease_acquired_receiver) = tokio::sync::oneshot::channel();
    let replacement = tokio::spawn({
        let state = fixture.state.clone();
        let tenant_id = fixture.tenant_id;
        let agent_id = fixture.agent_id;
        async move {
            let _lease = state.sessions().transition_lease(agent_id).await;
            let _ = lease_acquired_sender.send(());
            state
                .agents()
                .claim_online_session(
                    tenant_id,
                    agent_id,
                    &SessionToken::new().persisted_id(),
                    "replacement",
                    &pandar_core::created_at_now(),
                )
                .await
                .unwrap();
        }
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut lease_acquired_receiver)
            .await
            .is_err(),
        "replacement acquired the transition lease while recovery owned it"
    );

    pause.resume();
    let (status, body) = request.await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    tokio::time::timeout(Duration::from_secs(1), &mut lease_acquired_receiver)
        .await
        .unwrap()
        .unwrap();
    replacement.await.unwrap();
    assert!(fixture.command_receiver.recv().await.unwrap().is_ok());
}

#[tokio::test]
async fn concurrent_tenant_recoveries_persist_and_dispatch_only_one_command() {
    let fixture = RecoveryFixture::new_file(
        "tenant-native-race",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;

    let (left, right) = tokio::join!(
        fixture.request("resume", ERROR_GENERATION),
        fixture.request("ignore", ERROR_GENERATION),
    );

    let statuses = [left.0, right.0];
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::OK)
            .count(),
        1
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::BAD_REQUEST)
            .count(),
        1
    );
    let unavailable = if left.0 == StatusCode::BAD_REQUEST {
        left.1
    } else {
        right.1
    };
    assert_eq!(
        decode::<ErrorResponse>(unavailable).error,
        "printer_operation_unavailable"
    );
    assert_eq!(fixture.state.commands().count().await.unwrap(), 1);
}

#[tokio::test]
async fn studio_native_recovery_blocks_web_until_the_command_is_terminal() {
    let fixture = RecoveryFixture::new(
        "tenant-native-studio-overlap",
        "20P123456789",
        [
            AgentCapability::HandlePrintError,
            AgentCapability::HandlePrintErrorSequenceZeroPubackOnly,
        ],
    )
    .await;
    let studio = fixture
        .state
        .commands()
        .create_printer_operation_sent_with_audit(
            fixture.tenant_id,
            &fixture.printer_id,
            fixture.agent_id,
            PrinterOperationKind::HandlePrintError {
                error_action: PrintErrorAction::Resume,
                print_error: BUILD_PLATE_MISMATCH,
                printer_job_id: "job-7".to_owned(),
                sequence_id: 20_042,
            },
            crate::repositories::AuditActor::tenant_token(
                None,
                "studio-overlap",
                vec!["plugin:studio"],
            ),
        )
        .await
        .unwrap();

    let (status, body) = fixture.request("stop", ERROR_GENERATION).await;
    assert_unavailable(status, body);
    assert_eq!(fixture.state.commands().count().await.unwrap(), 1);

    fixture
        .state
        .commands()
        .mark_succeeded(studio.id, fixture.tenant_id, fixture.agent_id)
        .await
        .unwrap();
    let (status, body) = fixture.request("stop", ERROR_GENERATION).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(fixture.state.commands().count().await.unwrap(), 2);
}

#[tokio::test]
async fn tenant_recovery_dispatch_failure_marks_the_sent_command_failed() {
    let mut fixture = RecoveryFixture::new(
        "tenant-native-send-failure",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;
    fixture.command_receiver.close();

    let (status, body) = fixture.request("resume", ERROR_GENERATION).await;

    assert_unavailable(status, body);
    let command = crate::entities::commands::Entity::find()
        .one(&fixture.state.database().sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(command.status, "failed");
    assert!(command.error.unwrap().contains("ChannelClosed"));
}
