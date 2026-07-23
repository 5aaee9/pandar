use pandar_core::{AgentId, TenantId};
use tokio::sync::{mpsc, oneshot};
use tonic::Status;

use crate::{
    AppState,
    grpc::commands::{
        CommandConversionOptions, SessionQueuedDispatch, dispatch_next_queued_for_session,
    },
    protocol::agent::v1::HubCommand,
    sessions::SessionToken,
};

#[derive(Clone, Copy)]
pub(super) struct OutboundSession {
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub token: SessionToken,
}

pub(super) fn spawn_outbound_pump(
    state: AppState,
    session: OutboundSession,
    mut wake_receiver: mpsc::Receiver<()>,
    mut close_receiver: mpsc::Receiver<()>,
    mut status_receiver: mpsc::Receiver<Status>,
    command_sender: mpsc::Sender<Result<HubCommand, Status>>,
) -> oneshot::Receiver<()> {
    let (ready_sender, ready_receiver) = oneshot::channel();
    tokio::spawn(async move {
        let mut ready_sender = Some(ready_sender);
        let OutboundSession {
            tenant_id,
            agent_id,
            token,
        } = session;
        loop {
            let keep_running = drain_commands(
                &state,
                session,
                &mut close_receiver,
                &mut status_receiver,
                &mut ready_sender,
                &command_sender,
            )
            .await;
            if !keep_running {
                break;
            }
            tokio::select! {
                biased;
                _ = close_receiver.recv() => {
                    finalize_closing_session(
                        &state, tenant_id, agent_id, token,
                    ).await;
                    if let Ok(status) = status_receiver.try_recv() {
                        let _ = command_sender.send(Err(status)).await;
                    }
                    break;
                },
                Some(status) = status_receiver.recv() => {
                    let _ = status_receiver.recv().await;
                    if !state.sessions().is_current(agent_id, token).await {
                        finalize_closing_session(
                            &state, tenant_id, agent_id, token,
                        ).await;
                    }
                    let _ = command_sender.send(Err(status)).await;
                    break;
                }
                Some(()) = wake_receiver.recv() => {}
                else => break,
            }
        }
    });
    ready_receiver
}

async fn drain_commands(
    state: &AppState,
    session: OutboundSession,
    close_receiver: &mut mpsc::Receiver<()>,
    status_receiver: &mut mpsc::Receiver<Status>,
    ready_sender: &mut Option<oneshot::Sender<()>>,
    command_sender: &mpsc::Sender<Result<HubCommand, Status>>,
) -> bool {
    let OutboundSession {
        tenant_id,
        agent_id,
        token,
    } = session;
    loop {
        let dispatch = match tokio::select! {
            biased;
            _ = close_receiver.recv() => {
                finalize_closing_session(
                    state, tenant_id, agent_id, token,
                ).await;
                if let Ok(status) = status_receiver.try_recv() {
                    signal_ready(ready_sender);
                    return send_error(command_sender, status).await;
                }
                signal_ready(ready_sender);
                return false;
            },
            dispatch = dispatch_next_queued_for_session(
                state,
                tenant_id,
                agent_id,
                token,
                command_sender,
                conversion_options(state),
            ) => dispatch,
        } {
            Ok(dispatch) => {
                signal_ready(ready_sender);
                dispatch
            }
            Err(err) => {
                signal_ready(ready_sender);
                return send_error(command_sender, err).await;
            }
        };
        match dispatch {
            SessionQueuedDispatch::Sent | SessionQueuedDispatch::FailedAndContinue => {}
            SessionQueuedDispatch::Empty => return true,
            SessionQueuedDispatch::SessionEnded => {
                if let Ok(status) = status_receiver.try_recv() {
                    return send_error(command_sender, status).await;
                }
                return false;
            }
            SessionQueuedDispatch::ChannelClosed => return false,
        }
    }
}

fn signal_ready(ready_sender: &mut Option<oneshot::Sender<()>>) {
    if let Some(ready_sender) = ready_sender.take() {
        let _ = ready_sender.send(());
    }
}

async fn finalize_closing_session(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
) {
    if let Err(status) = super::commands::finalize_required_features_for_closing_session(
        state, tenant_id, agent_id, token,
    )
    .await
    {
        tracing::error!(
            code = ?status.code(),
            error = %status,
            "failed to finalize queued command for closing agent session"
        );
    }
}

fn conversion_options(state: &AppState) -> CommandConversionOptions {
    CommandConversionOptions {
        require_artifact_download_path: state.artifact_storage().backend().requires_hub_fetch(),
    }
}

async fn send_error(
    command_sender: &mpsc::Sender<Result<HubCommand, Status>>,
    status: Status,
) -> bool {
    command_sender.send(Err(status)).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::PrintProjectFilePayload;
    use pandar_core::{CommandStatus, JobStatus};

    #[tokio::test]
    async fn strict_missing_artifact_path_does_not_mark_print_sent() {
        let state = AppState::sqlite_for_tests().await.unwrap();
        let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
        let agent = state.agents().create(tenant.id, "agent").await.unwrap();
        let printer_id = crate::repositories::test_helpers::insert_printer_fixture(
            state.database(),
            tenant.id,
            agent.id,
        )
        .await
        .unwrap();
        let job = state
            .jobs()
            .create_print_job(crate::repositories::CreatePrintJob {
                tenant_id: tenant.id,
                printer_id: printer_id.clone(),
                agent_id: agent.id,
                artifact_id: "artifact-1".to_string(),
                artifact_filename: "plate.3mf".to_string(),
                artifact_content_type: "model/3mf".to_string(),
                artifact_size_bytes: 42,
                artifact_storage_path: format!("{}/artifact-1/plate.3mf", tenant.id),
                artifact_metadata_json: None,
                plate_id: 1,
                use_ams: true,
                bed_leveling: false,
                auto_bed_leveling: pandar_core::PrintCalibrationMode::Off,
                flow_cali: false,
                auto_flow_cali: pandar_core::PrintCalibrationMode::Off,
                auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
                timelapse: true,
                ams_mapping_json: None,
                ams_mapping2_json: None,
                ams_mapping_info_json: None,
            })
            .await
            .unwrap();
        replace_payload_without_download_path(&state, job.job.command_id).await;

        let token = crate::grpc::register_test_session(&state, tenant.id, agent.id).await;
        let (command_sender, _command_receiver) = mpsc::channel(1);
        let err = dispatch_next_queued_for_session(
            &state,
            tenant.id,
            agent.id,
            token,
            &command_sender,
            CommandConversionOptions {
                require_artifact_download_path: true,
            },
        )
        .await
        .unwrap_err();

        assert_eq!(err.message(), "missing artifact download path");
        assert_eq!(
            state
                .commands()
                .get_for_tenant(tenant.id, job.job.command_id)
                .await
                .unwrap()
                .unwrap()
                .status,
            CommandStatus::Queued
        );
        assert_eq!(
            state
                .jobs()
                .get_for_tenant(tenant.id, job.job.id)
                .await
                .unwrap()
                .unwrap()
                .job
                .status,
            JobStatus::Queued
        );
    }

    async fn replace_payload_without_download_path(
        state: &AppState,
        command_id: pandar_core::CommandId,
    ) {
        let payload = PrintProjectFilePayload {
            job_id: "job-1".to_string(),
            artifact_id: "artifact-1".to_string(),
            printer_id: "printer-1".to_string(),
            serial_number: "serial".to_string(),
            filename: "plate.3mf".to_string(),
            storage_path: "tenant/artifact/plate.3mf".to_string(),
            artifact_download_path: String::new(),
            size_bytes: 42,
            plate_id: 1,
            use_ams: true,
            bed_leveling: false,
            auto_bed_leveling: pandar_core::PrintCalibrationMode::Off,
            flow_cali: false,
            auto_flow_cali: pandar_core::PrintCalibrationMode::Off,
            auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
            timelapse: true,
            ams_mapping_json: None,
            ams_mapping2_json: None,
            ams_mapping_info_json: None,
            studio_submission_id: crate::test_support::studio_submission_id_for_tests(),
            studio_metadata: Some(crate::test_support::studio_metadata_for_tests()),
        };
        let payload_json = serde_json::to_string(&payload).unwrap();
        let crate::db::Database::Sqlite(pool) = state.database() else {
            panic!("expected SQLite database");
        };
        sqlx::query("UPDATE commands SET payload_json = ?2 WHERE id = ?1")
            .bind(command_id.to_string())
            .bind(payload_json)
            .execute(pool)
            .await
            .unwrap();
    }
}
