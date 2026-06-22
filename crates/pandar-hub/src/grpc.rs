use std::pin::Pin;

use pandar_core::{AgentId, AgentStatus, TenantId};
use tokio::sync::mpsc;
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
use tonic::{Request, Response, Status};

use crate::{
    AppState,
    grpc::commands::{
        handle_ack_and_job, handle_result_and_job, hub_command_from_record, mark_sent_and_job,
        parse_command_id, repository_status,
    },
    grpc::print_reports::handle_print_report,
    grpc::printer_snapshots::handle_snapshot,
    protocol::agent::v1::{
        AgentEvent, AgentHello, CommandAck, CommandResult, HubCommand,
        agent_control_server::AgentControl, agent_event,
    },
    sessions::{AgentSession, SessionToken},
};

pub mod commands;
pub mod print_reports;
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
            .get(agent_id)
            .await
            .map_err(repository_status)?;
        let Some(agent) = agent else {
            return Err(Status::not_found("agent not found"));
        };
        if agent.tenant_id != tenant_id {
            return Err(Status::permission_denied(
                "agent belongs to a different tenant",
            ));
        }

        let now = pandar_core::created_at_now();
        validate_rfc3339(&now)?;
        self.state
            .agents()
            .update_connection(agent_id, AgentStatus::Online, Some(&hello.version), &now)
            .await
            .map_err(repository_status)?;

        let (wake_sender, wake_receiver) = mpsc::channel(16);
        let (close_sender, close_receiver) = mpsc::channel(1);
        let token = SessionToken::new();
        self.state
            .sessions()
            .register(AgentSession {
                token,
                tenant_id,
                agent_id,
                name: hello.name,
                version: hello.version,
                connected_at: now.clone(),
                last_heartbeat_at: now,
                wake_sender,
                close_sender,
            })
            .await;

        let (command_sender, command_receiver) = mpsc::channel(16);
        let (status_sender, status_receiver) = mpsc::channel(1);
        spawn_inbound_handler(
            self.state.clone(),
            tenant_id,
            agent_id,
            token,
            inbound,
            status_sender,
        );
        spawn_outbound_pump(
            self.state.clone(),
            tenant_id,
            agent_id,
            wake_receiver,
            close_receiver,
            status_receiver,
            command_sender,
        );
        Ok(Box::pin(ReceiverStream::new(command_receiver)))
    }
}

type ResponseStream = Pin<Box<dyn Stream<Item = Result<HubCommand, Status>> + Send>>;

#[tonic::async_trait]
impl AgentControl for AgentControlService {
    type ReverseConnectStream = ResponseStream;

    async fn reverse_connect(
        &self,
        request: Request<tonic::Streaming<AgentEvent>>,
    ) -> Result<Response<Self::ReverseConnectStream>, Status> {
        self.connect_stream(request.into_inner())
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

fn spawn_inbound_handler(
    state: AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    mut inbound: impl Stream<Item = Result<AgentEvent, Status>> + Send + Unpin + 'static,
    status_sender: mpsc::Sender<Status>,
) {
    tokio::spawn(async move {
        while let Some(event) = inbound.next().await {
            let event = match event {
                Ok(event) => event,
                Err(err) => {
                    tracing::error!(error = ?err, "failed to read agent event");
                    let _ = status_sender
                        .send(Status::internal("failed to read agent stream"))
                        .await;
                    break;
                }
            };

            if let Err(err) = handle_event(&state, tenant_id, agent_id, token, event).await {
                tracing::error!(error = ?err, "failed to handle agent event");
                let _ = status_sender.send(err).await;
                break;
            }
        }

        state.sessions().remove_if_current(agent_id, token).await;
    });
}

async fn handle_event(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    event: AgentEvent,
) -> Result<(), Status> {
    match event.event {
        Some(agent_event::Event::Heartbeat(heartbeat)) => {
            validate_rfc3339(&heartbeat.observed_at)?;
            let Some(_) = state
                .sessions()
                .touch_heartbeat_if_current(agent_id, token, &heartbeat.observed_at)
                .await
            else {
                return Ok(());
            };
            state
                .agents()
                .update_connection(agent_id, AgentStatus::Online, None, &heartbeat.observed_at)
                .await
                .map_err(repository_status)?;
            Ok(())
        }
        Some(agent_event::Event::CommandAck(ack)) => {
            match state
                .sessions()
                .while_current(agent_id, token, || {
                    handle_ack(state, tenant_id, agent_id, ack)
                })
                .await
            {
                Some(result) => result,
                None => Ok(()),
            }
        }
        Some(agent_event::Event::CommandResult(result)) => {
            match state
                .sessions()
                .while_current(agent_id, token, || {
                    handle_result(state, tenant_id, agent_id, result)
                })
                .await
            {
                Some(result) => result,
                None => Ok(()),
            }
        }
        Some(agent_event::Event::PrinterSnapshot(snapshot)) => {
            match state
                .sessions()
                .while_current(agent_id, token, || {
                    handle_snapshot(state, tenant_id, agent_id, snapshot)
                })
                .await
            {
                Some(result) => result,
                None => Ok(()),
            }
        }
        Some(agent_event::Event::PrintJobReport(report)) => {
            match state
                .sessions()
                .while_current(agent_id, token, || {
                    handle_print_report(state, tenant_id, agent_id, report)
                })
                .await
            {
                Some(result) => result,
                None => Ok(()),
            }
        }
        Some(agent_event::Event::Hello(_)) | None => Ok(()),
    }
}

async fn handle_ack(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    ack: CommandAck,
) -> Result<(), Status> {
    let command_id = parse_command_id(&ack.command_id)?;
    handle_ack_and_job(
        state,
        tenant_id,
        agent_id,
        command_id,
        ack.accepted,
        ack.error,
    )
    .await
}

async fn handle_result(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    result: CommandResult,
) -> Result<(), Status> {
    let command_id = parse_command_id(&result.command_id)?;
    handle_result_and_job(
        state,
        tenant_id,
        agent_id,
        command_id,
        result.success,
        result.error,
    )
    .await
}

fn spawn_outbound_pump(
    state: AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    mut wake_receiver: mpsc::Receiver<()>,
    mut close_receiver: mpsc::Receiver<()>,
    mut status_receiver: mpsc::Receiver<Status>,
    command_sender: mpsc::Sender<Result<HubCommand, Status>>,
) {
    tokio::spawn(async move {
        loop {
            if !drain_commands(
                &state,
                tenant_id,
                agent_id,
                &mut close_receiver,
                &command_sender,
            )
            .await
            {
                break;
            }
            tokio::select! {
                biased;
                Some(()) = close_receiver.recv() => break,
                Some(status) = status_receiver.recv() => {
                    let _ = command_sender.send(Err(status)).await;
                    break;
                }
                Some(()) = wake_receiver.recv() => {}
                else => break,
            }
        }
    });
}

async fn drain_commands(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    close_receiver: &mut mpsc::Receiver<()>,
    command_sender: &mpsc::Sender<Result<HubCommand, Status>>,
) -> bool {
    loop {
        let command = match tokio::select! {
            biased;
            Some(()) = close_receiver.recv() => return false,
            command = state.commands().next_queued_for_agent(tenant_id, agent_id) => command,
        } {
            Ok(Some(command)) => command,
            Ok(None) => return true,
            Err(err) => return send_error(command_sender, repository_status(err)).await,
        };

        let command = match tokio::select! {
            biased;
            Some(()) = close_receiver.recv() => return false,
            command = mark_sent_and_job(state, command, tenant_id, agent_id) => command,
        } {
            Ok(command) => command,
            Err(err) => return send_error(command_sender, err).await,
        };

        let command = match hub_command_from_record(command) {
            Ok(command) => command,
            Err(err) => return send_error(command_sender, err).await,
        };

        if command_sender.send(Ok(command)).await.is_err() {
            return false;
        }
    }
}

async fn send_error(
    command_sender: &mpsc::Sender<Result<HubCommand, Status>>,
    status: Status,
) -> bool {
    command_sender.send(Err(status)).await.is_ok()
}

fn validate_rfc3339(value: &str) -> Result<(), Status> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .map(|_| ())
        .map_err(|_| Status::invalid_argument("timestamp must be RFC3339"))
}
