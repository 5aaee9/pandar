# Agent AMS Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let operators refresh current AMS/external-spool material state through the agent, keep material state updated from printer MQTT broadcasts over the existing gRPC agent stream, and show the refreshed state in the printer inventory UI.

**Architecture:** Add first-class material refresh protocol messages while reusing the existing Bambu MQTT `pushall` command and normalized material patch JSON. Hub persists material patches through the existing material snapshot repository, publishes material-aware `printer_snapshot` browser events, and exposes a per-printer HTTP command route for the UI. Frontend adds a compact row-level `Refresh AMS` action and fixes spool-related Chinese copy to use `料盘`.

**Tech Stack:** Rust 2024, tokio, tonic/prost, axum, SeaORM-backed repositories, serde_json, Next.js App Router, React, next-intl, Tailwind, Vitest/React Testing Library.

## Global Constraints

- Approved spec: `docs/superpowers/specs/2026-07-02-agent-ams-refresh-design.md` is binding.
- Agent-to-Hub material synchronization must use the existing reverse gRPC stream, not WebSocket.
- Hub-to-browser live updates may continue using the existing printer event WebSocket.
- Use existing material patch JSON and `printer_material_snapshots`; do not add database migrations.
- `PrinterMaterialsSnapshot` has no outer timestamp; the embedded patch `observed_at` is the canonical timestamp for persistence, stale comparison, and HTTP responses.
- `refresh_printers` succeeds and emits only `PrinterSnapshot` when printer state refresh succeeds but no AMS material patch is received before the total material deadline.
- Single-printer `RefreshPrinterMaterials` fails when no AMS material patch is received before the total material deadline.
- Material event-local validation failures are logged and dropped without closing the reverse gRPC stream; session identity mismatches remain fatal.
- Browser `printer_snapshot` event payloads must include the same sanitized `materials` field as tenant printer HTTP responses.
- Chinese spool-related copy must use `料盘`; `spool` must not be translated as `盘子`.
- Preserve lower-level error cause chains by formatting errors with `{err:#}` before logging or returning redacted printer/agent errors.
- Avoid unrelated refactors and speculative features.
- Update `docs/roadmap.md` after implementation and add the new material refresh HTTP route to `docs/development.md`.
- Run `cargo fmt`, `cargo clippy`, and `cargo nextest run --manifest-path Cargo.toml --workspace` after code edits when feasible.
- `$sdd-workflow` controls commits: do not create task-local commits. Commit and push only after final spec-implementation reviewer approval and fresh verification.

---

## File Structure

- Modify `proto/pandar/agent/v1/agent.proto`: add `PrinterMaterialsSnapshot` agent event and `RefreshPrinterMaterials` hub command.
- Modify `crates/pandar-agent/src/machine/mqtt.rs`: add material refresh helpers, total-deadline scan, material event builder, and report-forwarder material event emission.
- Modify `crates/pandar-agent/src/machine/mod.rs`: add `PrinterRefreshResult`, `MaterialRefreshResult`, and `refresh_printer_materials` gateway method.
- Modify `crates/pandar-agent/src/machine/runtime.rs`: propagate material refresh through configured and runtime-linked printers.
- Modify `crates/pandar-agent/src/commands.rs`: handle `RefreshPrinterMaterials` and emit material events from `RefreshPrinters` results.
- Modify agent tests in `crates/pandar-agent/src/machine/mqtt/tests.rs`, `crates/pandar-agent/src/machine/materials/tests.rs`, `crates/pandar-agent/src/commands/tests.rs`, and runtime test support in `crates/pandar-agent/src/machine/runtime.rs`.
- Modify `crates/pandar-hub/src/repositories/materials.rs` and submodules: return explicit material patch outcomes.
- Modify `crates/pandar-hub/src/repositories/jobs/print_reports.rs`: use material patch outcomes to publish material-only printer updates.
- Modify `crates/pandar-hub/src/printer_events.rs`: make printer event payloads material-aware.
- Modify `crates/pandar-hub/src/grpc/printer_snapshots.rs`, `crates/pandar-hub/src/grpc/print_reports.rs`, and add/modify a material snapshot handler under `crates/pandar-hub/src/grpc/`.
- Modify `crates/pandar-hub/src/repositories/commands.rs`, `crates/pandar-hub/src/repositories/commands/audit.rs`, `crates/pandar-hub/src/repositories/commands/enqueue.rs`, and `crates/pandar-hub/src/grpc/commands.rs`: add `refresh_printer_materials` command payload/enqueue/conversion.
- Modify `crates/pandar-hub/src/routes/printers.rs` and `crates/pandar-hub/src/routes.rs`: add per-printer material refresh route.
- Modify Hub tests in `crates/pandar-hub/src/repositories/tests/materials.rs`, `crates/pandar-hub/src/repositories/tests/jobs/*`, `crates/pandar-hub/src/grpc/tests/*`, `crates/pandar-hub/src/routes/tests/printers.rs`, and `crates/pandar-hub/src/routes/tests/printer_events_ws.rs`.
- Modify `frontend/app/actions.ts`, `frontend/app/action-status.ts`, `frontend/app/dashboard-inventory.tsx`, `frontend/app/dashboard-types.ts`, and `frontend/app/dashboard-runtime.tsx` so frontend actions and browser event typing match the material-aware event shape.
- Modify frontend tests in `frontend/app/actions.test.ts`, add `frontend/app/dashboard-inventory.test.tsx`, and modify `frontend/app/action-status-toast.test.tsx`.
- Modify `frontend/messages/en.json` and `frontend/messages/zh.json`.
- Modify `docs/roadmap.md`; optionally modify `docs/development.md` for route docs.

---

### Task 1: Protocol And Agent Material Refresh

**Files:**
- Modify: `proto/pandar/agent/v1/agent.proto`
- Modify: `crates/pandar-agent/src/machine/mqtt.rs`
- Modify: `crates/pandar-agent/src/machine/mod.rs`
- Modify: `crates/pandar-agent/src/machine/runtime.rs`
- Modify: `crates/pandar-agent/src/commands.rs`
- Test: `crates/pandar-agent/src/machine/mqtt/tests.rs`
- Test: `crates/pandar-agent/src/commands/tests.rs`

**Interfaces:**
- Produces proto `AgentEvent::PrinterMaterialsSnapshot(PrinterMaterialsSnapshot { serial, printer_id, printer_materials_json })`.
- Produces proto `HubCommand::RefreshPrinterMaterials(RefreshPrinterMaterials { printer_id, serial_number })`.
- Produces `PrinterRefreshResult { snapshot: MachineSnapshot, materials: Option<MaterialRefreshResult> }`.
- Produces `MaterialRefreshResult { serial: String, printer_id: Option<String>, printer_materials_json: String }`.
- Produces `BambuMachineGateway::refresh_printer_materials(&self, serial_number: &str, printer_id: Option<&str>) -> anyhow::Result<MaterialRefreshResult>`.

- [ ] **Step 1: Write failing protocol/agent MQTT tests**

Add tests that fail before implementation:

```rust
#[tokio::test]
async fn refresh_printer_returns_material_patch_when_pushall_report_has_ams() {
    let transport = FakeMqttTransport::with_reports([
        get_version_report("A1 Mini"),
        json!({"print": {"gcode_state": "IDLE", "ams": {"ams": [{"id": "0", "tray": [{"id": "0", "tray_type": "PLA", "tray_color": "FF0000"}]}], "tray_now": "0"}}}),
    ]);

    let refreshed = refresh_printer(&transport, &endpoint(), Duration::from_secs(1)).await.unwrap();

    assert_eq!(refreshed.snapshot.serial, "01S00EXAMPLE");
    let materials = refreshed.materials.unwrap();
    let patch: serde_json::Value = serde_json::from_str(&materials.printer_materials_json).unwrap();
    assert_eq!(patch["type"], "printer_material_patch");
    assert_eq!(patch["ams_units"][0]["trays"][0]["type"], "PLA");
}

#[tokio::test]
async fn refresh_printer_keeps_first_snapshot_and_continues_until_ams_patch() {
    let transport = FakeMqttTransport::with_reports([
        get_version_report("A1 Mini"),
        json!({"print": {"gcode_state": "IDLE"}}),
        json!({"print": {"gcode_state": "IDLE", "ams": {"ams": [{"id": "0", "tray": [{"id": "0", "tray_type": "PLA"}]}]}}}),
    ]);

    let refreshed = refresh_printer(&transport, &endpoint(), Duration::from_secs(1)).await.unwrap();

    assert_eq!(refreshed.snapshot.state, "IDLE");
    assert!(refreshed.materials.is_some());
}

#[tokio::test]
async fn material_refresh_uses_total_deadline_for_infinite_non_ams_reports() {
    let transport = FakeMqttTransport::with_infinite_unrelated_reports();

    let err = refresh_printer_materials(&transport, &endpoint(), None, Duration::from_millis(10))
        .await
        .unwrap_err();

    let error = format!("{err:#}");
    assert!(error.contains("no AMS material report received before timeout"));
}
```

Add command tests that fail before implementation:

```rust
#[tokio::test]
async fn refresh_printer_materials_command_emits_material_snapshot_and_success() {
    let config = test_config();
    let gateway = FakeGateway::ok_with_materials([
        refresh_result(
            snapshot("SERIAL123", "garage", Some("A1 Mini"), "READY"),
            material_result("SERIAL123", Some("printer-1")),
        ),
    ]);
    let (sender, mut events) = mpsc::channel(8);

    handle_command_with_gateway(&config, &gateway, &sender, refresh_materials_command("printer-1", "SERIAL123")).await.unwrap();

    assert!(matches!(events.recv().await.unwrap().event, Some(agent_event::Event::CommandAck(_))));
    assert!(matches!(events.recv().await.unwrap().event, Some(agent_event::Event::PrinterMaterialsSnapshot(_))));
    assert!(matches!(events.recv().await.unwrap().event, Some(agent_event::Event::CommandResult(result)) if result.success));
}

#[tokio::test]
async fn refresh_printers_emits_snapshot_then_material_snapshot_then_success() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = FakeGateway::ok_with_materials([
        refresh_result(
            snapshot("SERIAL1", "garage", Some("A1 Mini"), "READY"),
            material_result("SERIAL1", None),
        ),
    ]);
    let (sender, mut receiver) = mpsc::channel(4);

    handle_command_with_gateway(&config, &gateway, &sender, refresh_command(command_id.clone())).await.unwrap();
    drop(sender);

    assert_eq!(receiver.recv().await.unwrap(), ack_event(&config, &command_id));
    assert_snapshot(receiver.recv().await.unwrap(), "SERIAL1", "garage", "A1 Mini", "READY");
    assert_material_snapshot(receiver.recv().await.unwrap(), "SERIAL1", None);
    assert_eq!(receiver.recv().await.unwrap(), success_event(&config, &command_id));
    assert!(receiver.recv().await.is_none());
}

#[tokio::test]
async fn refresh_printers_succeeds_with_snapshot_only_when_materials_absent() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = FakeGateway::ok_with_materials([
        PrinterRefreshResult { snapshot: snapshot("SERIAL1", "garage", Some("A1 Mini"), "READY"), materials: None },
    ]);
    let (sender, mut receiver) = mpsc::channel(3);

    handle_command_with_gateway(&config, &gateway, &sender, refresh_command(command_id.clone())).await.unwrap();
    drop(sender);

    assert_eq!(receiver.recv().await.unwrap(), ack_event(&config, &command_id));
    assert_snapshot(receiver.recv().await.unwrap(), "SERIAL1", "garage", "A1 Mini", "READY");
    assert_eq!(receiver.recv().await.unwrap(), success_event(&config, &command_id));
    assert!(receiver.recv().await.is_none());
}

#[tokio::test]
async fn refresh_printer_materials_missing_serial_and_timeout_fail_with_redacted_errors() {
    let config = test_config();
    let gateway = FakeGateway::material_fail_with_access_code(
        "ACCESS-CODE-SECRET",
        anyhow::anyhow!("no configured Bambu printer matches serial SERIAL404"),
    );
    let (sender, mut receiver) = mpsc::channel(2);
    let command_id = uuid::Uuid::new_v4().to_string();

    handle_command_with_gateway(&config, &gateway, &sender, refresh_materials_command("printer-1", "SERIAL404", command_id.clone())).await.unwrap();
    drop(sender);

    assert_eq!(receiver.recv().await.unwrap(), ack_event(&config, &command_id));
    let failure = receiver.recv().await.unwrap();
    assert_failure_contains(failure, &command_id, "no configured Bambu printer matches serial SERIAL404");
    assert!(!format!("{:?}", receiver).contains("ACCESS-CODE-SECRET"));
}

#[tokio::test]
async fn refresh_printer_materials_command_timeout_emits_ack_then_failure_without_material_snapshot() {
    let config = test_config();
    let gateway = FakeGateway::material_fail_with_access_code(
        "ACCESS-CODE-SECRET",
        anyhow::anyhow!("timed out waiting for MQTT report").context("no AMS material report received before timeout"),
    );
    let (sender, mut receiver) = mpsc::channel(3);
    let command_id = uuid::Uuid::new_v4().to_string();

    handle_command_with_gateway(&config, &gateway, &sender, refresh_materials_command("printer-1", "SERIAL1", command_id.clone())).await.unwrap();
    drop(sender);

    assert_eq!(receiver.recv().await.unwrap(), ack_event(&config, &command_id));
    let failure = receiver.recv().await.unwrap();
    assert_failure_contains(failure, &command_id, "no AMS material report received before timeout");
    assert!(receiver.recv().await.is_none());
}

#[tokio::test]
async fn refresh_printer_materials_command_works_for_runtime_linked_printer() {
    let config = test_config();
    let gateway = TestRuntimeBambuMachineGateway::new(
        Vec::new(),
        FakeMachineFileTransfer::default(),
        Duration::from_millis(50),
    );
    gateway.push_command_transport(FakeMqttTransport::with_reports([
        get_version_report("A1 Mini"),
        json!({"print": {"gcode_state": "READY", "ams": {"ams": [{"id": "0", "tray": [{"id": "0", "tray_type": "PLA"}]}]}}}),
    ])).await;
    gateway.set_discovered_printers(vec![DiscoveredPrinter {
        serial_number: Some("SERIAL123".to_owned()),
        host: "192.0.2.10".to_owned(),
        name: Some("garage".to_owned()),
        model: Some("A1 Mini".to_owned()),
        source: "ssdp",
    }]).await;
    let (sender, mut events) = mpsc::channel(8);

    handle_command_with_gateway(&config, &gateway, &sender, link_printer_command(uuid::Uuid::new_v4().to_string(), "ACCESS-CODE-SECRET")).await.unwrap();
    drain_until_success(&mut events).await;

    let command_id = uuid::Uuid::new_v4().to_string();
    handle_command_with_gateway(&config, &gateway, &sender, refresh_materials_command("printer-1", "SERIAL123", command_id.clone())).await.unwrap();

    assert_eq!(events.recv().await.unwrap(), ack_event(&config, &command_id));
    assert_material_snapshot(events.recv().await.unwrap(), "SERIAL123", Some("printer-1"));
}

#[tokio::test]
async fn refresh_printer_materials_command_unknown_runtime_serial_fails_redacted() {
    let config = test_config();
    let gateway = TestRuntimeBambuMachineGateway::new(
        Vec::new(),
        FakeMachineFileTransfer::default(),
        Duration::from_millis(50),
    );
    let (sender, mut events) = mpsc::channel(4);
    let command_id = uuid::Uuid::new_v4().to_string();

    handle_command_with_gateway(&config, &gateway, &sender, refresh_materials_command("printer-1", "UNKNOWN", command_id.clone())).await.unwrap();

    assert_eq!(events.recv().await.unwrap(), ack_event(&config, &command_id));
    let failure = events.recv().await.unwrap();
    assert_failure_contains(failure, &command_id, "no configured Bambu printer matches serial UNKNOWN");
}

#[tokio::test]
async fn forward_print_reports_emits_material_snapshot_for_unsolicited_ams_report() {
    let config = test_config();
    let transport = FakeMqttTransport::with_reports([json!({
        "print": {"gcode_state": "IDLE", "ams": {"ams": [{"id": "0", "tray": [{"id": "0", "tray_type": "PLA"}]}]}}
    })]);
    let (sender, mut receiver) = mpsc::channel(2);

    let task = tokio::spawn(async move {
        forward_print_reports(&config, &transport, &endpoint(), Duration::from_millis(50), &sender).await.unwrap();
    });

    let first = receiver.recv().await.unwrap();
    assert!(matches!(first.event, Some(agent_event::Event::PrintJobReport(_))));
    let second = receiver.recv().await.unwrap();
    assert_material_snapshot(second, "01S00EXAMPLE", None);
    task.abort();
}
```

Add test helpers in `commands/tests.rs` as part of the RED step so test intent is concrete:

```rust
fn refresh_result(snapshot: MachineSnapshot, materials: MaterialRefreshResult) -> PrinterRefreshResult {
    PrinterRefreshResult { snapshot, materials: Some(materials) }
}

fn material_result(serial: &str, printer_id: Option<&str>) -> MaterialRefreshResult {
    MaterialRefreshResult {
        serial: serial.to_owned(),
        printer_id: printer_id.map(str::to_owned),
        printer_materials_json: json!({
            "type": "printer_material_patch",
            "observed_at": "2026-07-02T00:00:00Z",
            "ams_units": [{"unit_id": "0", "trays": [{"tray_id": "0", "type": "PLA"}]}],
            "external_spools": []
        }).to_string(),
    }
}
```

- [ ] **Step 2: Run focused tests to verify RED**

Run:

```bash
cargo test -p pandar-agent machine::mqtt::tests::refresh_printer_returns_material_patch_when_pushall_report_has_ams
cargo test -p pandar-agent machine::mqtt::tests::refresh_printer_keeps_first_snapshot_and_continues_until_ams_patch
cargo test -p pandar-agent machine::mqtt::tests::material_refresh_uses_total_deadline_for_infinite_non_ams_reports
cargo test -p pandar-agent commands::tests::refresh_printer_materials_command_emits_material_snapshot_and_success
cargo test -p pandar-agent commands::tests::refresh_printers_emits_snapshot_then_material_snapshot_then_success
cargo test -p pandar-agent commands::tests::refresh_printers_succeeds_with_snapshot_only_when_materials_absent
cargo test -p pandar-agent commands::tests::refresh_printer_materials_missing_serial_and_timeout_fail_with_redacted_errors
cargo test -p pandar-agent commands::tests::refresh_printer_materials_command_timeout_emits_ack_then_failure_without_material_snapshot
cargo test -p pandar-agent commands::tests::refresh_printer_materials_command_works_for_runtime_linked_printer
cargo test -p pandar-agent commands::tests::refresh_printer_materials_command_unknown_runtime_serial_fails_redacted
cargo test -p pandar-agent machine::mqtt::tests::forward_print_reports_emits_material_snapshot_for_unsolicited_ams_report
```

Expected: tests fail because proto variants, result types, or material refresh helper are missing.

- [ ] **Step 3: Implement protocol and agent material refresh**

Update `agent.proto` with the spec's new messages. Update agent code so the material refresh path:

```rust
pub async fn refresh_printer_materials<T>(
    transport: &T,
    endpoint: &BambuPrinterEndpoint,
    printer_id: Option<&str>,
    report_timeout: Duration,
) -> anyhow::Result<MaterialRefreshResult>
where
    T: BambuMqttTransport + ?Sized,
{
    let topics = BambuMqttTopics::for_serial(&endpoint.serial);
    transport.subscribe(&topics.report).await?;
    transport.publish(PublishedMqttCommand {
        topic: topics.request.clone(),
        payload: BambuMqttCommand::RequestPushAll.payload(),
        qos: BAMBU_MQTT_QOS,
    }).await?;

    let deadline = tokio::time::Instant::now() + report_timeout;
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            anyhow::bail!("no AMS material report received before timeout");
        }
        let remaining = deadline.saturating_duration_since(now);
        let report = match transport.next_report(remaining).await {
            Ok(report) => report,
            Err(err) if tokio::time::Instant::now() >= deadline => {
                return Err(err).context("no AMS material report received before timeout");
            }
            Err(err) => return Err(err),
        };
        let observed_at = pandar_core::created_at_now();
        if let Some(patch) = normalize_material_patch(&report, &observed_at) {
            return Ok(MaterialRefreshResult {
                serial: endpoint.serial.clone(),
                printer_id: printer_id.map(str::to_owned),
                printer_materials_json: serde_json::to_string(&patch)?,
            });
        }
    }
}
```

Keep final code idiomatic and avoid duplicating subscription/publish setup more than needed. `refresh_printer` should return `PrinterRefreshResult`: it uses the first valid pushall report to build `snapshot`, then continues scanning reports under the same total material deadline until a normalized material patch is found or the deadline expires. If a later report contains AMS data before the deadline, `materials` is `Some`; if no AMS data arrives, `materials` is `None` and the snapshot result still succeeds. Command dispatch must emit `PrinterSnapshot` before `PrinterMaterialsSnapshot` for `RefreshPrinters` results.

- [ ] **Step 4: Verify GREEN for agent**

Run:

```bash
cargo test -p pandar-agent machine::mqtt::tests
cargo test -p pandar-agent commands::tests
```

Expected: all selected agent tests pass.

---

### Task 2: Hub Material Outcome And Material-Aware Events

**Files:**
- Modify: `crates/pandar-hub/src/repositories/materials.rs`
- Modify: `crates/pandar-hub/src/repositories/materials/patch.rs`
- Modify: `crates/pandar-hub/src/repositories/jobs/print_reports.rs`
- Modify: `crates/pandar-hub/src/printer_events.rs`
- Modify: `crates/pandar-hub/src/grpc/printer_snapshots.rs`
- Modify: `crates/pandar-hub/src/grpc/print_reports.rs`
- Add or modify: `crates/pandar-hub/src/grpc/printer_materials.rs`
- Modify: `crates/pandar-hub/src/grpc.rs`
- Test: `crates/pandar-hub/src/repositories/tests/materials.rs`
- Test: `crates/pandar-hub/src/repositories/tests/postgres.rs`
- Test: `crates/pandar-hub/src/grpc/tests/printer_snapshots.rs`
- Test: `crates/pandar-hub/src/grpc/tests/print_reports.rs`
- Test: `crates/pandar-hub/src/routes/tests/printer_events_ws.rs`

**Interfaces:**
- Produces `MaterialPatchOutcome::{Empty, Invalid { error }, Older, Unchanged(MaterialSnapshot), Changed(MaterialSnapshot)}`.
- Produces a helper for building material-aware printer event DTOs from `Printer` plus latest `MaterialSnapshot`.
- Produces `PrinterEvent::PrinterSnapshot { printer: Box<PrinterEventPrinter> }`, where `PrinterEventPrinter` serializes the same fields as `PrinterResponse`, including sanitized `materials`.
- Produces non-fatal `PrinterMaterialsSnapshot` handling for event-local validation failures.

- [ ] **Step 1: Write failing Hub material outcome and event tests**

Add repository tests:

```rust
#[tokio::test]
async fn material_repository_reports_changed_unchanged_empty_invalid_and_older_outcomes() {
    let (materials, tenant, agent, printer_id) = fixture().await;

    let input = |body: serde_json::Value| MaterialPatchInput {
        tenant_id: tenant.id,
        agent_id: agent.id,
        printer_id: printer_id.clone(),
        serial_number: "serial".to_owned(),
        printer_materials_json: body.to_string(),
    };

    assert!(matches!(materials.upsert_from_patch_outcome(MaterialPatchInput {
        tenant_id: tenant.id,
        agent_id: agent.id,
        printer_id: printer_id.clone(),
        serial_number: "serial".to_owned(),
        printer_materials_json: String::new(),
    }).await.unwrap(), MaterialPatchOutcome::Empty));
    assert!(matches!(materials.upsert_from_patch_outcome(input(json!({"type":"wrong"}))).await.unwrap(), MaterialPatchOutcome::Invalid { .. }));

    let patch = |observed_at: &str| json!({
        "type": "printer_material_patch",
        "observed_at": observed_at,
        "ams_units": [{"unit_id": "0", "trays": [{"tray_id": "0", "type": "PLA"}]}],
        "external_spools": []
    });

    let changed = materials.upsert_from_patch_outcome(input(patch("2026-07-02T00:00:00Z"))).await.unwrap();
    assert!(matches!(changed, MaterialPatchOutcome::Changed(_)));
    let unchanged = materials.upsert_from_patch_outcome(input(patch("2026-07-02T00:00:00Z"))).await.unwrap();
    assert!(matches!(unchanged, MaterialPatchOutcome::Unchanged(_)));
    let older = materials.upsert_from_patch_outcome(input(patch("2026-07-01T00:00:00Z"))).await.unwrap();
    assert!(matches!(older, MaterialPatchOutcome::Older));
}
```

Add PostgreSQL parity coverage in `crates/pandar-hub/src/repositories/tests/postgres.rs` using the existing `PANDAR_TEST_POSTGRES_URL` gated harness:

```rust
#[tokio::test]
async fn postgres_material_patch_outcomes_match_sqlite_when_configured() {
    let Some(database) = postgres_database_if_configured().await else { return; };
    let (materials, tenant, agent, printer_id) = postgres_material_fixture(database).await;

    let changed = materials.upsert_from_patch_outcome(valid_material_input(tenant.id, agent.id, &printer_id, "2026-07-02T00:00:00Z")).await.unwrap();
    assert!(matches!(changed, MaterialPatchOutcome::Changed(_)));
    let unchanged = materials.upsert_from_patch_outcome(valid_material_input(tenant.id, agent.id, &printer_id, "2026-07-02T00:00:00Z")).await.unwrap();
    assert!(matches!(unchanged, MaterialPatchOutcome::Unchanged(_)));
    let older = materials.upsert_from_patch_outcome(valid_material_input(tenant.id, agent.id, &printer_id, "2026-07-01T00:00:00Z")).await.unwrap();
    assert!(matches!(older, MaterialPatchOutcome::Older));
}
```

Add gRPC/browser event tests:

```rust
#[tokio::test]
async fn printer_snapshot_event_includes_latest_materials() {
    let state = state().await;
    let (tenant_id, agent_id, printer_id) = fixture_printer_with_materials(&state).await;
    let mut receiver = state.printer_events().subscribe(tenant_id).await;

    handle_snapshot(&state, tenant_id, agent_id, snapshot("SERIAL", "Printer", "A1", "IDLE")).await.unwrap();

    let event = receiver.recv().await.unwrap();
    let PrinterEvent::PrinterSnapshot { printer } = event else { panic!("expected printer snapshot") };
    assert_eq!(printer.id, printer_id);
    assert_eq!(printer.materials.unwrap().ams_units[0]["unit_id"], "0");
}

#[tokio::test]
async fn malformed_material_snapshot_event_is_dropped_without_closing_stream() {
    let state = state().await;
    let (tenant_id, agent_id, printer_id) = fixture_printer(&state).await;
    let token = SessionToken::new();

    handle_event(&state, tenant_id, agent_id, token, material_event(
        tenant_id,
        agent_id,
        PrinterMaterialsSnapshot {
            serial: "SERIAL".to_owned(),
            printer_id: printer_id.clone(),
            printer_materials_json: "not json".to_owned(),
        },
    )).await.unwrap();
    assert!(state.materials().latest_for_printer(tenant_id, &printer_id).await.unwrap().is_none());

    handle_event(&state, tenant_id, agent_id, token, snapshot_event(
        tenant_id,
        agent_id,
        snapshot("SERIAL", "Printer", "A1", "IDLE"),
    )).await.unwrap();
}

#[tokio::test]
async fn printer_materials_snapshot_upserts_and_publishes_sanitized_materials() {
    let state = state().await;
    let (tenant_id, agent_id, printer_id) = fixture_printer(&state).await;
    let mut receiver = state.printer_events().subscribe(tenant_id).await;

    handle_materials_snapshot(&state, tenant_id, agent_id, PrinterMaterialsSnapshot {
        serial: "serial".to_owned(),
        printer_id: printer_id.clone(),
        printer_materials_json: json!({
            "type": "printer_material_patch",
            "observed_at": "2026-07-02T00:00:00Z",
            "ams_units": [{"unit_id": "0", "trays": [{"tray_id": "0", "type": "PLA", "access_token": "secret"}]}],
            "external_spools": []
        }).to_string(),
    }).await.unwrap();

    let event = receiver.recv().await.unwrap();
    let PrinterEvent::PrinterSnapshot { printer } = event else { panic!("expected printer snapshot") };
    assert_eq!(printer.materials.as_ref().unwrap().observed_at, "2026-07-02T00:00:00Z");
    assert!(!serde_json::to_string(&printer).unwrap().contains("access_token"));
    assert!(!serde_json::to_string(&printer).unwrap().contains("secret"));
}

#[tokio::test]
async fn printer_materials_snapshot_without_printer_id_resolves_by_agent_and_serial() {
    let state = state().await;
    let (tenant_id, agent_id, printer_id) = fixture_printer(&state).await;
    let mut receiver = state.printer_events().subscribe(tenant_id).await;

    handle_materials_snapshot(&state, tenant_id, agent_id, PrinterMaterialsSnapshot {
        serial: "serial".to_owned(),
        printer_id: String::new(),
        printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z"),
    }).await.unwrap();

    let snapshot = state.materials().latest_for_printer(tenant_id, &printer_id).await.unwrap().unwrap();
    assert_eq!(snapshot.serial_number, "serial");
    let event = receiver.recv().await.unwrap();
    let PrinterEvent::PrinterSnapshot { printer } = event else { panic!("expected printer snapshot") };
    assert_eq!(printer.id, printer_id);
    assert!(printer.materials.is_some());
}

#[tokio::test]
async fn printer_materials_snapshot_with_mismatched_printer_id_and_serial_is_dropped() {
    let state = state().await;
    let (tenant_id, agent_id, printer_id) = fixture_printer(&state).await;

    handle_materials_snapshot(&state, tenant_id, agent_id, PrinterMaterialsSnapshot {
        serial: "other-serial".to_owned(),
        printer_id: printer_id.clone(),
        printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z"),
    }).await.unwrap();

    assert!(state.materials().latest_for_printer(tenant_id, &printer_id).await.unwrap().is_none());
}

#[tokio::test]
async fn printer_materials_snapshot_with_printer_owned_by_other_agent_or_tenant_is_dropped() {
    let state = state().await;
    let (tenant_id, agent_id, _printer_id) = fixture_printer(&state).await;
    let (other_tenant_id, other_agent_id, other_printer_id) = fixture_printer_for_other_tenant_and_agent(&state).await;
    let mut receiver = state.printer_events().subscribe(tenant_id).await;

    handle_materials_snapshot(&state, tenant_id, agent_id, PrinterMaterialsSnapshot {
        serial: "serial".to_owned(),
        printer_id: other_printer_id.clone(),
        printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z"),
    }).await.unwrap();

    assert!(state.materials().latest_for_printer(tenant_id, &other_printer_id).await.unwrap().is_none());
    assert!(state.materials().latest_for_printer(other_tenant_id, &other_printer_id).await.unwrap().is_none());
    assert!(receiver.try_recv().is_err());

    handle_materials_snapshot(&state, other_tenant_id, other_agent_id, PrinterMaterialsSnapshot {
        serial: "serial".to_owned(),
        printer_id: other_printer_id.clone(),
        printer_materials_json: valid_material_patch("2026-07-02T00:01:00Z"),
    }).await.unwrap();
    assert!(state.materials().latest_for_printer(other_tenant_id, &other_printer_id).await.unwrap().is_some());
}

#[tokio::test]
async fn printer_materials_snapshot_event_local_failures_are_dropped_without_stream_close() {
    let state = state().await;
    let (tenant_id, agent_id, printer_id) = fixture_printer(&state).await;
    let mut receiver = state.printer_events().subscribe(tenant_id).await;

    for event in [
        PrinterMaterialsSnapshot { serial: String::new(), printer_id: printer_id.clone(), printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z") },
        PrinterMaterialsSnapshot { serial: "serial".to_owned(), printer_id: printer_id.clone(), printer_materials_json: String::new() },
        PrinterMaterialsSnapshot { serial: "serial".to_owned(), printer_id: "not-a-uuid".to_owned(), printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z") },
        PrinterMaterialsSnapshot { serial: "serial".to_owned(), printer_id: uuid::Uuid::new_v4().to_string(), printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z") },
        PrinterMaterialsSnapshot { serial: "serial".to_owned(), printer_id: printer_id.clone(), printer_materials_json: valid_material_patch("not-rfc3339") },
    ] {
        handle_materials_snapshot(&state, tenant_id, agent_id, event).await.unwrap();
    }

    assert!(state.materials().latest_for_printer(tenant_id, &printer_id).await.unwrap().is_none());
    assert!(receiver.try_recv().is_err());

    handle_materials_snapshot(&state, tenant_id, agent_id, PrinterMaterialsSnapshot {
        serial: "serial".to_owned(),
        printer_id: printer_id.clone(),
        printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z"),
    }).await.unwrap();
    assert!(receiver.recv().await.is_ok());
}

#[tokio::test]
async fn older_and_unchanged_material_events_do_not_publish_noop_printer_events() {
    let state = state().await;
    let (tenant_id, agent_id, printer_id) = fixture_printer(&state).await;
    handle_materials_snapshot(&state, tenant_id, agent_id, PrinterMaterialsSnapshot {
        serial: "serial".to_owned(),
        printer_id: printer_id.clone(),
        printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z"),
    }).await.unwrap();

    let mut receiver = state.printer_events().subscribe(tenant_id).await;
    for observed_at in ["2026-07-01T00:00:00Z", "2026-07-02T00:00:00Z"] {
        handle_materials_snapshot(&state, tenant_id, agent_id, PrinterMaterialsSnapshot {
            serial: "serial".to_owned(),
            printer_id: printer_id.clone(),
            printer_materials_json: valid_material_patch(observed_at),
        }).await.unwrap();
    }

    assert!(receiver.try_recv().is_err());
}

#[tokio::test]
async fn material_only_print_report_publishes_material_aware_printer_event() {
    let state = state().await;
    let (tenant_id, agent_id, printer_id) = fixture_printer(&state).await;
    let mut receiver = state.printer_events().subscribe(tenant_id).await;

    handle_print_report(&state, tenant_id, agent_id, PrintJobReport {
        serial: "serial".to_owned(),
        observed_at: "2026-07-02T00:00:00Z".to_owned(),
        printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z"),
        ..report("serial".to_owned(), String::new(), String::new())
    }).await.unwrap();

    let event = receiver.recv().await.unwrap();
    let PrinterEvent::PrinterSnapshot { printer } = event else { panic!("expected printer snapshot") };
    assert_eq!(printer.id, printer_id);
    assert!(printer.materials.is_some());
}
```

- [ ] **Step 2: Run focused tests to verify RED**

Run:

```bash
cargo test -p pandar-hub repositories::tests::materials::material_repository_reports_changed_unchanged_empty_invalid_and_older_outcomes
cargo test -p pandar-hub repositories::tests::postgres::postgres_material_patch_outcomes_match_sqlite_when_configured
cargo test -p pandar-hub grpc::tests::printer_snapshots::printer_snapshot_event_includes_latest_materials
cargo test -p pandar-hub grpc::tests::printer_materials::malformed_material_snapshot_event_is_dropped_without_closing_stream
cargo test -p pandar-hub grpc::tests::printer_materials::printer_materials_snapshot_upserts_and_publishes_sanitized_materials
cargo test -p pandar-hub grpc::tests::printer_materials::printer_materials_snapshot_without_printer_id_resolves_by_agent_and_serial
cargo test -p pandar-hub grpc::tests::printer_materials::printer_materials_snapshot_with_mismatched_printer_id_and_serial_is_dropped
cargo test -p pandar-hub grpc::tests::printer_materials::printer_materials_snapshot_with_printer_owned_by_other_agent_or_tenant_is_dropped
cargo test -p pandar-hub grpc::tests::printer_materials::printer_materials_snapshot_event_local_failures_are_dropped_without_stream_close
cargo test -p pandar-hub grpc::tests::printer_materials::older_and_unchanged_material_events_do_not_publish_noop_printer_events
cargo test -p pandar-hub grpc::tests::print_reports::material_only_print_report_publishes_material_aware_printer_event
```

Expected: tests fail because outcome enum and material-aware event payloads are missing.

- [ ] **Step 3: Implement material outcomes and event DTO construction**

Implement the outcome enum in `repositories/materials.rs`. Keep the existing `upsert_from_patch` API as a compatibility helper if many callers use it, but route new behavior through an outcome-returning method:

```rust
pub enum MaterialPatchOutcome {
    Empty,
    Invalid { error: String },
    Older,
    Unchanged(MaterialSnapshot),
    Changed(MaterialSnapshot),
}

pub async fn upsert_from_patch_outcome(
    &self,
    input: MaterialPatchInput,
) -> RepositoryResult<MaterialPatchOutcome>;
```

Compare the merged payload and `observed_at` against the current snapshot to return `Unchanged` without a write when nothing changes. Return `Invalid` for parse errors instead of panicking or ending the gRPC stream; log the formatted cause chain at the caller boundary.

Move the printer response/event DTO conversion into a helper usable by both routes and event publishers, or make `PrinterResponse` visible inside the crate. The event payload must serialize `materials` exactly like the HTTP printer response after `scrub_material_json`.

Update `handle_snapshot`, `handle_print_report`, and the new material snapshot handler so material changes publish a material-aware `printer_snapshot` event. Do not publish no-op `Empty`, `Older`, or `Unchanged` material events.

- [ ] **Step 4: Verify GREEN for Hub material/event behavior**

Run:

```bash
cargo test -p pandar-hub repositories::tests::materials
cargo test -p pandar-hub grpc::tests::printer_snapshots
cargo test -p pandar-hub grpc::tests::print_reports
cargo test -p pandar-hub routes::tests::printer_events_ws
```

Expected: all selected Hub material/event tests pass.

---

### Task 3: Hub Material Refresh Command And Route

**Files:**
- Modify: `crates/pandar-hub/src/repositories/commands.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/audit.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/enqueue.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/ownership.rs`
- Modify: `crates/pandar-hub/src/grpc/commands.rs`
- Modify: `crates/pandar-hub/src/routes/printers.rs`
- Modify: `crates/pandar-hub/src/routes.rs`
- Test: `crates/pandar-hub/src/routes/tests/printers.rs`
- Test: `crates/pandar-hub/src/grpc/tests/commands.rs`
- Test: `crates/pandar-hub/src/repositories/tests/commands.rs`
- Test: `crates/pandar-hub/src/repositories/tests/postgres_commands.rs`

**Interfaces:**
- Produces `RefreshPrinterMaterialsPayload { printer_id: String, serial_number: String }`.
- Produces repository enqueue method `enqueue_refresh_printer_materials_with_audit(tenant_id, printer_id, actor)` that resolves the owning agent.
- Produces route `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/materials:refresh` requiring `Operator`.
- Produces gRPC conversion for command kind `refresh_printer_materials`.

- [ ] **Step 1: Write failing route/repository/conversion tests**

Add route test:

```rust
#[tokio::test]
async fn refresh_printer_materials_enqueues_for_owning_agent_and_wakes_it() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id).await.unwrap();

    let (wake_sender, mut wake_receiver) = mpsc::channel(1);
    let (close_sender, _close_receiver) = mpsc::channel(1);
    let (command_sender, _command_receiver) = mpsc::channel(1);
    state.sessions().register(AgentSession {
        token: SessionToken::new(),
        tenant_id,
        agent_id,
        name: "agent".to_owned(),
        version: "test".to_owned(),
        connected_at: "2026-07-02T00:00:00Z".to_owned(),
        last_heartbeat_at: "2026-07-02T00:00:00Z".to_owned(),
        wake_sender,
        close_sender,
        command_sender,
        pending_live_commands: empty_pending_live_commands(),
    }).await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/materials:refresh"),
        None,
        &token,
    ).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["kind"], "refresh_printer_materials");
    assert_eq!(body["agent_id"], agent_id.to_string());
    assert_eq!(body["printer_id"], printer_id);
    let payload: serde_json::Value = serde_json::from_str(body["payload_json"].as_str().unwrap()).unwrap();
    assert_eq!(payload["printer_id"], printer_id);
    assert_eq!(payload["serial_number"], "serial");
    assert!(wake_receiver.try_recv().is_ok());

    let audit = state.audit_events().list_for_tenant(tenant_id).await.unwrap();
    assert!(audit.iter().any(|event| event.action == "printer.refresh_materials"));
}

#[tokio::test]
async fn refresh_printer_materials_rejects_invalid_and_missing_printers() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, _agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/not-a-uuid/materials:refresh"),
        None,
        &token,
    ).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_printer_id");

    let missing = uuid::Uuid::new_v4();
    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{missing}/materials:refresh"),
        None,
        &token,
    ).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "printer_not_found");
}
```


Add conversion test:

```rust
#[test]
fn converts_refresh_printer_materials_command_to_proto() {
    let command = command_record("refresh_printer_materials", r#"{"printer_id":"printer-1","serial_number":"SERIAL"}"#);

    let proto = hub_command_from_record(command).unwrap();

    match proto.command.unwrap() {
        hub_command::Command::RefreshPrinterMaterials(command) => {
            assert_eq!(command.printer_id, "printer-1");
            assert_eq!(command.serial_number, "SERIAL");
        }
        other => panic!("expected refresh materials command, got {other:?}"),
    }
}

#[tokio::test]
async fn postgres_refresh_printer_materials_command_matches_sqlite_when_configured() {
    let Some(database) = postgres_database_if_configured().await else { return; };
    let (tenant, agent, printer_id) = postgres_tenant_agent_printer_fixture(database.clone()).await;
    let commands = CommandRepository::new(database);

    let command = commands
        .enqueue_refresh_printer_materials_with_audit(tenant.id, &printer_id, AuditActor::system())
        .await
        .unwrap();

    assert_eq!(command.kind, "refresh_printer_materials");
    assert_eq!(command.agent_id, agent.id);
    assert_eq!(command.printer_id.as_deref(), Some(printer_id.as_str()));
    let payload: serde_json::Value = serde_json::from_str(&command.payload_json).unwrap();
    assert_eq!(payload["printer_id"], printer_id);
    assert_eq!(payload["serial_number"], "serial");
}

#[test]
fn refresh_printer_materials_requires_no_new_migration_files() {
    let sqlite_phase_14 = include_str!("../../../migrations/sqlite/20260623010000_phase_14_materials.sql");
    let postgres_phase_14 = include_str!("../../../migrations/postgres/20260623010000_phase_14_materials.sql");

    assert!(sqlite_phase_14.contains("printer_material_snapshots"));
    assert!(postgres_phase_14.contains("printer_material_snapshots"));
}
```

- [ ] **Step 2: Run focused tests to verify RED**

Run:

```bash
cargo test -p pandar-hub routes::tests::printers::refresh_printer_materials_enqueues_for_owning_agent_and_wakes_it
cargo test -p pandar-hub routes::tests::printers::refresh_printer_materials_rejects_invalid_and_missing_printers
cargo test -p pandar-hub grpc::tests::commands::converts_refresh_printer_materials_command_to_proto
cargo test -p pandar-hub repositories::tests::postgres_commands::postgres_refresh_printer_materials_command_matches_sqlite_when_configured
cargo test -p pandar-hub repositories::tests::phase1::refresh_printer_materials_requires_no_new_migration_files
```

Expected: tests fail because route, payload, and conversion do not exist.

- [ ] **Step 3: Implement Hub command, audit, conversion, and route**

Add the payload struct and repository method. Reuse existing ownership helpers to load the printer and validate tenant ownership. Set `printer_id` on the command row. Add audit action `printer.refresh_materials` or `agent.refresh_printer_materials` with metadata containing `printer_id` and `serial_number` but no secrets.

Add route wiring:

```rust
.route(
    "/api/v1/tenants/{tenant_id}/printers/{printer_id}/materials:refresh",
    post(printers::refresh_printer_materials),
)
```

The route authorizes `Operator`, parses `printer_id`, enqueues for the owning agent, wakes that agent, and returns `CommandResponse`.

- [ ] **Step 4: Verify GREEN for Hub command route**

Run:

```bash
cargo test -p pandar-hub routes::tests::printers
cargo test -p pandar-hub grpc::tests::commands
cargo test -p pandar-hub repositories::tests::commands
```

Expected: all selected command/route tests pass.

---

### Task 4: Frontend Inventory Action And Translations

**Files:**
- Modify: `frontend/app/actions.ts`
- Modify: `frontend/app/action-status.ts`
- Modify: `frontend/app/dashboard-inventory.tsx`
- Modify: `frontend/app/action-status-toast.test.tsx`
- Modify: `frontend/app/actions.test.ts`
- Add: `frontend/app/dashboard-inventory.test.tsx`
- Modify: `frontend/messages/en.json`
- Modify: `frontend/messages/zh.json`

**Interfaces:**
- Produces server action `refreshPrinterMaterials(formData: FormData)`.
- Produces inventory copy keys `inventory.refreshAms` and runtime status `materials_refresh_queued`.
- Produces per-row form fields `tenant_id` and `printer_id` posting to the new server action.

- [ ] **Step 1: Write failing frontend tests**

Update `frontend/app/actions.test.ts` with a server action test:

```ts
it("posts refresh printer materials to the API and redirects to devices", async () => {
  const form = new FormData();
  form.set("tenant_id", "tenant-1");
  form.set("printer_id", "printer-1");

  fetchMock.mockResolvedValueOnce(jsonResponse({ id: "command-1" }));

  await expect(refreshPrinterMaterials(form)).rejects.toMatchObject({ digest: expect.stringContaining("NEXT_REDIRECT") });

  expect(fetchMock).toHaveBeenCalledWith(
    "http://localhost:8080/api/v1/tenants/tenant-1/printers/printer-1/materials:refresh",
    expect.objectContaining({ method: "POST" }),
  );
});
```

Add or update a render test so a printer row contains the localized `Refresh AMS` button and hidden tenant/printer ids. Update `ActionStatusToast` tests to expect `materials_refresh_queued` copy.

Update `frontend/app/action-status-toast.test.tsx` or the existing action-status unit assertions:

```ts
it("treats queued material refresh as a success status", () => {
  expect(actionStatusTone("materials_refresh_queued")).toBe("success");
});
```

Add a translation assertion for Chinese spool copy:

```ts
expect(zh.material.externalSpool).toContain("料盘");
expect(zh.material.externalSpool).not.toContain("盘子");
```

- [ ] **Step 2: Run frontend tests to verify RED**

Run:

```bash
npm --prefix frontend test -- actions.test.ts action-status-toast.test.tsx dashboard-inventory.test.tsx
```

Expected: tests fail because the action, copy, or UI button is missing.

- [ ] **Step 3: Implement frontend action and row button**

Add server action:

```ts
export async function refreshPrinterMaterials(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const printerId = stringField(formData, "printer_id");
  const response = await postJson(
    `/api/v1/tenants/${tenantId}/printers/${printerId}/materials:refresh`,
    {},
  );
  redirect(statusUrl(tenantId, response.ok ? "materials_refresh_queued" : await errorCode(response)));
}
```

In `PrinterInventory`, import `refreshPrinterMaterials` and render a compact form near material details:

```tsx
<form action={refreshPrinterMaterials} className="mt-2">
  <input type="hidden" name="tenant_id" value={printer.tenant_id} />
  <input type="hidden" name="printer_id" value={printer.id} />
  <button className="rounded-md border border-slate-300 px-2 py-1 text-xs font-medium text-slate-700 hover:bg-slate-100" type="submit">
    {t('refreshAms')}
  </button>
</form>
```

Use existing component/button styling if the surrounding file already has a local pattern. Keep the row grid stable on mobile and desktop.

Update messages:

```json
"runtime": { "actionStatus": { "materials_refresh_queued": "AMS refresh queued" } },
"inventory": { "refreshAms": "Refresh AMS" }
```

Chinese:

```json
"runtime": { "actionStatus": { "materials_refresh_queued": "AMS 刷新已入队" } },
"inventory": { "refreshAms": "刷新 AMS" }
```

Confirm spool-related Chinese copy uses `料盘`.

Add `materials_refresh_queued` to `knownPositiveActionStatuses` in `frontend/app/action-status.ts` so queued material refresh does not render with the error tone.

- [ ] **Step 4: Verify GREEN for frontend**

Run:

```bash
npm --prefix frontend test -- actions.test.ts action-status-toast.test.tsx dashboard-inventory.test.tsx
npm --prefix frontend test -- --run
```

Expected: selected tests and the full frontend Vitest suite pass.

---

### Task 5: Documentation, Formatting, And Focused Integration Checks

**Files:**
- Modify: `docs/roadmap.md`
- Modify: `docs/development.md`
- Review only: current diff across `proto/`, `crates/`, `frontend/`, `docs/`

**Interfaces:**
- Produces roadmap entry describing gRPC-backed Agent AMS refresh, `refresh_printers` material refresh, per-printer UI action, and material-aware live updates.
- Produces development docs for the new per-printer material refresh HTTP route.

- [ ] **Step 1: Update docs**

Add a concise completed item near the top of `docs/roadmap.md`:

```markdown
- Added Agent-backed AMS refresh: printer refresh now opportunistically refreshes AMS/external-spool snapshots from Bambu MQTT `pushall`, operators can queue per-printer AMS refreshes from the printer inventory, Agent material-only updates sync to Hub over gRPC, and Hub publishes material-aware printer updates to the browser event stream.
```

If `docs/development.md` still lists printer command routes, add:

```markdown
- `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/materials:refresh` queues an Agent command to refresh the printer's current AMS/external-spool material snapshot. The Agent synchronizes material patches to Hub over gRPC; browser live updates continue through the tenant printer event stream.
```

- [ ] **Step 2: Run formatting and focused validation**

Run:

```bash
cargo fmt
cargo test -p pandar-agent machine::mqtt::tests
cargo test -p pandar-agent machine::materials::tests
cargo test -p pandar-agent commands::tests
cargo test -p pandar-hub routes::tests::printers
cargo test -p pandar-hub grpc::tests
cargo test -p pandar-hub repositories::tests::materials
npm --prefix frontend test -- --run
```

Expected: all commands exit 0.

- [ ] **Step 3: Run repo-required validation**

Run:

```bash
cargo clippy --workspace --all-targets --all-features
cargo nextest run --manifest-path Cargo.toml --workspace
```

Expected: both commands exit 0. If environment dependencies prevent completion, capture the exact command, exit code, and error, then keep the focused validation output as supporting evidence.

- [ ] **Step 4: Optional live smoke after local verification**

Use the out-of-band machine credentials from the user, not committed files. Check that the deployed agent can reach the printer and that a refresh queues/updates materials without logging the printer token. Do not write the token to docs, fixtures, or command output artifacts.

- [ ] **Step 5: Review final diff before SDD implementation review**

Run:

```bash
git status --short
git diff --stat
```

Expected: only intended spec, plan, Rust, frontend, proto, and docs files are changed.

---

## Self-Review

- Spec coverage: Tasks cover translation, Agent MQTT/gRPC material refresh, refresh-printers material updates, per-printer Hub route, material-aware browser events, frontend action, docs, and validation.
- Red-flag scan: no incomplete marker or unspecified task remains.
- Type consistency: `PrinterMaterialsSnapshot`, `RefreshPrinterMaterials`, `PrinterRefreshResult`, `MaterialRefreshResult`, and `MaterialPatchOutcome` names are consistent across tasks.
