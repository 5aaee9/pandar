# Agent Printer Linking Design

## Scope

Add a tenant-scoped flow that lets an authorized user submit Bambu LAN printer connection details from the Pandar UI and have the selected `pandar-agent` link that printer at runtime.

This change covers:

- Hub HTTP API, command repository payloads, command conversion, and audit events.
- gRPC `HubCommand` schema for a printer-link command.
- Agent-side runtime printer linking, validation, command acknowledgement/result handling, and snapshot emission.
- Frontend `/agents` UI for entering printer connection details and submitting them to a selected agent.
- English and Chinese UI copy, action status handling, tests, and roadmap updates.

This change does not cover:

- Persisting printer access codes in the Hub database.
- Long-term agent-side config file persistence across agent process restarts.
- Cross-Hub secret relay for deployments where the HTTP request lands on a different Hub process than the agent reverse stream.
- Automatic credential discovery or pairing with printer cloud accounts.
- Bulk printer linking.
- Editing or deleting an already linked printer.
- A new database table or migration.

## Existing Context

Pandar already has a reverse control plane: Hub persists tenant/agent-scoped commands in `commands`, wakes the live reverse gRPC stream, converts queued records into `HubCommand`, and records agent acknowledgements/results. Existing command kinds include `refresh_printers`, `discover_printers`, `diagnose_printer`, `printer_operation`, and `print_project_file`.

The agent currently builds its machine gateway once from `PANDAR_PRINTERS` at startup. Each `BambuPrinterEndpoint` contains `host`, `serial`, `access_code`, optional `model`, and optional `name`. Bambu LAN MQTT uses `device/{serial}/report` and `device/{serial}/request`, so a manual link flow needs a serial number unless the serial was obtained from discovery. Existing SSDP discovery can return host, serial, name, and model, but it does not return access code.

Hub printer inventory is derived from agent `PrinterSnapshot` events. The Hub does not need to store printer credentials to list or dispatch to linked printers; it only stores non-secret inventory rows after the agent reports snapshots.

## Options Considered

### Recommended: Runtime Command With Agent-Local In-Memory Link

Add a `link_printer` command whose secret-bearing payload includes host, serial number, access code, optional display name, and optional model. The Hub validates request shape, creates a command row with a redacted non-secret payload, audits redacted metadata, and sends the full secret-bearing proto command only through the currently connected local reverse gRPC stream. The agent validates the supplied endpoint by creating a runtime Bambu transport, refreshing the printer over MQTT, adding the endpoint to its in-memory gateway, starting report forwarding for that endpoint, emitting a printer snapshot, and returning a command result that excludes the access code.

Tradeoffs:

- Matches the existing command architecture and does not introduce a credential database.
- Enables linking without restarting the agent.
- Avoids storing access codes in `commands.payload_json` or any other Hub database field.
- Requires the agent to be connected to the Hub process handling the HTTP request. If the agent is not locally connected, the API rejects the request with `agent_not_connected` instead of creating a durable command that cannot be replayed without the secret.
- The link is lost when the agent process restarts unless the operator also updates `PANDAR_PRINTERS`; this is acceptable for this feature because durable agent config persistence is explicitly out of scope.
- Because secrets are never persisted, Hub crash recovery cannot replay an in-flight `link_printer` command. A stale live-command cleanup path marks old non-terminal `link_printer` commands failed so users can retry instead of seeing a permanent `sent` state.

### Alternative: Hub-Stored Printer Credentials

Store access code and host in Hub and push them to agents when needed.

Tradeoffs:

- Would survive agent restarts, but contradicts the current credential boundary where Bambu access codes stay agent-local.
- Requires secret storage, redaction, migrations, and rotation semantics.
- Too broad for the requested UI-to-agent linking flow.

### Alternative: UI Writes Agent Configuration File

Have the Hub send a command that makes the agent modify local config or environment-like state and restart itself.

Tradeoffs:

- Could survive restarts, but requires choosing a config file format/location, write permissions, reload semantics, and rollback behavior.
- Higher operational risk and unnecessary for this feature.

## API Design

Endpoint:

```text
POST /api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer
```

Authorization:

- Require tenant `operator` role or stronger, matching refresh/discovery/diagnostics command dispatch.
- The API verifies the agent belongs to the tenant before creating the command record.
- The API requires a current local reverse session for that tenant/agent. If no matching local session exists, it returns `409 { "error": "agent_not_connected" }` and does not create a command row.
- In multi-Hub deployments, this check is intentionally local to the Hub process that received the HTTP request. If the persisted agent status is `online` because the agent is connected to a sibling Hub process, this endpoint still returns `409 agent_not_connected`. Cross-Hub secret relay is out of scope and the operator-facing docs must call out that runtime printer linking requires routing the request to the Hub instance that owns the agent stream.

Request JSON:

```json
{
  "host": "192.0.2.10",
  "serial_number": "SERIAL123",
  "access_code": "12345678",
  "name": "Office X1C",
  "model": "X1 Carbon"
}
```

Validation:

- `host`, `serial_number`, and `access_code` are required non-empty strings after trimming.
- `name` and `model` are optional; blank values are normalized to `null`.
- Unknown fields are rejected.
- Invalid JSON returns `400 { "error": "bad_request" }`.
- Missing/blank required fields return `400 { "error": "bad_request" }`.
- Invalid tenant or agent IDs use the existing `invalid_tenant_id` / `invalid_agent_id` errors.
- Missing/cross-tenant agents use the existing `agent_not_found` behavior through repository ownership checks.

API errors:

- API failures that happen before a command row is created return non-2xx JSON errors and redirect through the existing `/agents?tenant=...&status=<api_error>` action-status path in the frontend server action.
- After a command row exists, the API returns `200` with that `CommandResponse`, even if direct stream send immediately failed and the command status is already `failed`. The frontend redirects to `/agents?tenant=...&command=<command_id>` for every `200` command response so users inspect the command status and error in one place.
- `agent_not_connected` is an error status, not a positive queued status.

Success response:

- `200` with the existing `CommandResponse` shape.
- `kind` is `link_printer`.
- `payload_json` contains only non-secret fields: `host`, `serial_number`, optional `name`, optional `model`, and `access_code` set to `"[redacted]"`. The exact response must never contain the submitted access code.
- Preferred redacted payload shape is:

```json
{
  "host": "192.0.2.10",
  "serial_number": "SERIAL123",
  "access_code": "[redacted]",
  "name": "Office X1C",
  "model": "X1 Carbon"
}
```

Audit behavior:

- Successful dispatch records `agent.link_printer` with target type `agent`, target ID set to the agent ID, and metadata containing `host`, `serial_number`, optional `name`, optional `model`, and no access code.

## Command And Protocol Design

Secret-bearing runtime payload type:

```rust
pub struct LinkPrinterPayload {
    pub host: String,
    pub serial_number: String,
    pub access_code: String,
    pub name: Option<String>,
    pub model: Option<String>,
}
```

Persisted command:

- `kind = "link_printer"`
- `printer_id = null`
- `payload_json` stores a redacted payload only. The persisted value is suitable to return from `CommandResponse` and `GET /api/v1/tenants/{tenant_id}/commands/{command_id}` without extra transformation.
- Because the secret is not persisted, `link_printer` is not processed by `next_queued_for_agent` and must never be inserted with `queued` status. Add a repository method that creates the redacted command row with `sent` status in the same transaction that writes the audit event.
- Extend `AgentSession` to store the reverse-stream command sender and an in-memory pending live-command set scoped to that session token.

Direct dispatch ordering:

1. Validate request body, authorize the caller, verify tenant/agent ownership, and capture the current local session token. If no local session exists, return `409 agent_not_connected` and do not create a command row.
2. Insert the redacted command row with `sent` status and the audit event in one database transaction. If this transaction fails, do not send the proto command.
3. Call a `SessionRegistry` helper that holds the session map lock, verifies the captured session token is still current, records the command ID in that session's pending live-command set, and uses non-async `try_send` to place `HubCommand::LinkPrinter` on that session's command sender while still under the same lock. This is the required token recheck/send point; do not clone a sender and send later without revalidation.
4. If the helper finds the token is no longer current, mark the command `failed` with redacted `agent connection closed before printer link completed` and return `200` with the failed command response when the command can be loaded. This covers session replacement/removal between lookup and post-commit dispatch.
5. If `try_send` fails, remove the command ID from the pending set before releasing the lock, mark the command `failed` with a redacted `agent connection closed before printer link completed` or `agent command channel unavailable before printer link completed` error, and return `200` with the failed command response when the command can be loaded. If marking failed also fails, log the full redacted error chain and return `500 internal_server_error`.
6. If `try_send` succeeds, return `200` with the `sent` command response. Normal agent ack/result events move the command to `acknowledged`, `succeeded`, or `failed`.
7. When a terminal result is processed, remove the command ID from the pending set. If result handling cannot update the command, preserve/log the full redacted context; session cleanup still treats the command ID as pending.
8. If the reverse stream closes with pending live-command IDs still in that session set, session cleanup marks those non-terminal `link_printer` commands failed with the redacted `agent connection closed before printer link completed` error. Cleanup must target the session's recorded pending command IDs, not all tenant/agent `link_printer` commands, so replacement sessions cannot fail commands dispatched on a newer session.

Session replacement/removal race behavior:

- If the session disappears before the command row is created, return `409 agent_not_connected` and create no command row.
- If the session disappears or is replaced after the command row is created but before the registry helper's token recheck/send point, mark the created command failed and return the command response.
- If the registry helper successfully `try_send`s while the token is current, later session closure is handled by ack/result events or the pending-set cleanup above.

Crash and stale-command cleanup:

- Add a backend-neutral command repository method that marks stale, unowned non-terminal `link_printer` commands older than 5 minutes as failed with `printer link dispatch expired before completion`.
- Non-terminal for this cleanup means `sent` or `acknowledged`; `queued` must not be created for `link_printer`, and `succeeded`/`failed` are left untouched.
- Stale cleanup must receive the set of command IDs currently present in all local session pending live-command sets and skip those IDs. Active long-running link attempts stay owned by their session and are not failed by the periodic cleanup.
- Run this cleanup from the existing runtime/session maintenance loop or a small sibling loop using the same error-context logging style. This covers Hub crashes between redacted command-row commit and direct send, Hub crashes after direct send but before ack/result, and lost pending-set state after restart where no current session pending set owns the command.
- If a late ack or result arrives after stale cleanup already marked the command failed, existing terminal-transition rules apply: the command remains terminal failed, and the handler logs the full redacted context for the stale result instead of resurrecting or changing the command.
- The stale cleanup must not require the access code and must not log request payloads.

Protocol additions:

```proto
message LinkPrinter {
  string host = 1;
  string serial_number = 2;
  string access_code = 3;
  string name = 4;
  string model = 5;
}

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
```

Command conversion:

- `hub_command_from_record` does not support replaying persisted `link_printer` commands. If it encounters `kind = "link_printer"` through the durable queued-command path, it logs the full context and returns `Status::failed_precondition("link printer command requires live secret dispatch")` without marking the command sent.
- The route builds `HubCommand::LinkPrinter` from the in-memory validated request payload and maps optional `name`/`model` to empty proto strings when absent.

Command result:

- On success, agent returns JSON shaped as:

```json
{
  "type": "printer_link",
  "serial_number": "SERIAL123",
  "host": "192.0.2.10",
  "name": "Office X1C",
  "model": "X1 Carbon",
  "status": "online"
}
```

Redaction contract:

- Access code must never appear in `CommandResult.result_json`, command errors after redaction, audit metadata, frontend status, or snapshot payloads.
- Access code must never appear in persisted `commands.payload_json`, HTTP command responses, `GET /commands/{command_id}`, audit metadata, command result JSON, command error strings, logs, frontend status, or snapshot payloads.
- Hub must not log incoming `link_printer` request bodies. Do not add request-body logging for this route.
- At every Hub error/log/result boundary that has seen the raw request payload, format the full error chain first with `{err:#}`, then redact with `crate::redaction::redact_secrets` and a link-printer-specific replacement of the submitted access code before storing, returning, or logging the message.
- At every agent error/log/result boundary that has seen the raw request payload, format the full error chain first with `{err:#}`, then redact with the submitted access code before sending `CommandResult.error` or logging. This can use the existing access-code redaction helpers, but the test contract is the raw submitted access code is absent from every visible string.
- The replacement token for access codes in visible strings is `[redacted]` unless an existing helper already emits a more specific token such as `[REDACTED_ACCESS_CODE]`; tests assert absence of the submitted raw access code rather than an exact token.
- Host/IP, serial number, name, and model are not treated as secrets for this feature.

## Agent Runtime Design

The agent needs a mutable runtime gateway because startup printers are currently fixed for the lifetime of `run_once`.

Design:

- Introduce a focused runtime gateway wrapper in `crates/pandar-agent/src/machine` that owns linked `BambuPrinterEndpoint` entries and their MQTT/file-transfer adapters behind a per-agent `tokio::sync::Mutex`.
- Minimal interface:
  - It implements the existing `BambuMachineGateway` trait for `discover_printers`, `diagnose_printer`, `refresh_printers`, `validate_printer`, `print_project_file`, `operate_printer`, and `redact_error`.
  - It adds a `link_printer(endpoint, config, sender, report_timeout)` method used only by `LinkPrinter` command handling.
  - It stores printers by serial number with their endpoint, command MQTT transport, file-transfer adapter, and report-forwarding task handle.
  - It serializes `link_printer` mutations, refresh, diagnostics, print dispatch, and live operations through the same gateway lock. This favors predictable behavior over parallel machine operations for this feature.
  - With no linked printers, `discover_printers` still runs SSDP, `refresh_printers` returns an empty list, diagnostics for an unknown serial returns the existing configured-printer problem result, and validate/print/operation fail with the existing no-configured-printer style errors.
- Startup `PANDAR_PRINTERS` entries seed this runtime gateway exactly as before.
- `link_printer` constructs a new `BambuPrinterEndpoint` from the command, creates Bambu MQTT/FTPS adapters through the same runtime factories used by configured printers, refreshes the printer to prove the credentials and serial/host combination work, then inserts or replaces the endpoint for that serial.
- If an endpoint with the same serial already exists, the new endpoint replaces it only after validation succeeds. This supports correcting a host/access-code/name/model without a separate edit feature.
- If two `link_printer` commands for the same serial arrive concurrently, they are serialized by the gateway lock. The first successful validation installs first; the later successful validation replaces it. A later failed validation leaves the existing installed endpoint unchanged.
- Report forwarding is one task per serial. Replacement cancels the previous serial's forwarding task only at the install commit point, then starts a new forwarding task for the replacement endpoint. There must not be duplicate report-forwarding tasks for the same serial after a successful replacement.
- Emit a `PrinterSnapshot` event from the validated refresh result before returning command success.
- Redact the submitted access code from all link failure strings with the existing redaction helper behavior.

Agent-side commit and cleanup:

- Validation happens before mutating the runtime gateway. Validation failure leaves any existing endpoint and report-forwarding task unchanged.
- Report-forwarding startup means preparing any endpoint/transport state needed for the forwarding task and spawning the task. If preparation fails before spawn, the command fails and the previous endpoint/task remains active. If preparation succeeds and the task is spawned, later task failure is logged with redaction but does not roll back the installed endpoint.
- The install commit point is successful insertion/replacement in the runtime gateway plus successful spawn of the single report-forwarding task for that serial.
- After the commit point, the agent keeps the endpoint installed even if sending the snapshot or final command result fails because the Hub stream closed. The Hub command cleanup will mark the command failed, and a later refresh/report reconnect can surface the already-linked printer.
- Before the commit point, any failure leaves the runtime gateway unchanged.

No-printer startup behavior:

- Agents that start with `PANDAR_PRINTERS=[]` must still be able to accept `link_printer` commands. This removes the current permanent `NoopMachineGateway` branch for the command-processing loop and replaces it with an initially empty runtime gateway.

Command handling:

- On `LinkPrinter`, the agent sends `CommandAck { accepted: true }` after request shape is usable and before attempting network validation.
- If runtime validation succeeds, it emits a printer snapshot and then a success result with redacted structured JSON.
- If validation fails, it emits a failed result with a redacted full error chain.
- The command handler should not persist secrets to disk.

## Frontend Design

Surface:

- Add a `LinkPrinterForm` as a new section in `AgentsView`, rendered after `AgentPairingGuidance` and before `LinkedAgentsSection`, because the form creates new agent-printer links while the linked agents table remains focused on existing agent rows.
- The form is not shown as enabled when no tenant is selected or no agents exist.
- The form contains:
  - Agent select.
  - Host/IP input.
  - Serial number input.
  - Access code password input.
  - Optional printer name input.
  - Optional model input.
  - Submit button.
- The form is compact and operational, matching the existing dashboard section style. It should not use marketing or tutorial copy.
- The agent select lists all tenant agents and includes each agent's status in the option label. It defaults to the first `online` agent when present, otherwise the first agent. The API remains authoritative for the local-session requirement because a persisted `online` status can still mean the agent is connected to a sibling Hub process.

Behavior:

- Server action `linkPrinter(formData)` calls `POST /api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer`.
- On any `200` command response, redirect to `/agents?tenant=...&command=<command_id>` so the existing command result panel can show the link status and result/error.
- On non-2xx API failure before command creation, redirect to `/agents?tenant=...&status=<api_error>`.
- Access code input must use `type="password"`, `autoComplete="off"`, and must not be rendered back to the page after submit.
- If the API returns `agent_not_connected`, show it through the existing action-status toast path after redirect.

Command result parsing:

- Extend `CommandResultData` and `parseCommandResult` to recognize `type: "printer_link"`.
- Add a concrete `PrinterLinkResult` renderer in `frontend/app/diagnostics-panel.tsx` alongside `DiscoveryResult` and `DiagnosticResult`. It renders host, serial number, name/model when present, and status.

Discovery integration:

- This feature does not require one-click linking from discovery results. Discovered host/serial/name/model remain useful manual input references.

## Tests

Hub route/repository tests:

- Operator or tenant admin can dispatch `link_printer` for a tenant-owned locally connected agent.
- A missing local reverse session returns `409 agent_not_connected` and does not create a command row.
- Viewer cannot dispatch it.
- Blank `host`, `serial_number`, or `access_code` returns `400 bad_request`.
- Unknown request fields return `400 bad_request`.
- Cross-tenant/missing agent behavior matches existing command ownership errors.
- The dispatch audit event excludes the access code and includes host/serial metadata.
- Persisted `commands.payload_json`, the dispatch response, and `GET /commands/{command_id}` for `link_printer` never contain the submitted access code.
- Persisted `link_printer` commands are inserted as `sent`, not `queued`, so the durable outbound pump never tries to replay them.
- A local-session send failure marks the created command failed with a redacted error.
- Session replacement/removal between lookup and direct send is covered by the registry helper's token recheck: pre-row races return `agent_not_connected`, post-row races mark the created command failed without sending the secret-bearing command.
- A stream close before ack/result marks non-terminal live-only `link_printer` commands failed instead of leaving them stuck.
- The stream-close cleanup fails only the pending live-command IDs recorded on that closing session, not all commands for the tenant/agent.
- Stale live-command cleanup marks only unowned `sent`/`acknowledged` `link_printer` commands older than 5 minutes failed, covering Hub restart windows that lose the in-memory pending set without failing active pending commands.
- Late ack/result events after stale cleanup do not change terminal failed command state and are logged with redaction.
- Captured logs or test-visible errors for validation failure, send failure, stale cleanup, and result handling failure do not contain the submitted access code. If a test cannot capture logs in that module, it must at least assert command errors/results omit the access code and that the route does not log request bodies.
- In a sibling-Hub test or simulated local-session-missing test, an agent that is not present in the receiving process's local session registry returns `agent_not_connected` and creates no command row.

Hub gRPC conversion tests:

- The route builds direct proto `HubCommand::LinkPrinter` with all fields from the in-memory request payload.
- The durable queued-command converter rejects persisted `link_printer` with the expected failed-precondition status.

Agent tests:

- Handling `LinkPrinter` on an initially empty gateway sends ack, emits a printer snapshot, and sends success result JSON without access code.
- Runtime-linked printers can be refreshed by a later `RefreshPrinters` command without restarting the agent.
- A validation failure returns a failed result whose error redacts the access code.
- Linking the same serial twice replaces the endpoint only after the second validation succeeds.
- Concurrent same-serial links are serialized; a later failed validation does not remove the earlier working endpoint.
- Replacement leaves only one report-forwarding task for the serial.
- Report-forwarding preparation failure during same-serial replacement leaves the previous endpoint and previous forwarding task active and returns a failed command result.
- If the Hub stream closes after the install commit point, the runtime endpoint remains installed and later refresh can report it.

Frontend tests:

- `/agents` renders the link-printer form with agent select, host, serial, access code, optional name, and optional model fields.
- The form is disabled or replaced by an empty state when no tenant or no agents exist.
- `CommandResultData` parsing/rendering supports `printer_link` results.
- `agent_not_connected` renders as an error status through the existing fallback behavior.

## Documentation And Roadmap

- Update `docs/roadmap.md` with the completed runtime printer-linking feature and the restart-persistence limitation.
- Update `docs/development.md` or the nearest existing agent-operations document with a short note: UI-linked printers are runtime-only, do not survive agent restart unless added to `PANDAR_PRINTERS`, and multi-Hub deployments must route the link request to the Hub process that owns the agent reverse stream because cross-Hub secret relay is not implemented.
- No deployment configuration reference change is required because no new environment variable or schema migration is introduced.

## Acceptance Criteria

- A user can choose an agent on `/agents`, enter printer host/IP, serial number, access code, optional name/model, and submit a link request.
- Hub creates an audited `link_printer` command only when a matching local reverse session is connected, and never stores or returns the access code.
- A connected local agent receives the command over the existing reverse gRPC stream and validates the printer through the Bambu LAN runtime path.
- A successful link emits a printer snapshot so the printer appears in tenant inventory and can be used by later refresh/dispatch flows without restarting the agent.
- Agents that started with no configured printers can link a printer at runtime.
- Link failures preserve full lower-level context for debugging while redacting access codes.
- Requests for agents without a current local reverse session fail with `agent_not_connected` and do not create unreplayable queued commands.
- SQLite and PostgreSQL behavior remain backend-neutral; no backend-specific schema/query logic is introduced.
- Rust formatting, Clippy, workspace tests, frontend tests/build checks, and roadmap update are completed before final commit/push, or any environment blocker is reported with exact output.
