use std::time::Duration;

use axum::body::Bytes;
use futures_util::StreamExt;

use super::*;
use pandar_protocol::agent::v1::{
    AgentCameraChunk, AgentCameraClosed, AgentCameraEvent, AgentCameraHello, agent_camera_event,
    hub_command,
};

#[tokio::test]
async fn reverse_camera_events_cannot_modify_another_agents_stream() {
    let state = fixture_state().await;
    let (tenant_id, owner_id) = tenant_agent(&state).await;
    let attacker = paired_agent(&state, tenant_id, "attacker").await;
    let (_owner_control_stream, _owner_control_sender) = connect_live(
        &state,
        vec![hello_event_with_credential(
            tenant_id,
            owner_id,
            TEST_AGENT_CREDENTIAL,
        )],
    )
    .await
    .unwrap();
    let (_attacker_control_stream, _attacker_control_sender) = connect_live(
        &state,
        vec![hello_event_with_credential(
            tenant_id,
            attacker.id,
            TEST_AGENT_CREDENTIAL,
        )],
    )
    .await
    .unwrap();
    let (command_sender, mut command_receiver) = mpsc::channel(1);
    let mut camera = state
        .camera_sessions()
        .open_stream(tenant_id, owner_id, "SERIAL-1".to_owned(), command_sender)
        .await
        .unwrap();
    let stream_id = match command_receiver.recv().await.unwrap().unwrap().command {
        Some(hub_command::Command::CameraStream(command)) => command.stream_id,
        other => panic!("expected camera stream command, got {other:?}"),
    };

    let (_attacker_stream, attacker_sender) =
        connect_camera_live(&state, camera_hello_event(tenant_id, attacker.id)).await;
    attacker_sender
        .send(Ok(AgentCameraEvent {
            tenant_id: tenant_id.to_string(),
            agent_id: owner_id.to_string(),
            event_id: "injected-chunk".to_owned(),
            event: Some(agent_camera_event::Event::Chunk(AgentCameraChunk {
                stream_id: stream_id.clone(),
                data: b"injected".to_vec(),
            })),
        }))
        .await
        .unwrap();
    attacker_sender
        .send(Ok(AgentCameraEvent {
            tenant_id: tenant_id.to_string(),
            agent_id: owner_id.to_string(),
            event_id: "injected-close".to_owned(),
            event: Some(agent_camera_event::Event::Closed(AgentCameraClosed {
                stream_id: stream_id.clone(),
                success: false,
                error: "injected".to_owned(),
            })),
        }))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;

    let (_owner_stream, owner_sender) =
        connect_camera_live(&state, camera_hello_event(tenant_id, owner_id)).await;
    owner_sender
        .send(Ok(AgentCameraEvent {
            tenant_id: tenant_id.to_string(),
            agent_id: owner_id.to_string(),
            event_id: "owner-chunk".to_owned(),
            event: Some(agent_camera_event::Event::Chunk(AgentCameraChunk {
                stream_id,
                data: b"owner".to_vec(),
            })),
        }))
        .await
        .unwrap();

    let chunk = tokio::time::timeout(Duration::from_secs(1), camera.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(chunk, Bytes::from_static(b"owner"));
}

#[tokio::test]
async fn oversized_camera_chunk_terminates_the_http_stream() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (command_sender, mut command_receiver) = mpsc::channel(1);
    let mut camera = state
        .camera_sessions()
        .open_stream(tenant_id, agent_id, "SERIAL-1".to_owned(), command_sender)
        .await
        .unwrap();
    let stream_id = match command_receiver.recv().await.unwrap().unwrap().command {
        Some(hub_command::Command::CameraStream(command)) => command.stream_id,
        other => panic!("expected camera stream command, got {other:?}"),
    };

    let keep_stream = handle_camera_event(
        &state,
        agent_id,
        AgentCameraEvent {
            tenant_id: tenant_id.to_string(),
            agent_id: agent_id.to_string(),
            event_id: "oversized-chunk".to_owned(),
            event: Some(agent_camera_event::Event::Chunk(AgentCameraChunk {
                stream_id,
                data: vec![0; crate::camera_sessions::MAX_CAMERA_CHUNK_BYTES + 1],
            })),
        },
    )
    .await;

    assert!(!keep_stream);
    assert!(camera.next().await.unwrap().is_err());
}

async fn connect_camera_live(
    state: &AppState,
    hello: AgentCameraEvent,
) -> (
    CameraResponseStream,
    mpsc::Sender<Result<AgentCameraEvent, Status>>,
) {
    let (sender, receiver) = mpsc::channel(4);
    sender.send(Ok(hello)).await.unwrap();
    let stream = AgentControlService::new(state.clone())
        .connect_camera_stream(ReceiverStream::new(receiver), None)
        .await
        .unwrap();
    (stream, sender)
}

fn camera_hello_event(tenant_id: TenantId, agent_id: AgentId) -> AgentCameraEvent {
    AgentCameraEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "camera-hello".to_owned(),
        event: Some(agent_camera_event::Event::Hello(AgentCameraHello {
            credential: TEST_AGENT_CREDENTIAL.to_owned(),
        })),
    }
}
