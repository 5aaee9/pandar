use pandar_core::{AgentId, TenantId};
use tokio::sync::mpsc;
use tonic::Status;

use crate::{
    AppState, grpc::commands::next_hub_command_for_agent, protocol::agent::v1::HubCommand,
};

pub(super) fn spawn_outbound_pump(
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
        let hub_command = match tokio::select! {
            biased;
            Some(()) = close_receiver.recv() => return false,
            command = next_hub_command_for_agent(state, tenant_id, agent_id) => command,
        } {
            Ok(Some(command)) => command,
            Ok(None) => return true,
            Err(err) => return send_error(command_sender, err).await,
        };

        if command_sender.send(Ok(hub_command)).await.is_err() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        grpc::commands::{CommandConversionOptions, next_hub_command_for_agent_with_options},
        repositories::PrintProjectFilePayload,
    };
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
                flow_cali: false,
                timelapse: true,
                ams_mapping_json: None,
                ams_mapping2_json: None,
            })
            .await
            .unwrap();
        replace_payload_without_download_path(&state, job.job.command_id).await;

        let err = next_hub_command_for_agent_with_options(
            &state,
            tenant.id,
            agent.id,
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
            flow_cali: false,
            timelapse: true,
            ams_mapping_json: None,
            ams_mapping2_json: None,
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
