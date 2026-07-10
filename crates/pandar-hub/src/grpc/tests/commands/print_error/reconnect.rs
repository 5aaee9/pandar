use super::*;

use crate::repositories::LinkPrinterPayload;

#[tokio::test]
async fn accepted_live_print_error_is_failed_before_replacement_pumps_start() {
    let fixture = live_fixture().await;
    let old_token = fixture
        .state
        .sessions()
        .get(fixture.agent_id)
        .await
        .unwrap()
        .token;
    fixture
        .sender
        .send(Ok(command_ack_event(
            fixture.tenant_id,
            fixture.agent_id,
            fixture.command.id,
            true,
        )))
        .await
        .unwrap();
    fixture.wait_for_status(CommandStatus::Acknowledged).await;
    assert!(fixture.pending());

    let (mut replacement_stream, _replacement_sender) = super::super::connect_live(
        &fixture.state,
        vec![capable_hello_event(fixture.tenant_id, fixture.agent_id)],
    )
    .await
    .unwrap();
    let replacement_token = fixture
        .state
        .sessions()
        .get(fixture.agent_id)
        .await
        .unwrap()
        .token;
    assert_ne!(replacement_token, old_token);

    let command = fixture.command().await;
    assert_eq!(command.status, CommandStatus::Failed);
    assert_eq!(
        command.error.as_deref(),
        Some("agent session replaced before printer operation completed")
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), replacement_stream.next())
            .await
            .is_err(),
        "the replacement stream must not receive the old live command"
    );

    fixture
        .sender
        .send(Ok(command_result_event(
            fixture.tenant_id,
            fixture.agent_id,
            fixture.command.id,
        )))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(25)).await;
    assert_eq!(fixture.command().await.status, CommandStatus::Failed);
    assert_eq!(
        fixture
            .state
            .sessions()
            .get(fixture.agent_id)
            .await
            .unwrap()
            .token,
        replacement_token
    );
}

#[tokio::test]
async fn close_then_reconnect_cannot_acknowledge_detached_live_print_error() {
    let fixture = live_fixture().await;
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let agent_id = fixture.agent_id;
    let command_id = fixture.command.id;
    let transition = state
        .sessions()
        .get(agent_id)
        .await
        .unwrap()
        .live_command_transition
        .lock_owned()
        .await;

    drop(fixture.sender);
    wait_for_session_removal(&state, agent_id).await;
    let (_replacement_stream, replacement_sender) =
        super::super::connect_live(&state, vec![capable_hello_event(tenant_id, agent_id)])
            .await
            .unwrap();
    send_ack_then_barrier(&state, tenant_id, agent_id, command_id, &replacement_sender).await;

    assert_eq!(
        load(&state, tenant_id, command_id).await.status,
        CommandStatus::Sent
    );
    drop(transition);
    assert_failed(
        &state,
        tenant_id,
        command_id,
        "agent connection closed before printer operation completed",
    )
    .await;
}

#[tokio::test]
async fn expiry_then_reconnect_cannot_complete_detached_live_print_error() {
    let fixture = live_fixture().await;
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let agent_id = fixture.agent_id;
    let command_id = fixture.command.id;
    let session = state.sessions().get(agent_id).await.unwrap();
    state
        .sessions()
        .touch_heartbeat_if_current(agent_id, session.token, "2026-07-10T00:00:00Z")
        .await
        .unwrap();
    let transition = session.live_command_transition.clone().lock_owned().await;
    let expired = state
        .sessions()
        .expire_stale(
            "2026-07-10T00:01:00Z",
            Duration::from_secs(45),
            state.agents(),
        )
        .await
        .unwrap();
    assert_eq!(expired.len(), 1);
    let cleanup_state = state.clone();
    let cleanup = tokio::spawn(async move {
        crate::sessions::live_commands::fail_pending_live_commands(
            &cleanup_state,
            tenant_id,
            agent_id,
            expired.into_iter().next().unwrap(),
            "agent session expired before printer operation completed",
        )
        .await;
    });
    let (_replacement_stream, replacement_sender) =
        super::super::connect_live(&state, vec![capable_hello_event(tenant_id, agent_id)])
            .await
            .unwrap();
    send_result_then_barrier(&state, tenant_id, agent_id, command_id, &replacement_sender).await;

    assert_eq!(
        load(&state, tenant_id, command_id).await.status,
        CommandStatus::Sent
    );
    drop(transition);
    cleanup.await.unwrap();
    assert_failed(
        &state,
        tenant_id,
        command_id,
        "agent session expired before printer operation completed",
    )
    .await;
}

#[tokio::test]
async fn not_pending_link_printer_result_is_not_treated_as_durable() {
    let state = super::super::fixture_state().await;
    let (tenant_id, agent_id) = super::super::tenant_agent(&state).await;
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: "SECRET-RECONNECT".to_owned(),
                name: None,
            },
            super::super::test_audit_actor(),
        )
        .await
        .unwrap();
    let (_stream, sender) =
        super::super::connect_live(&state, vec![super::super::hello_event(tenant_id, agent_id)])
            .await
            .unwrap();

    send_result_then_barrier(&state, tenant_id, agent_id, command.id, &sender).await;

    assert_eq!(
        load(&state, tenant_id, command.id).await.status,
        CommandStatus::Sent
    );
}

#[tokio::test]
async fn not_pending_durable_command_keeps_ack_and_result_handling() {
    let state = super::super::fixture_state().await;
    let (tenant_id, agent_id) = super::super::tenant_agent(&state).await;
    let command_id = super::super::sent_command(&state, tenant_id, agent_id).await;
    let (_stream, sender) =
        super::super::connect_live(&state, vec![super::super::hello_event(tenant_id, agent_id)])
            .await
            .unwrap();

    sender
        .send(Ok(command_ack_event(tenant_id, agent_id, command_id, true)))
        .await
        .unwrap();
    wait_for_barrier(&state, tenant_id, agent_id, "2026-07-10T00:02:00Z", &sender).await;
    assert_eq!(
        load(&state, tenant_id, command_id).await.status,
        CommandStatus::Acknowledged
    );

    sender
        .send(Ok(command_result_event(tenant_id, agent_id, command_id)))
        .await
        .unwrap();
    wait_for_barrier(&state, tenant_id, agent_id, "2026-07-10T00:03:00Z", &sender).await;
    assert_eq!(
        load(&state, tenant_id, command_id).await.status,
        CommandStatus::Succeeded
    );
}

async fn send_result_then_barrier(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    command_id: CommandId,
    sender: &mpsc::Sender<Result<AgentEvent, Status>>,
) {
    sender
        .send(Ok(command_result_event(tenant_id, agent_id, command_id)))
        .await
        .unwrap();
    wait_for_barrier(state, tenant_id, agent_id, "2026-07-10T00:02:00Z", sender).await;
}

async fn send_ack_then_barrier(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    command_id: CommandId,
    sender: &mpsc::Sender<Result<AgentEvent, Status>>,
) {
    sender
        .send(Ok(command_ack_event(tenant_id, agent_id, command_id, true)))
        .await
        .unwrap();
    wait_for_barrier(state, tenant_id, agent_id, "2026-07-10T00:02:00Z", sender).await;
}

async fn wait_for_barrier(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    observed_at: &str,
    sender: &mpsc::Sender<Result<AgentEvent, Status>>,
) {
    sender
        .send(Ok(super::super::heartbeat_event(
            tenant_id,
            agent_id,
            observed_at,
        )))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while state
            .sessions()
            .get(agent_id)
            .await
            .unwrap()
            .last_heartbeat_at
            != observed_at
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

async fn wait_for_session_removal(state: &AppState, agent_id: AgentId) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while state.sessions().get(agent_id).await.is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

async fn assert_failed(state: &AppState, tenant_id: TenantId, command_id: CommandId, reason: &str) {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let command = load(state, tenant_id, command_id).await;
            if command.status == CommandStatus::Failed {
                assert_eq!(command.error.as_deref(), Some(reason));
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

async fn load(state: &AppState, tenant_id: TenantId, command_id: CommandId) -> CommandRecord {
    state
        .commands()
        .get_for_tenant(tenant_id, command_id)
        .await
        .unwrap()
        .unwrap()
}
