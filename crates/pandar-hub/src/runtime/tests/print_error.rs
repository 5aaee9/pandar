use std::{collections::HashSet, sync::Arc, time::Duration};

use pandar_core::{AgentId, CommandId, CommandRecord, CommandStatus, TenantId};
use tokio::sync::{Mutex, mpsc};

use super::*;
use crate::{
    protocol::agent::v1::AgentCapability,
    repositories::{PrintErrorAction, PrinterOperationKind},
    sessions::{AgentSession, PendingLiveCommand, SessionToken, empty_pending_live_commands},
};

#[tokio::test]
async fn local_close_cleans_exact_removed_session_and_preserves_replacement() {
    let (state, tenant_id, agent_id) = fixture("local-close").await;
    let old_command = native(&state, tenant_id, agent_id, 20_042).await;
    let replacement_command = native(&state, tenant_id, agent_id, 20_043).await;
    let mut old_session = register_pending_session(
        &state,
        tenant_id,
        agent_id,
        old_command.id,
        "2026-07-10T00:00:00Z",
    )
    .await;
    let transition = old_session.transition.clone().lock_owned().await;

    let close_state = state.clone();
    let close = tokio::spawn(async move {
        close_state.close_agent(tenant_id, agent_id).await;
    });
    wait_for_close(&mut old_session.close_receiver).await;
    assert!(state.sessions().get(agent_id).await.is_none());

    let replacement = register_pending_session(
        &state,
        tenant_id,
        agent_id,
        replacement_command.id,
        "2026-07-10T00:00:01Z",
    )
    .await;
    drop(transition);
    close.await.unwrap();

    assert_failed(
        &state,
        tenant_id,
        old_command.id,
        "agent session closed before printer operation completed",
    )
    .await;
    assert_replacement_preserved(
        &state,
        tenant_id,
        agent_id,
        replacement.token,
        replacement_command.id,
    )
    .await;
}

#[tokio::test]
async fn cluster_close_cleans_exact_removed_session_and_preserves_replacement() {
    let (state, tenant_id, agent_id) = fixture("cluster-close").await;
    let sibling = state.sibling_for_tests();
    let (control_plane, ready) = spawn_control_plane_ready(sibling.clone());
    ready.await.unwrap().unwrap();
    let old_command = native(&state, tenant_id, agent_id, 20_042).await;
    let replacement_command = native(&state, tenant_id, agent_id, 20_043).await;
    let mut old_session = register_pending_session(
        &sibling,
        tenant_id,
        agent_id,
        old_command.id,
        "2026-07-10T00:00:00Z",
    )
    .await;
    let transition = old_session.transition.clone().lock_owned().await;

    state.close_agent(tenant_id, agent_id).await;
    wait_for_close(&mut old_session.close_receiver).await;
    assert!(sibling.sessions().get(agent_id).await.is_none());
    let replacement = register_pending_session(
        &sibling,
        tenant_id,
        agent_id,
        replacement_command.id,
        "2026-07-10T00:00:01Z",
    )
    .await;
    drop(transition);

    assert_failed(
        &state,
        tenant_id,
        old_command.id,
        "agent session closed before printer operation completed",
    )
    .await;
    assert_replacement_preserved(
        &sibling,
        tenant_id,
        agent_id,
        replacement.token,
        replacement_command.id,
    )
    .await;
    control_plane.abort();
}

#[tokio::test]
async fn stale_expiry_cleans_exact_removed_session_and_preserves_replacement() {
    let (state, tenant_id, agent_id) = fixture("stale-expiry").await;
    state
        .agents()
        .update_connection(
            agent_id,
            pandar_core::AgentStatus::Online,
            Some("0.1.0"),
            "2026-07-10T00:00:00Z",
        )
        .await
        .unwrap();
    let old_command = native(&state, tenant_id, agent_id, 20_042).await;
    let replacement_command = native(&state, tenant_id, agent_id, 20_043).await;
    let old_session = register_pending_session(
        &state,
        tenant_id,
        agent_id,
        old_command.id,
        "2026-07-10T00:00:00Z",
    )
    .await;
    let transition = old_session.transition.clone().lock_owned().await;

    let expiry_state = state.clone();
    let expiry = tokio::spawn(async move {
        expire_stale_sessions_with_timeout(
            &expiry_state,
            "2026-07-10T00:01:00Z",
            Duration::from_secs(45),
        )
        .await
    });
    wait_for_session_removal(&state, agent_id).await;
    let replacement = register_pending_session(
        &state,
        tenant_id,
        agent_id,
        replacement_command.id,
        "2026-07-10T00:01:00Z",
    )
    .await;
    drop(transition);

    assert_eq!(expiry.await.unwrap().unwrap(), 1);
    assert_failed(
        &state,
        tenant_id,
        old_command.id,
        "agent session expired before printer operation completed",
    )
    .await;
    assert_replacement_preserved(
        &state,
        tenant_id,
        agent_id,
        replacement.token,
        replacement_command.id,
    )
    .await;
}

#[tokio::test]
async fn stale_recovery_uses_only_process_local_live_command_owners() {
    let (state, tenant_id, agent_id) = fixture("stale-recovery").await;
    let sibling = state.sibling_for_tests();
    let local_owned = native(&state, tenant_id, agent_id, 20_042).await;
    let sibling_owned = native(&state, tenant_id, agent_id, 20_043).await;
    let unowned = native(&state, tenant_id, agent_id, 20_044).await;
    for command_id in [local_owned.id, sibling_owned.id, unowned.id] {
        super::set_command_updated_at(&state, command_id, "2026-07-10T00:00:00Z").await;
    }
    let local_session = register_pending_session(
        &state,
        tenant_id,
        agent_id,
        local_owned.id,
        "2026-07-10T00:05:59Z",
    )
    .await;
    let sibling_session = register_pending_session(
        &sibling,
        tenant_id,
        agent_id,
        sibling_owned.id,
        "2026-07-10T00:05:59Z",
    )
    .await;

    let failed = fail_stale_live_commands_with_timeouts(
        &state,
        "2026-07-10T00:06:00Z",
        Duration::from_secs(300),
        Duration::from_secs(45),
    )
    .await
    .unwrap();

    assert_eq!(failed, 2);
    assert_eq!(
        load(&state, tenant_id, local_owned.id).await.status,
        CommandStatus::Sent
    );
    for command_id in [sibling_owned.id, unowned.id] {
        assert_failed(
            &state,
            tenant_id,
            command_id,
            "live printer operation owner unavailable before completion",
        )
        .await;
    }
    assert_eq!(
        state.sessions().get(agent_id).await.unwrap().token,
        local_session.token
    );
    assert_eq!(
        sibling.sessions().get(agent_id).await.unwrap().token,
        sibling_session.token
    );
    assert!(
        sibling
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&sibling_owned.id)
    );
}
struct RegisteredSession {
    token: SessionToken,
    transition: Arc<Mutex<()>>,
    close_receiver: mpsc::Receiver<()>,
}

async fn fixture(slug: &str) -> (AppState, TenantId, AgentId) {
    let state = AppState::sqlite_for_tests().await.unwrap();
    let tenant = state.tenants().create(slug, slug).await.unwrap();
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    (state, tenant.id, agent.id)
}

async fn native(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    sequence_id: u64,
) -> CommandRecord {
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("A1"),
    )
    .await
    .unwrap();
    state
        .commands()
        .create_printer_operation_sent_with_audit(
            tenant_id,
            &printer_id,
            agent_id,
            PrinterOperationKind::HandlePrintError {
                error_action: PrintErrorAction::Resume,
                print_error: 83_918_929,
                printer_job_id: "job-7".to_owned(),
                sequence_id,
            },
            super::test_audit_actor(),
        )
        .await
        .unwrap()
}

async fn register_pending_session(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    command_id: CommandId,
    observed_at: &str,
) -> RegisteredSession {
    let token = SessionToken::new();
    let transition = Arc::new(Mutex::new(()));
    let pending = empty_pending_live_commands();
    pending
        .lock()
        .unwrap()
        .insert(command_id, PendingLiveCommand::new(None));
    let (close_sender, close_receiver) = mpsc::channel(1);
    let previous = state
        .sessions()
        .register(AgentSession {
            token,
            tenant_id,
            agent_id,
            name: "agent".to_owned(),
            version: "0.1.0".to_owned(),
            connected_at: observed_at.to_owned(),
            last_heartbeat_at: observed_at.to_owned(),
            wake_sender: mpsc::channel(1).0,
            close_sender,
            command_sender: mpsc::channel(1).0,
            capabilities: HashSet::from([AgentCapability::HandlePrintError]),
            pending_live_commands: pending,
            live_command_transition: transition.clone(),
        })
        .await;
    assert!(previous.is_none());
    RegisteredSession {
        token,
        transition,
        close_receiver,
    }
}

async fn wait_for_close(close_receiver: &mut mpsc::Receiver<()>) {
    tokio::time::timeout(Duration::from_secs(1), close_receiver.recv())
        .await
        .unwrap()
        .expect("removed session close signal");
}

async fn wait_for_session_removal(state: &AppState, agent_id: AgentId) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while state.sessions().get(agent_id).await.is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

async fn assert_failed(state: &AppState, tenant_id: TenantId, command_id: CommandId, reason: &str) {
    let command = wait_for_status(state, tenant_id, command_id, CommandStatus::Failed).await;
    assert_eq!(command.error.as_deref(), Some(reason));
}

async fn assert_replacement_preserved(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    command_id: CommandId,
) {
    assert_eq!(state.sessions().get(agent_id).await.unwrap().token, token);
    assert_eq!(
        load(state, tenant_id, command_id).await.status,
        CommandStatus::Sent
    );
    assert!(
        state
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&command_id)
    );
}

async fn wait_for_status(
    state: &AppState,
    tenant_id: TenantId,
    command_id: CommandId,
    status: CommandStatus,
) -> CommandRecord {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let command = load(state, tenant_id, command_id).await;
            if command.status == status {
                return command;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}

async fn load(state: &AppState, tenant_id: TenantId, command_id: CommandId) -> CommandRecord {
    state
        .commands()
        .get_for_tenant(tenant_id, command_id)
        .await
        .unwrap()
        .unwrap()
}
