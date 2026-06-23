use tokio_stream::StreamExt;
use tonic::Code;

use super::*;
use pandar_core::CommandStatus;

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
