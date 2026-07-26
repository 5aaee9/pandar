use pandar_core::{AgentId, TenantId};
use tonic::Status;

use crate::{
    AppState,
    protocol::agent::v1::{AgentCameraEvent, AgentCameraHello, agent_camera_event},
};

pub(super) fn parse_camera_hello(
    event: AgentCameraEvent,
) -> Result<(TenantId, AgentId, AgentCameraHello), Status> {
    let tenant_id = TenantId::parse(&event.tenant_id)
        .map_err(|_| Status::invalid_argument("tenant_id must be a UUID"))?;
    let agent_id = AgentId::parse(&event.agent_id)
        .map_err(|_| Status::invalid_argument("agent_id must be a UUID"))?;
    let Some(agent_camera_event::Event::Hello(hello)) = event.event else {
        return Err(Status::failed_precondition(
            "first event must be AgentCameraHello",
        ));
    };

    Ok((tenant_id, agent_id, hello))
}

pub(super) async fn handle_camera_event(
    state: &AppState,
    agent_id: AgentId,
    event: AgentCameraEvent,
) -> bool {
    match event.event {
        Some(agent_camera_event::Event::Chunk(chunk)) => {
            if chunk.data.len() > crate::camera_sessions::MAX_CAMERA_CHUNK_BYTES {
                tracing::warn!(
                    size_bytes = chunk.data.len(),
                    max_bytes = crate::camera_sessions::MAX_CAMERA_CHUNK_BYTES,
                    "agent camera chunk exceeds size limit"
                );
                state
                    .camera_sessions()
                    .close_stream(
                        agent_id,
                        &chunk.stream_id,
                        false,
                        "camera_chunk_too_large".to_owned(),
                    )
                    .await;
                return false;
            }
            state
                .camera_sessions()
                .push_chunk(
                    agent_id,
                    &chunk.stream_id,
                    axum::body::Bytes::from(chunk.data),
                )
                .await;
        }
        Some(agent_camera_event::Event::Closed(closed)) => {
            state
                .camera_sessions()
                .close_stream(agent_id, &closed.stream_id, closed.success, closed.error)
                .await;
        }
        Some(agent_camera_event::Event::Hello(_)) | None => {}
    }
    true
}
