# Phase 9: Print Report Reconciliation Design

Phase 9 makes Pandar represent physical printer progress from Bambu MQTT reports without changing the Phase 5 dispatch contract. A successful `PrintProjectFile` command result still means the agent uploaded the artifact and published the MQTT `project_file` command. Physical start, progress, completion, failure, and cancellation are recorded in separate print-lifecycle fields and events.

## Goals

- Normalize Bambu MQTT reports into agent-to-hub print progress events.
- Correlate reports to Pandar jobs using Phase 5 `project_file` identity plus report fields.
- Persist physical print status, progress fields, terminal failure details, and normalized machine events in both SQLite and PostgreSQL.
- Broadcast tenant-scoped job progress over WebSocket and expose it through job list/detail APIs.
- Show physical progress and terminal reason in the existing frontend job history.
- Keep command dispatch lifecycle and physical print lifecycle separate in naming, schema, API shape, and UI.

## Non-Goals

- No live-printer integration tests or required LAN credentials.
- No authentication redesign; Phase 10 handles Clerk/Logto and authenticated browser WebSockets.
- No AMS/filament modeling; Phase 14 owns first-class spool and filament state.
- No discovery or compatibility matrix; Phase 13 owns those diagnostics.
- No SeaORM migration in this phase.

## Current Constraints

- `jobs.status` currently mirrors the command ledger: `queued`, `sent`, `acknowledged`, `succeeded`, `failed`.
- Phase 5 `jobs.status = succeeded` means dispatch finished at the agent boundary, not physical completion.
- The agent currently pulls one MQTT report only for `RefreshPrinters`; there is no continuous report event path.
- `AgentEvent` supports hello, heartbeat, printer snapshot, command ack, and command result only.
- Existing WebSocket support is tenant-scoped and in-memory under `/api/v1/tenants/{tenant_id}/printer-events`.
- Generated protobuf Rust output is ignored and must be regenerated before builds, not committed.

## Data Model

Add a backend-neutral physical print lifecycle to `jobs`:

- `print_status`: one of `pending`, `running`, `completed`, `failed`, `cancelled`.
- `printer_state`: latest raw Bambu `gcode_state` or normalized printer state string.
- `progress_percent`: latest valid `mc_percent` as an integer percentage.
- `remaining_time_minutes`: latest valid `mc_remaining_time`.
- `current_layer`: latest valid `layer_num`.
- `total_layers`: latest valid `total_layer_num`.
- `active_file`: latest `gcode_file` or `subtask_name` when present.
- `last_progress_percent`: highest valid progress observed for this job.
- `last_layer`: highest valid layer observed for this job.
- `print_error`: terminal physical-print error or diagnostic summary.
- `print_started_at`, `print_finished_at`: RFC3339 timestamps set by reconciliation transitions.
- `print_updated_at`: latest accepted print report timestamp.

The initial row for a new job uses `print_status = pending` and nullable progress fields. Command transitions update `jobs.status` and `jobs.error`; report reconciliation updates only print-lifecycle columns.

Add `machine_events`:

- `id`, `tenant_id`, `agent_id`, `printer_id`, optional `job_id`.
- `event_key`: deterministic replay-stable dedupe key, unique per tenant.
- `kind`: `print_progress`, `print_terminal`, `print_error`, or `hms`.
- `severity`: `info`, `warning`, or `error`.
- `message`: short normalized diagnostic.
- `code`: optional Bambu error/HMS code.
- `payload_json`: compact normalized JSON payload for debugging.
- `observed_at`, `created_at`.

`event_key` must not use the gRPC envelope `event_id` because reconnect replay can create a fresh envelope id for the same printer report. Keys are built from normalized stable report content:

- Progress event key: `print-progress:{job_id}:{observed_at}:{gcode_state}:{percent}:{current_layer}:{total_layers}`. Progress events are useful but can be frequent; duplicate identical samples collapse, distinct samples are retained.
- Terminal event key: `print-terminal:{job_id}:{terminal_status}`. A job has at most one persisted terminal event for each terminal status, and terminal reconciliation cannot regress to non-terminal state.
- Print error event key: `print-error:{job_id}:{code_or_message_hash}:{observed_at}`. If no code is present, hash the normalized message and compact payload.
- HMS event key: `hms:{printer_id}:{code}:{observed_at}`. If the same HMS code repeats at a new time it is retained as a new event; exact replay is deduped.
- Uncorrelated diagnostic key: `machine:{printer_id}:{kind}:{code_or_message_hash}:{observed_at}`.

If a generated key collides with an existing row, the insert is treated as idempotent success and the existing event is not modified.

Both SQLite and PostgreSQL migrations must add equivalent columns, indexes, and constraints. Database-specific syntax can differ only inside migration files or repository adapters; externally visible behavior must match.

## Agent MQTT Normalization

Add a normalized report model in `pandar-agent` machine MQTT code:

- `PrintReportProgress` fields:
  - `serial`, optional `job_id`, optional `artifact_id`, optional `subtask_id`.
  - `gcode_state`, `percent`, `remaining_time_minutes`, `current_layer`, `total_layers`.
  - `gcode_file`, `subtask_name`.
  - `print_error`, HMS/error details as structured entries.
  - `observed_at`.
- Job identity extraction:
  - Prefer explicit report `print.task_id` as Pandar `job_id` when present.
  - Use `print.subtask_id` to match Phase 5 artifact id.
  - Preserve `gcode_file` and `subtask_name` for fallback correlation and display, but do not rely on filename alone when an id match exists.
- Invalid or missing numeric progress fields remain `None` instead of becoming `0`.
- Valid numeric ranges:
  - `percent`: `0..=100`.
  - `remaining_time_minutes`: `0..=4320`.
  - `current_layer` and `total_layers`: `0..=100000`.
  - Values outside these ranges are ignored for that field and do not reject the whole report.
- `last_progress_percent` and `last_layer` are hub reconciliation concerns; the agent sends observed values.

Report state mapping:

- `RUNNING` -> physical `running`.
- `FINISH` -> physical `completed`.
- `FAILED` -> physical `failed`.
- `IDLE` after the hub already saw `running` for the same job -> physical `cancelled`.
- Other states update `printer_state` and progress fields but do not create terminal status.

The agent must emit report events without requiring a print command to be in flight. On startup with configured printers, it should subscribe to each report topic and forward report events over the existing reverse gRPC stream. Empty printer config remains non-networked.

## gRPC Contract

Extend `AgentEvent` with:

- `PrintJobReport print_job_report = 15`.

`PrintJobReport` contains:

- printer identity: `serial`.
- correlation fields: `job_id`, `artifact_id`, `subtask_id`, `gcode_file`, `subtask_name`.
- observed state: `gcode_state`, `percent`, `remaining_time_minutes`, `current_layer`, `total_layers`.
- diagnostics: repeated `MachineDiagnostic diagnostics`.
- `observed_at`.

`MachineDiagnostic` contains `kind`, `severity`, `code`, `message`, and `payload_json`.

The hub validates UUID-shaped `job_id` and `artifact_id` only when present. Blank optional strings are treated as absent. Events from stale agent sessions are ignored through the same current-session token check used for snapshots and command results.

Required validation:

- `serial` is required and must be non-blank after trimming. Missing or blank serial rejects the event with `invalid_argument`.
- `observed_at` is required and must parse as RFC3339. Invalid timestamps reject the event with `invalid_argument`; the hub must not silently replace them with server time.
- Optional string fields are trimmed. Blank `job_id`, `artifact_id`, `subtask_id`, `gcode_file`, and `subtask_name` become absent.
- Present `job_id` and `artifact_id` must parse as UUIDs. Invalid values reject the event with `invalid_argument` because they claim Pandar identity but cannot be matched safely.
- `subtask_id` is accepted as a string because firmware/report sources may not guarantee UUID formatting; UUID-shaped values are preferred for artifact matching.
- Numeric fields outside the valid ranges above are treated as absent, not as rejection, because Bambu reports can omit or transiently reset progress fields.
- Diagnostic entries with blank `kind` are dropped. Blank severity becomes `info`; unknown severity becomes `warning`.

## Hub Reconciliation

Add repository operations behind the existing database-neutral boundary:

- Find a candidate job by tenant, agent, printer serial, and one of:
  - exact `job_id`.
  - exact `artifact_id` / `subtask_id`.
  - deterministic active-file fallback.
- Active-file fallback:
  - Eligible jobs must have the same tenant, agent, and resolved printer id.
  - Eligible jobs must have non-terminal physical `print_status` (`pending` or `running`).
  - Eligible jobs must have been created within the last 24 hours.
  - The report active file is matched against the job artifact filename exactly after trimming path components from `gcode_file`; `subtask_name` can match the artifact filename stem.
  - If exactly one eligible job matches, use it.
  - If zero or more than one eligible jobs match, do not correlate the report; record printer-level diagnostics only. Ambiguous fallback must not update any job.
  - Exact `job_id` wins over artifact/subtask id, and artifact/subtask id wins over active-file fallback.
- Apply a print report idempotently:
  - Ignore reports that do not correlate to a tenant job, but record non-job machine diagnostics when possible.
  - Update progress fields with latest valid observed values.
  - Preserve monotonic `last_progress_percent` and `last_layer`.
  - Set `print_started_at` the first time status becomes `running`.
  - Set `print_finished_at` once for `completed`, `failed`, or `cancelled`.
  - Do not move a terminal physical status back to non-terminal on stale/replayed reports.
  - Treat `IDLE` as `cancelled` only when the current persisted `print_status` is `running`.
- Insert diagnostic machine events using the event dedupe key.

Terminal report behavior:

- `FINISH` marks `print_status = completed`.
- `FAILED` marks `print_status = failed` and stores `print_error` from `print_error`, diagnostic message, or a normalized fallback.
- `IDLE` after `running` marks `print_status = cancelled`; `print_error` should describe an abort/cancel path when no structured diagnostic exists.

## API And WebSocket

Keep existing job response fields:

- `status` remains the dispatch status.
- `command.status` remains the dispatch status.

Add a nested `print` object:

- `status`, `printer_state`, `progress_percent`, `remaining_time_minutes`.
- `current_layer`, `total_layers`, `active_file`.
- `last_progress_percent`, `last_layer`.
- `error`, `started_at`, `finished_at`, `updated_at`.

Add tenant WebSocket job events. The simplest acceptable shape is to extend the existing tenant event hub/route with a `job_progress` event on `/api/v1/tenants/{tenant_id}/printer-events`. The event includes `job` in the same response shape used by job detail. A separate `/job-events` route is acceptable only if the implementation keeps auth, tenant scoping, and tests equivalent.

The WebSocket should broadcast after a report changes the persisted job or inserts terminal diagnostics. It does not need durable replay; HTTP job list/detail remains the initial-state source.

## Frontend

Update the existing dashboard job table:

- Label dispatch state separately from physical print state.
- Show progress percent and layer progress when present.
- Show remaining time when present.
- Show terminal physical error/reason when present.
- Keep the current HTTP fetch path as the source of initial state.

Phase 9 may display progress from server-rendered HTTP responses. Browser-side WebSocket consumption for rich live updates can remain Phase 15, but the hub WebSocket event must exist and be tested in Phase 9.

## Tests

Required coverage:

- Agent MQTT normalization extracts `task_id`, `subtask_id`, progress, layer, file, and diagnostics from representative Bambu report JSON.
- Agent emits `PrintJobReport` events from configured printer report subscriptions without live sockets by using fake MQTT transports.
- Proto generation is required before Rust build/tests, and generated outputs remain ignored.
- Hub rejects malformed required fields and ignores stale-session report events.
- Hub correlates by job id, artifact/subtask id, and same-printer active file fallback.
- Hub maps `RUNNING`, `FINISH`, `FAILED`, and `IDLE after RUNNING` to physical print status while preserving dispatch `jobs.status`.
- Hub progress updates are idempotent and do not duplicate terminal `machine_events`.
- SQLite and PostgreSQL repository tests cover migrations, progress persistence, terminal transitions, and event dedupe. PostgreSQL tests may keep the existing environment-dependent skip behavior.
- Job list/detail JSON includes dispatch status and nested print state.
- Tenant WebSocket receives a `job_progress` event when a print report is reconciled.
- Frontend build/typecheck covers the updated job response shape.
- Active-file fallback tests cover zero matches, one match, and ambiguous duplicate filename matches.

## Docs Impact

Update docs after implementation:

- `docs/roadmap.md`: mark Phase 9 completed, summarize implemented report reconciliation, and move Immediate Next to Phase 10.
- `docs/architecture.md`: document `PrintJobReport`, physical print lifecycle fields, machine event dedupe rules, and the distinction between dispatch status and print status.
- Phase 9 implementation plan/spec artifacts under `docs/superpowers/`.
- API/WebSocket documentation in architecture notes or roadmap: nested job `print` response shape and `job_progress` tenant event shape.
- Migration/schema notes: both SQLite and PostgreSQL add the same job print-lifecycle columns and `machine_events` table.

## Acceptance Criteria

- A dispatch-succeeded job can later become physically `running`, `completed`, `failed`, or `cancelled` from MQTT reports without changing the meaning of dispatch `status`.
- Hub restart or agent reconnect can replay the latest report without duplicating terminal machine events or regressing terminal physical status.
- Users can inspect job history/detail responses and the frontend job table to see physical progress and terminal reason.
- Tenant WebSocket subscribers receive job progress events after reconciliation.
- `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo nextest run --manifest-path "Cargo.toml" --workspace` pass after regenerating protobuf output.
