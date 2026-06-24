# Phase 27 Reference-Backed Live Printer Controls Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build typed pause, resume, stop, and print-speed controls that flow from the dashboard through Hub durable commands to agent MQTT publish, while preserving command-vs-physical-state separation.

**Architecture:** Move printer compatibility policy into `pandar-core`, use Hub as the authoritative compatibility and authorization gate, serialize accepted controls as `printer_control` commands, and let the agent publish only typed reference-backed MQTT payloads. The UI mirrors compatibility for disabled states, but the Hub API remains authoritative.

**Tech Stack:** Rust workspace with axum, SeaORM/sqlx-backed repositories, tonic/prost protobuf generation, tokio tests, Next.js server actions and React components.

---

## File Map

- Create `crates/pandar-core/src/compatibility.rs`: shared model normalization and capability policy.
- Modify `crates/pandar-core/src/lib.rs`: export compatibility module.
- Modify `crates/pandar-agent/src/machine/compatibility.rs`: re-export shared compatibility definitions, preserving existing agent import paths.
- Modify `crates/pandar-agent/src/machine/mod.rs`: add typed `PrinterControl` gateway API and configured MQTT dispatch.
- Modify `crates/pandar-agent/src/commands.rs`: handle protobuf `PrinterControl`, validate action/speed, emit ack/result events.
- Modify `crates/pandar-agent/src/commands/tests.rs`: add command handler tests and fake gateway support.
- Modify `crates/pandar-agent/src/machine/tests.rs`: add configured gateway MQTT publish tests.
- Modify `proto/pandar/agent/v1/agent.proto`: add `PrinterControl` message and `HubCommand` oneof entry.
- Modify `crates/pandar-hub/src/repositories/commands.rs`, `enqueue.rs`, `audit.rs`, `ownership.rs`, `mod.rs`: add payload/action types, enqueue-with-audit, ownership/model lookup, exports.
- Modify `crates/pandar-hub/src/repositories/mod.rs`: add `PrinterControlUnavailable` error and test helper for printer model fixtures.
- Modify `crates/pandar-hub/src/routes.rs`, `routes/printers.rs`, `routes/tests/printer_commands.rs`: add endpoint, request validation, API error mapping, route tests.
- Modify `crates/pandar-hub/src/grpc/commands.rs`, `grpc/tests/commands.rs`: convert persisted `printer_control` records to protobuf commands.
- Modify `frontend/app/actions.ts`, `frontend/app/recovery-actions.tsx`: add server action and rendered controls.
- Update docs: `docs/compatibility/phase-27-live-printer-controls.md`, `docs/roadmap.md`, `docs/development.md`, and any stale architecture wording found by search.

---

### Task 1: Shared Compatibility Policy

**Files:**
- Create: `crates/pandar-core/src/compatibility.rs`
- Modify: `crates/pandar-core/src/lib.rs`
- Modify: `crates/pandar-agent/src/machine/compatibility.rs`

- [ ] **Step 1: Write failing core compatibility tests**

Add tests in `crates/pandar-core/src/compatibility.rs` under `#[cfg(test)] mod tests`:

```rust
#[test]
fn live_controls_are_supported_only_for_known_phase_27_models() {
    assert!(live_controls_supported(Some("A1")));
    assert!(live_controls_supported(Some("A1 Mini")));
    assert!(live_controls_supported(Some("N7")));
    assert!(live_controls_supported(Some("N6")));
    assert!(!live_controls_supported(None));
    assert!(!live_controls_supported(Some("Mystery Model")));
}

#[test]
fn compatibility_serializes_live_controls_capability() {
    let value = serde_json::to_value(compatibility_for_model(Some("A1 Mini"))).unwrap();

    assert_eq!(value["normalized_model"], "A1_MINI");
    assert_eq!(value["features"]["live_controls"], "supported");
}
```

- [ ] **Step 2: Run the focused failing test**

Run:

```bash
cargo test -p pandar-core compatibility::tests::live_controls_are_supported_only_for_known_phase_27_models
```

Expected: FAIL because `pandar_core::compatibility` does not exist yet.

- [ ] **Step 3: Implement shared compatibility module**

Move the current definitions from `crates/pandar-agent/src/machine/compatibility.rs` into `crates/pandar-core/src/compatibility.rs`, add `live_controls` to `CompatibilityFeatures`, and add:

```rust
pub fn live_controls_supported(model: Option<&str>) -> bool {
    compatibility_for_model(model).features.live_controls == Capability::Supported
}
```

Set `live_controls: Capability::Supported` for `A1`, `A1_MINI`, `P2S`, and `X2D`; keep `Unknown` for missing/unknown models.
Preserve the existing aliases in `normalize_model`, including `N7 -> P2S`, `N6 -> X2D`, `BAMBULABA1 -> A1`, and `A1 Mini` spellings, so the tests for `N7` and `N6` pass through normalization rather than separate matrix rows.

Update the existing whole-struct serialization assertion `absent_model_serializes_null_and_unknown_features` so the expected JSON includes `"live_controls": "unknown"` under `features`. Review any other exact `CompatibilityFeatures` serialization assertions touched by the move and add the new field there.

Export it from `crates/pandar-core/src/lib.rs`:

```rust
pub mod compatibility;
```

Replace `crates/pandar-agent/src/machine/compatibility.rs` with a re-export:

```rust
pub use pandar_core::compatibility::*;
```

- [ ] **Step 4: Run compatibility tests**

Run:

```bash
cargo test -p pandar-core compatibility
cargo test -p pandar-agent machine::compatibility
```

Expected: PASS. Existing agent compatibility tests should still pass through the re-export.

### Task 2: Proto And Hub Command Conversion

**Files:**
- Modify: `proto/pandar/agent/v1/agent.proto`
- Modify: `crates/pandar-hub/src/repositories/mod.rs`
- Modify: `crates/pandar-hub/src/repositories/commands.rs`
- Modify: `crates/pandar-hub/src/grpc/commands.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/commands.rs`

- [ ] **Step 1: Write failing gRPC conversion test**

In `crates/pandar-hub/src/grpc/tests/commands.rs`, add a test next to `grpc_hub_command_from_record_maps_discovery_and_diagnostics`:

```rust
#[tokio::test]
async fn grpc_hub_command_from_record_maps_printer_control() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = PrinterControlPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
        action: PrinterControlAction::SetPrintSpeed,
        speed_mode: Some(4),
    };
    let command = CommandRecord::from_parts(CommandRecordParts {
        id: CommandId::new(),
        tenant_id,
        agent_id,
        printer_id: Some(printer_id),
        kind: "printer_control".to_string(),
        status: "queued".to_string(),
        payload_json: serde_json::to_string(&payload).unwrap(),
        result_json: None,
        error: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    })
    .unwrap();

    let hub_command = hub_command_from_record(command).unwrap();

    assert!(matches!(
        hub_command.command,
        Some(hub_command::Command::PrinterControl(command))
            if command.serial_number == "SERIAL123"
                && command.action == "set_print_speed"
                && command.speed_mode == 4
    ));
}
```

Also import `AgentId`, `CommandId`, `CommandRecord`, `CommandRecordParts`, `TenantId`, `PrinterControlAction`, and `PrinterControlPayload` in that test file.

- [ ] **Step 2: Run the focused failing test**

Run:

```bash
cargo test -p pandar-hub grpc_hub_command_from_record_maps_printer_control
```

Expected: FAIL because proto/repository conversion types are not implemented.

- [ ] **Step 3: Add proto message**

Modify `proto/pandar/agent/v1/agent.proto`:

```proto
message HubCommand {
  string command_id = 1;
  oneof command {
    RefreshPrinters refresh_printers = 10;
    PrintProjectFile print_project_file = 11;
    DiscoverPrinters discover_printers = 12;
    DiagnosePrinter diagnose_printer = 13;
    PrinterControl printer_control = 14;
  }
}

message PrinterControl {
  string serial_number = 1;
  string action = 2;
  uint32 speed_mode = 3;
}
```

- [ ] **Step 4: Add repository payload/action types**

In `crates/pandar-hub/src/repositories/commands.rs`, add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrinterControlAction {
    Pause,
    Resume,
    Stop,
    SetPrintSpeed,
}

impl PrinterControlAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Stop => "stop",
            Self::SetPrintSpeed => "set_print_speed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrinterControlPayload {
    pub printer_id: String,
    pub serial_number: String,
    pub action: PrinterControlAction,
    pub speed_mode: Option<u8>,
}
```

Do not add a basic `enqueue_printer_control(...)` method here. Task 3 adds the audited enqueue path that validates tenant/printer ownership, model compatibility, and speed-mode semantics.

In `crates/pandar-hub/src/repositories/mod.rs`, immediately add the new public re-exports so the gRPC conversion code and tests compile in this task:

```rust
pub use commands::{
    CommandRepository, DiagnosePrinterPayload, DiscoverPrintersPayload, PrintProjectFilePayload,
    PrinterControlAction, PrinterControlPayload,
};
```

- [ ] **Step 5: Convert command records to proto**

In `crates/pandar-hub/src/grpc/commands.rs`, import `PrinterControl` and `PrinterControlPayload`, then add a `printer_control` match arm:

```rust
"printer_control" => {
    let payload: PrinterControlPayload = serde_json::from_str(&command.payload_json)
        .map_err(|err| {
            tracing::error!(
                command_id = %command.id,
                error = %format!("{err:#}"),
                "failed to deserialize printer control command payload"
            );
            Status::internal("invalid printer control command payload")
        })?;
    hub_command::Command::PrinterControl(PrinterControl {
        serial_number: payload.serial_number,
        action: payload.action.as_str().to_string(),
        speed_mode: payload.speed_mode.map(u32::from).unwrap_or(0),
    })
}
```

- [ ] **Step 6: Run focused gRPC conversion test**

Run:

```bash
cargo test -p pandar-hub grpc_hub_command_from_record_maps_printer_control
```

Expected: PASS.

### Task 3: Hub Repository, Audit, And API Route

**Files:**
- Modify: `crates/pandar-hub/src/repositories/mod.rs`
- Modify: `crates/pandar-hub/src/repositories/commands.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/enqueue.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/audit.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/ownership.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/commands.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/postgres_commands.rs`
- Modify: `crates/pandar-hub/src/routes.rs`
- Modify: `crates/pandar-hub/src/routes/printers.rs`
- Modify: `crates/pandar-hub/src/routes/tests/printer_commands.rs`

- [ ] **Step 1: Write failing route tests**

Add tests to `crates/pandar-hub/src/routes/tests/printer_commands.rs`:

```rust
#[tokio::test]
async fn printer_control_requires_operator_role() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let agent = state.agents().create(tenant.id, "shop-agent").await.unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant.id,
        agent.id,
        Some("A1 Mini"),
    )
    .await
    .unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "viewer-control-token",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{}/printers/{printer_id}/controls", tenant.id),
        Some(json!({ "action": "pause" })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({ "error": "role_forbidden" }));
}

#[tokio::test]
async fn printer_control_enqueues_audits_and_wakes_owning_agent() {
    let state = state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("A1 Mini"),
    )
    .await
    .unwrap();
    let (wake_sender, mut wake_receiver) = mpsc::channel(1);
    let (close_sender, _) = mpsc::channel(1);
    state
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id,
            name: "shop-agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
        })
        .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
        Some(json!({ "action": "set_print_speed", "speed_mode": 4 })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["kind"], "printer_control");
    assert_eq!(body["printer_id"], printer_id);
    assert_eq!(
        serde_json::from_str::<Value>(body["payload_json"].as_str().unwrap()).unwrap()["action"],
        "set_print_speed"
    );
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("agent should be woken")
        .expect("wake channel should stay open");
    let events = state.audit_events().list_for_tenant(tenant_id).await.unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "printer.dispatch_control")
        .expect("printer control audit event");
    assert_eq!(event.target_type, "printer");
    assert_eq!(event.target_id.as_deref(), Some(printer_id.as_str()));
    let metadata = serde_json::from_str::<Value>(&event.metadata_json).unwrap();
    assert_eq!(metadata["action"], "set_print_speed");
    assert_eq!(metadata["speed_mode"], 4);
    assert_eq!(metadata["agent_id"], agent_id.to_string());
    assert!(metadata["serial_number"].as_str().unwrap().starts_with("serial-"));
}

#[tokio::test]
async fn printer_control_rejects_unknown_model_before_command_or_audit_insert() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        None,
    )
    .await
    .unwrap();

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
        Some(json!({ "action": "pause" })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "printer_control_unavailable" }));
    assert_eq!(state.commands().count().await.unwrap(), 0);
    assert!(state.audit_events().list_for_tenant(tenant_id).await.unwrap().is_empty());
}

#[tokio::test]
async fn printer_control_wakes_owning_agent_not_sibling() {
    let state = state().await;
    let sibling = sibling_state(&state);
    let _control_plane = start_control_plane(sibling.clone()).await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let other_agent = state.agents().create(tenant_id, "other-agent").await.unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("A1 Mini"),
    )
    .await
    .unwrap();
    let (wake_sender, mut wake_receiver) = mpsc::channel(1);
    let (close_sender, _) = mpsc::channel(1);
    sibling
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id,
            name: "shop-agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
        })
        .await;
    let (other_wake_sender, mut other_wake_receiver) = mpsc::channel(1);
    let (other_close_sender, _) = mpsc::channel(1);
    sibling
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id: other_agent.id,
            name: "other-agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender: other_wake_sender,
            close_sender: other_close_sender,
        })
        .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
        Some(json!({ "action": "pause" })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["agent_id"], agent_id.to_string());
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("owning agent should be woken")
        .expect("wake channel should stay open");
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), other_wake_receiver.recv())
            .await
            .is_err()
    );
}
```

Also add the following invalid-input test:

```rust
#[tokio::test]
async fn printer_control_rejects_invalid_action_and_speed_payloads() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("A1 Mini"),
    )
    .await
    .unwrap();

    for payload in [
        json!({ "action": "dance" }),
        json!({ "action": "set_print_speed" }),
        json!({ "action": "set_print_speed", "speed_mode": 0 }),
        json!({ "action": "set_print_speed", "speed_mode": 5 }),
        json!({ "action": "pause", "speed_mode": 2 }),
        json!({ "action": "pause", "raw_command": "project_file" }),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
            Some(payload),
            &token,
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body, json!({ "error": "invalid_printer_control" }));
    }
}
```

- [ ] **Step 2: Run focused failing route tests**

Run:

```bash
cargo test -p pandar-hub printer_control_
```

Expected: FAIL because route, stable control payload rejection, helper, and enqueue audit are missing.

- [ ] **Step 3: Write failing repository tests for SQLite and PostgreSQL**

In `crates/pandar-hub/src/repositories/tests/commands.rs`, add:

```rust
#[tokio::test]
async fn command_enqueue_printer_control_derives_agent_persists_payload_and_audits() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        &database,
        tenant.id,
        agent.id,
        Some("A1 Mini"),
    )
    .await
    .unwrap();

    let command = commands
        .enqueue_printer_control_with_audit(
            tenant.id,
            &printer_id,
            PrinterControlAction::Pause,
            None,
            test_audit_actor(),
        )
        .await
        .unwrap();

    assert_eq!(command.kind, "printer_control");
    assert_eq!(command.agent_id, agent.id);
    assert_eq!(command.printer_id.as_deref(), Some(printer_id.as_str()));
    let payload: Value = serde_json::from_str(&command.payload_json).unwrap();
    assert_eq!(payload["printer_id"], printer_id);
    assert_eq!(payload["action"], "pause");
    assert_eq!(payload["speed_mode"], Value::Null);
}

#[tokio::test]
async fn command_enqueue_printer_control_rejects_unknown_model_before_insert() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        &database,
        tenant.id,
        agent.id,
        None,
    )
    .await
    .unwrap();

    let err = commands
        .enqueue_printer_control_with_audit(
            tenant.id,
            &printer_id,
            PrinterControlAction::Pause,
            None,
            test_audit_actor(),
        )
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::PrinterControlUnavailable));
    assert_eq!(commands.count().await.unwrap(), 0);
    assert!(AuditEventRepository::new(database).list_for_tenant(tenant.id).await.unwrap().is_empty());
}
```

If `commands.rs` does not already have an audit actor helper, add:

```rust
fn test_audit_actor() -> AuditActor {
    AuditActor::user("repository-test")
}
```

In `crates/pandar-hub/src/repositories/tests/postgres_commands.rs`, add the same behavior inside a PostgreSQL test guarded by `postgres_database().await`:

```rust
#[tokio::test]
async fn postgres_printer_control_enqueue_behavior_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());
    let audit = AuditEventRepository::new(database.clone());
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        &database,
        tenant.id,
        agent.id,
        Some("A1 Mini"),
    )
    .await
    .unwrap();

    let command = commands
        .enqueue_printer_control_with_audit(
            tenant.id,
            &printer_id,
            PrinterControlAction::Stop,
            None,
            test_audit_actor(),
        )
        .await
        .unwrap();
    assert_eq!(command.agent_id, agent.id);
    assert_eq!(command.kind, "printer_control");
    assert_eq!(audit.list_for_tenant(tenant.id).await.unwrap().len(), 1);

    let unsupported = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        &database,
        tenant.id,
        agent.id,
        None,
    )
    .await
    .unwrap();
    assert!(matches!(
        commands
            .enqueue_printer_control_with_audit(
                tenant.id,
                &unsupported,
                PrinterControlAction::Pause,
                None,
                test_audit_actor(),
            )
            .await
            .unwrap_err(),
        RepositoryError::PrinterControlUnavailable
    ));
}
```

Also add invalid-speed repository assertions for `SetPrintSpeed` with `None`, `Some(0)`, `Some(5)`, and `Pause` with `Some(2)` returning `RepositoryError::InvalidPrinterControl`.

- [ ] **Step 4: Run focused failing repository tests**

Run:

```bash
cargo test -p pandar-hub command_enqueue_printer_control
cargo test -p pandar-hub postgres_printer_control_enqueue_behavior_when_configured
```

Expected: FAIL because repository payload, errors, helper, and enqueue audit are missing.

- [ ] **Step 5: Add test helper with explicit model**

Inside the existing `#[cfg(test)] pub(crate) mod test_helpers` block in `crates/pandar-hub/src/repositories/mod.rs`, add:

```rust
pub(crate) async fn insert_printer_fixture_with_model(
    database: &Database,
    tenant_id: TenantId,
    agent_id: AgentId,
    model: Option<&str>,
) -> anyhow::Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    match database {
        Database::Sqlite(pool) => {
            sqlx::query(
                "INSERT INTO printers (id, tenant_id, agent_id, serial_number, name, model, status, last_seen_at, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            )
            .bind(&id)
            .bind(tenant_id.to_string())
            .bind(agent_id.to_string())
            .bind(format!("serial-{id}"))
            .bind("Fixture Printer")
            .bind(model)
            .bind("offline")
            .bind("2026-06-20T00:00:00Z")
            .execute(pool)
            .await
            .context("failed to insert SQLite printer fixture")?;
        }
        Database::Postgres(pool) => {
            sqlx::query(
                "INSERT INTO printers (id, tenant_id, agent_id, serial_number, name, model, status, last_seen_at, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)",
            )
            .bind(&id)
            .bind(tenant_id.to_string())
            .bind(agent_id.to_string())
            .bind(format!("serial-{id}"))
            .bind("Fixture Printer")
            .bind(model)
            .bind("offline")
            .bind("2026-06-20T00:00:00Z")
            .execute(pool)
            .await
            .context("failed to insert PostgreSQL printer fixture")?;
        }
    }

    Ok(id)
}
```

Update `insert_printer_fixture` to call the new helper with `None` to avoid duplication.

- [ ] **Step 6: Add repository errors and API mapping**

In `RepositoryError`, add:

```rust
#[error("printer control is unavailable for this printer")]
PrinterControlUnavailable,
#[error("invalid printer control request")]
InvalidPrinterControl,
```

In `impl From<RepositoryError> for ApiError` in `crates/pandar-hub/src/routes.rs`, map it to:

```rust
RepositoryError::PrinterControlUnavailable => {
    Self::new(StatusCode::BAD_REQUEST, "printer_control_unavailable")
}
RepositoryError::InvalidPrinterControl => {
    Self::new(StatusCode::BAD_REQUEST, "invalid_printer_control")
}
```

- [ ] **Step 7: Add tenant printer lookup**

In `crates/pandar-hub/src/repositories/commands/ownership.rs`, add a function that returns the persisted printer record data needed by printer-targeted commands. This lookup is by `tenant_id + printer_id`; it derives the owning `agent_id` from the persisted printer instead of requiring the route to know the agent:

```rust
pub struct CommandPrinter {
    pub id: String,
    pub agent_id: AgentId,
    pub serial_number: String,
    pub model: Option<String>,
}

pub async fn printer_for_tenant(
    database: &Database,
    tenant_id: TenantId,
    printer_id: &str,
) -> RepositoryResult<CommandPrinter> {
    let printer = printers::Entity::find_by_id(printer_id)
        .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
        .one(&database.sea_orm_connection())
        .await
        .context("failed to verify command printer ownership")?;

    printer
        .map(|printer| CommandPrinter {
            id: printer.id,
            agent_id: AgentId::parse(&printer.agent_id).map_err(anyhow::Error::from)?,
            serial_number: printer.serial_number,
            model: printer.model,
        })
        .transpose()
        .map_err(RepositoryError::from)?
        .ok_or(RepositoryError::MissingPrinter)
}
```

Keep existing `printer_serial_for_agent` by adding a small internal query or a separate `printer_for_agent` helper for print jobs. Do not change print-job behavior.

- [ ] **Step 8: Implement audited enqueue**

In `commands/audit.rs`, add `enqueue_printer_control_with_audit(...)` that:

- accepts `(database, tenant_id, printer_id, action, speed_mode, actor)`;
- loads `CommandPrinter` with `printer_for_tenant`;
- verifies the derived `printer.agent_id` still belongs to the tenant using `verify_agent_owner`;
- checks `pandar_core::compatibility::live_controls_supported(printer.model.as_deref())`;
- validates speed rules:
  - `SetPrintSpeed` requires `Some(1..=4)`;
  - other actions require `speed_mode == None`;
- inserts command and audit in one transaction with `agent_id: printer.agent_id` and `printer_id: Some(printer_id)`.
- writes audit metadata exactly as:

```rust
serde_json::json!({
    "agent_id": printer.agent_id.to_string(),
    "serial_number": printer.serial_number,
    "action": action.as_str(),
    "speed_mode": speed_mode,
})
```

For non-speed actions, `speed_mode` may be omitted or serialized as `null`; route tests must assert required fields and print-speed metadata.

In `commands.rs`, expose `CommandRepository::enqueue_printer_control_with_audit(tenant_id, printer_id, action, speed_mode, actor)`. The payload/action type re-export already happened in Task 2 and should remain in `repositories/mod.rs`.

- [ ] **Step 9: Add route handler**

In `routes/printers.rs`, add:

```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PrinterControlRequest {
    action: PrinterControlAction,
    speed_mode: Option<u8>,
}
```

Add `printer_control(...)` with `payload: Result<Json<PrinterControlRequest>, JsonRejection>`. It must parse tenant/printer, authorize `Operator`, map any `JsonRejection` to `ApiError::bad_request("invalid_printer_control")`, call the repository with `auth::audit_actor(&auth)`, wake `command.agent_id` returned on the command record, and return `CommandResponse`. The route must not accept or parse `agent_id`.

`PrinterControlRequest` must keep `#[serde(deny_unknown_fields)]` so unknown actions, malformed speed types, and extra fields return `400 {"error":"invalid_printer_control"}` before repository enqueue, command insert, audit insert, or wakeup.

Register route in `routes.rs`:

```rust
.route(
    "/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls",
    post(printers::printer_control),
)
```

- [ ] **Step 10: Run focused route and repository tests**

Run:

```bash
cargo test -p pandar-hub printer_control_
cargo test -p pandar-hub command_enqueue_printer_control
```

Expected: PASS.

### Task 4: Agent Command Handler And Gateway MQTT Dispatch

**Files:**
- Modify: `crates/pandar-agent/src/machine/mod.rs`
- Modify: `crates/pandar-agent/src/commands.rs`
- Modify: `crates/pandar-agent/src/commands/tests.rs`
- Modify: `crates/pandar-agent/src/machine/tests.rs`

- [ ] **Step 1: Write failing gateway test**

In `crates/pandar-agent/src/machine/tests.rs`, add:

```rust
#[tokio::test]
async fn configured_control_printer_publishes_pause_to_request_topic() {
    let mqtt = FakeMqttTransport::default();
    let mut endpoint = endpoint("SERIAL1");
    endpoint.model = None;
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint, mqtt.clone(), FakeMachineFileTransfer::default())],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .control_printer("SERIAL1", PrinterControl::Pause)
        .await
        .unwrap();

    assert_eq!(
        mqtt.published_commands().await,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: json!({"print": {"command": "pause", "sequence_id": "0"}}),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}
```

Also add one print-speed test for mode 4.

- [ ] **Step 2: Write failing command handler tests**

In `crates/pandar-agent/src/commands/tests.rs`, add tests:

- `printer_control_valid_emits_ack_and_success_with_result_json`
- `printer_control_unknown_serial_rejects_ack_without_dispatch`
- `printer_control_invalid_speed_rejects_ack_without_dispatch`
- `printer_control_publish_failure_emits_ack_then_failure_with_redacted_context`
- `printer_control_does_not_reject_missing_local_model`

Use `protocol::agent::v1::PrinterControl` to build `HubCommand`.

- [ ] **Step 3: Run focused failing tests**

Run:

```bash
cargo test -p pandar-agent printer_control_
```

Expected: FAIL because the agent trait and handler do not support printer controls.

- [ ] **Step 4: Add typed control enum and gateway method**

In `machine/mod.rs`, add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrinterControl {
    Pause,
    Resume,
    Stop,
    SetPrintSpeed(mqtt::PrintSpeed),
}
```

Add to `BambuMachineGateway`:

```rust
async fn control_printer(
    &self,
    serial_number: &str,
    control: PrinterControl,
) -> anyhow::Result<()>;
```

Implement it for `NoopMachineGateway` with the existing no-printer error shape.

Implement it for `ConfiguredBambuMachineGateway` by finding the endpoint by serial and publishing:

```rust
let payload = match control {
    PrinterControl::Pause => BambuMqttCommand::PausePrint,
    PrinterControl::Resume => BambuMqttCommand::ResumePrint,
    PrinterControl::Stop => BambuMqttCommand::StopPrint,
    PrinterControl::SetPrintSpeed(speed) => BambuMqttCommand::SetPrintSpeed(speed),
};
mqtt.publish(PublishedMqttCommand {
    topic: BambuMqttTopics::for_serial(&endpoint.serial).request,
    payload: payload.payload(),
    qos: BAMBU_MQTT_QOS,
})
.await
.with_context(|| format!("publish printer control {} to {}", control.as_str(), endpoint.serial))
```

Do not check endpoint model here.

- [ ] **Step 5: Add command handler support**

In `commands.rs`, import protobuf `PrinterControl`, add a match arm in `handle_command_with_reader`, and implement:

- parse action string into `machine::PrinterControl`;
- reject ack on unknown action;
- reject ack on missing or invalid speed for `set_print_speed`;
- reject ack if non-speed action carries non-zero `speed_mode`;
- call `gateway.validate_printer(...)` before accepted ack;
- emit accepted ack;
- call `gateway.control_printer(...)`;
- emit success with result JSON:

```json
{"type":"printer_control","serial_number":"SERIAL1","action":"pause","dispatch":"mqtt_published"}
```

On error, use `gateway.redact_error(&format!("{err:#}"))`.

- [ ] **Step 6: Run focused agent tests**

Run:

```bash
cargo test -p pandar-agent printer_control_
cargo test -p pandar-agent machine::tests::configured_control_printer
```

Expected: PASS.

### Task 5: Status-Separation Test

**Files:**
- Modify: `crates/pandar-hub/src/grpc/tests/commands.rs` or `crates/pandar-hub/src/repositories/tests/jobs/lifecycle.rs`

- [ ] **Step 1: Write failing status-separation test**

Add a focused test that creates a print job, enqueues and completes a `printer_control` command for the same printer, then reloads the job and proves `job.print.status` did not change:

```rust
#[tokio::test]
async fn printer_control_success_does_not_mutate_physical_print_status() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("A1 Mini"),
    )
    .await
    .unwrap();
    let created = state
        .jobs()
        .create_print_job(CreatePrintJob {
            tenant_id,
            printer_id: printer_id.clone(),
            agent_id,
            artifact_id: "artifact-1".to_string(),
            artifact_filename: "plate.3mf".to_string(),
            artifact_content_type: "model/3mf".to_string(),
            artifact_size_bytes: 42,
            artifact_storage_path: format!("{tenant_id}/artifact-1/plate.3mf"),
            plate_id: 1,
            use_ams: true,
            flow_cali: false,
            timelapse: false,
            ams_mapping_json: None,
            ams_mapping2_json: None,
        })
        .await
        .unwrap();
    let control = state
        .commands()
        .enqueue_printer_control_with_audit(
            tenant_id,
            &printer_id,
            PrinterControlAction::Stop,
            None,
            test_audit_actor(),
        )
        .await
        .unwrap();

    state.commands().mark_sent(control.id, tenant_id, agent_id).await.unwrap();
    state.commands().mark_acknowledged(control.id, tenant_id, agent_id).await.unwrap();
    state
        .commands()
        .mark_succeeded_with_result(
            control.id,
            tenant_id,
            agent_id,
            Some(r#"{"type":"printer_control","action":"stop"}"#.to_string()),
        )
        .await
        .unwrap();

    let reloaded = state
        .jobs()
        .get_for_tenant(tenant_id, created.job.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reloaded.job.print.status, PrintStatus::Pending);
}
```

Import `CreatePrintJob`, `PrinterControlAction`, and `PrintStatus` from the existing crate modules. This test should pass once the repository implementation keeps printer-control command transitions on `commands` only.

- [ ] **Step 2: Run status-separation test**

Run:

```bash
cargo test -p pandar-hub printer_control_success_does_not_mutate_physical_print_status
```

Expected: PASS after Task 3 implements printer-control command lifecycle without job mutation.

### Task 6: Frontend Controls

**Files:**
- Modify: `frontend/app/actions.ts`
- Modify: `frontend/app/recovery-actions.tsx`

- [ ] **Step 1: Add server action**

In `frontend/app/actions.ts`, add:

```ts
export async function controlPrinter(formData: FormData) {
  const tenantId = stringField(formData, 'tenant_id')
  const printerId = stringField(formData, 'printer_id')
  const action = stringField(formData, 'action')
  const speedMode = nullableField(formData, 'speed_mode')
  const response = await postJson(`/api/v1/tenants/${tenantId}/printers/${printerId}/controls`, {
    action,
    speed_mode: speedMode ? Number(speedMode) : undefined,
  })
  redirect(statusUrl(tenantId, response.ok ? 'printer_control_queued' : await errorCode(response)))
}
```

- [ ] **Step 2: Add local compatibility helper**

In `recovery-actions.tsx`, add a small local helper that mirrors the shared model keys:

```ts
function liveControlsAvailable(printer: Printer) {
  const normalized = printer.model?.trim().toUpperCase().replace(/[ _-]/g, '')
  return normalized === 'A1'
    || normalized === 'A1MINI'
    || normalized === 'A1M'
    || normalized === 'A1MIN'
    || normalized === 'BAMBULABA1MINI'
    || normalized === 'BAMBULABA1'
    || normalized === 'P2S'
    || normalized === 'N7'
    || normalized === 'X2D'
    || normalized === 'N6'
}
```

This is advisory only; backend rejects remain authoritative.

Add a local table near the helper for the shared aliases and use it from `liveControlsAvailable`, rather than scattering string comparisons through render code:

```ts
const liveControlModelKeys = new Set(['A1', 'A1MINI', 'A1M', 'A1MIN', 'BAMBULABA1MINI', 'BAMBULABA1', 'P2S', 'N7', 'X2D', 'N6'])
```

Before the build, verify the helper table covers the core aliases by scanning for the exact literals `A1M`, `A1MIN`, `BAMBULABA1`, `N7`, and `N6` in `frontend/app/recovery-actions.tsx`.

- [ ] **Step 3: Render controls in recovery actions**

Replace the static unavailable text with controls for `job.printer_id`:

- find `const printer = printers.find((candidate) => candidate.id === job.printer_id)`;
- if missing, render `Printer record unavailable for live controls`;
- if incompatible, render `Live controls unavailable for unknown printer model`;
- if compatible, render forms for Pause, Resume, Stop, and speed select using `controlPrinter`.

Keep button and input dimensions stable, use existing compact button style, and use text `Queue pause`, `Queue resume`, `Queue stop`, `Queue speed` to avoid claiming physical state changed.

- [ ] **Step 4: Run frontend build**

Run:

```bash
rg -n "A1M|A1MIN|BAMBULABA1|N7|N6" frontend/app/recovery-actions.tsx
npm --prefix frontend run build
```

Expected: `rg` prints the advisory compatibility alias table, then build PASS.

### Task 7: Documentation Updates

**Files:**
- Create: `docs/compatibility/phase-27-live-printer-controls.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/development.md`
- Modify: `docs/architecture.md` only for stale live-control wording found by search

- [ ] **Step 1: Add compatibility evidence doc**

Create `docs/compatibility/phase-27-live-printer-controls.md` with:

- reference payload table for pause/resume/stop/print-speed;
- QoS 1 requirement;
- compatibility policy for A1/A1 Mini/P2S/X2D and aliases;
- command lifecycle vs physical status separation;
- local no-network verification commands;
- real-printer probe status as not run unless hardware evidence exists.

- [ ] **Step 2: Update roadmap**

In `docs/roadmap.md`, mark Phase 27 implementation language as completed/in progress based on actual verification, and keep Phase 28 as next.

- [ ] **Step 3: Remove stale unavailable wording**

Search:

```bash
rg -n "pause, resume, and stop|Pause/resume/stop|live printer control is not implemented|not implemented yet" docs frontend/app
```

Update docs so they say Phase 27 controls are typed, compatibility-gated, and dispatch-only. Do not claim real-printer probe evidence unless it was gathered.

### Task 8: Full Verification And Final Review Package

**Files:**
- No new code files unless earlier tasks reveal a focused fix.

- [ ] **Step 1: Run formatting**

Run:

```bash
cargo fmt --check
```

Expected: PASS. If it fails, run `cargo fmt`, then rerun `cargo fmt --check`.

- [ ] **Step 2: Run Rust lint**

Run:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: PASS.

- [ ] **Step 3: Run full Rust tests**

Run:

```bash
cargo nextest run --manifest-path Cargo.toml --workspace
```

Expected: PASS.

- [ ] **Step 4: Run frontend build**

Run:

```bash
npm --prefix frontend run build
```

Expected: PASS.

- [ ] **Step 5: Run diff hygiene**

Run:

```bash
git diff --check
git status --short
```

Expected: no whitespace errors; status shows only intended Phase 27 files.

- [ ] **Step 6: Prepare final reviewer input**

Collect:

- spec path: `docs/superpowers/specs/2026-06-24-phase-27-reference-backed-live-printer-controls-design.md`;
- plan path: this file;
- `git diff --stat`;
- outputs from Steps 1-5.

Pass those to Codex and opencode implementation reviewers per `$sdd-workflow`.

---

## Self-Review Checklist

- Spec coverage: shared compatibility, Hub API, repository/audit, proto dispatch, agent gateway/handler, frontend, docs, tests, and verification are all mapped to tasks.
- Placeholder scan: no `TBD`, `TODO`, or unspecified "write tests" steps remain.
- Type consistency: `PrinterControlAction`, `PrinterControlPayload`, `PrinterControl`, `PrinterControlRequest`, and protobuf `PrinterControl` names are consistently scoped by crate.
- Risk controls: no raw command path, no DB migration, no physical print status mutation, and no agent local-model compatibility rejection.
