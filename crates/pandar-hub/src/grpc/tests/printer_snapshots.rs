use tonic::Code;

use super::*;
use crate::protocol::agent::v1::PrinterSnapshot;

#[tokio::test]
async fn grpc_printer_snapshot_persists_printer_state() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (_stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    sender
        .send(Ok(snapshot_event(
            tenant_id,
            agent_id,
            snapshot(" SN-001 ", " X1 Carbon ", " X1C ", " idle "),
        )))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let printers = state.printers().list_for_tenant(tenant_id).await.unwrap();
    assert_eq!(printers.len(), 1);
    assert_eq!(printers[0].agent_id, agent_id);
    assert_eq!(printers[0].serial_number, "SN-001");
    assert_eq!(printers[0].name, "X1 Carbon");
    assert_eq!(printers[0].model.as_deref(), Some("X1C"));
    assert_eq!(printers[0].status, "idle");
    assert!(printers[0].last_seen_at.ends_with('Z'));
}

#[tokio::test]
async fn grpc_printer_snapshot_rejects_empty_serial() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    sender
        .send(Ok(snapshot_event(
            tenant_id,
            agent_id,
            snapshot(" ", "X1 Carbon", "X1C", "idle"),
        )))
        .await
        .unwrap();
    let err = stream.next().await.unwrap().unwrap_err();

    assert_eq!(err.code(), Code::InvalidArgument);
    assert!(
        state
            .printers()
            .list_for_tenant(tenant_id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn stale_replaced_stream_snapshot_does_not_mutate_printer_state() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (_old_stream, old_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let (_new_stream, _new_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    old_sender
        .send(Ok(snapshot_event(
            tenant_id,
            agent_id,
            snapshot("SN-STALE", "Stale Printer", "X1C", "idle"),
        )))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    assert!(
        state
            .printers()
            .list_for_tenant(tenant_id)
            .await
            .unwrap()
            .is_empty()
    );
}

pub(super) fn snapshot(serial: &str, name: &str, model: &str, state: &str) -> PrinterSnapshot {
    PrinterSnapshot {
        serial: serial.to_string(),
        name: name.to_string(),
        model: model.to_string(),
        state: state.to_string(),
    }
}

pub(super) fn snapshot_event(
    tenant_id: TenantId,
    agent_id: AgentId,
    snapshot: PrinterSnapshot,
) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "event".to_string(),
        event: Some(agent_event::Event::PrinterSnapshot(snapshot)),
    }
}
