# Add Printer Type And Bambu Metadata Autodiscovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let operators link a Bambu printer with agent, type, IPv4 address, access code, and optional name only, while the agent discovers serial/model metadata and reports it after onboarding completes.

**Architecture:** Keep the existing live-only link-printer dispatch model, but change the public request/proto payload so Hub never accepts or sends operator-supplied serial/model. Hub validates `type = "BambuLab"` and IPv4 host input, persists only redacted type/host/name command metadata, and continues to reject durable replay. The agent resolves Bambu serial/model from existing SSDP discovery before runtime MQTT validation, then emits a snapshot and structured command result containing the discovered serial/model.

**Tech Stack:** Rust 2024, axum, tonic/prost, SeaORM/SQLx-backed repositories, tokio, Next.js App Router, React, next-intl, Tailwind/shadcn-style local UI primitives, Vitest/React Testing Library.

## Global Constraints

- Approved spec: `docs/superpowers/specs/2026-07-02-add-printer-type-autodiscovery-design.md` is binding.
- Operators must not input printer serial number or model in the add-printer form.
- Public link-printer requests must include `type`, `host`, `access_code`, and optional `name`; `type` must be exactly `BambuLab`.
- `host` must parse as an IPv4 address; hostname support is out of scope for this flow.
- Legacy `serial_number` and `model` request fields must be rejected by `serde(deny_unknown_fields)` with `400 { "error": "bad_request" }`.
- The `LinkPrinter` proto must reserve old `serial_number = 2` and `model = 5` tags and names, and must carry `host`, `access_code`, `name`, and `printer_type` only.
- Persisted `link_printer` rows must never be converted or replayed into live `HubCommand::LinkPrinter`; durable gRPC conversion must continue returning `FailedPrecondition`.
- Access codes must remain redacted in persisted command payloads, audit metadata, command results, command errors, logs, and frontend status paths.
- The agent must send discovered serial/model as supplemental onboarding metadata through both `PrinterSnapshot` and successful `printer_link` result JSON.
- Hub persistent behavior must remain backend-neutral for SQLite and PostgreSQL.
- Preserve lower-level error cause chains by formatting with `{err:#}` before redaction at error/log/result boundaries.
- No database migration and no legacy fallback.
- Frontend must follow existing dashboard style and localization patterns.
- Update `docs/development.md` and `docs/roadmap.md` after implementation.
- Run `cargo fmt`, `cargo clippy`, `cargo nextest run --manifest-path "Cargo.toml" --workspace`, and focused frontend tests. If a broad verification command cannot complete, capture the exact failure and run the most relevant focused checks.
- `$sdd-workflow` controls commits: do not create task-local commits. Commit and push only after spec-implementation reviewer approval and final verification.

---

## File Structure

- Modify `proto/pandar/agent/v1/agent.proto`: reserve old serial/model fields in `LinkPrinter` and add `printer_type = 6`.
- Modify `crates/pandar-hub/src/repositories/commands.rs`: change link-printer payload structs to `printer_type`, `host`, `access_code`, and optional `name`.
- Modify `crates/pandar-hub/src/repositories/commands/audit.rs`: remove serial/model from link-printer audit metadata and include printer type.
- Modify `crates/pandar-hub/src/routes/printers.rs`: change HTTP request shape, validate `BambuLab` type and IPv4 host, reject legacy fields, and construct the new proto command.
- Modify `crates/pandar-hub/src/grpc/commands.rs`: adapt live pending-secret redaction to the new payload shape while keeping persisted replay rejection.
- Modify Hub test-only live-command constructors in `crates/pandar-hub/src/runtime.rs` and `crates/pandar-hub/src/sessions.rs` so exact filtered Hub test builds still compile after the proto/payload shape changes.
- Modify Hub tests in `crates/pandar-hub/src/repositories/tests/commands.rs`, `crates/pandar-hub/src/routes/tests/printers.rs`, `crates/pandar-hub/src/grpc/tests/commands.rs`, and `crates/pandar-hub/src/grpc/tests/lifecycle.rs`.
- Modify PostgreSQL-specific Hub tests in `crates/pandar-hub/src/repositories/tests/postgres_commands.rs` so the backend-neutral link-printer persistence contract is covered for PostgreSQL as well as SQLite/default repository tests.
- Modify `crates/pandar-agent/src/commands.rs`: resolve discovered Bambu serial/model before runtime link validation and report discovered metadata in snapshot/result events.
- Modify agent reconnect/runtime test support in `crates/pandar-agent/src/lib.rs` and `crates/pandar-agent/src/machine/runtime.rs` so runtime-linked reconnect tests compile and provide discovery results for the new link flow.
- Modify `crates/pandar-agent/src/commands/tests.rs`: cover discovery success, missing discovery, missing serial, unsupported type, redaction, and supplemental serial/model reporting.
- Modify `frontend/app/link-printer-form.tsx`: add type selector, rename host label to printer IPv4 address, remove serial/model inputs.
- Modify `frontend/app/actions.ts`: post the new link-printer payload.
- Create `frontend/app/actions.test.ts`: pin server action JSON shape.
- Modify `frontend/app/agent-pairing-guidance.test.tsx`: update form assertions.
- Modify `frontend/messages/en.json` and `frontend/messages/zh.json`: update link-printer labels/copy.
- Modify `docs/development.md` and `docs/roadmap.md`: document the new add-printer discovery requirement and completion status.

---

### Task 1: Hub And Proto Link-Printer Contract

**Files:**
- Modify: `proto/pandar/agent/v1/agent.proto`
- Modify: `crates/pandar-hub/src/repositories/commands.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/audit.rs`
- Modify: `crates/pandar-hub/src/routes/printers.rs`
- Modify: `crates/pandar-hub/src/grpc/commands.rs`
- Test: `crates/pandar-hub/src/runtime.rs`
- Test: `crates/pandar-hub/src/sessions.rs`
- Test: `crates/pandar-hub/src/repositories/tests/commands.rs`
- Test: `crates/pandar-hub/src/repositories/tests/postgres_commands.rs`
- Test: `crates/pandar-hub/src/routes/tests/printers.rs`
- Test: `crates/pandar-hub/src/grpc/tests/commands.rs`
- Test: `crates/pandar-hub/src/grpc/tests/lifecycle.rs`

**Interfaces:**
- Consumes: existing live-only link-printer route/session dispatch and pending-secret redaction.
- Produces: `LinkPrinterPayload { printer_type: String, host: String, access_code: String, name: Option<String> }`.
- Produces: `RedactedLinkPrinterPayload { printer_type: String, host: String, access_code: String, name: Option<String> }` where `access_code` is always `"[redacted]"`.
- Produces: `LinkPrinterRequest { printer_type, host, access_code, name }` with `#[serde(rename = "type")]` for the public field.
- Produces: proto `LinkPrinter { host, access_code, name, printer_type }` with reserved old serial/model tags and names.
- Produces: route behavior that returns `400 { "error": "bad_request" }` for blank type/host/access code, unsupported type, non-IPv4 host, legacy serial/model fields, and any other unknown field.

- [x] **Step 1: Update failing Hub tests for the new request and persistence contract**

Update `crates/pandar-hub/src/routes/tests/printers.rs` so `link_printer_body` sends the new request shape:

```rust
fn link_printer_body(access_code: &str) -> serde_json::Value {
    json!({
        "type": "BambuLab",
        "host": "192.0.2.10",
        "access_code": access_code,
        "name": "Office X1C"
    })
}
```

Update `link_printer_direct_sends_secret_but_persists_only_redacted_payload` assertions:

```rust
match command.command.unwrap() {
    hub_command::Command::LinkPrinter(command) => {
        assert_eq!(command.printer_type, "BambuLab");
        assert_eq!(command.host, "192.0.2.10");
        assert_eq!(command.access_code, access_code);
        assert_eq!(command.name, "Office X1C");
    }
    other => panic!("expected link-printer command, got {other:?}"),
}

let payload: serde_json::Value = serde_json::from_str(body["payload_json"].as_str().unwrap()).unwrap();
assert_eq!(payload["printer_type"], "BambuLab");
assert_eq!(payload["host"], "192.0.2.10");
assert_eq!(payload["access_code"], "[redacted]");
assert_eq!(payload["name"], "Office X1C");
assert!(payload.get("serial_number").is_none());
assert!(payload.get("model").is_none());
```

Replace `link_printer_maps_absent_or_blank_optional_name_model_to_empty_proto_strings` with a test that only checks optional name normalization:

```rust
#[tokio::test]
async fn link_printer_maps_absent_or_blank_optional_name_to_empty_proto_string() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let agent_id = agent["id"].as_str().unwrap();
    let (sender, mut receiver) = mpsc::channel(2);
    state.sessions().register_local_agent_sender(AgentId::parse(agent_id).unwrap(), sender).await;

    for request in [
        json!({ "type": "BambuLab", "host": "192.0.2.10", "access_code": "SECRET-LINK-CODE" }),
        json!({ "type": "BambuLab", "host": "192.0.2.11", "access_code": "SECRET-LINK-CODE", "name": "   " }),
    ] {
        let (status, _) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
            Some(request),
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        match receiver.recv().await.unwrap().command.unwrap() {
            hub_command::Command::LinkPrinter(command) => assert_eq!(command.name, ""),
            other => panic!("expected link-printer command, got {other:?}"),
        }
    }
}
```

Add invalid-contract cases to `link_printer_rejects_blank_required_fields` or a new route test:

```rust
#[tokio::test]
async fn link_printer_rejects_invalid_type_host_and_legacy_metadata_fields() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let agent_id = agent["id"].as_str().unwrap();

    for request in [
        json!({ "type": "", "host": "192.0.2.10", "access_code": "SECRET-LINK-CODE" }),
        json!({ "type": "Other", "host": "192.0.2.10", "access_code": "SECRET-LINK-CODE" }),
        json!({ "type": "BambuLab", "host": "printer.local", "access_code": "SECRET-LINK-CODE" }),
        json!({ "type": "BambuLab", "host": "192.0.2.10", "access_code": "SECRET-LINK-CODE", "serial_number": "SERIAL123" }),
        json!({ "type": "BambuLab", "host": "192.0.2.10", "access_code": "SECRET-LINK-CODE", "model": "X1 Carbon" }),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
            Some(request),
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "bad_request");
    }
    assert_eq!(state.commands().list_for_tenant(TenantId::parse(tenant_id).unwrap()).await.unwrap().len(), 0);
}
```

Update `crates/pandar-hub/src/repositories/tests/commands.rs` link-printer payload assertions:

```rust
let payload: serde_json::Value = serde_json::from_str(&command.payload_json).unwrap();
assert_eq!(payload["printer_type"], "BambuLab");
assert_eq!(payload["host"], "192.0.2.10");
assert_eq!(payload["access_code"], "[redacted]");
assert_eq!(payload["name"], "Office X1C");
assert!(payload.get("serial_number").is_none());
assert!(payload.get("model").is_none());

let metadata: serde_json::Value = serde_json::from_str(&event.metadata_json).unwrap();
assert_eq!(metadata["printer_type"], "BambuLab");
assert_eq!(metadata["host"], "192.0.2.10");
assert_eq!(metadata["name"], "Office X1C");
assert!(metadata.get("serial_number").is_none());
assert!(metadata.get("model").is_none());
```

Update link-printer helpers in `crates/pandar-hub/src/grpc/tests/commands.rs` and `crates/pandar-hub/src/grpc/tests/lifecycle.rs` so payloads use `printer_type: "BambuLab"` and no serial/model. Keep the existing tests that assert persisted replay returns `FailedPrecondition` and pending-secret result redaction works.

Update test-only Hub constructors in `crates/pandar-hub/src/runtime.rs` and `crates/pandar-hub/src/sessions.rs` so they compile with the new shape:

```rust
LinkPrinterPayload {
    printer_type: "BambuLab".to_owned(),
    host: "192.0.2.10".to_owned(),
    access_code: format!("SECRET-{serial}"),
    name: None,
}
```

```rust
LinkPrinter {
    host: "192.0.2.10".to_owned(),
    access_code: "SECRET-LINK-CODE".to_owned(),
    name: String::new(),
    printer_type: "BambuLab".to_owned(),
}
```

Update `crates/pandar-hub/src/repositories/tests/postgres_commands.rs` so `postgres_link_printer_command_behavior_when_configured` asserts the same backend-neutral payload/audit contract for PostgreSQL:

```rust
let payload: Value = serde_json::from_str(&old_owned.payload_json).unwrap();
assert_eq!(old_owned.kind, "link_printer");
assert_eq!(old_owned.status, CommandStatus::Sent);
assert_eq!(payload["printer_type"], "BambuLab");
assert_eq!(payload["host"], "192.0.2.10");
assert_eq!(payload["access_code"], "[redacted]");
assert_eq!(payload["name"], "Office X1C");
assert!(payload.get("serial_number").is_none());
assert!(payload.get("model").is_none());
assert!(!old_owned.payload_json.contains("SECRET-OWNED"));

let metadata: Value = serde_json::from_str(&event.metadata_json).unwrap();
assert_eq!(metadata["printer_type"], "BambuLab");
assert_eq!(metadata["host"], "192.0.2.10");
assert_eq!(metadata["name"], "Office X1C");
assert!(metadata.get("serial_number").is_none());
assert!(metadata.get("model").is_none());
assert!(!event.metadata_json.contains("SECRET-OWNED"));
```

Update the same file's `link_payload` helper:

```rust
fn link_payload(serial: &str) -> LinkPrinterPayload {
    LinkPrinterPayload {
        printer_type: "BambuLab".to_owned(),
        host: "192.0.2.10".to_owned(),
        access_code: format!("SECRET-{serial}"),
        name: Some("Office X1C".to_owned()),
    }
}
```

- [x] **Step 2: Run focused tests and confirm the expected failure**

Run:

```bash
cargo test -p pandar-hub routes::tests::printers::link_printer_direct_sends_secret_but_persists_only_redacted_payload -- --exact
```

Expected: FAIL before implementation, either from Rust compile errors about removed/renamed fields or assertion failures showing the route still expects `serial_number`/`model` and does not send `printer_type`.

- [x] **Step 3: Implement the proto and Hub contract change**

Change `proto/pandar/agent/v1/agent.proto`:

```proto
message LinkPrinter {
  string host = 1;
  reserved 2;
  reserved "serial_number";
  string access_code = 3;
  string name = 4;
  reserved 5;
  reserved "model";
  string printer_type = 6;
}
```

Change `crates/pandar-hub/src/repositories/commands.rs` link-printer structs:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LinkPrinterPayload {
    pub printer_type: String,
    pub host: String,
    pub access_code: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RedactedLinkPrinterPayload {
    pub printer_type: String,
    pub host: String,
    pub access_code: String,
    pub name: Option<String>,
}

impl LinkPrinterPayload {
    pub fn redacted(&self) -> RedactedLinkPrinterPayload {
        RedactedLinkPrinterPayload {
            printer_type: self.printer_type.clone(),
            host: self.host.clone(),
            access_code: "[redacted]".to_owned(),
            name: self.name.clone(),
        }
    }
}
```

Change `crates/pandar-hub/src/repositories/commands/audit.rs` link-printer metadata:

```rust
let metadata = serde_json::json!({
    "printer_type": payload.printer_type,
    "host": payload.host,
    "name": payload.name,
});
```

Change `crates/pandar-hub/src/routes/printers.rs` request and validation. Add `use std::net::Ipv4Addr;` near the existing imports, then use this request shape:

```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LinkPrinterRequest {
    #[serde(rename = "type")]
    printer_type: String,
    host: String,
    access_code: String,
    name: Option<String>,
}
```

Update `LinkPrinterRequest::into_payload`:

```rust
impl LinkPrinterRequest {
    fn into_payload(self) -> Result<LinkPrinterPayload, ApiError> {
        let printer_type = trim_required(self.printer_type)?;
        if printer_type != "BambuLab" {
            return Err(ApiError::bad_request("bad_request"));
        }

        let host = trim_required(self.host)?;
        host.parse::<Ipv4Addr>()
            .map_err(|_| ApiError::bad_request("bad_request"))?;

        Ok(LinkPrinterPayload {
            printer_type,
            host,
            access_code: trim_required(self.access_code)?,
            name: trim_optional(self.name),
        })
    }
}
```

Update `link_printer_hub_command`:

```rust
fn link_printer_hub_command(command_id: CommandId, payload: &LinkPrinterPayload) -> HubCommand {
    HubCommand {
        command_id: command_id.to_string(),
        command: Some(hub_command::Command::LinkPrinter(LinkPrinter {
            host: payload.host.clone(),
            access_code: payload.access_code.clone(),
            name: payload.name.clone().unwrap_or_default(),
            printer_type: payload.printer_type.clone(),
        })),
    }
}
```

Update every test helper construction of `LinkPrinterPayload` in Hub tests to the new fields:

```rust
LinkPrinterPayload {
    printer_type: "BambuLab".to_owned(),
    host: "192.0.2.10".to_owned(),
    access_code: access_code.to_owned(),
    name: Some("Office X1C".to_owned()),
}
```

Update every test helper construction of proto `LinkPrinter` in Hub tests to the new fields:

```rust
LinkPrinter {
    host: "192.0.2.10".to_owned(),
    access_code: access_code.to_owned(),
    name: "Office X1C".to_owned(),
    printer_type: "BambuLab".to_owned(),
}
```

- [x] **Step 4: Run focused Hub tests and generated-proto checks**

Run:

```bash
cargo test -p pandar-hub routes::tests::printers::link_printer_direct_sends_secret_but_persists_only_redacted_payload -- --exact
cargo test -p pandar-hub routes::tests::printers::link_printer_rejects_invalid_type_host_and_legacy_metadata_fields -- --exact
cargo test -p pandar-hub repositories::tests::commands::command_create_link_printer_sent_persists_redacted_payload_and_audit -- --exact
cargo test -p pandar-hub repositories::tests::postgres_commands::postgres_link_printer_command_behavior_when_configured -- --exact
cargo test -p pandar-hub grpc::tests::commands::grpc_hub_command_from_record_rejects_persisted_link_printer_replay -- --exact
cargo test -p pandar-hub runtime::tests::runtime_stale_link_printer_cleanup_skips_pending_live_commands -- --exact
cargo test -p pandar-hub sessions::tests::sessions_replacement_race_does_not_leave_pending_command -- --exact
```

Expected: PASS. If generated protobuf code is stale, these commands should trigger the crate build script; if they do not, run `cargo clean -p pandar-hub -p pandar-agent` once and rerun the focused tests.

---

### Task 2: Agent Metadata Discovery And Completion Reporting

**Files:**
- Modify: `crates/pandar-agent/src/commands.rs`
- Test: `crates/pandar-agent/src/lib.rs`
- Test: `crates/pandar-agent/src/machine/runtime.rs`
- Test: `crates/pandar-agent/src/commands/tests.rs`

**Interfaces:**
- Consumes: proto `LinkPrinter { host, access_code, name, printer_type }` from Task 1.
- Consumes: `BambuMachineGateway::discover_printers(timeout_seconds) -> anyhow::Result<PrinterDiscoveryResult>`.
- Consumes: `BambuMachineGateway::link_printer(endpoint, config, sender) -> anyhow::Result<MachineSnapshot>`.
- Produces: `BambuPrinterEndpoint` built from submitted host/access code/name and discovered serial/model.
- Produces: ack then failure event for unsupported printer type, discovery miss, missing serial, and runtime validation failure.
- Produces: successful `PrinterSnapshot` and `printer_link` result JSON with discovered `serial_number` and `model` values.

- [x] **Step 1: Update failing agent tests for discovery-backed linking**

In `crates/pandar-agent/src/commands/tests.rs`, import the discovered printer type:

```rust
use crate::machine::discovery::{DiscoveredPrinter, PrinterDiscoveryResult};
```

Update `link_printer_command` helper:

```rust
fn link_printer_command(command_id: String, access_code: &str) -> HubCommand {
    HubCommand {
        command_id,
        command: Some(hub_command::Command::LinkPrinter(LinkPrinter {
            host: "192.0.2.10".to_owned(),
            access_code: access_code.to_owned(),
            name: "Office X1C".to_owned(),
            printer_type: "BambuLab".to_owned(),
        })),
    }
}
```

Update `crates/pandar-agent/src/lib.rs` test imports so the reconnect-preservation test can seed SSDP discovery data:

```rust
use crate::machine::{
    BambuMachineGateway,
    discovery::DiscoveredPrinter,
    file_transfer::FakeMachineFileTransfer,
    mqtt::FakeMqttTransport,
    runtime::test_support::TestRuntimeBambuMachineGateway,
};
```

Update `crates/pandar-agent/src/lib.rs` test helper `link_printer_command()` to use the new proto shape:

```rust
fn link_printer_command() -> HubCommand {
    HubCommand {
        command_id: uuid::Uuid::new_v4().to_string(),
        command: Some(hub_command::Command::LinkPrinter(LinkPrinter {
            host: "192.0.2.10".to_owned(),
            access_code: "12345678".to_owned(),
            name: "office".to_owned(),
            printer_type: "BambuLab".to_owned(),
        })),
    }
}
```

Update `ended_command_stream_preserves_runtime_linked_printer_for_reconnect` to seed discovery before sending the link command:

```rust
gateway
    .set_discovered_printers(vec![DiscoveredPrinter {
        serial_number: Some("SERIAL123".to_owned()),
        host: "192.0.2.10".to_owned(),
        name: Some("office".to_owned()),
        model: Some("X1 Carbon".to_owned()),
        source: "ssdp",
    }])
    .await;
```

Update `crates/pandar-agent/src/machine/runtime.rs` test-support gateway discovery so reconnect-preservation tests can still runtime-link through the new discovery-backed flow. In the `test_support` imports, include `DiscoveredPrinter`:

```rust
use crate::machine::{
    diagnostics::PrinterDiagnosticResult,
    discovery::{DiscoveredPrinter, PrinterDiscoveryResult},
    file_transfer::{MachineFileTransfer, TransferModeCache},
    mqtt::BambuMqttTransport,
};
```

Add a test-support discovery fixture field to the struct:

```rust
pub(crate) struct TestRuntimeBambuMachineGateway<T, F> {
    inner: tokio::sync::Mutex<ConfiguredBambuMachineGateway<T, F>>,
    discovered_printers: tokio::sync::Mutex<Vec<DiscoveredPrinter>>,
    report_tasks: tokio::sync::Mutex<HashMap<String, JoinHandle<()>>>,
    command_transports: tokio::sync::Mutex<VecDeque<anyhow::Result<T>>>,
    report_preparation_errors: tokio::sync::Mutex<VecDeque<anyhow::Error>>,
    report_task_replacement_pause: tokio::sync::Mutex<Option<ReportTaskReplacementPause>>,
    redaction_access_codes: StdMutex<Vec<String>>,
    transfer: F,
    report_timeout: Duration,
}
```

Add this setter inside the existing `impl<T, F> TestRuntimeBambuMachineGateway<T, F>` block:

```rust
pub(crate) async fn set_discovered_printers(&self, printers: Vec<DiscoveredPrinter>) {
    *self.discovered_printers.lock().await = printers;
}
```

Initialize `discovered_printers` in `TestRuntimeBambuMachineGateway::new` from the initially configured endpoints so existing configured-printer tests retain discovery behavior if they need it later:

```rust
let discovered_printers = printers
    .iter()
    .map(|(endpoint, _, _)| DiscoveredPrinter {
        serial_number: Some(endpoint.serial.clone()),
        host: endpoint.host.clone(),
        name: endpoint.name.clone(),
        model: endpoint.model.clone(),
        source: "ssdp",
    })
    .collect();
```

Set the struct field in `Self { ... }`:

```rust
discovered_printers: tokio::sync::Mutex::new(discovered_printers),
```

In the `test_support` `impl BambuMachineGateway for TestRuntimeBambuMachineGateway`, return the configured discovery fixture instead of an empty list:

```rust
async fn discover_printers(
    &self,
    _timeout_seconds: u32,
) -> anyhow::Result<PrinterDiscoveryResult> {
    Ok(PrinterDiscoveryResult::new(
        self.discovered_printers.lock().await.clone(),
    ))
}
```

Add helper constructors near `LinkGateway`:

```rust
fn discovered_printer(host: &str, serial: Option<&str>, model: Option<&str>) -> DiscoveredPrinter {
    DiscoveredPrinter {
        serial_number: serial.map(str::to_owned),
        host: host.to_owned(),
        name: Some("Discovered Office X1C".to_owned()),
        model: model.map(str::to_owned),
        source: "ssdp",
    }
}

fn link_printer_command_with_type(command_id: String, access_code: &str, printer_type: &str) -> HubCommand {
    HubCommand {
        command_id,
        command: Some(hub_command::Command::LinkPrinter(LinkPrinter {
            host: "192.0.2.10".to_owned(),
            access_code: access_code.to_owned(),
            name: "Office X1C".to_owned(),
            printer_type: printer_type.to_owned(),
        })),
    }
}
```

Change `LinkGateway` so discovery is configurable and link calls can be counted:

```rust
#[derive(Debug, Clone)]
struct LinkGateway {
    discovery: Arc<Mutex<anyhow::Result<PrinterDiscoveryResult>>>,
    result: Arc<Mutex<anyhow::Result<MachineSnapshot>>>,
    linked_endpoints: Arc<Mutex<Vec<BambuPrinterEndpoint>>>,
    access_code: Option<String>,
}

impl LinkGateway {
    fn success(snapshot: MachineSnapshot) -> Self {
        Self {
            discovery: Arc::new(Mutex::new(Ok(PrinterDiscoveryResult::new(vec![discovered_printer(
                "192.0.2.10",
                Some("SERIAL123"),
                Some("X1 Carbon"),
            )])))),
            result: Arc::new(Mutex::new(Ok(snapshot))),
            linked_endpoints: Arc::new(Mutex::new(Vec::new())),
            access_code: None,
        }
    }

    fn discovery_result(printers: Vec<DiscoveredPrinter>) -> Self {
        Self {
            discovery: Arc::new(Mutex::new(Ok(PrinterDiscoveryResult::new(printers)))),
            result: Arc::new(Mutex::new(Ok(snapshot("SERIAL123", "Office X1C", Some("X1 Carbon"), "READY")))),
            linked_endpoints: Arc::new(Mutex::new(Vec::new())),
            access_code: None,
        }
    }

    fn failure(access_code: &str) -> Self {
        Self {
            discovery: Arc::new(Mutex::new(Ok(PrinterDiscoveryResult::new(vec![discovered_printer(
                "192.0.2.10",
                Some("SERIAL123"),
                Some("X1 Carbon"),
            )])))),
            result: Arc::new(Mutex::new(
                Err(anyhow::anyhow!("bad access code {access_code}"))
                    .context("validate runtime printer SERIAL123"),
            )),
            linked_endpoints: Arc::new(Mutex::new(Vec::new())),
            access_code: Some(access_code.to_owned()),
        }
    }

    async fn linked_endpoints(&self) -> Vec<BambuPrinterEndpoint> {
        self.linked_endpoints.lock().await.clone()
    }
}
```

Update `BambuMachineGateway for LinkGateway`:

```rust
async fn discover_printers(&self, _timeout_seconds: u32) -> anyhow::Result<PrinterDiscoveryResult> {
    let mut discovery = self.discovery.lock().await;
    std::mem::replace(&mut *discovery, Ok(PrinterDiscoveryResult::new(Vec::new())))
}

async fn link_printer(
    &self,
    endpoint: BambuPrinterEndpoint,
    _config: &AgentConfig,
    _sender: &mpsc::Sender<AgentEvent>,
) -> anyhow::Result<MachineSnapshot> {
    assert_eq!(endpoint.host, "192.0.2.10");
    assert_eq!(endpoint.serial, "SERIAL123");
    assert_eq!(endpoint.access_code, "SECRET-LINK-CODE");
    assert_eq!(endpoint.name.as_deref(), Some("Office X1C"));
    assert_eq!(endpoint.model.as_deref(), Some("X1 Carbon"));
    self.linked_endpoints.lock().await.push(endpoint);
    let mut result = self.result.lock().await;
    std::mem::replace(&mut *result, Ok(snapshot("SERIAL123", "unused", None, "unused")))
}
```

Update `link_printer_emits_ack_snapshot_and_success_without_access_code` to assert supplemental metadata in both snapshot and result:

```rust
assert_snapshot(
    receiver.recv().await.unwrap(),
    "SERIAL123",
    "Office X1C",
    "X1 Carbon",
    "READY",
);
match receiver.recv().await.unwrap().event.unwrap() {
    agent_event::Event::CommandResult(result) => {
        assert!(result.success);
        assert!(!result.result_json.contains(access_code));
        let json: serde_json::Value = serde_json::from_str(&result.result_json).unwrap();
        assert_eq!(json["type"], "printer_link");
        assert_eq!(json["serial_number"], "SERIAL123");
        assert_eq!(json["host"], "192.0.2.10");
        assert_eq!(json["name"], "Office X1C");
        assert_eq!(json["model"], "X1 Carbon");
        assert_eq!(json["status"], "READY");
    }
    other => panic!("expected command result, got {other:?}"),
}
assert_eq!(gateway.linked_endpoints().await.len(), 1);
```

Add discovery failure tests:

```rust
#[tokio::test]
async fn link_printer_fails_when_discovery_does_not_find_host() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = LinkGateway::discovery_result(vec![discovered_printer(
        "192.0.2.11",
        Some("OTHER"),
        Some("A1 Mini"),
    )]);
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        link_printer_command(command_id.clone(), "SECRET-LINK-CODE"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(receiver.recv().await.unwrap(), ack_event(&config, &command_id));
    assert_failure_contains(receiver.recv().await.unwrap(), &command_id, "could not discover printer at 192.0.2.10");
    assert!(receiver.recv().await.is_none());
    assert!(gateway.linked_endpoints().await.is_empty());
}

#[tokio::test]
async fn link_printer_fails_when_discovered_printer_has_no_serial() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = LinkGateway::discovery_result(vec![discovered_printer(
        "192.0.2.10",
        None,
        Some("X1 Carbon"),
    )]);
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        link_printer_command(command_id.clone(), "SECRET-LINK-CODE"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(receiver.recv().await.unwrap(), ack_event(&config, &command_id));
    assert_failure_contains(receiver.recv().await.unwrap(), &command_id, "printer serial could not be discovered for 192.0.2.10");
    assert!(receiver.recv().await.is_none());
    assert!(gateway.linked_endpoints().await.is_empty());
}

#[tokio::test]
async fn link_printer_rejects_unsupported_type_without_discovery() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = LinkGateway::discovery_result(vec![discovered_printer(
        "192.0.2.10",
        Some("SERIAL123"),
        Some("X1 Carbon"),
    )]);
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        link_printer_command_with_type(command_id.clone(), "SECRET-LINK-CODE", "Other"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(receiver.recv().await.unwrap(), ack_event(&config, &command_id));
    assert_failure_contains(receiver.recv().await.unwrap(), &command_id, "unsupported printer type Other");
    assert!(receiver.recv().await.is_none());
    assert!(gateway.linked_endpoints().await.is_empty());
}
```

- [x] **Step 2: Run focused tests and confirm the expected failure**

Run:

```bash
cargo test -p pandar-agent commands::tests::link_printer_emits_ack_snapshot_and_success_without_access_code -- --exact
```

Expected: FAIL before implementation because the generated proto/helper still expects serial/model or because `LinkGateway::discover_printers` is still unreachable.

- [x] **Step 3: Implement discovery-backed link-printer handling**

Change the imports in `crates/pandar-agent/src/commands.rs`:

```rust
use anyhow::{Context, anyhow};
```

Replace the start of `emit_link_printer_events` with ack, type validation, discovery, and endpoint construction:

```rust
sender
    .send(ack_event(config, command_id))
    .await
    .context("queue link-printer command ack")?;

let access_code = command.access_code;
let access_code_for_error = access_code.clone();
let printer_type = command.printer_type.trim().to_owned();
if printer_type != "BambuLab" {
    sender
        .send(failure_event(
            config,
            command_id,
            format!("unsupported printer type {printer_type}"),
        ))
        .await
        .context("queue link-printer unsupported type failure")?;
    return Ok(());
}

let host = command.host.trim().to_owned();
let endpoint = match gateway
    .discover_printers(3)
    .await
    .with_context(|| format!("discover Bambu printer at {host}"))
    .and_then(|result| {
        let printer = result
            .printers
            .into_iter()
            .find(|printer| printer.host.trim() == host.as_str())
            .ok_or_else(|| anyhow!("could not discover printer at {host}"))?;
        let serial = non_blank_string(printer.serial_number.unwrap_or_default())
            .ok_or_else(|| anyhow!("printer serial could not be discovered for {host}"))?;
        Ok(BambuPrinterEndpoint {
            host: host.clone(),
            serial,
            access_code: access_code.clone(),
            name: non_blank_string(command.name),
            model: printer.model.and_then(non_blank_string),
        })
    }) {
    Ok(endpoint) => endpoint,
    Err(err) => {
        let error = redact_link_error(gateway, &format!("{err:#}"), &access_code_for_error);
        sender
            .send(failure_event(config, command_id, error))
            .await
            .context("queue link-printer discovery failure")?;
        return Ok(());
    }
};
```

Keep the existing `gateway.link_printer(endpoint.clone(), config, sender).await` match, but use the endpoint built above. The success JSON must continue to use `snapshot.serial`, `snapshot.name`, `snapshot.model`, and `snapshot.state`:

```rust
let result_json = serde_json::json!({
    "type": "printer_link",
    "serial_number": snapshot.serial,
    "host": endpoint.host,
    "name": snapshot.name,
    "model": snapshot.model,
    "status": snapshot.state,
})
.to_string();
```

Keep runtime failure logging redacted:

```rust
let error = redact_link_error(gateway, &format!("{err:#}"), &endpoint.access_code);
tracing::warn!(
    serial = %endpoint.serial,
    error = %error,
    "runtime printer link failed"
);
```

- [x] **Step 4: Run focused agent tests**

Run:

```bash
cargo test -p pandar-agent commands::tests::link_printer_emits_ack_snapshot_and_success_without_access_code -- --exact
cargo test -p pandar-agent commands::tests::link_printer_fails_when_discovery_does_not_find_host -- --exact
cargo test -p pandar-agent commands::tests::link_printer_fails_when_discovered_printer_has_no_serial -- --exact
cargo test -p pandar-agent commands::tests::link_printer_rejects_unsupported_type_without_discovery -- --exact
cargo test -p pandar-agent commands::tests::link_printer_failure_redacts_access_code_from_result_error -- --exact
```

Expected: PASS, with no command result or log containing `SECRET-LINK-CODE`.

---

### Task 3: Frontend Add-Printer Form And Server Action

**Files:**
- Modify: `frontend/app/link-printer-form.tsx`
- Modify: `frontend/app/actions.ts`
- Create: `frontend/app/actions.test.ts`
- Modify: `frontend/app/agent-pairing-guidance.test.tsx`
- Modify: `frontend/messages/en.json`
- Modify: `frontend/messages/zh.json`

**Interfaces:**
- Consumes: Hub request shape from Task 1.
- Produces: form fields `agent_id`, hidden `tenant_id`, `type`, `host`, `access_code`, and optional `name`.
- Produces: `linkPrinter(formData)` POST body `{ type, host, access_code, name }` with no `serial_number` or `model` keys.
- Produces: localized labels for Type and Printer IPv4 address.

- [x] **Step 1: Write/update frontend tests for the new form and action payload**

Update `frontend/app/agent-pairing-guidance.test.tsx` form assertions:

```tsx
expect(screen.getByLabelText("Agent")).toHaveValue("agent-online");
expect(screen.getByLabelText("Type")).toHaveValue("BambuLab");
expect(screen.getByLabelText("Printer IPv4 address")).toHaveAttribute("name", "host");
expect(screen.getByLabelText("Access code")).toHaveAttribute("name", "access_code");
expect(screen.getByLabelText("Name")).toHaveAttribute("name", "name");
expect(screen.queryByLabelText("Serial number")).not.toBeInTheDocument();
expect(screen.queryByLabelText("Model")).not.toBeInTheDocument();
```

Update the no-tenant empty-state assertion because the existing English message says `printer credentials`, which is no longer accurate for a host/access-code submission. Keep the existing empty-state titles unchanged:

```tsx
expect(screen.getByText("Select a tenant to link a printer."));
expect(screen.getByText("Choose a tenant from the header before submitting printer connection details."));
expect(screen.getByText("No agents available for printer linking."));
expect(screen.getByText("Pair an agent before linking a printer."));
```

Create `frontend/app/actions.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { linkPrinter } from './actions'

const redirectMock = vi.fn((url: string) => {
  throw new Error(`NEXT_REDIRECT:${url}`)
})

vi.mock('next/navigation', () => ({
  redirect: redirectMock,
}))

vi.mock('./api-auth', () => ({
  requireAuth: vi.fn(async () => undefined),
  apiHeaders: vi.fn(async () => ({ 'content-type': 'application/json' })),
}))

describe('linkPrinter', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(JSON.stringify({ id: 'command-1' }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
      ),
    )
  })

  it('posts type, host, access code, and optional name without serial or model', async () => {
    const formData = new FormData()
    formData.set('tenant_id', 'tenant-1')
    formData.set('agent_id', 'agent-1')
    formData.set('type', 'BambuLab')
    formData.set('host', '192.0.2.10')
    formData.set('access_code', 'SECRET-LINK-CODE')
    formData.set('name', 'Office X1C')

    await expect(linkPrinter(formData)).rejects.toThrow('NEXT_REDIRECT:/agents?tenant=tenant-1&command=command-1')

    expect(fetch).toHaveBeenCalledWith(
      'http://localhost:8080/api/v1/tenants/tenant-1/agents/agent-1/link-printer',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({
          type: 'BambuLab',
          host: '192.0.2.10',
          access_code: 'SECRET-LINK-CODE',
          name: 'Office X1C',
        }),
      }),
    )
    const init = vi.mocked(fetch).mock.calls[0][1] as RequestInit
    const body = JSON.parse(String(init.body)) as Record<string, unknown>
    expect(body.serial_number).toBeUndefined()
    expect(body.model).toBeUndefined()
  })
})
```

- [x] **Step 2: Run focused frontend tests and confirm the expected failure**

Run:

```bash
npm run test --workspace pandar-web -- agent-pairing-guidance actions.test
```

Expected: FAIL before implementation because the form still renders serial/model and the server action still posts those keys instead of `type`.

- [x] **Step 3: Implement the form, action, and messages**

Change `frontend/app/link-printer-form.tsx` after the agent selector to add the type selector:

```tsx
<label className="flex flex-col gap-1 text-sm">
  <span className="text-xs font-medium text-slate-500">{t('type')}</span>
  <select
    className="h-9 rounded-md border border-slate-300 bg-white px-2 text-sm text-slate-950"
    defaultValue="BambuLab"
    name="type"
    required
  >
    <option value="BambuLab">{t('typeBambuLab')}</option>
  </select>
</label>
```

Keep the host input but let the label copy come from `t('host')`, which now means Printer IPv4 address. Remove the entire serial-number label/input block and the entire model label/input block. The remaining editable fields are type, host, access code, and optional name.

Change `frontend/app/actions.ts` `linkPrinter` body:

```ts
const response = await postJson(
  `/api/v1/tenants/${tenantId}/agents/${agentId}/link-printer`,
  {
    type: stringField(formData, "type"),
    host: stringField(formData, "host"),
    access_code: stringField(formData, "access_code"),
    name: nullableField(formData, "name"),
  },
);
```

Update `frontend/messages/en.json` `linkPrinter` keys:

```json
{
  "title": "Link printer to agent",
  "subtitleTenant": "Submit Bambu LAN connection details for {name}.",
  "subtitleNone": "No tenant selected.",
  "meta": "{count} agents",
  "noTenantTitle": "Select a tenant to link a printer.",
  "noTenantMessage": "Choose a tenant from the header before submitting printer connection details.",
  "noAgentsTitle": "No agents available for printer linking.",
  "noAgentsMessage": "Pair an agent before linking a printer.",
  "agent": "Agent",
  "agentOption": "{name} ({status})",
  "type": "Type",
  "typeBambuLab": "BambuLab",
  "host": "Printer IPv4 address",
  "accessCode": "Access code",
  "name": "Name",
  "submit": "Link printer"
}
```

Update `frontend/messages/zh.json` `linkPrinter` keys with equivalent labels:

```json
{
  "title": "将打印机连接到 Agent",
  "subtitleTenant": "提交 {name} 的 Bambu 局域网连接信息。",
  "subtitleNone": "未选择租户。",
  "meta": "{count} 个 Agent",
  "noTenantTitle": "选择租户后连接打印机。",
  "noTenantMessage": "提交打印机连接信息前，请先从页头选择租户。",
  "noAgentsTitle": "没有可用于连接打印机的 Agent。",
  "noAgentsMessage": "连接打印机前请先配对 Agent。",
  "agent": "Agent",
  "agentOption": "{name}（{status}）",
  "type": "类型",
  "typeBambuLab": "BambuLab",
  "host": "打印机 IPv4 地址",
  "accessCode": "访问码",
  "name": "名称",
  "submit": "连接打印机"
}
```

Keep unrelated diagnostics strings such as global `model` labels unchanged, because command result rendering still displays discovered model metadata.

- [x] **Step 4: Run focused frontend tests**

Run:

```bash
npm run test --workspace pandar-web -- agent-pairing-guidance actions.test
```

Expected: PASS.

---

### Task 4: Documentation And Verification Preparation

**Files:**
- Modify: `docs/development.md`
- Modify: `docs/roadmap.md`

**Interfaces:**
- Consumes: implemented behavior from Tasks 1-3.
- Produces: user/developer docs stating runtime add-printer now asks for type, IPv4 address, access code, and optional name, while agent discovery supplies serial/model after onboarding.
- Produces: roadmap entry recording the completed add-printer simplification.

- [x] **Step 1: Update development docs**

In `docs/development.md`, keep the existing `PANDAR_PRINTERS` configured-printer documentation intact. Add this paragraph near the runtime printer-linking section:

```markdown
Dashboard runtime printer linking is separate from `PANDAR_PRINTERS`. The Agents page add-printer form sends `type = BambuLab`, printer IPv4 address, access code, and optional display name to the selected live local agent. Operators do not enter serial number or model for this flow; the agent resolves those values through Bambu SSDP discovery and MQTT validation, then reports them back through the printer snapshot and command result. If SSDP cannot see the submitted IPv4 address or the discovery response lacks a serial number, the link command fails without persisting the access code in Hub storage.
```

- [x] **Step 2: Update roadmap**

In `docs/roadmap.md`, update the existing runtime printer-linking completed bullet that currently says operators submit host/IP, serial, access code, and optional name/model. Replace it with:

```markdown
- Updated runtime printer linking from the dashboard Agents page: operators now submit printer type (`BambuLab`), printer IPv4 address, access code, and optional name only; the agent discovers serial/model during Bambu onboarding and reports the completed metadata through snapshots and the link result, while Hub continues to store only redacted command/audit data.
```

- [x] **Step 3: Run full formatting and focused verification**

Run:

```bash
cargo fmt
cargo test -p pandar-hub routes::tests::printers::link_printer_direct_sends_secret_but_persists_only_redacted_payload -- --exact
cargo test -p pandar-hub routes::tests::printers::link_printer_rejects_invalid_type_host_and_legacy_metadata_fields -- --exact
cargo test -p pandar-hub repositories::tests::commands::command_create_link_printer_sent_persists_redacted_payload_and_audit -- --exact
cargo test -p pandar-hub repositories::tests::postgres_commands::postgres_link_printer_command_behavior_when_configured -- --exact
cargo test -p pandar-hub grpc::tests::commands::grpc_hub_command_from_record_rejects_persisted_link_printer_replay -- --exact
cargo test -p pandar-hub runtime::tests::runtime_stale_link_printer_cleanup_skips_pending_live_commands -- --exact
cargo test -p pandar-hub sessions::tests::sessions_replacement_race_does_not_leave_pending_command -- --exact
cargo test -p pandar-agent commands::tests::link_printer_emits_ack_snapshot_and_success_without_access_code -- --exact
cargo test -p pandar-agent commands::tests::link_printer_fails_when_discovery_does_not_find_host -- --exact
cargo test -p pandar-agent commands::tests::link_printer_fails_when_discovered_printer_has_no_serial -- --exact
cargo test -p pandar-agent commands::tests::link_printer_rejects_unsupported_type_without_discovery -- --exact
cargo test -p pandar-agent tests::ended_command_stream_preserves_runtime_linked_printer_for_reconnect -- --exact
npm run test --workspace pandar-web -- agent-pairing-guidance actions.test
```

Expected: PASS.

- [x] **Step 4: Run broad verification before implementation review**

Run:

```bash
cargo clippy
cargo nextest run --manifest-path "Cargo.toml" --workspace
npm run test --workspace pandar-web
```

Expected: PASS. If a broad command is blocked by environment, capture the exact command, error output, and the focused tests that did pass before requesting the final independent implementation review.

---

## Self-Review

**Spec coverage:**
- Form removes serial/model inputs and adds default `BambuLab` type selector: Task 3.
- Server action posts type/host/access_code/name only: Task 3.
- Hub request validation rejects legacy serial/model, invalid type, blank fields, and non-IPv4 host: Task 1.
- Hub persisted payload and audit metadata are redacted and contain no operator serial/model for both default repository tests and PostgreSQL-specific repository tests: Task 1.
- Proto reserves old serial/model tags and sends `printer_type` only: Task 1.
- Durable replay of persisted `link_printer` remains rejected: Task 1.
- Agent discovers serial/model before MQTT runtime link validation: Task 2.
- Agent sends discovered serial/model in snapshot and successful `printer_link` result JSON: Task 2.
- Discovery miss, missing serial, unsupported type, and access-code redaction are tested: Task 2.
- Docs and roadmap update are covered: Task 4.

**Red-flag scan:** The plan contains no deferred implementation markers and every task has concrete files, snippets, and verification commands.

**Type consistency:** The plan uses `printer_type` internally/proto-side and public JSON field `type`; `LinkPrinterPayload`, `RedactedLinkPrinterPayload`, `LinkPrinterRequest`, and proto `LinkPrinter` names match across tasks.
