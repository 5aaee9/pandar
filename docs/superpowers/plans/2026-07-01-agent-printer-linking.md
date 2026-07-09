# Agent Printer Linking Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let an authorized dashboard user submit Bambu printer host, serial, access code, and optional metadata to a selected online local agent so the agent links the printer at runtime without Hub credential persistence.

**Architecture:** Add a live-only `link_printer` command path. Hub stores only a redacted `sent` command row and audit event, sends the secret-bearing proto command directly to the currently connected local reverse stream after a session-token recheck, and fails stale unowned live commands instead of replaying them. The agent owns mutable runtime printer state, validates the endpoint over Bambu LAN MQTT, installs/replaces the endpoint in memory, starts one report-forwarding task per serial, emits a snapshot, and returns a redacted structured result.

**Tech Stack:** Rust 2024, axum, tonic/prost, SeaORM/SQLx-backed repositories, tokio, Next.js App Router, React, next-intl, Tailwind/shadcn-style local UI primitives, Vitest/React Testing Library.

## Global Constraints

- Approved spec: `docs/superpowers/specs/2026-07-01-agent-printer-linking-design.md` is binding.
- Do not persist raw printer access codes in Hub database rows, audit metadata, command results, command errors, logs, frontend status, or snapshots.
- `link_printer` persisted `payload_json` uses `access_code: "[redacted]"`.
- Hub persistent behavior must remain backend-neutral for SQLite and PostgreSQL.
- Preserve lower-level error cause chains by formatting with `{err:#}` before redaction at error/log/result boundaries.
- No legacy fallback or backwards-compatibility shim for old link-printer behavior; this is a new command.
- Frontend must follow existing dashboard style and localization patterns.
- Update `docs/roadmap.md` after code changes and add the approved runtime-link limitation note to `docs/development.md` or the nearest agent-operations document.
- Run `cargo fmt`, `cargo clippy`, and `cargo nextest run --manifest-path "Cargo.toml" --workspace` after implementation, plus focused frontend tests/build checks.

---

## File Structure

- Modify `proto/pandar/agent/v1/agent.proto`: add `LinkPrinter` and `HubCommand.link_printer = 15`.
- Modify `crates/pandar-hub/src/repositories/commands.rs`: add link-printer payload structs and repository methods.
- Modify `crates/pandar-hub/src/repositories/commands/inserts.rs`: support inserting commands with explicit `CommandStatus::Sent` while keeping existing queued inserts unchanged.
- Modify `crates/pandar-hub/src/repositories/commands/audit.rs`: add audited sent-row creation for link-printer.
- Modify `crates/pandar-hub/src/repositories/commands/transitions.rs`: add backend-neutral stale unowned link-printer cleanup.
- Modify `crates/pandar-hub/src/grpc/commands.rs`: reject durable replay of persisted `link_printer` and keep late terminal behavior explicit.
- Modify `crates/pandar-hub/src/sessions.rs`: store command sender and session-scoped pending live-command IDs; add direct dispatch helper and pending-ID collection.
- Modify `crates/pandar-hub/src/grpc.rs`: register command sender in sessions, remove pending IDs on terminal result, and fail pending live commands on session close.
- Modify `crates/pandar-hub/src/runtime.rs`: run stale unowned live-command cleanup.
- Modify `crates/pandar-hub/src/routes/printers.rs` and `crates/pandar-hub/src/routes.rs`: add `POST /api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer`.
- Modify hub tests under `crates/pandar-hub/src/repositories/tests/commands.rs`, `routes/tests/printers.rs`, `grpc/tests/commands.rs`, and `sessions.rs` tests.
- Modify `crates/pandar-agent/src/machine/mod.rs`: extend gateway trait with `link_printer` and expose runtime gateway module.
- Create `crates/pandar-agent/src/machine/runtime.rs`: mutable runtime gateway with one report task per serial.
- Modify `crates/pandar-agent/src/commands.rs`: handle proto `LinkPrinter` command.
- Modify `crates/pandar-agent/src/lib.rs`: use runtime gateway for empty and configured startup printer sets.
- Modify agent tests in `crates/pandar-agent/src/commands/tests.rs` and `crates/pandar-agent/src/machine/tests.rs`.
- Modify frontend files `frontend/app/actions.ts`, `frontend/app/action-status.ts`, `frontend/app/command-result-parser.ts`, `frontend/app/dashboard-types.ts`, `frontend/app/dashboard-view-content.tsx`, `frontend/app/diagnostics-panel.tsx`, `frontend/app/agent-pairing-guidance.test.tsx`, `frontend/app/action-status-toast.test.tsx`, `frontend/messages/en.json`, and `frontend/messages/zh.json`.
- Update `docs/development.md` and `docs/roadmap.md`.

---

### Task 1: Protocol And Hub Command Repository Primitives

**Files:**

- Modify: `proto/pandar/agent/v1/agent.proto`
- Modify: `crates/pandar-hub/src/repositories/commands.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/inserts.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/audit.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/transitions.rs`
- Modify: `crates/pandar-hub/src/repositories/mod.rs`
- Modify: `crates/pandar-hub/src/grpc/commands.rs`
- Modify: `crates/pandar-hub/src/redaction.rs`
- Test: `crates/pandar-hub/src/repositories/tests/commands.rs`
- Test: `crates/pandar-hub/src/grpc/tests/commands.rs`

**Interfaces:**

- Produces: `LinkPrinterPayload { host, serial_number, access_code, name, model }`.
- Produces: `RedactedLinkPrinterPayload { host, serial_number, access_code: "[redacted]", name, model }`.
- Produces: `CommandRepository::create_link_printer_sent_with_audit(tenant_id, agent_id, payload, actor) -> RepositoryResult<CommandRecord>`.
- Produces: `CommandRepository::fail_stale_unowned_link_printer_commands(now, timeout, owned_command_ids) -> RepositoryResult<u64>`.
- Produces: `crate::redaction::redact_link_printer_secret(message: &str, access_code: &str) -> String`.
- Produces: proto `hub_command::Command::LinkPrinter` variant for later tasks.
- Consumes: existing command status transitions, audit repository helpers, and SeaORM command entity.

- [ ] **Step 1: Add failing repository tests for redacted sent-row creation**

Add tests to `crates/pandar-hub/src/repositories/tests/commands.rs`:

```rust
#[tokio::test]
async fn command_create_link_printer_sent_persists_redacted_payload_and_audit() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let audit = AuditEventRepository::new(database.clone());
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let access_code = "SECRET-LINK-CODE";

    let command = commands
        .create_link_printer_sent_with_audit(
            tenant.id,
            agent.id,
            LinkPrinterPayload {
                host: "192.0.2.10".to_owned(),
                serial_number: "SERIAL123".to_owned(),
                access_code: access_code.to_owned(),
                name: Some("Office X1C".to_owned()),
                model: Some("X1 Carbon".to_owned()),
            },
            test_audit_actor(),
        )
        .await
        .unwrap();

    assert_eq!(command.kind, "link_printer");
    assert_eq!(command.status, CommandStatus::Sent);
    assert_eq!(command.printer_id, None);
    assert!(!command.payload_json.contains(access_code));
    let payload: serde_json::Value = serde_json::from_str(&command.payload_json).unwrap();
    assert_eq!(payload["host"], "192.0.2.10");
    assert_eq!(payload["serial_number"], "SERIAL123");
    assert_eq!(payload["access_code"], "[redacted]");
    assert_eq!(payload["name"], "Office X1C");
    assert_eq!(payload["model"], "X1 Carbon");

    let events = audit.list_for_tenant(tenant.id).await.unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "agent.link_printer")
        .expect("link printer audit event");
    assert_eq!(event.target_type, "agent");
    assert_eq!(event.target_id.as_deref(), Some(agent.id.to_string().as_str()));
    assert!(!event.metadata_json.contains(access_code));
    let metadata: serde_json::Value = serde_json::from_str(&event.metadata_json).unwrap();
    assert_eq!(metadata["host"], "192.0.2.10");
    assert_eq!(metadata["serial_number"], "SERIAL123");
}
```

- [ ] **Step 2: Add failing repository tests for stale unowned cleanup**

Add tests to `crates/pandar-hub/src/repositories/tests/commands.rs`:

```rust
#[tokio::test]
async fn stale_link_printer_cleanup_skips_owned_pending_commands() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let old_owned = commands
        .create_link_printer_sent_with_audit(tenant.id, agent.id, link_payload("OWNED"), test_audit_actor())
        .await
        .unwrap();
    let old_unowned = commands
        .create_link_printer_sent_with_audit(tenant.id, agent.id, link_payload("UNOWNED"), test_audit_actor())
        .await
        .unwrap();

    set_command_updated_at(&database, old_owned.id, "2026-07-01T00:00:00Z").await;
    set_command_updated_at(&database, old_unowned.id, "2026-07-01T00:00:00Z").await;

    let failed = commands
        .fail_stale_unowned_link_printer_commands(
            "2026-07-01T00:06:00Z",
            std::time::Duration::from_secs(300),
            &[old_owned.id],
        )
        .await
        .unwrap();

    assert_eq!(failed, 1);
    assert_eq!(
        commands.get_for_tenant(tenant.id, old_owned.id).await.unwrap().unwrap().status,
        CommandStatus::Sent,
    );
    let failed_command = commands.get_for_tenant(tenant.id, old_unowned.id).await.unwrap().unwrap();
    assert_eq!(failed_command.status, CommandStatus::Failed);
    assert_eq!(
        failed_command.error.as_deref(),
        Some("printer link dispatch expired before completion"),
    );
}
```

Add a private test helper in `repositories/tests/commands.rs` that updates `commands.updated_at` through the test `Database` returned by `repositories()`:

```rust
async fn set_command_updated_at(database: &crate::db::Database, command_id: CommandId, updated_at: &str) {
    match database {
        crate::db::Database::Sqlite(pool) => {
            sqlx::query("UPDATE commands SET updated_at = ?2 WHERE id = ?1")
                .bind(command_id.to_string())
                .bind(updated_at)
                .execute(pool)
                .await
                .unwrap();
        }
        crate::db::Database::Postgres(pool) => {
            sqlx::query("UPDATE commands SET updated_at = $2 WHERE id = $1")
                .bind(command_id.to_string())
                .bind(updated_at)
                .execute(pool)
                .await
                .unwrap();
        }
    }
}

fn link_payload(serial: &str) -> LinkPrinterPayload {
    LinkPrinterPayload {
        host: "192.0.2.10".to_owned(),
        serial_number: serial.to_owned(),
        access_code: format!("SECRET-{serial}"),
        name: None,
        model: None,
    }
}
```

- [ ] **Step 3: Add failing Hub redaction helper tests**

Add tests to `crates/pandar-hub/src/redaction.rs`:

```rust
#[test]
fn redacts_link_printer_secret_as_key_value_and_standalone_value() {
    let message = "failed with access_code=SECRET-LINK-CODE\nCaused by:\n    printer rejected SECRET-LINK-CODE";

    let redacted = redact_link_printer_secret(message, "SECRET-LINK-CODE");

    assert!(redacted.contains("Caused by:"));
    assert!(!redacted.contains("SECRET-LINK-CODE"));
    assert!(redacted.contains("[redacted]"));
}
```

- [ ] **Step 4: Add failing gRPC conversion test for durable replay rejection**

Add to `crates/pandar-hub/src/grpc/tests/commands.rs`:

```rust
#[test]
fn grpc_hub_command_from_record_rejects_persisted_link_printer_replay() {
    let command = CommandRecord::from_parts(CommandRecordParts {
        id: CommandId::new(),
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        printer_id: None,
        kind: "link_printer".to_string(),
        status: "sent".to_string(),
        payload_json: r#"{"host":"192.0.2.10","serial_number":"SERIAL123","access_code":"[redacted]"}"#.to_string(),
        result_json: None,
        error: None,
        created_at: "2026-07-01T00:00:00Z".to_string(),
        updated_at: "2026-07-01T00:00:00Z".to_string(),
    })
    .unwrap();

    let err = hub_command_from_record(command).unwrap_err();

    assert_eq!(err.code(), Code::FailedPrecondition);
    assert_eq!(err.message(), "link printer command requires live secret dispatch");
}
```

- [ ] **Step 5: Implement proto addition**

Add to `proto/pandar/agent/v1/agent.proto`:

```proto
message HubCommand {
  string command_id = 1;
  oneof command {
    RefreshPrinters refresh_printers = 10;
    PrintProjectFile print_project_file = 11;
    DiscoverPrinters discover_printers = 12;
    DiagnosePrinter diagnose_printer = 13;
    PrinterOperation printer_operation = 14;
    LinkPrinter link_printer = 15;
  }
}

message LinkPrinter {
  string host = 1;
  string serial_number = 2;
  string access_code = 3;
  string name = 4;
  string model = 5;
}
```

Place `LinkPrinter` near the other command messages.

- [ ] **Step 6: Implement command payload structs and redaction serialization**

In `crates/pandar-hub/src/repositories/commands.rs`, add:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkPrinterPayload {
    pub host: String,
    pub serial_number: String,
    pub access_code: String,
    pub name: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactedLinkPrinterPayload {
    pub host: String,
    pub serial_number: String,
    pub access_code: String,
    pub name: Option<String>,
    pub model: Option<String>,
}

impl LinkPrinterPayload {
    pub fn redacted(&self) -> RedactedLinkPrinterPayload {
        RedactedLinkPrinterPayload {
            host: self.host.clone(),
            serial_number: self.serial_number.clone(),
            access_code: "[redacted]".to_owned(),
            name: self.name.clone(),
            model: self.model.clone(),
        }
    }
}
```

Export `LinkPrinterPayload` from `crates/pandar-hub/src/repositories/mod.rs`.

- [ ] **Step 7: Implement explicit-status insert helper**

In `crates/pandar-hub/src/repositories/commands/inserts.rs`, keep `insert` as the queued default and add:

```rust
pub async fn insert_with_status<C>(
    connection: &C,
    input: InsertCommand<'_>,
    status: CommandStatus,
) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    commands::ActiveModel {
        id: Set(input.id.to_string()),
        tenant_id: Set(input.tenant_id.to_string()),
        agent_id: Set(input.agent_id.to_string()),
        printer_id: Set(input.printer_id.map(str::to_owned)),
        kind: Set(input.kind.to_owned()),
        status: Set(status.as_str().to_owned()),
        payload_json: Set(input.payload_json.to_owned()),
        result_json: Set(None),
        error: Set(None),
        created_at: Set(input.created_at.to_owned()),
        updated_at: Set(input.created_at.to_owned()),
    }
    .insert(connection)
    .await
    .map(|_| ())
    .map_err(|err| {
        RepositoryError::Database(anyhow::Error::new(err).context("failed to insert command"))
    })
}

pub async fn insert<C>(connection: &C, input: InsertCommand<'_>) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    insert_with_status(connection, input, CommandStatus::Queued).await
}
```

- [ ] **Step 8: Implement audited sent-row creation**

In `crates/pandar-hub/src/repositories/commands/audit.rs`, add `create_link_printer_sent_with_audit` that verifies agent ownership, serializes `payload.redacted()`, inserts with `CommandStatus::Sent`, records `agent.link_printer`, commits, and returns the command. The command row insert and audit event insert must run in one database transaction; if either insert fails, neither row is committed.

Audit metadata must be:

```rust
serde_json::json!({
    "host": payload.host,
    "serial_number": payload.serial_number,
    "name": payload.name,
    "model": payload.model,
})
```

In `CommandRepository`, add a public wrapper method with the same signature.

- [ ] **Step 9: Implement stale unowned cleanup**

Add a repository method that calculates cutoff from `now - timeout`, then updates commands where:

- `kind == "link_printer"`
- `status IN ("sent", "acknowledged")`
- `updated_at < cutoff_rfc3339`
- `id NOT IN owned_command_ids`

Set:

- `status = "failed"`
- `error = "printer link dispatch expired before completion"`
- `updated_at = now`

Return affected row count.

Use SeaORM `Entity::update_many()` with `Condition::any()` for the two statuses. Keep it backend-neutral.

- [ ] **Step 10: Add failing Hub result redaction tests**

Add tests to `crates/pandar-hub/src/grpc/tests/commands.rs`:

```rust
#[tokio::test]
async fn grpc_link_printer_failed_result_redacts_access_code() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "SECRET-LINK-CODE";
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                host: "192.0.2.10".to_owned(),
                serial_number: "SERIAL123".to_owned(),
                access_code: access_code.to_owned(),
                name: None,
                model: None,
            },
            test_audit_actor(),
        )
        .await
        .unwrap();

    handle_result_and_job(
        &state,
        tenant_id,
        agent_id,
        command.id,
        false,
        format!("validation failed for access_code={access_code}"),
        String::new(),
    )
    .await
    .unwrap();

    let stored = state.commands().get_for_tenant(tenant_id, command.id).await.unwrap().unwrap();
    assert_eq!(stored.status, CommandStatus::Failed);
    assert!(!stored.error.unwrap().contains(access_code));
}

#[tokio::test]
async fn grpc_late_link_printer_result_logs_without_access_code() {
    let logs = CapturedLogs::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.writer())
        .with_ansi(false)
        .finish();
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "SECRET-LINK-CODE";
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                host: "192.0.2.10".to_owned(),
                serial_number: "SERIAL123".to_owned(),
                access_code: access_code.to_owned(),
                name: None,
                model: None,
            },
            test_audit_actor(),
        )
        .await
        .unwrap();
    state
        .commands()
        .mark_failed(command.id, tenant_id, agent_id, "printer link dispatch expired before completion")
        .await
        .unwrap();

    let _guard = tracing::subscriber::set_default(subscriber);
    let err = handle_result_and_job(
        &state,
        tenant_id,
        agent_id,
        command.id,
        false,
        format!("validation failed for access_code={access_code}"),
        String::new(),
    )
    .await
    .unwrap_err();
    tracing::error!(error = %format!("{err:#}"), "failed to process late link printer result");
    drop(_guard);

    let captured = logs.to_string();
    assert!(captured.contains("failed to process late link printer result"));
    assert!(!captured.contains(access_code));
}
```

Add a local `CapturedLogs` helper in `crates/pandar-hub/src/grpc/tests/commands.rs` if no shared helper exists for that module. The implementation should match the small `tracing_subscriber::fmt::MakeWriter` helper already used in `crates/pandar-hub/src/repositories/tests/materials/log_capture.rs`.

- [ ] **Step 11: Implement Hub redaction helper and result-error redaction**

In `crates/pandar-hub/src/redaction.rs`, add:

```rust
pub fn redact_link_printer_secret(message: &str, access_code: &str) -> String {
    let redacted = redact_secrets(message);
    if access_code.is_empty() {
        redacted
    } else {
        redacted.replace(access_code, "[redacted]")
    }
}
```

In `crates/pandar-hub/src/grpc/commands.rs`, redact command ack/result errors before storing them. After loading the command record and before calling `mark_failed` or `mark_failed_with_result`, call `crate::redaction::redact_secrets(&error)` for every command. For `link_printer`, this is the available Hub-side redaction because the raw access code is intentionally not persisted; route/runtime boundaries that still have the submitted access code must additionally call `redact_link_printer_secret`.

- [ ] **Step 12: Reject durable replay in gRPC conversion**

In `hub_command_from_record_with_options`, add:

```rust
"link_printer" => {
    tracing::error!(
        command_id = %command.id,
        "link printer command reached durable queued-command conversion"
    );
    return Err(Status::failed_precondition(
        "link printer command requires live secret dispatch",
    ));
}
```

- [ ] **Step 13: Run focused Rust tests for Task 1**

Run:

```bash
cargo test -p pandar-hub repositories::tests::commands -- --nocapture
cargo test -p pandar-hub grpc::tests::commands -- --nocapture
cargo test -p pandar-hub redaction::tests::redacts_link_printer_secret_as_key_value_and_standalone_value -- --nocapture
```

Expected: both commands exit 0.

---

### Task 2: Hub Live Session Dispatch And HTTP Route

**Files:**

- Modify: `crates/pandar-hub/src/sessions.rs`
- Modify: `crates/pandar-hub/src/grpc.rs`
- Modify: `crates/pandar-hub/src/runtime.rs`
- Modify: `crates/pandar-hub/src/routes/printers.rs`
- Modify: `crates/pandar-hub/src/routes.rs`
- Modify: `crates/pandar-hub/src/routes/tests.rs`
- Test: `crates/pandar-hub/src/routes/tests/printers.rs`
- Test: `crates/pandar-hub/src/sessions.rs`
- Test: `crates/pandar-hub/src/grpc/tests/lifecycle.rs`

**Interfaces:**

- Consumes: Task 1 `LinkPrinterPayload`, redacted sent-row repository method, proto `HubCommand::LinkPrinter`.
- Produces: `POST /api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer`.
- Produces: `SessionRegistry::current_token`, `SessionRegistry::try_dispatch_live_command`, `SessionRegistry::pending_live_command_ids` that aggregates every local session's pending set, and pending cleanup helpers.

- [ ] **Step 1: Add failing session tests for token recheck and pending cleanup**

In `crates/pandar-hub/src/sessions.rs` tests, add tests that register a session with a command sender, dispatch a dummy `HubCommand`, replace the session, and assert dispatch using the old token returns not-current without sending the command to the stale receiver.

Test shape:

```rust
#[tokio::test]
async fn sessions_live_dispatch_rechecks_token_before_send() {
    let registry = SessionRegistry::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let old_token = SessionToken::new();
    let new_token = SessionToken::new();
    let (old_command_sender, mut old_command_receiver) = mpsc::channel(1);
    let (new_command_sender, _new_command_receiver) = mpsc::channel(1);

    registry.register(test_session(tenant_id, agent_id, old_token, old_command_sender)).await;
    registry.register(test_session(tenant_id, agent_id, new_token, new_command_sender)).await;

    let err = registry
        .try_dispatch_live_command(
            tenant_id,
            agent_id,
            old_token,
            CommandId::new(),
            HubCommand { command_id: CommandId::new().to_string(), command: None },
        )
        .await
        .unwrap_err();

    assert_eq!(err, LiveDispatchError::NotCurrent);
    assert!(old_command_receiver.try_recv().is_err());
}
```

Keep helper code local to the test module.

Add a second session test that proves stale cleanup input is aggregate across sessions:

```rust
#[tokio::test]
async fn sessions_pending_live_command_ids_aggregates_all_sessions() {
    let registry = SessionRegistry::new();
    let tenant_id = TenantId::new();
    let agent_a = AgentId::new();
    let agent_b = AgentId::new();
    let token_a = SessionToken::new();
    let token_b = SessionToken::new();
    let (sender_a, _receiver_a) = mpsc::channel(2);
    let (sender_b, _receiver_b) = mpsc::channel(2);
    let command_a = CommandId::new();
    let command_b = CommandId::new();

    registry.register(test_session(tenant_id, agent_a, token_a, sender_a)).await;
    registry.register(test_session(tenant_id, agent_b, token_b, sender_b)).await;

    registry
        .try_dispatch_live_command(
            tenant_id,
            agent_a,
            token_a,
            command_a,
            HubCommand { command_id: command_a.to_string(), command: None },
        )
        .await
        .unwrap();
    registry
        .try_dispatch_live_command(
            tenant_id,
            agent_b,
            token_b,
            command_b,
            HubCommand { command_id: command_b.to_string(), command: None },
        )
        .await
        .unwrap();

    let pending = registry.pending_live_command_ids().await;

    assert!(pending.contains(&command_a));
    assert!(pending.contains(&command_b));
}
```

- [ ] **Step 2: Add failing route tests for no local session and direct send**

In `crates/pandar-hub/src/routes/tests/printers.rs`, add:

```rust
#[tokio::test]
async fn link_printer_requires_operator_role() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, _) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let agent_id = agent["id"].as_str().unwrap();
    let token = auth_token_for_role(
        &state,
        tenant_id,
        crate::repositories::UserRole::Viewer,
        "viewer-link-printer-token",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        Some(link_printer_body("SECRET-LINK-CODE")),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({ "error": "role_forbidden" }));
    assert_eq!(state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn link_printer_rejects_missing_local_session_without_command_row() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let agent_id = agent["id"].as_str().unwrap();

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        Some(link_printer_body("SECRET-LINK-CODE")),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body, json!({ "error": "agent_not_connected" }));
    assert_eq!(state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn link_printer_direct_sends_secret_but_persists_only_redacted_payload() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let (command_sender, mut command_receiver) = tokio::sync::mpsc::channel(1);
    register_route_test_session(&state, tenant_id, agent_id, command_sender).await;
    let access_code = "SECRET-LINK-CODE";

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        Some(link_printer_body(access_code)),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["kind"], "link_printer");
    assert_eq!(body["status"], "sent");
    assert!(!body.to_string().contains(access_code));
    assert_eq!(body["payload_json"].as_str().unwrap().contains(access_code), false);

    let sent = command_receiver.recv().await.unwrap().unwrap();
    match sent.command.unwrap() {
        hub_command::Command::LinkPrinter(command) => {
            assert_eq!(command.access_code, access_code);
            assert_eq!(command.host, "192.0.2.10");
            assert_eq!(command.serial_number, "SERIAL123");
        }
        other => panic!("expected link printer command, got {other:?}"),
    }
}
```

Add helpers `link_printer_body`, `register_route_test_session` in the same test module.

Add a route log-capture test in the same module, using a local `CapturedLogs` helper matching `crates/pandar-hub/src/repositories/tests/materials/log_capture.rs`, that submits `SECRET-LINK-CODE` to the no-local-session path and asserts captured logs do not contain `SECRET-LINK-CODE`. This verifies the route does not add request-body logging or tracing fields containing the request payload.

Add payload validation tests in the same module:

```rust
#[tokio::test]
async fn link_printer_rejects_blank_required_fields() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let agent_id = agent["id"].as_str().unwrap();

    for body in [
        json!({ "host": "", "serial_number": "SERIAL123", "access_code": "SECRET-LINK-CODE" }),
        json!({ "host": "192.0.2.10", "serial_number": "", "access_code": "SECRET-LINK-CODE" }),
        json!({ "host": "192.0.2.10", "serial_number": "SERIAL123", "access_code": "" }),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
            Some(body),
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body, json!({ "error": "bad_request" }));
    }

    assert_eq!(state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn link_printer_rejects_unknown_fields() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let agent_id = agent["id"].as_str().unwrap();

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        Some(json!({
            "host": "192.0.2.10",
            "serial_number": "SERIAL123",
            "access_code": "SECRET-LINK-CODE",
            "unexpected": true
        })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "bad_request" }));
    assert_eq!(state.commands().count().await.unwrap(), 0);
}
```

- [ ] **Step 3: Add failing session test for replacement race after row creation**

In `crates/pandar-hub/src/sessions.rs` tests, add a second test that creates a command ID, registers an old session, captures the old token, registers a replacement session, and calls `try_dispatch_live_command` with the old token and a `HubCommand::LinkPrinter`. Assert it returns `LiveDispatchError::NotCurrent`, the old receiver receives nothing, and `pending_live_command_ids()` does not contain the command ID. The route will map this `NotCurrent` result to a failed command after Task 2 Step 9.

- [ ] **Step 4: Implement session fields and helpers**

In `AgentSession`, add:

```rust
pub command_sender: mpsc::Sender<Result<HubCommand, Status>>,
pub pending_live_commands: Arc<std::sync::Mutex<std::collections::HashSet<CommandId>>>,
```

Add `LiveDispatchError`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveDispatchError {
    NotCurrent,
    ChannelClosed,
    ChannelFull,
}
```

Add methods:

```rust
pub async fn current_token(&self, tenant_id: TenantId, agent_id: AgentId) -> Option<SessionToken>;

pub async fn try_dispatch_live_command(
    &self,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    command_id: CommandId,
    command: HubCommand,
) -> Result<(), LiveDispatchError>;

pub async fn pending_live_command_ids(&self) -> Vec<CommandId>;

pub async fn remove_pending_live_command(
    &self,
    agent_id: AgentId,
    token: SessionToken,
    command_id: CommandId,
) -> bool;
```

`try_dispatch_live_command` must hold the session map lock for token check, pending insert, and `try_send`. `pending_live_command_ids` must iterate over every current local session in the registry and return the union of every session-scoped pending set. `remove_pending_live_command` must hold the session map lock, verify the stored session token still matches the token passed by the inbound gRPC handler, and remove the command ID only from that current session's pending set.

- [ ] **Step 5: Register command sender in gRPC sessions**

In `AgentControlService::connect_stream`, create `(command_sender, command_receiver)` before `AgentSession` registration and store `command_sender.clone()` plus a fresh pending set in the session. Keep passing `command_sender` to `spawn_outbound_pump` as before.

- [ ] **Step 6: Remove pending IDs on terminal command result**

In the `CommandResult` branch of `handle_event`, keep using the inbound stream's `token` and the existing `while_current(agent_id, token, ...)` guard. Change `handle_result` to return the parsed `CommandId` on success, or parse the command ID before calling `handle_result_and_job`. After `handle_result_and_job` returns success, call `state.sessions().remove_pending_live_command(agent_id, token, command_id).await`. This removal is token-scoped to the current session and must not remove a command ID from a replacement session. For failed-precondition late results after stale cleanup, log redacted context and do not resurrect terminal failed commands; leave the pending ID in place so stream-close cleanup still owns it if the same session is still current.

- [ ] **Step 7: Fail pending live commands on stream close**

In `spawn_inbound_handler`, use the `AgentSession` returned by `remove_if_current`. Drain its pending set and call `state.commands().mark_failed(...)` for each ID with `agent connection closed before printer link completed`. Preserve full context in logs with `{err:#}`; this error string contains no secret.

- [ ] **Step 8: Add stale unowned cleanup runtime hook**

In `runtime.rs`, update the existing session expiry loop or add a small loop to call `state.commands().fail_stale_unowned_link_printer_commands(&now, Duration::from_secs(300), &state.sessions().pending_live_command_ids().await)`. Because `pending_live_command_ids` aggregates all sessions, stale cleanup must skip active pending commands for every local agent stream. Log errors with `{err:#}` after redacting with `crate::redaction::redact_secrets`.

Add a runtime or repository test that captures logs for a stale cleanup failure carrying `access_code=SECRET-LINK-CODE` in its error context and asserts `SECRET-LINK-CODE` is absent from captured logs. Use the same local `CapturedLogs` pattern as the route log-capture test.

- [ ] **Step 9: Implement HTTP route and API error mapping**

In `routes.rs`, add:

```rust
.route(
    "/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer",
    post(printers::link_printer),
)
```

In `routes/printers.rs`, add request type:

```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LinkPrinterRequest {
    host: String,
    serial_number: String,
    access_code: String,
    name: Option<String>,
    model: Option<String>,
}
```

Normalize with `trim_required` and `trim_optional` helpers. Do not add request-body logging, `Debug` tracing, or tracing span fields for `LinkPrinterRequest` or `LinkPrinterPayload`; only log route metadata such as tenant ID, agent ID, command ID, and redacted errors. Build `LinkPrinterPayload`, capture current token before creating the command, create redacted sent command, build proto `HubCommand::LinkPrinter` with `name` and `model` mapped to empty proto strings when absent, then call `try_dispatch_live_command`.

Post-row dispatch error handling must be:

- `LiveDispatchError::NotCurrent`: mark the command failed with `agent connection closed before printer link completed`, load the command, and return `200` with the failed command response.
- `LiveDispatchError::ChannelClosed` or `LiveDispatchError::ChannelFull`: remove the command ID from the pending set inside `try_dispatch_live_command`, mark the command failed with `agent command channel unavailable before printer link completed`, load the command, and return `200` with the failed command response.
- If marking failed or loading the failed command fails after any post-row dispatch error, log the full redacted error chain and return `500 { "error": "internal_server_error" }`.

Add a unit test for a route-local helper with this shape:

```rust
async fn fail_link_printer_dispatch_after_commit<F, Fut>(
    command_id: CommandId,
    tenant_id: TenantId,
    agent_id: AgentId,
    payload: &LinkPrinterPayload,
    mark_failed: F,
) -> Result<CommandRecord, ApiError>
where
    F: FnOnce(CommandId, TenantId, AgentId, String) -> Fut,
    Fut: Future<Output = RepositoryResult<CommandRecord>>,
```

The production route passes a closure that calls `state.commands().mark_failed(...)` and then loads/returns the command. The test passes a closure returning `RepositoryError::Database(anyhow!("failed while handling access_code=SECRET-LINK-CODE"))`, asserts the helper returns `ApiError { status: INTERNAL_SERVER_ERROR, code: "internal_server_error" }`, and asserts captured logs do not contain `SECRET-LINK-CODE`.

Add `ApiError` mapping for `agent_not_connected` with `StatusCode::CONFLICT`; this can be a route-local error branch rather than a repository error.

- [ ] **Step 10: Run focused Hub route/session tests**

Run:

```bash
cargo test -p pandar-hub sessions::tests::sessions_live_dispatch_rechecks_token_before_send -- --nocapture
cargo test -p pandar-hub routes::tests::printers::link_printer -- --nocapture
cargo test -p pandar-hub grpc::tests::lifecycle -- --nocapture
```

Expected: all exit 0.

---

### Task 3: Agent Runtime Gateway And Link Command Handling

**Files:**

- Create: `crates/pandar-agent/src/machine/runtime.rs`
- Modify: `crates/pandar-agent/src/machine/mod.rs`
- Modify: `crates/pandar-agent/src/commands.rs`
- Modify: `crates/pandar-agent/src/lib.rs`
- Test: `crates/pandar-agent/src/commands/tests.rs`
- Test: `crates/pandar-agent/src/machine/tests.rs`

**Interfaces:**

- Consumes: proto `LinkPrinter` from Task 1.
- Produces: `BambuMachineGateway::link_printer(...)` default method and `RuntimeBambuMachineGateway` real implementation.
- Produces: agent `CommandResult.result_json` for `{"type":"printer_link", ...}`.
- Produces: runtime gateway that replaces the current no-printer `NoopMachineGateway` command loop.

- [ ] **Step 1: Add failing command handler tests for LinkPrinter**

In `crates/pandar-agent/src/commands/tests.rs`, add a fake gateway implementing the new `link_printer` method and tests:

```rust
#[tokio::test]
async fn link_printer_emits_ack_snapshot_and_success_without_access_code() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = LinkGateway::success(snapshot("SERIAL123", "Office X1C", Some("X1 Carbon"), "READY"));
    let (sender, mut receiver) = mpsc::channel(3);
    let access_code = "SECRET-LINK-CODE";

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        link_printer_command(command_id.clone(), access_code),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(receiver.recv().await.unwrap(), ack_event(&config, &command_id));
    assert_snapshot(receiver.recv().await.unwrap(), "SERIAL123", "Office X1C", "X1 Carbon", "READY");
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert!(result.success);
            assert!(!result.result_json.contains(access_code));
            let json: serde_json::Value = serde_json::from_str(&result.result_json).unwrap();
            assert_eq!(json["type"], "printer_link");
            assert_eq!(json["serial_number"], "SERIAL123");
            assert_eq!(json["host"], "192.0.2.10");
            assert_eq!(json["status"], "online");
        }
        other => panic!("expected command result, got {other:?}"),
    }
}
```

The `link_printer_command` test helper must build the command ID on the outer `HubCommand`, not on the inner `LinkPrinter` payload:

```rust
fn link_printer_command(command_id: String, access_code: &str) -> HubCommand {
    HubCommand {
        command_id,
        command: Some(hub_command::Command::LinkPrinter(LinkPrinter {
            host: "192.0.2.10".to_owned(),
            serial_number: "SERIAL123".to_owned(),
            access_code: access_code.to_owned(),
            name: "Office X1C".to_owned(),
            model: "X1 Carbon".to_owned(),
        })),
    }
}
```

Also add a failure test proving `SECRET-LINK-CODE` is absent from the failed result error, and a log-capture assertion using the same `CapturedLogs` pattern as `crates/pandar-agent/src/machine/mqtt/tests.rs` proving validation failure logs do not contain `SECRET-LINK-CODE`.

- [ ] **Step 2: Add failing runtime gateway tests**

In `crates/pandar-agent/src/machine/tests.rs`, add tests for the runtime gateway with fake transports. If a generic factory is too much for production code, keep the production runtime gateway simple and test equivalent replacement semantics through a test-only runtime gateway type in `runtime.rs`.

Required test cases:

- Empty runtime gateway `refresh_printers()` returns empty.
- Successful `link_printer` installs an endpoint and later `refresh_printers()` returns that snapshot.
- Same-serial replacement after validation success replaces endpoint metadata and leaves one report task handle.
- Same-serial replacement after validation failure leaves previous endpoint active.
- Concurrent same-serial `link_printer` calls are serialized by the runtime gateway lock; arrange one fake validation to pause while a second call starts, then assert the second call cannot replace the endpoint until the first releases the lock.
- Report-forwarding preparation failure before spawn leaves previous endpoint active.

- [ ] **Step 3: Extend `BambuMachineGateway` trait**

In `machine/mod.rs`, add default method:

```rust
async fn link_printer(
    &self,
    endpoint: BambuPrinterEndpoint,
    config: &crate::AgentConfig,
    sender: &tokio::sync::mpsc::Sender<crate::protocol::agent::v1::AgentEvent>,
) -> anyhow::Result<MachineSnapshot> {
    let _ = (endpoint, config, sender);
    bail!("runtime printer linking is not supported by this gateway")
}
```

Keep existing fake gateways compiling by relying on the default.

- [ ] **Step 4: Implement `LinkPrinter` command handling**

In `commands.rs`, import `LinkPrinter`. Preserve the outer `HubCommand.command_id` before matching the oneof payload and pass that ID into the link-printer handler:

```rust
let command_id = command.command_id.clone();
match command.command {
    Some(hub_command::Command::LinkPrinter(link)) => {
        emit_link_printer_events(config, gateway, sender, &command_id, link).await
    }
    // existing command arms stay on their current behavior
}
```

Implement `emit_link_printer_events`:

1. Convert proto fields into `BambuPrinterEndpoint` with `name/model` as `None` when blank.
2. Send accepted ack.
3. Call `gateway.link_printer(endpoint.clone(), config, sender).await`.
4. On success, send `printer_snapshot_event(config, snapshot.clone())` and success result JSON.
5. On failure, format `{err:#}`, redact through `gateway.redact_error` and an explicit submitted-access-code replacement, log only the redacted string, and send the redacted failed result.

Use result shape:

```rust
serde_json::json!({
    "type": "printer_link",
    "serial_number": snapshot.serial,
    "host": endpoint.host,
    "name": snapshot.name,
    "model": snapshot.model,
    "status": snapshot.state,
})
```

- [ ] **Step 5: Implement runtime gateway module**

Create `machine/runtime.rs` with a production type:

```rust
pub struct RuntimeBambuMachineGateway {
    inner: tokio::sync::Mutex<ConfiguredBambuMachineGateway<RumqttcBambuMqttTransport>>,
    report_tasks: tokio::sync::Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    config: AgentConfig,
    sender: mpsc::Sender<AgentEvent>,
    report_timeout: Duration,
}
```

Expose:

```rust
impl RuntimeBambuMachineGateway {
    pub fn new(
        config: AgentConfig,
        printers: Vec<BambuPrinterEndpoint>,
        sender: mpsc::Sender<AgentEvent>,
        report_timeout: Duration,
    ) -> Self;

    pub async fn start_initial_report_forwarders(&self);
}
```

Implementation rules:

- `new` builds command transports through `RumqttcBambuMqttTransport::connect` and an inner `ConfiguredBambuMachineGateway`.
- Startup report forwarding uses `RumqttcBambuMqttTransport::connect_for_reports` per endpoint.
- `link_printer` builds a command transport for the endpoint, locks `inner`, validates by calling `refresh_printer(&transport, &endpoint, report_timeout)`, replaces the endpoint in the inner configured gateway only after validation succeeds, aborts any old report task for the serial, starts exactly one replacement report task, and returns the validation snapshot.
- If validation fails, do not mutate inner gateway or report task map.
- If report task spawn preparation fails before spawn, do not mutate inner gateway. In production, preparation is constructing the report transport and task inputs before `tokio::spawn`; tests should cover this through a test-only runtime gateway constructor that can fail before spawning.

To make replacement possible, add a small method to `ConfiguredBambuMachineGateway`:

```rust
pub fn replace_printer(&mut self, endpoint: BambuPrinterEndpoint, mqtt: T, transfer: F)
```

For production with default FTPS, create `FtpsMachineFileTransfer::new(endpoint.clone())` in `RuntimeBambuMachineGateway` and call `replace_printer`.

- [ ] **Step 6: Update `run_once` to always use runtime gateway**

In `lib.rs`, remove the branch that permanently uses `NoopMachineGateway` when startup printers are empty. After opening the reverse stream and spawning heartbeat, create:

```rust
let gateway = RuntimeBambuMachineGateway::new(
    config.clone(),
    printers,
    sender.clone(),
    DEFAULT_REPORT_TIMEOUT,
);
gateway.start_initial_report_forwarders().await;
while let Some(command) = commands.next().await.transpose().context("read hub command from reverse stream")? {
    handle_command_with_gateway(&config, &gateway, &sender, command).await?;
}
```

- [ ] **Step 7: Run focused agent tests**

Run:

```bash
cargo test -p pandar-agent commands::tests::link_printer -- --nocapture
cargo test -p pandar-agent machine::tests::runtime -- --nocapture
cargo test -p pandar-agent --lib -- --nocapture
```

Expected: all exit 0.

---

### Task 4: Frontend Link Form And Command Result Rendering

**Files:**

- Modify: `frontend/app/actions.ts`
- Modify: `frontend/app/action-status.ts`
- Modify: `frontend/app/command-result-parser.ts`
- Modify: `frontend/app/dashboard-types.ts`
- Modify: `frontend/app/dashboard-view-content.tsx`
- Modify: `frontend/app/diagnostics-panel.tsx`
- Modify: `frontend/app/agent-pairing-guidance.test.tsx`
- Modify: `frontend/app/action-status-toast.test.tsx`
- Modify: `frontend/messages/en.json`
- Modify: `frontend/messages/zh.json`

**Interfaces:**

- Consumes: Hub `POST /link-printer` route and `printer_link` command result JSON.
- Produces: `linkPrinter(formData)` server action.
- Produces: `PrinterLinkResultData` union variant and result renderer.
- Produces: `/agents` link-printer form placed after pairing guidance and before linked agents.

- [ ] **Step 1: Add failing frontend tests for form placement and fields**

Extend `frontend/app/agent-pairing-guidance.test.tsx` mock actions with `linkPrinter: vi.fn()`. Add:

```tsx
it("renders link-printer form between pairing guidance and linked agents", () => {
  renderAgentsView({
    agents: [
      {
        id: "agent-online",
        tenant_id: tenant.id,
        name: "Online agent",
        status: "online",
        created_at: "2026-06-30T00:00:00Z",
      },
      {
        id: "agent-offline",
        tenant_id: tenant.id,
        name: "Offline agent",
        status: "offline",
        created_at: "2026-06-30T00:00:00Z",
      },
    ],
  });

  const pairingHeading = screen.getByRole("heading", {
    name: "Pair a local agent",
  });
  const linkHeading = screen.getByRole("heading", {
    name: "Link printer to agent",
  });
  const linkedAgentsHeading = screen.getByRole("heading", {
    name: "Linked agents",
  });

  expect(
    pairingHeading.compareDocumentPosition(linkHeading) &
      Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();
  expect(
    linkHeading.compareDocumentPosition(linkedAgentsHeading) &
      Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();
  expect(screen.getByLabelText("Agent")).toHaveValue("agent-online");
  expect(screen.getByLabelText("Host or IP address")).toHaveAttribute(
    "name",
    "host",
  );
  expect(screen.getByLabelText("Serial number")).toHaveAttribute(
    "name",
    "serial_number",
  );
  expect(screen.getByLabelText("Access code")).toHaveAttribute(
    "type",
    "password",
  );
  expect(screen.getByRole("button", { name: "Link printer" })).toHaveAttribute(
    "type",
    "submit",
  );
});
```

Add no-tenant/no-agent empty state assertions.

- [ ] **Step 2: Add failing parser/result tests**

Create or extend a frontend test file for `parseCommandResult`:

```ts
it("parses printer link command results", () => {
  const parsed = parseCommandResult({
    id: "cmd1",
    tenant_id: "tenant1",
    agent_id: "agent1",
    printer_id: null,
    kind: "link_printer",
    status: "succeeded",
    payload_json: "{}",
    error: null,
    result_json: JSON.stringify({
      type: "printer_link",
      serial_number: "SERIAL123",
      host: "192.0.2.10",
      name: "Office X1C",
      model: "X1 Carbon",
      status: "READY",
    }),
    created_at: "2026-07-01T00:00:00Z",
    updated_at: "2026-07-01T00:00:00Z",
  });

  expect(parsed).toEqual({
    type: "printer_link",
    serial_number: "SERIAL123",
    host: "192.0.2.10",
    name: "Office X1C",
    model: "X1 Carbon",
    status: "READY",
  });
});
```

- [ ] **Step 3: Implement server action**

In `actions.ts`, add:

```ts
export async function linkPrinter(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const agentId = stringField(formData, "agent_id");
  const response = await postJson(
    `/api/v1/tenants/${tenantId}/agents/${agentId}/link-printer`,
    {
      host: stringField(formData, "host"),
      serial_number: stringField(formData, "serial_number"),
      access_code: stringField(formData, "access_code"),
      name: nullableField(formData, "name"),
      model: nullableField(formData, "model"),
    },
  );
  if (!response.ok) {
    redirect(agentsStatusUrl(tenantId, await errorCode(response)));
  }
  const command = (await response.json()) as { id: string };
  redirect(commandUrl(tenantId, command.id));
}
```

- [ ] **Step 4: Implement result type and parser**

In `dashboard-types.ts`, add:

```ts
export type PrinterLinkResultData = {
  type: "printer_link";
  serial_number: string;
  host: string;
  name?: string;
  model?: string;
  status: string;
};

export type CommandResultData =
  | DiscoveryResultData
  | DiagnosticResultData
  | PrinterLinkResultData;
```

In `command-result-parser.ts`, parse `printer_link` only when `serial_number`, `host`, and `status` are strings.

- [ ] **Step 5: Implement link-printer form UI**

In `dashboard-view-content.tsx`, add a `LinkPrinterForm` component or extract it into a new file if the file exceeds 400 LOC. Render in `AgentsView`:

```tsx
<AgentPairingGuidance selectedTenant={selectedTenant} restricted={adminUnavailable} />
<LinkPrinterForm selectedTenant={selectedTenant} agents={agents} />
<LinkedAgentsSection selectedTenant={selectedTenant} agents={agents} />
```

Form rules:

- `selectedTenant === null`: show empty state.
- `agents.length === 0`: show empty state.
- Agent select labels include name and status; default value is first online agent or first agent.
- Access code input uses `type="password"` and `autoComplete="off"`.
- Include optional name/model inputs.

- [ ] **Step 6: Render printer link command result**

In `diagnostics-panel.tsx`, add `PrinterLinkResult` next to `DiscoveryResult` and `DiagnosticResult`:

```tsx
function PrinterLinkResult({ result }: { result: PrinterLinkResultData }) {
  const t = useTranslations("diagnostics");
  return (
    <div className="grid gap-3 px-4 py-3 text-sm sm:grid-cols-2 lg:grid-cols-5">
      <DetailLine label={t("colHost")} value={result.host} mono />
      <DetailLine label={t("colSerial")} value={result.serial_number} mono />
      <DetailLine label={t("colName")} value={result.name ?? "-"} />
      <DetailLine label={t("colModel")} value={result.model ?? "-"} />
      <DetailLine
        label={t("colStatus")}
        value={<StatusBadge value={result.status} />}
      />
    </div>
  );
}
```

If `DetailLine` is not imported, import it from `dashboard-ui`.

- [ ] **Step 7: Add translations and status helper coverage**

Add `agent_not_connected` to `runtime.actionStatus` in English and Chinese as an operator-facing error. Do not add it to `knownPositiveActionStatuses`.

English copy:

```json
"agent_not_connected": "Agent is not connected to this Hub process"
```

Chinese copy:

```json
"agent_not_connected": "Agent 未连接到当前 Hub 进程"
```

Add a test in `action-status-toast.test.tsx`:

```ts
expect(actionStatusTone("agent_not_connected")).toBe("error");
expect(formatActionStatus("agent_not_connected", tStatus)).toBe(
  "Agent is not connected to this Hub process",
);
```

- [ ] **Step 8: Run focused frontend tests**

Run:

```bash
npm run test:web -- --run frontend/app/agent-pairing-guidance.test.tsx frontend/app/action-status-toast.test.tsx frontend/app/command-result-parser.test.ts
```

If Vitest does not accept those paths through the root script, run:

```bash
npm --prefix frontend run test -- --run app/agent-pairing-guidance.test.tsx app/action-status-toast.test.tsx app/command-result-parser.test.ts
```

Expected: tests exit 0.

---

### Task 5: Documentation, Roadmap, And Integration Checks

**Files:**

- Modify: `docs/development.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/architecture.md`

**Interfaces:**

- Consumes: implemented behavior from Tasks 1-4.
- Produces: operator-facing runtime-link limitation docs, architecture boundary note, and roadmap completion entry.

- [ ] **Step 1: Update development docs**

In `docs/development.md`, near the `PANDAR_PRINTERS` and agent command sections, add:

```md
Printers can also be linked at runtime from the dashboard Agents page. Runtime-linked printers are held only in the running `pandar-agent` process: they do not survive an agent restart unless the same printer is added to `PANDAR_PRINTERS`. The Hub never stores the submitted Bambu access code. In multi-Hub deployments, runtime linking must be routed to the Hub process that owns the agent's reverse gRPC stream; otherwise the API returns `agent_not_connected` and no command row is created.
```

- [ ] **Step 2: Update architecture docs**

In `docs/architecture.md`, near the Hub command ledger or agent machine-transport sections, add a concise note:

```md
Runtime printer linking is the exception to durable command replay: the Hub stores only a redacted `link_printer` command record and sends the Bambu access code directly to the currently connected local agent stream. If the local stream is unavailable, the request fails with `agent_not_connected`; in-flight live-only commands are failed by session/stale cleanup instead of replayed after restart.
```

- [ ] **Step 3: Update roadmap**

In `docs/roadmap.md`, under `## Completed`, add a bullet:

```md
- Added runtime printer linking from the dashboard Agents page: operators can submit host/IP, serial, access code, and optional name/model to a locally connected agent; Hub stores only redacted command/audit data, the agent validates and links the printer in memory, and runtime-linked printers are documented as non-persistent across agent restarts unless also configured in `PANDAR_PRINTERS`.
```

- [ ] **Step 4: Run formatting**

Run:

```bash
cargo fmt
npx prettier --write docs/development.md docs/roadmap.md frontend/messages/en.json frontend/messages/zh.json
```

If `npx prettier` is unavailable, run `nix fmt` during final verification and include that output.

- [ ] **Step 5: Run focused integration checks**

Run:

```bash
cargo test -p pandar-hub routes::tests::printers::link_printer -- --nocapture
cargo test -p pandar-hub repositories::tests::commands -- --nocapture
cargo test -p pandar-agent commands::tests::link_printer -- --nocapture
npm run test:web -- --run frontend/app/agent-pairing-guidance.test.tsx frontend/app/action-status-toast.test.tsx frontend/app/command-result-parser.test.ts
```

Expected: all exit 0.

---

### Task 6: Full Verification And Final Review Prep

**Files:**

- No planned source edits unless verification exposes an issue.

**Interfaces:**

- Consumes: all implemented tasks.
- Produces: clean verification evidence and final review package.

- [ ] **Step 1: Run Rust formatting and lint**

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: both exit 0.

- [ ] **Step 2: Run Rust workspace tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: exits 0.

- [ ] **Step 3: Run frontend tests and build**

Run:

```bash
npm run test:web
npm run build:web
```

Expected: both exit 0.

- [ ] **Step 4: Inspect diff for secret leaks**

Run:

```bash
rg -n "SECRET-LINK-CODE|SECRET|access_code" crates/pandar-hub crates/pandar-agent frontend docs/superpowers/specs/2026-07-01-agent-printer-linking-design.md docs/superpowers/plans/2026-07-01-agent-printer-linking.md docs/development.md docs/roadmap.md
```

Expected: test-only secret literals may appear only in tests and plan/spec examples. Runtime code must only use field names, redaction helpers, or `[redacted]`/`[REDACTED_ACCESS_CODE]` strings, not real access-code values.

- [ ] **Step 5: Review git status and diff**

Run:

```bash
git status --short
git diff --stat
git diff --check
```

Expected: only intended files changed; `git diff --check` exits 0.

- [ ] **Step 6: Prepare final implementation review inputs**

Record:

```bash
git rev-parse HEAD
git diff --stat
git diff -U10 > /tmp/pandar-agent-printer-linking-final.diff
```

Use the SDD workflow final implementation review gate with the spec path, this plan path, final diff, and verification output.
