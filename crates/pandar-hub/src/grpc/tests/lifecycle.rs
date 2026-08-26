use tokio_stream::StreamExt;
use tonic::Code;

use super::*;
use crate::repositories::LinkPrinterPayload;
use pandar_core::CommandStatus;
use pandar_protocol::agent::v1::{HubCommand, LinkPrinter, hub_command};

#[tokio::test]
async fn replacement_session_survives_old_stream_shutdown() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (_old_stream, old_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let old_token = state.sessions().get(agent_id).await.unwrap().token;

    let (_new_stream, _new_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let new_token = state.sessions().get(agent_id).await.unwrap().token;
    assert_ne!(old_token, new_token);

    drop(old_sender);
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    assert_eq!(
        state.sessions().get(agent_id).await.unwrap().token,
        new_token
    );
    let persisted = persisted_agent(&state, agent_id).await;
    assert_eq!(persisted.status, AgentStatus::Online.as_str());
    assert_eq!(persisted.current_session_id, Some(new_token.persisted_id()));
}

#[tokio::test]
async fn old_disconnect_cannot_clear_replacement_session() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let old_token = register_test_session(&state, tenant_id, agent_id).await;
    let replacement = register_test_session(&state, tenant_id, agent_id).await;

    assert!(
        disconnect_session(&state, tenant_id, agent_id, old_token)
            .await
            .is_none()
    );

    assert_eq!(
        state.sessions().get(agent_id).await.unwrap().token,
        replacement
    );
    let persisted = persisted_agent(&state, agent_id).await;
    assert_eq!(persisted.status, AgentStatus::Online.as_str());
    assert_eq!(
        persisted.current_session_id,
        Some(replacement.persisted_id())
    );
}

#[tokio::test]
async fn cross_process_disconnect_removes_local_stale_session_but_preserves_persisted_replacement()
{
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let local_token = register_test_session(&state, tenant_id, agent_id).await;
    let remote_token = SessionToken::new();
    state
        .agents()
        .claim_online_session(
            tenant_id,
            agent_id,
            &remote_token.persisted_id(),
            "remote",
            "2026-07-10T00:01:00Z",
        )
        .await
        .unwrap();

    let removed = disconnect_session(&state, tenant_id, agent_id, local_token).await;

    assert_eq!(removed.unwrap().token, local_token);
    assert!(state.sessions().get(agent_id).await.is_none());
    let persisted = persisted_agent(&state, agent_id).await;
    assert_eq!(persisted.status, AgentStatus::Online.as_str());
    assert_eq!(
        persisted.current_session_id,
        Some(remote_token.persisted_id())
    );
}

#[tokio::test]
async fn current_disconnect_clears_exact_persisted_session() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let token = register_test_session(&state, tenant_id, agent_id).await;

    let removed = disconnect_session(&state, tenant_id, agent_id, token).await;

    assert_eq!(removed.unwrap().token, token);
    assert!(state.sessions().get(agent_id).await.is_none());
    let persisted = persisted_agent(&state, agent_id).await;
    assert_eq!(persisted.status, AgentStatus::Offline.as_str());
    assert_eq!(persisted.current_session_id, None);
}

#[tokio::test]
async fn concurrent_registrations_serialize_persisted_claim_and_registry_install() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let token_a = SessionToken::new();
    let token_b = SessionToken::new();
    let mut paused = crate::sessions::transition_pause::install_after(token_a);

    let state_a = state.clone();
    let registration_a = tokio::spawn(async move {
        register_test_session_with_token(&state_a, tenant_id, agent_id, token_a).await
    });
    paused.wait_until_reached().await;

    let state_b = state.clone();
    let mut waiting = crate::sessions::transition_pause::observe_waiting(token_b);
    let registration_b = tokio::spawn(async move {
        register_test_session_with_token(&state_b, tenant_id, agent_id, token_b).await
    });
    waiting.wait_until_reached().await;
    assert!(!registration_b.is_finished());

    paused.resume();
    registration_a.await.unwrap();
    registration_b.await.unwrap();

    assert_eq!(state.sessions().get(agent_id).await.unwrap().token, token_b);
    let persisted = persisted_agent(&state, agent_id).await;
    assert_eq!(persisted.current_session_id, Some(token_b.persisted_id()));
    assert_eq!(persisted.status, AgentStatus::Online.as_str());
}

#[tokio::test]
async fn replacement_closes_old_response_stream() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (mut old_stream, _old_sender) =
        connect_live(&state, vec![hello_event(tenant_id, agent_id)])
            .await
            .unwrap();

    let (_new_stream, _new_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    assert!(old_stream.next().await.is_none());
}

#[tokio::test]
async fn replacement_stream_receives_commands_after_old_stream_closes() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (mut old_stream, _old_sender) =
        connect_live(&state, vec![hello_event(tenant_id, agent_id)])
            .await
            .unwrap();

    let (mut new_stream, _new_sender) =
        connect_live(&state, vec![hello_event(tenant_id, agent_id)])
            .await
            .unwrap();
    let command = state
        .commands()
        .enqueue_refresh_printers(tenant_id, agent_id)
        .await
        .unwrap();
    state.sessions().wake_local_agent(tenant_id, agent_id).await;

    assert!(old_stream.next().await.is_none());
    let hub_command = new_stream.next().await.unwrap().unwrap();
    assert_eq!(hub_command.command_id, command.id.to_string());
}

#[tokio::test]
async fn old_stream_heartbeat_does_not_touch_replacement_session() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (_old_stream, old_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    let (_new_stream, _new_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let replacement = state.sessions().get(agent_id).await.unwrap();

    old_sender
        .send(Ok(heartbeat_event(
            tenant_id,
            agent_id,
            "2026-06-20T00:10:00Z",
        )))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let current = state.sessions().get(agent_id).await.unwrap();
    assert_eq!(current.token, replacement.token);
    assert_eq!(current.last_heartbeat_at, replacement.last_heartbeat_at);
}

#[tokio::test]
async fn replacement_session_blocks_old_heartbeat_commit() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (_old_stream, _old_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let old_token = state.sessions().get(agent_id).await.unwrap().token;
    let mut paused = crate::sessions::transition_pause::install_before(old_token);

    let old_state = state.clone();
    let old_heartbeat = tokio::spawn(async move {
        handle_event(
            &old_state,
            tenant_id,
            agent_id,
            old_token,
            heartbeat_event(tenant_id, agent_id, "2026-07-10T00:10:00Z"),
        )
        .await
    });
    paused.wait_until_reached().await;

    let replacement = register_test_session(&state, tenant_id, agent_id).await;
    paused.resume();
    old_heartbeat.await.unwrap().unwrap();

    let persisted = persisted_agent(&state, agent_id).await;
    assert_eq!(
        persisted.current_session_id,
        Some(replacement.persisted_id())
    );
    assert_eq!(
        persisted.last_seen_at.as_deref(),
        Some("2026-07-10T00:00:00Z")
    );
    assert_eq!(persisted.status, AgentStatus::Online.as_str());
    let current = state.sessions().get(agent_id).await.unwrap();
    assert_eq!(current.token, replacement);
    assert_eq!(current.last_heartbeat_at, "2026-07-10T00:00:00Z");
}

#[tokio::test]
async fn stale_expiry_cannot_clear_replacement_session() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (_old_stream, _old_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let old_token = state.sessions().get(agent_id).await.unwrap().token;
    let mut paused = crate::sessions::transition_pause::install_before(old_token);

    let expiry_state = state.clone();
    let expiry = tokio::spawn(async move {
        expiry_state
            .sessions()
            .expire_stale(
                "2999-01-01T00:00:00Z",
                std::time::Duration::ZERO,
                expiry_state.agents(),
            )
            .await
    });
    paused.wait_until_reached().await;

    let replacement = register_test_session(&state, tenant_id, agent_id).await;
    paused.resume();
    assert!(expiry.await.unwrap().unwrap().is_empty());

    let persisted = persisted_agent(&state, agent_id).await;
    assert_eq!(
        persisted.current_session_id,
        Some(replacement.persisted_id())
    );
    assert_eq!(persisted.status, AgentStatus::Online.as_str());
    assert_eq!(
        state.sessions().get(agent_id).await.unwrap().token,
        replacement
    );
}

#[tokio::test]
async fn old_stream_ack_does_not_mutate_command_after_replacement() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let command_id = sent_command(&state, tenant_id, agent_id).await;
    let (_old_stream, old_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    let (_new_stream, _new_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    old_sender
        .send(Ok(ack_event(tenant_id, agent_id, command_id)))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let err = state
        .commands()
        .mark_sent(command_id, tenant_id, agent_id)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        crate::repositories::RepositoryError::InvalidCommandTransition { from, action }
            if from == CommandStatus::Sent.as_str() && action == "send"
    ));
}

#[tokio::test]
async fn invalid_heartbeat_timestamp_streams_invalid_argument() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    sender
        .send(Ok(heartbeat_event(tenant_id, agent_id, "not-rfc3339")))
        .await
        .unwrap();
    let err = stream.next().await.unwrap().unwrap_err();

    assert_eq!(err.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn replacement_fails_only_replaced_session_pending_live_commands() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (mut old_stream, old_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let old_token = state.sessions().get(agent_id).await.unwrap().token;
    let old_command = link_printer_command(&state, tenant_id, agent_id, "OLD").await;
    state
        .sessions()
        .try_dispatch_live_command(
            tenant_id,
            agent_id,
            old_token,
            old_command,
            link_hub_command(old_command, "OLD"),
        )
        .await
        .unwrap();
    let _ = old_stream.next().await.unwrap().unwrap();

    let (_new_stream, _new_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let new_token = state.sessions().get(agent_id).await.unwrap().token;
    let new_command = link_printer_command(&state, tenant_id, agent_id, "NEW").await;
    state
        .sessions()
        .try_dispatch_live_command(
            tenant_id,
            agent_id,
            new_token,
            new_command,
            link_hub_command(new_command, "NEW"),
        )
        .await
        .unwrap();

    let old_command = state
        .commands()
        .get_for_tenant(tenant_id, old_command)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(old_command.status, CommandStatus::Failed);
    assert_eq!(
        old_command.error.as_deref(),
        Some("agent session replaced before printer operation completed")
    );
    assert_eq!(
        state
            .commands()
            .get_for_tenant(tenant_id, new_command)
            .await
            .unwrap()
            .unwrap()
            .status,
        CommandStatus::Sent
    );

    drop(old_sender);
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert_eq!(
        state.sessions().get(agent_id).await.unwrap().token,
        new_token
    );
}

#[tokio::test]
async fn current_stream_close_fails_pending_live_commands() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let token = state.sessions().get(agent_id).await.unwrap().token;
    let command_id = link_printer_command(&state, tenant_id, agent_id, "CLOSE").await;
    state
        .sessions()
        .try_dispatch_live_command(
            tenant_id,
            agent_id,
            token,
            command_id,
            link_hub_command(command_id, "CLOSE"),
        )
        .await
        .unwrap();
    let _ = stream.next().await.unwrap().unwrap();

    drop(sender);
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let command = state
        .commands()
        .get_for_tenant(tenant_id, command_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(command.status, CommandStatus::Failed);
    assert_eq!(
        command.error.as_deref(),
        Some("agent connection closed before printer operation completed")
    );
}

async fn link_printer_command(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    serial: &str,
) -> CommandId {
    state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: format!("SECRET-{serial}"),
                name: None,
            },
            test_audit_actor(),
        )
        .await
        .unwrap()
        .id
}

fn link_hub_command(command_id: CommandId, serial: &str) -> HubCommand {
    HubCommand {
        command_id: command_id.to_string(),
        command: Some(hub_command::Command::LinkPrinter(LinkPrinter {
            host: "192.0.2.10".to_owned(),
            access_code: format!("SECRET-{serial}"),
            name: String::new(),
            printer_type: "BambuLab".to_owned(),
        })),
    }
}
