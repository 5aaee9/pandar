use super::support::audit_actor;
use super::*;

pub(super) struct FirmwareFixture {
    pub(super) state: AppState,
    pub(super) tenant_id: TenantId,
    pub(super) agent_id: AgentId,
    pub(super) printer_id: String,
    pub(super) serial: String,
    pub(super) token: SessionToken,
    pub(super) command_receiver:
        mpsc::Receiver<Result<pandar_protocol::agent::v1::HubCommand, Status>>,
}

impl FirmwareFixture {
    pub(super) async fn new(slug: &str) -> Self {
        let state = AppState::sqlite_for_tests().await.unwrap();
        Self::with_state(state, slug).await
    }

    pub(super) async fn new_file(slug: &str) -> Self {
        let state = AppState::file_sqlite_for_tests().await.unwrap();
        Self::with_state(state, slug).await
    }

    pub(super) async fn with_state(state: AppState, slug: &str) -> Self {
        let tenant = state
            .tenants()
            .create(slug, "Firmware Tenant")
            .await
            .unwrap();
        let agent = state
            .agents()
            .create(tenant.id, "firmware-agent")
            .await
            .unwrap();
        let printer_id = crate::repositories::test_helpers::insert_printer_fixture(
            state.database(),
            tenant.id,
            agent.id,
        )
        .await
        .unwrap();
        let printer = state
            .printers()
            .get_for_tenant(tenant.id, &printer_id)
            .await
            .unwrap()
            .unwrap();
        let token = SessionToken::new();
        let now = "2026-07-12T00:00:00Z";
        let (wake_sender, _) = mpsc::channel(1);
        let (close_sender, _) = mpsc::channel(1);
        let (command_sender, command_receiver) = mpsc::channel(8);
        {
            let _lease = state
                .sessions()
                .transition_lease_for_session(agent.id, token)
                .await;
            state
                .agents()
                .claim_online_session(
                    tenant.id,
                    agent.id,
                    &token.persisted_id(),
                    "firmware-test",
                    now,
                )
                .await
                .unwrap();
            state
                .sessions()
                .register(AgentSession {
                    token,
                    tenant_id: tenant.id,
                    agent_id: agent.id,
                    name: "firmware agent".to_owned(),
                    version: "test".to_owned(),
                    connected_at: now.to_owned(),
                    last_heartbeat_at: now.to_owned(),
                    wake_sender,
                    close_sender,
                    command_sender,
                    capabilities: HashSet::from([AgentCapability::FirmwareControl]),
                    pending_live_commands: empty_pending_live_commands(),
                    live_command_transition: Arc::new(tokio::sync::Mutex::new(())),
                })
                .await;
            state
                .printers()
                .establish_generation_if_current(
                    tenant.id,
                    agent.id,
                    &token.persisted_id(),
                    &printer.serial_number,
                    GENERATION,
                )
                .await
                .unwrap();
        }
        Self {
            state,
            tenant_id: tenant.id,
            agent_id: agent.id,
            printer_id,
            serial: printer.serial_number,
            token,
            command_receiver,
        }
    }

    pub(super) async fn prepare(
        &mut self,
        metadata: FirmwareControlMetadata,
    ) -> crate::firmware_control::PreparedFirmwareControl {
        let state = self.state.clone();
        let printer_id = self.printer_id.clone();
        let tenant_id = self.tenant_id;
        let prepare = tokio::spawn(async move {
            state
                .prepare_control(tenant_id, &printer_id, metadata, audit_actor())
                .await
                .unwrap()
        });
        let outbound = self.next_command().await;
        let command_id = CommandId::parse(&outbound.command_id).unwrap();
        assert!(matches!(
            outbound.command,
            Some(hub_command::Command::PrepareFirmwareControl(_))
        ));
        self.event(agent_event::Event::CommandAck(CommandAck {
            command_id: command_id.to_string(),
            accepted: true,
            error: String::new(),
        }))
        .await;
        self.event(agent_event::Event::FirmwarePrepared(FirmwarePrepared {
            command_id: command_id.to_string(),
            serial: self.serial.clone(),
            generation: GENERATION,
        }))
        .await;
        tokio::time::timeout(Duration::from_millis(200), prepare)
            .await
            .expect("prepare waiter must release transition lease")
            .unwrap()
    }

    pub(super) async fn next_command(&mut self) -> pandar_protocol::agent::v1::HubCommand {
        tokio::time::timeout(Duration::from_millis(200), self.command_receiver.recv())
            .await
            .expect("firmware command dispatch timed out")
            .expect("firmware command channel closed")
            .expect("firmware command status")
    }

    pub(super) async fn start_execute(
        &mut self,
        prepared_token: &str,
        command: FirmwareCommand,
    ) -> tokio::task::JoinHandle<crate::firmware_control::FirmwareExecuteResult> {
        let state = self.state.clone();
        let tenant_id = self.tenant_id;
        let prepared_token = prepared_token.to_owned();
        let waiter = tokio::spawn(async move {
            state
                .execute_control(tenant_id, &prepared_token, command)
                .await
                .unwrap()
        });
        let outbound = self.next_command().await;
        assert!(matches!(
            outbound.command,
            Some(hub_command::Command::ExecuteFirmwareControl(_))
        ));
        waiter
    }

    pub(super) fn close_command_channel(&mut self) {
        let (_sender, replacement) = mpsc::channel(1);
        self.command_receiver = replacement;
    }

    pub(super) async fn replace_session_without_cleanup(&self) -> AgentSession {
        let token = SessionToken::new();
        let now = "2026-07-12T00:00:01Z";
        let (wake_sender, _) = mpsc::channel(1);
        let (close_sender, _) = mpsc::channel(1);
        let (command_sender, _) = mpsc::channel(1);
        let _lease = self
            .state
            .sessions()
            .transition_lease_for_session(self.agent_id, token)
            .await;
        self.state
            .agents()
            .claim_online_session(
                self.tenant_id,
                self.agent_id,
                &token.persisted_id(),
                "replacement-test",
                now,
            )
            .await
            .unwrap();
        self.state
            .sessions()
            .register(AgentSession {
                token,
                tenant_id: self.tenant_id,
                agent_id: self.agent_id,
                name: "replacement agent".to_owned(),
                version: "test".to_owned(),
                connected_at: now.to_owned(),
                last_heartbeat_at: now.to_owned(),
                wake_sender,
                close_sender,
                command_sender,
                capabilities: HashSet::from([AgentCapability::FirmwareControl]),
                pending_live_commands: empty_pending_live_commands(),
                live_command_transition: Arc::new(tokio::sync::Mutex::new(())),
            })
            .await
            .expect("firmware fixture must replace its original session")
    }

    pub(super) async fn claim_authoritative_sibling_session(&self) -> AppState {
        let sibling = self.state.sibling_for_tests();
        let token = SessionToken::new();
        let now = "2026-07-12T00:00:02Z";
        let (wake_sender, _) = mpsc::channel(1);
        let (close_sender, _) = mpsc::channel(1);
        let (command_sender, _) = mpsc::channel(1);
        let _lease = sibling
            .sessions()
            .transition_lease_for_session(self.agent_id, token)
            .await;
        sibling
            .agents()
            .claim_online_session(
                self.tenant_id,
                self.agent_id,
                &token.persisted_id(),
                "sibling-test",
                now,
            )
            .await
            .unwrap();
        sibling
            .sessions()
            .register(AgentSession {
                token,
                tenant_id: self.tenant_id,
                agent_id: self.agent_id,
                name: "sibling firmware agent".to_owned(),
                version: "test".to_owned(),
                connected_at: now.to_owned(),
                last_heartbeat_at: now.to_owned(),
                wake_sender,
                close_sender,
                command_sender,
                capabilities: HashSet::from([AgentCapability::FirmwareControl]),
                pending_live_commands: empty_pending_live_commands(),
                live_command_transition: Arc::new(tokio::sync::Mutex::new(())),
            })
            .await;
        assert!(
            self.state
                .sessions()
                .is_current(self.agent_id, self.token)
                .await
        );
        sibling
    }

    pub(super) async fn latest_command(&self) -> pandar_core::CommandRecord {
        let crate::db::Database::Sqlite(pool) = self.state.database() else {
            panic!("expected SQLite");
        };
        let id: String = sqlx::query_scalar(
            "SELECT id FROM commands WHERE tenant_id = ?1 ORDER BY created_at DESC, rowid DESC LIMIT 1",
        )
        .bind(self.tenant_id.to_string())
        .fetch_one(pool)
        .await
        .unwrap();
        self.command(CommandId::parse(&id).unwrap()).await
    }

    pub(super) async fn event(&self, event: agent_event::Event) {
        self.event_result(event).await.unwrap();
    }

    pub(super) async fn event_result(
        &self,
        event: agent_event::Event,
    ) -> Result<(), tonic::Status> {
        handle_event(
            &self.state,
            self.tenant_id,
            self.agent_id,
            self.token,
            AgentEvent {
                tenant_id: self.tenant_id.to_string(),
                agent_id: self.agent_id.to_string(),
                event_id: uuid::Uuid::new_v4().to_string(),
                event: Some(event),
            },
        )
        .await
    }

    pub(super) async fn command(&self, command_id: CommandId) -> pandar_core::CommandRecord {
        self.state
            .commands()
            .get_for_tenant(self.tenant_id, command_id)
            .await
            .unwrap()
            .unwrap()
    }

    pub(super) async fn insert_raw_command(
        &self,
        id: CommandId,
        kind: &str,
        status: CommandStatus,
    ) {
        let crate::db::Database::Sqlite(pool) = self.state.database() else {
            panic!("expected SQLite");
        };
        sqlx::query(
            "INSERT INTO commands (id, tenant_id, agent_id, printer_id, kind, status, payload_json, result_json, error, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, '{}', NULL, NULL, ?7, ?7)",
        )
        .bind(id.to_string())
        .bind(self.tenant_id.to_string())
        .bind(self.agent_id.to_string())
        .bind(&self.printer_id)
        .bind(kind)
        .bind(status.as_str())
        .bind("2026-07-12T00:00:00Z")
        .execute(pool)
        .await
        .unwrap();
    }

    pub(super) async fn execute_sqlite(&self, sql: &'static str) {
        let crate::db::Database::Sqlite(pool) = self.state.database() else {
            panic!("expected SQLite");
        };
        sqlx::query(sql).execute(pool).await.unwrap();
    }

    pub(super) async fn set_command_updated_at(&self, command_id: CommandId, updated_at: &str) {
        let crate::db::Database::Sqlite(pool) = self.state.database() else {
            panic!("expected SQLite");
        };
        sqlx::query("UPDATE commands SET updated_at = ?1 WHERE id = ?2")
            .bind(updated_at)
            .bind(command_id.to_string())
            .execute(pool)
            .await
            .unwrap();
    }
}
