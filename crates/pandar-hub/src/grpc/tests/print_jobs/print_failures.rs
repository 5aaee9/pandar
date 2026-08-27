use pandar_core::{
    CommandId, CommandRecord, CommandStatus, PrintTransferFailure, PrintTransferPhase, TenantId,
};
use pandar_protocol::agent::v1::{CommandResult, agent_event};

use super::*;

const ACCESS_CODE: &str = "ACCESS-CODE-UNIQUE";

async fn wait_for_command_status(
    state: &AppState,
    tenant_id: TenantId,
    command_id: CommandId,
    expected: CommandStatus,
) -> CommandRecord {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let command = state
                .commands()
                .get_for_tenant(tenant_id, command_id)
                .await
                .unwrap()
                .unwrap();
            if command.status == expected {
                return command;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("command reached expected status")
}

#[tokio::test]
async fn grpc_print_transfer_failure_redacts_and_persists_phase_with_complete_cause() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let created = create_print_job(&state, tenant_id, agent_id, "artifact-transfer").await;
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let _ = stream.next().await.unwrap().unwrap();
    sender
        .send(Ok(ack_event(tenant_id, agent_id, created.job.command_id)))
        .await
        .unwrap();

    let cause = format!(
        "dispatch print job: start protected upload with access_code={ACCESS_CODE}: 522 SSL connection failed: session reuse required"
    );
    sender
        .send(Ok(AgentEvent {
            tenant_id: tenant_id.to_string(),
            agent_id: agent_id.to_string(),
            event_id: "transfer-failure".to_owned(),
            event: Some(agent_event::Event::CommandResult(CommandResult {
                command_id: created.job.command_id.to_string(),
                success: false,
                error: cause.clone(),
                result_json: serde_json::to_string(&PrintTransferFailure {
                    phase: PrintTransferPhase::DataConnection,
                    cause,
                })
                .unwrap(),
                firmware_result: None,
            })),
        }))
        .await
        .unwrap();
    let command = wait_for_command_status(
        &state,
        tenant_id,
        created.job.command_id,
        CommandStatus::Failed,
    )
    .await;
    assert_eq!(command.status, CommandStatus::Failed);
    let error = command.error.unwrap();
    assert!(error.contains("522 SSL connection failed"));
    assert!(error.contains("[redacted]"));
    assert!(!error.contains(ACCESS_CODE));
    let failure =
        serde_json::from_str::<PrintTransferFailure>(command.result_json.as_deref().unwrap())
            .unwrap();
    assert_eq!(failure.phase, PrintTransferPhase::DataConnection);
    assert_eq!(failure.cause, error);

    let job = state
        .jobs()
        .get_for_tenant(tenant_id, created.job.id)
        .await
        .unwrap()
        .unwrap()
        .job;
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error.as_deref(), Some(error.as_str()));
}

#[tokio::test]
async fn grpc_generic_print_failure_keeps_empty_result_json() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let created = create_print_job(&state, tenant_id, agent_id, "artifact-generic").await;
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let _ = stream.next().await.unwrap().unwrap();
    sender
        .send(Ok(ack_event(tenant_id, agent_id, created.job.command_id)))
        .await
        .unwrap();
    sender
        .send(Ok(AgentEvent {
            tenant_id: tenant_id.to_string(),
            agent_id: agent_id.to_string(),
            event_id: "generic-failure".to_owned(),
            event: Some(agent_event::Event::CommandResult(CommandResult {
                command_id: created.job.command_id.to_string(),
                success: false,
                error: "generic dispatch failure".to_owned(),
                result_json: String::new(),
                firmware_result: None,
            })),
        }))
        .await
        .unwrap();
    let command = wait_for_command_status(
        &state,
        tenant_id,
        created.job.command_id,
        CommandStatus::Failed,
    )
    .await;
    assert_eq!(command.status, CommandStatus::Failed);
    assert_eq!(command.error.as_deref(), Some("generic dispatch failure"));
    assert!(command.result_json.is_none());
}
