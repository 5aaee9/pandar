# Agent AMS Refresh Design

## Scope

Add an operator-visible AMS material refresh flow that uses the existing local agent and Bambu MQTT connection to populate the current Hub material snapshot for each printer.

This change covers:

- Chinese copy fixes where spool-related UI text must use `料盘`, never `盘子`.
- Agent-side refresh of AMS/external-spool material state during both the existing `refresh_printers` command and a new per-printer material refresh command.
- Agent-side MQTT report forwarding so spontaneous printer `report` broadcasts containing AMS data are normalized and sent to Hub through the existing reverse gRPC stream.
- Hub-side gRPC handling, ownership checks, material snapshot persistence, and tenant printer events for material-only changes.
- Frontend printer inventory display and a per-printer `Refresh AMS` action.
- Focused tests and `docs/roadmap.md` update.

This change does not cover:

- Spoolman or any external spool inventory integration.
- New database tables or migrations.
- Filament load/unload, tray select, RFID refresh, drying, or calibration commands.
- Replacing the browser WebSocket event stream. Agent-to-Hub synchronization must use gRPC; Hub-to-browser may continue using the existing printer event WebSocket.
- Persisting raw MQTT report JSON.

## Existing Context

`pandar-agent` already uses Bambu LAN MQTT on `device/{serial}/report` and `device/{serial}/request`. The `RefreshPrinters` path publishes `pushing.pushall`, reads one report, and returns `MachineSnapshot`. The report forwarding path subscribes to the printer report topic and emits `PrintJobReport` events over the reverse gRPC stream. Agent material normalization already converts `print.ams` data into a normalized `printer_material_patch` JSON payload.

`pandar-hub` already stores latest material state in `printer_material_snapshots`, merges patches by AMS unit/tray and external spool identity, removes credential-looking keys from HTTP material responses, and includes `materials` in tenant printer list/detail responses. Current material persistence is reached through print report handling, but material-only updates are not represented as a first-class event and do not reliably publish a printer update to browsers.

The frontend printer inventory already shows a compact material summary from `printer.materials`. Browser live updates consume the Hub printer event WebSocket and merge `printer_snapshot` events into local state.

## Product Behavior

The printer inventory row must continue to show the compact material summary and observation time. Operators can refresh AMS material state for an individual printer from that row. The action posts to Hub, queues a command for that printer's owning agent, and returns the normal action status feedback.

Existing agent-level `refresh_printers` must also refresh AMS material state for every refreshed printer. Operators who already use refresh do not need to run a separate AMS action to populate material state.

When the agent is connected and listening to MQTT reports, unsolicited printer reports that include AMS material data must update Hub through the reverse gRPC stream. Once Hub accepts a newer material patch, connected browsers for that tenant receive a printer update and the material summary changes without a manual page reload.

Chinese spool-related UI text must use `料盘`. `spool` must not be translated as `盘子`. Existing non-spool `plate` labels may be left unchanged unless they are in spool-related copy.

## Protocol Design

Extend `proto/pandar/agent/v1/agent.proto` with first-class material refresh and material snapshot messages:

```proto
message AgentEvent {
  ...
  oneof event {
    ...
    PrinterMaterialsSnapshot printer_materials_snapshot = 16;
  }
}

message PrinterMaterialsSnapshot {
  string serial = 1;
  string printer_id = 2;
  string printer_materials_json = 3;
}

message HubCommand {
  ...
  oneof command {
    ...
    RefreshPrinterMaterials refresh_printer_materials = 16;
  }
}

message RefreshPrinterMaterials {
  string printer_id = 1;
  string serial_number = 2;
}
```

`printer_materials_json` carries the existing normalized `printer_material_patch` JSON, not raw MQTT. The embedded patch `observed_at` is the only authoritative material observation timestamp. Hub persists it, compares it for stale-write protection, and exposes it in printer material HTTP responses. `PrinterMaterialsSnapshot` must not carry a separate outer timestamp. Material snapshot events must never carry empty material JSON; when no AMS patch is available, the agent omits the event. Invalid material JSON is rejected at the Hub material boundary with a full cause chain in logs and must not terminate the agent stream.

The existing `PrintJobReport.printer_materials_json` remains supported because print progress reports can still carry material state. The new `PrinterMaterialsSnapshot` is used for material-only updates from refresh commands and unsolicited MQTT reports.

## Agent Design

Add a gateway capability for refreshing one printer's material state by serial. For Bambu printers it should:

1. Find the configured or runtime-linked endpoint matching the requested serial number.
2. Subscribe to the printer report topic.
3. Publish the existing `pushing.pushall` request to the printer request topic.
4. Read reports until a report produces a normalized material patch or the existing report timeout expires.
5. Return the normalized patch JSON and observed timestamp.

The material-refresh scan uses one total deadline equal to the existing report timeout. Receiving unrelated or non-AMS reports must not reset the total deadline. Implement this by wrapping the whole scan loop in one timeout/deadline; each report read is bounded by the remaining time. If the deadline expires before a material patch appears, per-printer material refresh fails and `refresh_printers` emits only the printer snapshot as described below.

Introduce explicit agent-domain result types so the refresh command path does not hide material data inside `MachineSnapshot`:

```rust
pub struct PrinterRefreshResult {
    pub snapshot: MachineSnapshot,
    pub materials: Option<MaterialRefreshResult>,
}

pub struct MaterialRefreshResult {
    pub serial: String,
    pub printer_id: Option<String>,
    pub printer_materials_json: String,
}
```

`BambuMachineGateway::refresh_printers` should return `Vec<PrinterRefreshResult>` after this change. Existing snapshot-only callers use `result.snapshot`; command dispatch emits `PrinterSnapshot` first, then `PrinterMaterialsSnapshot` when `result.materials` is `Some`. `BambuMachineGateway::refresh_printer_materials(serial_number, printer_id)` should return `MaterialRefreshResult` for the single-printer command. `NoopMachineGateway` returns an empty refresh list and a clear missing-printer error for single-printer material refresh.

The existing `refresh_printers` command should continue emitting `PrinterSnapshot` events for every refreshed printer. In addition, each refreshed printer report that contains AMS data should emit `PrinterMaterialsSnapshot` with that printer's serial, known printer id when available, and normalized patch JSON. If a printer snapshot refresh succeeds but no AMS material patch appears before the existing report timeout, the `refresh_printers` command still succeeds for that printer, emits only `PrinterSnapshot`, preserves any previous material snapshot, and does not emit a stale/no-material event. This keeps the existing printer-inventory refresh resilient while opportunistically updating AMS state whenever the printer reports it. If the printer snapshot refresh itself fails, the existing command failure behavior still applies.

For the new per-printer material refresh command, the agent must validate that the requested serial is configured or runtime-linked, send command ack, run the same material refresh path, emit `PrinterMaterialsSnapshot` when a patch is available, then send command success. If no AMS material patch is received before the existing report timeout, the command fails with a redacted error such as `no AMS material report received before timeout` and emits no `PrinterMaterialsSnapshot`. Failure must redact known access codes and send command failure without crashing the reverse stream.

The report forwarder must continue listening to MQTT broadcast reports after connection. For every report whose normalized material patch is non-empty, it must emit `PrinterMaterialsSnapshot` over gRPC. If a report also maps to print progress, the existing `PrintJobReport` emission continues unchanged. This creates a material-specific gRPC path without relying on print-job semantics.

## Hub Design

Add a repository command payload for `refresh_printer_materials` containing `printer_id` and `serial_number`. Enqueueing the command must verify the printer belongs to the tenant and use the printer's owning `agent_id`; operators do not choose the agent manually for this route.

Add a tenant-scoped HTTP route:

```text
POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/materials:refresh
```

Authorization requires `Operator`. Success returns the existing `CommandResponse` shape. The route wakes the owning agent. Invalid tenant/printer ids return the existing bad-request errors, and missing printers return `printer_not_found`.

The gRPC command converter must map queued `refresh_printer_materials` rows to `HubCommand::RefreshPrinterMaterials`. Command ack/result handling uses the existing command lifecycle.

Hub/browser printer events must become material-aware. Replace or wrap the current `PrinterEvent::PrinterSnapshot { printer: pandar_core::Printer }` serialization so the WebSocket event carries the same printer DTO shape as tenant printer HTTP responses, including sanitized `materials: PrinterMaterialsResponse | null`. Existing printer snapshot events continue to publish after normal snapshot upserts, but the publisher must load the latest material snapshot and include it in the event. Material-only updates publish the same `printer_snapshot` event shape after the material repository returns `Changed`.

Add a Hub gRPC handler for `PrinterMaterialsSnapshot`:

- Validate tenant and agent identity from the authenticated gRPC stream.
- Require non-empty `serial` and non-empty `printer_materials_json`.
- Resolve the printer by `printer_id` when supplied, otherwise by tenant/agent/serial.
- If `printer_id` is supplied, require the resolved printer to belong to the authenticated tenant and agent and require `event.serial` to match `printer.serial_number`. A mismatch is logged and dropped, and must not write a material snapshot.
- Persist the canonical serial from the resolved printer, not the untrusted event serial, when `printer_id` is supplied.
- Upsert through the existing material repository.
- If the material patch is accepted and changed the current snapshot, publish a tenant `printer_snapshot` event containing the current printer response with the refreshed `materials` field.

Stream safety for material events is explicit: event-local material validation failures are logged and dropped with `Ok(())` so one malformed MQTT-derived material event does not close the reverse gRPC stream. This includes blank serial or material fields, malformed `printer_id`, unknown printer, `printer_id`/`serial` mismatch, invalid material JSON, invalid embedded patch timestamp, and older/no-op material outcomes. The only fatal stream errors remain the existing session-level protocol/authentication failures such as an outer `AgentEvent.tenant_id` or `agent_id` mismatch with the authenticated stream.

Define an explicit material patch outcome contract for Hub repository calls:

```rust
pub enum MaterialPatchOutcome {
    Empty,
    Invalid { error: String },
    Older,
    Unchanged(MaterialSnapshot),
    Changed(MaterialSnapshot),
}
```

`Empty` means the input has no material patch and no write occurs. `Invalid` means the material JSON/root/type/timestamp is malformed; handlers log the full lower-level cause chain at warn level and continue processing the rest of the agent event where possible. `Older` means the patch timestamp is older than the persisted snapshot and no write occurs. `Unchanged` means the patch is accepted but produces the same persisted material payload and observed timestamp. `Changed` means an insert or update changed the material snapshot. Printer events are published only for `Changed`. Existing callers that only need the snapshot can keep a helper that maps `Changed` and `Unchanged` to `Some(snapshot)`.

The existing print report path should also publish a tenant printer event when a material patch changes the current snapshot even if no job changed. This keeps unsolicited MQTT reports visible in the UI without broadcasting duplicate no-op updates.

## Documentation Impact

Update `docs/roadmap.md` after implementation with the completed Agent AMS refresh behavior. Update `docs/development.md` only if the new HTTP route needs operator/developer-facing API documentation alongside existing printer/job command routes. Do not add deployment documentation because the feature introduces no new environment variables, ports, or storage backends.

## Frontend Design

Add a server action `refreshPrinterMaterials(formData)` that posts to `/api/v1/tenants/{tenant_id}/printers/{printer_id}/materials:refresh` and redirects back to `/devices` with `status=materials_refresh_queued` or the returned error code.

In `PrinterInventory`, add a compact per-row form near the material summary. The button label should be `Refresh AMS` in English and `刷新 AMS` in Chinese. It must use the existing button/control vocabulary, remain keyboard-accessible, and not disrupt the row grid on mobile or desktop.

Add runtime/action-status copy for `materials_refresh_queued`. Keep the material summary concise; do not add a modal or separate page.

Frontend event handling can keep treating material updates as `printer_snapshot` events if Hub publishes a printer object whose `materials` field is current. A new browser event type is unnecessary for this scope.

## Testing

Agent tests must verify:

- MQTT material refresh publishes `pushing.pushall`, waits for a report with `print.ams`, and returns normalized material patch JSON.
- MQTT material refresh uses one total deadline and fails after repeated non-AMS reports instead of resetting the timeout forever.
- `refresh_printers` emits both `PrinterSnapshot` and `PrinterMaterialsSnapshot` when the refresh report includes AMS data.
- `refresh_printers` succeeds and emits only `PrinterSnapshot` when the refreshed report has printer state but no AMS material patch.
- `RefreshPrinterMaterials` ack/success emits `PrinterMaterialsSnapshot` for the requested serial when AMS material data is received.
- Missing configured serial and per-printer material MQTT timeout produce failed command results with redacted access codes.
- The report forwarder emits `PrinterMaterialsSnapshot` for unsolicited AMS reports.

Hub tests must verify:

- The new HTTP route enqueues `refresh_printer_materials` for the printer's owning agent, sets `printer_id`, audits the action, and wakes the agent.
- Invalid/missing printer cases return the expected API errors.
- gRPC conversion produces `RefreshPrinterMaterials` with `printer_id` and `serial_number`.
- `PrinterMaterialsSnapshot` upserts material state and publishes a `printer_snapshot` event containing sanitized materials.
- Existing `PrinterSnapshot` handling publishes a `printer_snapshot` event whose printer payload includes the latest sanitized `materials` field.
- `PrinterMaterialsSnapshot` has no outer timestamp; Hub persists and compares the embedded patch `observed_at` only.
- `PrinterMaterialsSnapshot` with mismatched `printer_id` and `serial` is logged/dropped without writing a snapshot or closing the stream.
- Material-only print reports publish a printer event when material state changes, even when no job changes.
- SQLite and PostgreSQL behavior remain backend-neutral; no migration is required.

Frontend tests must verify:

- The printer inventory row renders the `Refresh AMS` action when a tenant/printer exists.
- The server action posts to the new materials refresh route and redirects with `materials_refresh_queued` on success.
- Chinese material/spool copy uses `料盘` for spool-related strings and does not translate spool as `盘子`.

## Validation

Run targeted tests first:

- `cargo test -p pandar-agent machine::mqtt::tests`
- `cargo test -p pandar-agent machine::materials::tests`
- `cargo test -p pandar-agent commands::tests`
- `cargo test -p pandar-hub routes::tests::printers`
- `cargo test -p pandar-hub grpc::tests`
- `cargo test -p pandar-hub repositories::tests::materials`
- `npm --prefix frontend test -- --run`

Before completion, run the repo-required checks where feasible:

- `cargo fmt`
- `cargo clippy --workspace --all-targets --all-features`
- `cargo nextest run --manifest-path Cargo.toml --workspace`

Live validation can use the provided out-of-band machine and printer credentials after code-level tests pass. Do not write the live printer token into docs, logs, test fixtures, command results, or committed artifacts.

## Rollback And Safety

The change is code-only and schema-free. Rollback removes the new proto fields, route, command kind, frontend action, and material event handler while leaving existing material snapshots intact.

Do not store printer access tokens in Hub command payloads or frontend state. Preserve full lower-level error cause chains in logs using `{err:#}` or equivalent, with access token redaction for agent/printer errors.
