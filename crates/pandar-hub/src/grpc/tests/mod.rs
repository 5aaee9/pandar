use pandar_core::{AgentId, AgentStatus, CommandId, TenantId};
use tokio::sync::mpsc;
use tokio_stream::{StreamExt, iter, wrappers::ReceiverStream};
use tonic::{Code, Status};

use super::*;
use crate::protocol::agent::v1::{AgentHeartbeat, agent_event, hub_command};

mod commands;
mod lifecycle;

#[tokio::test]
async fn grpc_non_hello_first_event_rejected() {
    let state = fixture_state().await;
    let err = expect_connect_err(
        connect(
            &state,
            vec![heartbeat_event(
                TenantId::new(),
                AgentId::new(),
                "2026-06-20T00:00:00Z",
            )],
        )
        .await,
    );

    assert_eq!(err.code(), Code::FailedPrecondition);
}

#[tokio::test]
async fn grpc_malformed_ids_rejected() {
    let state = fixture_state().await;
    let err = expect_connect_err(
        connect(
            &state,
            vec![AgentEvent {
                tenant_id: "bad".to_string(),
                agent_id: "bad".to_string(),
                event_id: "event".to_string(),
                event: Some(agent_event::Event::Hello(hello())),
            }],
        )
        .await,
    );

    assert_eq!(err.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn grpc_missing_agent_rejected() {
    let state = fixture_state().await;
    let err = expect_connect_err(
        connect(&state, vec![hello_event(TenantId::new(), AgentId::new())]).await,
    );

    assert_eq!(err.code(), Code::NotFound);
}

#[tokio::test]
async fn grpc_tenant_mismatch_rejected() {
    let state = fixture_state().await;
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let other = state.tenants().create("beta", "Beta Labs").await.unwrap();
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();

    let err = expect_connect_err(connect(&state, vec![hello_event(other.id, agent.id)]).await);

    assert_eq!(err.code(), Code::PermissionDenied);
}

#[tokio::test]
async fn grpc_hello_marks_agent_online() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;

    let (_stream, _sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    let persisted = state.agents().get(agent_id).await.unwrap().unwrap();
    assert_eq!(persisted.status, AgentStatus::Online);
    assert!(state.sessions().get(agent_id).await.is_some());
}

#[tokio::test]
async fn grpc_heartbeat_updates_last_seen_and_session() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;

    let (_stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    sender
        .send(Ok(heartbeat_event(
            tenant_id,
            agent_id,
            "2026-06-20T00:02:00Z",
        )))
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert_eq!(
        state
            .sessions()
            .get(agent_id)
            .await
            .unwrap()
            .last_heartbeat_at,
        "2026-06-20T00:02:00Z"
    );
}

#[tokio::test]
async fn grpc_dispatch_to_online_agent_yields_refresh_and_marks_sent() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (mut stream, _sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    let command = state
        .sessions()
        .dispatch_refresh_printers(tenant_id, agent_id, state.commands())
        .await
        .unwrap();
    let hub_command = stream.next().await.unwrap().unwrap();

    assert_eq!(hub_command.command_id, command.id.to_string());
    assert!(matches!(
        hub_command.command,
        Some(hub_command::Command::RefreshPrinters(_))
    ));
    assert!(
        state
            .commands()
            .next_queued_for_agent(tenant_id, agent_id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn grpc_ack_and_result_update_command_status() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (mut stream, _sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let command = state
        .sessions()
        .dispatch_refresh_printers(tenant_id, agent_id, state.commands())
        .await
        .unwrap();
    let _ = stream.next().await.unwrap().unwrap();

    handle_ack(
        &state,
        tenant_id,
        agent_id,
        CommandAck {
            command_id: command.id.to_string(),
            accepted: true,
            error: String::new(),
        },
    )
    .await
    .unwrap();

    handle_result(
        &state,
        tenant_id,
        agent_id,
        CommandResult {
            command_id: command.id.to_string(),
            success: true,
            error: String::new(),
        },
    )
    .await
    .unwrap();
}

async fn fixture_state() -> AppState {
    AppState::sqlite_for_tests().await.unwrap()
}

pub(super) async fn tenant_agent(state: &AppState) -> (TenantId, AgentId) {
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    (tenant.id, agent.id)
}

pub(super) async fn sent_command(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> CommandId {
    let command = state
        .commands()
        .enqueue_refresh_printers(tenant_id, agent_id)
        .await
        .unwrap();
    state
        .commands()
        .mark_sent(command.id, tenant_id, agent_id)
        .await
        .unwrap();
    command.id
}

async fn connect(state: &AppState, events: Vec<AgentEvent>) -> Result<ResponseStream, Status> {
    AgentControlService::new(state.clone())
        .connect_stream(iter(events.into_iter().map(Ok)))
        .await
}

pub(super) async fn connect_live(
    state: &AppState,
    events: Vec<AgentEvent>,
) -> Result<(ResponseStream, mpsc::Sender<Result<AgentEvent, Status>>), Status> {
    let (sender, receiver) = mpsc::channel(events.len().max(1));
    for event in events {
        sender.send(Ok(event)).await.unwrap();
    }
    let stream = AgentControlService::new(state.clone())
        .connect_stream(ReceiverStream::new(receiver))
        .await?;
    Ok((stream, sender))
}

fn expect_connect_err(result: Result<ResponseStream, Status>) -> Status {
    match result {
        Ok(_) => panic!("expected connect to fail"),
        Err(err) => err,
    }
}

fn hello_event(tenant_id: TenantId, agent_id: AgentId) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "event".to_string(),
        event: Some(agent_event::Event::Hello(hello())),
    }
}

pub(super) fn heartbeat_event(
    tenant_id: TenantId,
    agent_id: AgentId,
    observed_at: &str,
) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "event".to_string(),
        event: Some(agent_event::Event::Heartbeat(AgentHeartbeat {
            observed_at: observed_at.to_string(),
        })),
    }
}

pub(super) fn ack_event(
    tenant_id: TenantId,
    agent_id: AgentId,
    command_id: CommandId,
) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "event".to_string(),
        event: Some(agent_event::Event::CommandAck(CommandAck {
            command_id: command_id.to_string(),
            accepted: true,
            error: String::new(),
        })),
    }
}

pub(super) fn success_event(
    tenant_id: TenantId,
    agent_id: AgentId,
    command_id: CommandId,
) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "event".to_string(),
        event: Some(agent_event::Event::CommandResult(CommandResult {
            command_id: command_id.to_string(),
            success: true,
            error: String::new(),
        })),
    }
}

fn hello() -> AgentHello {
    AgentHello {
        name: "agent".to_string(),
        version: "0.1.0".to_string(),
    }
}
