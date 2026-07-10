use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use pandar_core::CommandStatus;
use pandar_core::{AgentId, AgentStatus, TenantId};
use tokio::sync::mpsc;
use tracing_subscriber::fmt::MakeWriter;

use super::*;
use crate::repositories::{AuditActor, LinkPrinterPayload};
use crate::sessions::{AgentSession, SessionToken};

mod control_plane_close;
mod print_error;

#[tokio::test]
async fn runtime_expiry_tick_marks_stale_agent_offline() {
    let state = AppState::sqlite_for_tests().await.unwrap();
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    state
        .agents()
        .update_connection(
            agent.id,
            AgentStatus::Online,
            Some("0.1.0"),
            "2026-06-20T00:00:00Z",
        )
        .await
        .unwrap();
    let (wake_sender, _) = mpsc::channel(1);
    let (close_sender, _) = mpsc::channel(1);
    state
        .sessions()
        .register(AgentSession {
            token: SessionToken::new(),
            tenant_id: tenant.id,
            agent_id: agent.id,
            name: agent.name,
            version: "0.1.0".to_string(),
            connected_at: "2026-06-20T00:00:00Z".to_string(),
            last_heartbeat_at: "2026-06-20T00:00:00Z".to_string(),
            wake_sender,
            close_sender,
            command_sender: mpsc::channel(1).0,
            capabilities: std::collections::HashSet::new(),
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;

    let expired =
        expire_stale_sessions_with_timeout(&state, "2026-06-20T00:00:10Z", Duration::from_secs(5))
            .await
            .unwrap();

    assert_eq!(expired, 1);
    assert!(state.sessions().get(agent.id).await.is_none());
    let persisted = state.agents().get(agent.id).await.unwrap().unwrap();
    assert_eq!(persisted.status, AgentStatus::Offline);
}

#[tokio::test]
async fn runtime_stale_link_printer_cleanup_skips_pending_live_commands() {
    let state = AppState::sqlite_for_tests().await.unwrap();
    let tenant = state
        .tenants()
        .create("cleanup-acme", "Cleanup Acme")
        .await
        .unwrap();
    let agent = state
        .agents()
        .create(tenant.id, "cleanup-agent")
        .await
        .unwrap();
    let owned = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant.id,
            agent.id,
            link_payload("OWNED"),
            test_audit_actor(),
        )
        .await
        .unwrap();
    let unowned = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant.id,
            agent.id,
            link_payload("UNOWNED"),
            test_audit_actor(),
        )
        .await
        .unwrap();
    set_command_updated_at(&state, owned.id, "2026-07-01T00:00:00Z").await;
    set_command_updated_at(&state, unowned.id, "2026-07-01T00:00:00Z").await;
    let (command_sender, _command_receiver) = mpsc::channel(1);
    let pending = crate::sessions::empty_pending_live_commands();
    pending
        .lock()
        .unwrap()
        .insert(owned.id, crate::sessions::PendingLiveCommand::new(None));
    state
        .sessions()
        .register(AgentSession {
            token: SessionToken::new(),
            tenant_id: tenant.id,
            agent_id: agent.id,
            name: agent.name,
            version: "0.1.0".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender: mpsc::channel(1).0,
            close_sender: mpsc::channel(1).0,
            command_sender,
            capabilities: std::collections::HashSet::new(),
            pending_live_commands: pending,
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;

    let failed = fail_stale_live_commands_with_timeout(
        &state,
        "2026-07-01T00:06:00Z",
        Duration::from_secs(300),
    )
    .await
    .unwrap();

    assert_eq!(failed, 1);
    assert_eq!(
        state
            .commands()
            .get_for_tenant(tenant.id, owned.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        CommandStatus::Sent
    );
    assert_eq!(
        state
            .commands()
            .get_for_tenant(tenant.id, unowned.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        CommandStatus::Failed
    );
}

#[test]
fn runtime_stale_link_printer_cleanup_log_redacts_access_code() {
    let logs = CapturedLogs::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.writer())
        .with_ansi(false)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let err = anyhow::anyhow!("database failed with access_code=SECRET-LINK-CODE")
        .context("failed to sweep link printer commands");

    log_stale_live_command_cleanup_error(&err);
    drop(_guard);

    assert!(!logs.to_string().contains("SECRET-LINK-CODE"));
}

#[tokio::test]
async fn sibling_instance_can_wake_connected_agent() {
    let state = AppState::sqlite_for_tests().await.unwrap();
    let sibling = state.sibling_for_tests();
    let (_control_plane, ready) = spawn_control_plane_ready(sibling.clone());
    ready.await.unwrap().unwrap();
    let tenant = state
        .tenants()
        .create("wake-acme", "Wake Acme")
        .await
        .unwrap();
    let agent = state
        .agents()
        .create(tenant.id, "wake-agent")
        .await
        .unwrap();
    let (mut wake_receiver, _close_receiver) =
        register_test_session(&sibling, tenant.id, agent.id, "wake-agent").await;

    state.wake_agent(tenant.id, agent.id).await;

    tokio::time::timeout(Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("sibling agent should receive wake")
        .expect("wake channel should stay open");
}

#[tokio::test]
async fn sibling_agent_wake_ignores_wrong_tenant_and_agent() {
    let state = AppState::sqlite_for_tests().await.unwrap();
    let sibling = state.sibling_for_tests();
    let (_control_plane, ready) = spawn_control_plane_ready(sibling.clone());
    ready.await.unwrap().unwrap();
    let tenant = state
        .tenants()
        .create("wrong-wake-acme", "Wrong Wake Acme")
        .await
        .unwrap();
    let agent = state
        .agents()
        .create(tenant.id, "wrong-wake-agent")
        .await
        .unwrap();
    let (mut wake_receiver, _close_receiver) =
        register_test_session(&sibling, tenant.id, agent.id, "wrong-wake-agent").await;

    state.wake_agent(TenantId::new(), agent.id).await;
    state.wake_agent(tenant.id, AgentId::new()).await;

    assert!(
        tokio::time::timeout(Duration::from_millis(100), wake_receiver.recv())
            .await
            .is_err(),
        "wrong tenant or agent must not wake the sibling session"
    );
}

#[tokio::test]
async fn sibling_instance_can_close_connected_agent() {
    let state = AppState::sqlite_for_tests().await.unwrap();
    let sibling = state.sibling_for_tests();
    let (_control_plane, ready) = spawn_control_plane_ready(sibling.clone());
    ready.await.unwrap().unwrap();
    let tenant = state
        .tenants()
        .create("close-acme", "Close Acme")
        .await
        .unwrap();
    let agent = state
        .agents()
        .create(tenant.id, "close-agent")
        .await
        .unwrap();
    let (_wake_receiver, mut close_receiver) =
        register_test_session(&sibling, tenant.id, agent.id, "close-agent").await;

    state.close_agent(tenant.id, agent.id).await;

    tokio::time::timeout(Duration::from_secs(1), close_receiver.recv())
        .await
        .expect("sibling agent should receive close")
        .expect("close channel should stay open");
}

#[tokio::test]
async fn sibling_agent_close_ignores_wrong_tenant_and_agent() {
    let state = AppState::sqlite_for_tests().await.unwrap();
    let sibling = state.sibling_for_tests();
    let (_control_plane, ready) = spawn_control_plane_ready(sibling.clone());
    ready.await.unwrap().unwrap();
    let tenant = state
        .tenants()
        .create("wrong-close-acme", "Wrong Close Acme")
        .await
        .unwrap();
    let agent = state
        .agents()
        .create(tenant.id, "wrong-close-agent")
        .await
        .unwrap();
    let (_wake_receiver, mut close_receiver) =
        register_test_session(&sibling, tenant.id, agent.id, "wrong-close-agent").await;

    state.close_agent(TenantId::new(), agent.id).await;
    state.close_agent(tenant.id, AgentId::new()).await;

    assert!(
        tokio::time::timeout(Duration::from_millis(100), close_receiver.recv())
            .await
            .is_err(),
        "wrong tenant or agent must not close the sibling session"
    );
}

async fn register_test_session(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    name: &str,
) -> (mpsc::Receiver<()>, mpsc::Receiver<()>) {
    let (wake_sender, wake_receiver) = mpsc::channel(1);
    let (close_sender, close_receiver) = mpsc::channel(1);
    state
        .sessions()
        .register(AgentSession {
            token: SessionToken::new(),
            tenant_id,
            agent_id,
            name: name.to_owned(),
            version: "0.1.0".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
            command_sender: mpsc::channel(1).0,
            capabilities: std::collections::HashSet::new(),
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;
    (wake_receiver, close_receiver)
}

fn link_payload(serial: &str) -> LinkPrinterPayload {
    LinkPrinterPayload {
        printer_type: "BambuLab".to_owned(),
        host: "192.0.2.10".to_owned(),
        access_code: format!("SECRET-{serial}"),
        name: None,
    }
}

fn test_audit_actor() -> AuditActor {
    AuditActor::tenant_token(None, "test-runtime-token", vec!["*"])
}

async fn set_command_updated_at(
    state: &AppState,
    command_id: pandar_core::CommandId,
    updated_at: &str,
) {
    let crate::db::Database::Sqlite(pool) = state.database() else {
        panic!("expected SQLite database");
    };
    sqlx::query("UPDATE commands SET updated_at = ?2 WHERE id = ?1")
        .bind(command_id.to_string())
        .bind(updated_at)
        .execute(pool)
        .await
        .unwrap();
}

#[derive(Clone)]
struct CapturedLogs {
    output: Arc<Mutex<Vec<u8>>>,
}

impl CapturedLogs {
    fn new() -> Self {
        Self {
            output: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn writer(&self) -> TestLogWriter {
        TestLogWriter {
            output: self.output.clone(),
        }
    }
}

impl std::fmt::Display for CapturedLogs {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let output = self.output.lock().unwrap().clone();
        formatter.write_str(&String::from_utf8_lossy(&output))
    }
}

#[derive(Clone)]
struct TestLogWriter {
    output: Arc<Mutex<Vec<u8>>>,
}

impl<'writer> MakeWriter<'writer> for TestLogWriter {
    type Writer = TestLogBuffer;

    fn make_writer(&'writer self) -> Self::Writer {
        TestLogBuffer {
            output: self.output.clone(),
        }
    }
}

struct TestLogBuffer {
    output: Arc<Mutex<Vec<u8>>>,
}

impl Write for TestLogBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.output.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
