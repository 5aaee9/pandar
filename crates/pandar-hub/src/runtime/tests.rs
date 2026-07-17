use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use pandar_core::{AgentId, AgentStatus, PrintCalibrationMode, PrintStatus, TenantId};
use pandar_core::{CommandStatus, FirmwareControlMetadata};
use tokio::sync::mpsc;
use tracing_subscriber::fmt::MakeWriter;

use super::*;
use crate::repositories::{AuditActor, CreatePrintJob, FirmwareCommandOwner, LinkPrinterPayload};
use crate::sessions::{AgentSession, SessionToken};

mod control_plane_close;
mod print_error;
mod printer_event_epoch;

#[tokio::test]
async fn runtime_expiry_tick_marks_stale_agent_offline() {
    let state = AppState::sqlite_for_tests().await.unwrap();
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let token = SessionToken::new();
    state
        .agents()
        .claim_online_session(
            tenant.id,
            agent.id,
            &token.persisted_id(),
            "0.1.0",
            "2026-06-20T00:00:00Z",
        )
        .await
        .unwrap();
    let (wake_sender, _) = mpsc::channel(1);
    let (close_sender, _) = mpsc::channel(1);
    state
        .sessions()
        .register(AgentSession {
            token,
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
async fn runtime_stall_sweep_publishes_persisted_job_progress() {
    let state = AppState::sqlite_for_tests().await.unwrap();
    let (_control_plane, ready) = spawn_control_plane_ready(state.clone());
    ready.await.unwrap().unwrap();
    let tenant = state
        .tenants()
        .create("stalled-runtime", "Stalled Runtime")
        .await
        .unwrap();
    let agent = state
        .agents()
        .create(tenant.id, "stalled-agent")
        .await
        .unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture(
        state.database(),
        tenant.id,
        agent.id,
    )
    .await
    .unwrap();
    let created = state
        .jobs()
        .create_print_job(CreatePrintJob {
            tenant_id: tenant.id,
            printer_id,
            agent_id: agent.id,
            artifact_id: "stalled-runtime-artifact".to_owned(),
            artifact_filename: "stalled-runtime.3mf".to_owned(),
            artifact_content_type: "model/3mf".to_owned(),
            artifact_size_bytes: 42,
            artifact_storage_path: "stalled-runtime/stalled-runtime.3mf".to_owned(),
            artifact_metadata_json: None,
            plate_id: 1,
            use_ams: true,
            auto_bed_leveling: PrintCalibrationMode::Off,
            bed_leveling: false,
            flow_cali: false,
            auto_flow_cali: PrintCalibrationMode::Off,
            auto_offset_cali: PrintCalibrationMode::Off,
            timelapse: false,
            ams_mapping_json: None,
            ams_mapping2_json: None,
            ams_mapping_info_json: None,
        })
        .await
        .unwrap();
    state
        .jobs()
        .mark_print_sent(created.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    state
        .jobs()
        .mark_print_succeeded(created.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    set_job_waiting_at(&state, created.job.id, "2026-07-17T00:00:00Z").await;
    set_command_updated_at(&state, created.job.command_id, "2026-07-17T00:00:00Z").await;
    let mut events = state.printer_events().subscribe(tenant.id).await;

    assert_eq!(
        mark_stalled_pending_jobs_once(&state, "2026-07-17T00:15:01Z")
            .await
            .unwrap(),
        1,
    );
    let event = tokio::time::timeout(Duration::from_secs(1), events.recv())
        .await
        .expect("stall sweep should publish a printer event")
        .unwrap();
    let event = serde_json::to_value(event).unwrap();
    assert_eq!(event["type"], "job_progress");
    assert_eq!(event["job"]["id"], created.job.id.to_string());
    assert_eq!(event["job"]["print"]["status"], "stalled");
    assert_eq!(
        state
            .jobs()
            .get_for_tenant(tenant.id, created.job.id)
            .await
            .unwrap()
            .unwrap()
            .job
            .print
            .status,
        PrintStatus::Stalled,
    );
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

    let failed = fail_stale_live_commands_with_timeouts(
        &state,
        "2026-07-01T00:06:00Z",
        Duration::from_secs(300),
        Duration::from_secs(45),
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

#[tokio::test]
async fn runtime_firmware_command_startup_cleanup_fails_both_live_only_kinds() {
    let state = AppState::sqlite_for_tests().await.unwrap();
    let tenant = state
        .tenants()
        .create("firmware-cleanup", "Firmware Cleanup")
        .await
        .unwrap();
    let agent = state
        .agents()
        .create(tenant.id, "firmware-cleanup-agent")
        .await
        .unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture(
        state.database(),
        tenant.id,
        agent.id,
    )
    .await
    .unwrap();
    let refresh = state
        .commands()
        .create_firmware_refresh_sent_with_audit(
            tenant.id,
            &printer_id,
            agent.id,
            FirmwareCommandOwner {
                session_id: "firmware-cleanup-session".to_owned(),
                instance_id: state.instance_id(),
            },
            "refresh".to_owned(),
            test_audit_actor(),
        )
        .await
        .unwrap();
    let control = state
        .commands()
        .create_firmware_control_sent_with_audit(
            tenant.id,
            &printer_id,
            agent.id,
            FirmwareCommandOwner {
                session_id: "firmware-cleanup-session".to_owned(),
                instance_id: state.instance_id(),
            },
            FirmwareControlMetadata::UpgradeConfirm {
                sequence_id: "control".to_owned(),
                src_id: 1,
            },
            test_audit_actor(),
        )
        .await
        .unwrap();
    for command_id in [refresh.id, control.id] {
        set_command_updated_at(&state, command_id, "2026-07-01T00:00:00Z").await;
    }

    let failed = fail_stale_live_commands_with_timeouts(
        &state,
        "2026-07-01T00:06:00Z",
        Duration::from_secs(300),
        Duration::from_secs(45),
    )
    .await
    .unwrap();

    assert_eq!(failed, 2);
    for command_id in [refresh.id, control.id] {
        let command = state
            .commands()
            .get_for_tenant(tenant.id, command_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(command.status, CommandStatus::Failed);
        let result: crate::repositories::FirmwarePersistedResult =
            serde_json::from_str(command.result_json.as_deref().unwrap()).unwrap();
        assert_eq!(
            result.phase,
            crate::repositories::FirmwarePersistedPhase::PrePublishFailure
        );
        assert!(command.error.unwrap().contains("owner unavailable"));
    }
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

async fn set_job_waiting_at(state: &AppState, job_id: pandar_core::JobId, updated_at: &str) {
    let crate::db::Database::Sqlite(pool) = state.database() else {
        panic!("expected SQLite database");
    };
    sqlx::query(
        "UPDATE jobs SET updated_at = ?2, print_updated_at = NULL, \
         progress_percent = NULL, current_layer = NULL, print_started_at = NULL \
         WHERE id = ?1",
    )
    .bind(job_id.to_string())
    .bind(updated_at)
    .execute(pool)
    .await
    .unwrap();
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
