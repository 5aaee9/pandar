use std::{pin::Pin, sync::Arc};

use pandar_core::{AgentId, TenantId};
use tokio::sync::mpsc;
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
use tonic::{Request, Response, Status};

#[cfg(test)]
use crate::grpc::print_reports::handle_print_report;
#[cfg(test)]
use crate::grpc::printer_snapshots::handle_snapshot;
use inbound::spawn_inbound_handler;
#[cfg(test)]
use inbound::{disconnect_session, handle_ack, handle_event, handle_result};

#[cfg(test)]
use crate::protocol::agent::v1::CommandResult;
use crate::{
    AppState,
    grpc::commands::repository_status,
    grpc::outbound::{OutboundSession, spawn_outbound_pump},
    protocol::agent::v1::{
        AgentCameraEvent, AgentCameraHello, AgentCapability, AgentEvent, AgentHello,
        HubCameraCommand, HubCommand, agent_camera_event, agent_control_server::AgentControl,
        agent_event,
    },
    repositories::hash_secret,
    sessions::{
        AgentSession, SessionToken, empty_pending_live_commands,
        live_commands::fail_pending_live_commands,
    },
};

pub mod commands;
mod inbound;
mod outbound;
pub mod print_reports;
mod printer_firmware;
#[cfg(test)]
pub(crate) use printer_firmware::completion_pause as firmware_completion_pause;
pub mod printer_materials;
pub mod printer_snapshots;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub struct AgentControlService {
    state: AppState,
}

impl AgentControlService {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    async fn connect_stream<S>(&self, mut inbound: S) -> Result<ResponseStream, Status>
    where
        S: Stream<Item = Result<AgentEvent, Status>> + Send + Unpin + 'static,
    {
        let first = inbound
            .next()
            .await
            .transpose()
            .map_err(|err| {
                tracing::error!(error = ?err, "failed to read agent hello");
                Status::internal("failed to read agent stream")
            })?
            .ok_or_else(|| Status::failed_precondition("first event must be AgentHello"))?;

        let (tenant_id, agent_id, hello) = parse_hello(first)?;
        let agent = self
            .state
            .agents()
            .get_credential_record(agent_id)
            .await
            .map_err(repository_status)?;
        let Some(agent) = agent else {
            return Err(Status::not_found("agent not found"));
        };
        if agent.agent.tenant_id != tenant_id {
            return Err(Status::permission_denied(
                "agent belongs to a different tenant",
            ));
        }
        if agent.credential_hash.as_deref() != Some(hash_secret(&hello.credential).as_str())
            || agent.credential_revoked_at.is_some()
        {
            return Err(Status::unauthenticated("invalid agent credential"));
        }

        let now = pandar_core::created_at_now();
        validate_rfc3339(&now)?;
        let (wake_sender, wake_receiver) = mpsc::channel(16);
        let (close_sender, close_receiver) = mpsc::channel(1);
        let (command_sender, command_receiver) = mpsc::channel(16);
        let token = SessionToken::new();
        let capabilities = hello
            .capabilities
            .into_iter()
            .filter_map(|value| AgentCapability::try_from(value).ok())
            .collect();
        let session = AgentSession {
            token,
            tenant_id,
            agent_id,
            name: hello.name,
            version: hello.version,
            connected_at: now.clone(),
            last_heartbeat_at: now.clone(),
            wake_sender,
            close_sender,
            command_sender: command_sender.clone(),
            capabilities,
            pending_live_commands: empty_pending_live_commands(),
            live_command_transition: Arc::new(tokio::sync::Mutex::new(())),
        };
        let replaced = {
            let _lease = self
                .state
                .sessions()
                .transition_lease_for_session(agent_id, token)
                .await;
            self.state
                .agents()
                .claim_online_session(
                    tenant_id,
                    agent_id,
                    &token.persisted_id(),
                    &session.version,
                    &now,
                )
                .await
                .map_err(repository_status)?;
            self.state.sessions().register(session).await
        };
        if let Some(session) = replaced {
            fail_pending_live_commands(
                &self.state,
                tenant_id,
                agent_id,
                session,
                "agent session replaced before printer operation completed",
            )
            .await;
        }

        let (status_sender, status_receiver) = mpsc::channel(1);
        spawn_inbound_handler(
            self.state.clone(),
            tenant_id,
            agent_id,
            token,
            inbound,
            status_sender,
        );
        let outbound_ready = spawn_outbound_pump(
            self.state.clone(),
            OutboundSession {
                tenant_id,
                agent_id,
                token,
            },
            wake_receiver,
            close_receiver,
            status_receiver,
            command_sender,
        );
        outbound_ready.await.map_err(|err| {
            tracing::error!(error = ?err, "agent outbound pump stopped before becoming ready");
            Status::internal("failed to start agent outbound pump")
        })?;
        Ok(Box::pin(ReceiverStream::new(command_receiver)))
    }

    async fn connect_camera_stream<S>(&self, mut inbound: S) -> Result<CameraResponseStream, Status>
    where
        S: Stream<Item = Result<AgentCameraEvent, Status>> + Send + Unpin + 'static,
    {
        let first = inbound
            .next()
            .await
            .transpose()
            .map_err(|err| {
                tracing::error!(error = ?err, "failed to read agent camera hello");
                Status::internal("failed to read agent camera stream")
            })?
            .ok_or_else(|| Status::failed_precondition("first event must be AgentCameraHello"))?;

        let (tenant_id, agent_id, hello) = parse_camera_hello(first)?;
        let agent = self
            .state
            .agents()
            .get_credential_record(agent_id)
            .await
            .map_err(repository_status)?;
        let Some(agent) = agent else {
            return Err(Status::not_found("agent not found"));
        };
        if agent.agent.tenant_id != tenant_id {
            return Err(Status::permission_denied(
                "agent belongs to a different tenant",
            ));
        }
        if agent.credential_hash.as_deref() != Some(hash_secret(&hello.credential).as_str())
            || agent.credential_revoked_at.is_some()
        {
            return Err(Status::unauthenticated("invalid agent credential"));
        }

        let (keepalive_sender, command_receiver) = mpsc::channel(1);
        let state = self.state.clone();
        tokio::spawn(async move {
            let _keepalive_sender = keepalive_sender;
            while let Some(event) = inbound.next().await {
                match event {
                    Ok(event) => handle_camera_event(&state, agent_id, event).await,
                    Err(err) => {
                        tracing::warn!(error = ?err, "agent camera stream ended with error");
                        break;
                    }
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(command_receiver)))
    }
}

#[cfg(test)]
pub(crate) async fn handle_event_for_tests(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    event: AgentEvent,
) -> Result<(), Status> {
    inbound::handle_event(state, tenant_id, agent_id, token, event).await
}

type ResponseStream = Pin<Box<dyn Stream<Item = Result<HubCommand, Status>> + Send>>;
type CameraResponseStream = Pin<Box<dyn Stream<Item = Result<HubCameraCommand, Status>> + Send>>;

#[tonic::async_trait]
impl AgentControl for AgentControlService {
    type ReverseConnectStream = ResponseStream;
    type ReverseCameraStream = CameraResponseStream;

    async fn reverse_connect(
        &self,
        request: Request<tonic::Streaming<AgentEvent>>,
    ) -> Result<Response<Self::ReverseConnectStream>, Status> {
        self.connect_stream(request.into_inner())
            .await
            .map(Response::new)
    }

    async fn reverse_camera(
        &self,
        request: Request<tonic::Streaming<AgentCameraEvent>>,
    ) -> Result<Response<Self::ReverseCameraStream>, Status> {
        self.connect_camera_stream(request.into_inner())
            .await
            .map(Response::new)
    }
}

fn parse_hello(event: AgentEvent) -> Result<(TenantId, AgentId, AgentHello), Status> {
    let tenant_id = TenantId::parse(&event.tenant_id)
        .map_err(|_| Status::invalid_argument("tenant_id must be a UUID"))?;
    let agent_id = AgentId::parse(&event.agent_id)
        .map_err(|_| Status::invalid_argument("agent_id must be a UUID"))?;
    let Some(agent_event::Event::Hello(hello)) = event.event else {
        return Err(Status::failed_precondition(
            "first event must be AgentHello",
        ));
    };

    Ok((tenant_id, agent_id, hello))
}

fn parse_camera_hello(
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

async fn handle_camera_event(state: &AppState, agent_id: AgentId, event: AgentCameraEvent) {
    match event.event {
        Some(agent_camera_event::Event::Chunk(chunk)) => {
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
}

fn validate_rfc3339(value: &str) -> Result<(), Status> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .map(|_| ())
        .map_err(|_| Status::invalid_argument("timestamp must be RFC3339"))
}

#[cfg(test)]
pub(crate) async fn register_test_session(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> SessionToken {
    let token = SessionToken::new();
    register_test_session_with_token(state, tenant_id, agent_id, token).await;
    token
}

#[cfg(test)]
pub(crate) async fn register_test_session_with_token(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
) {
    let now = "2026-07-10T00:00:00Z";
    let (wake_sender, _) = mpsc::channel(1);
    let (close_sender, _) = mpsc::channel(1);
    let (command_sender, _) = mpsc::channel(1);
    let _lease = state
        .sessions()
        .transition_lease_for_session(agent_id, token)
        .await;
    state
        .agents()
        .claim_online_session(tenant_id, agent_id, &token.persisted_id(), "test", now)
        .await
        .unwrap();
    state
        .sessions()
        .register(AgentSession {
            token,
            tenant_id,
            agent_id,
            name: "test agent".to_owned(),
            version: "test".to_owned(),
            connected_at: now.to_owned(),
            last_heartbeat_at: now.to_owned(),
            wake_sender,
            close_sender,
            command_sender,
            capabilities: std::collections::HashSet::new(),
            pending_live_commands: empty_pending_live_commands(),
            live_command_transition: Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;
}
