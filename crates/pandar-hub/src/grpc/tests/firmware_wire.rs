use super::*;
use crate::protocol::agent::v1::{CommandResult, FirmwareCommandResult, agent_event};

#[tokio::test]
async fn firmware_wire_results_enter_typed_validation_before_generic_handling() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let command_id = CommandId::new().to_string();
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    sender
        .send(Ok(AgentEvent {
            tenant_id: tenant_id.to_string(),
            agent_id: agent_id.to_string(),
            event_id: "firmware-result".into(),
            event: Some(agent_event::Event::CommandResult(CommandResult {
                command_id: command_id.clone(),
                success: false,
                error: String::new(),
                result_json: String::new(),
                firmware_result: Some(FirmwareCommandResult {
                    command_id,
                    serial: "SERIAL".into(),
                    generation: 1,
                    transient_status: None,
                    outcome: None,
                }),
            })),
        }))
        .await
        .unwrap();

    let error = stream.next().await.unwrap().unwrap_err();
    assert_eq!(error.code(), Code::InvalidArgument);
    assert!(error.message().contains("firmware result outcome"));
}
