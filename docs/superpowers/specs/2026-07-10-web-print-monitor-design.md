# Web Print Monitor and Build-Plate Mismatch Design

## Goal

Make the Web device card show the same useful live-print information that Bambu Studio shows instead of reducing the printer to `RUNNING`. Add a Pandar-owned build-plate mismatch prompt that uses the native print-error recovery operation implemented for Studio.

The accepted interaction is:

- show live task name, percentage, current/total layer, and remaining time while a print is active;
- automatically open a build-plate mismatch dialog;
- retain an inline warning on the printer card after the dialog is dismissed or an action is sent;
- expose the native recovery actions supported by that printer family;
- remove the warning only after the printer explicitly reports that the error is cleared.

## Current State

The Agent and Hub already collect and persist all required fields from Bambu MQTT reports:

- `gcode_state`;
- `task_id` and `subtask_id`;
- `subtask_name` and `gcode_file`;
- progress percentage;
- remaining minutes;
- current and total layers;
- numeric `print_error`;
- the printer-native `job_id`;
- typed HMS entries.

The Studio plugin API already exposes these fields. The ordinary tenant printer REST response and WebSocket `printer_snapshot` do not, so the Web card currently receives only the coarse printer `status`, temperatures, and materials. A Studio- or printer-originated task also has no Pandar `Job`, so joining the printer to the Jobs API cannot solve this reliably.

The existing tenant `/controls` route deliberately rejects `handle_print_error`. Only the Studio plugin route can currently use the capability-checked, live-only dispatcher.

Studio guards every HMS Resume/Ignore action with the job state derived from `job_attr`; Pandar does not currently parse or transport that field. Adding it to the typed Agent report is required for native-safe Web behavior.

## Considered Approaches

### 1. Derive the display from Pandar Jobs

Rejected. Studio- and printer-originated tasks may have no Pandar Job. The current real task is one such case.

### 2. Add a new compact live-status event

Rejected after compatibility review. It minimizes payload size, but adds a fourth public WebSocket discriminator. Existing Android builds use a sealed three-variant decoder, and already-loaded Web builds treat every unknown discriminator as `command_result`. Updating every deployed client before Hub rollout would materially widen this Web feature.

### 3. Enrich the existing printer snapshot and resynchronize on reconnect

Selected. Tenant printer REST responses and the existing `printer_snapshot` event carry the same nested, merged print state. The Hub publishes a full snapshot only when device live-print state changes, not for last-seen-only reports. The Web refreshes the authoritative printer list on every WebSocket connection before declaring the channel live. This keeps the existing public event variants, covers every print origin, closes reconnect gaps, and trades a larger but lower-frequency payload for compatibility and simpler merge semantics.

## Public Printer State

### REST shape

An upgraded Hub logically adds a required top-level `state_revision` and nested `print` object to the ordinary tenant printer list and detail responses and to full `printer_snapshot` events:

```json
{
  "id": "<printer-id>",
  "status": "RUNNING",
  "state_revision": 37,
  "print": {
    "task_generation": 4,
    "error_generation": 9,
    "job_state": 0,
    "gcode_state": "RUNNING",
    "task_id": "<optional task id>",
    "subtask_id": "<optional subtask id>",
    "progress_percent": 42,
    "remaining_time_minutes": 11,
    "current_layer": 2,
    "total_layers": 128,
    "gcode_file": "/data/Metadata/plate_1.gcode",
    "subtask_name": "Cube",
    "print_error": 83918929,
    "printer_job_id": "<printer-native job id>",
    "hms": [{ "attr": 83887616, "code": 131184 }]
  }
}
```

`task_generation`, `error_generation`, and `hms` are required inside `print`; every device-reported field, including derived `job_state`, is nullable. `job_state` is exactly `(job_attr >> 4) & 0x0f`, matching Studio's `get_flag_bits(jobAttr, 4, 4)`. `printer_job_id` preserves an explicitly reported empty string; `null` means the printer has not supplied the field. Raw `job_attr`, session markers, host, and access code are not added to the public response.

Wiring the tenant list and detail handlers to repository methods that hydrate `PrinterWithLiveStatus` is new work in this change; both handlers must return the same merged state. The Studio plugin response keeps its existing flat Studio-compatible contract.

`state_revision` is a database-backed, monotonically increasing integer per printer. Every accepted mutation of the public non-material printer row atomically increments it with a database expression, including inventory/temperature snapshots, print reports, `last_seen_at`, and user-visible printer edits. Concurrent PostgreSQL writers therefore cannot lose an increment. A last-seen-only print report still produces no WebSocket event, so revisions may legitimately skip between delivered snapshots. Material snapshots retain their independent `materials.observed_at` ordering because materials are stored outside the printer row.

For rolling compatibility, the Rust control-plane representation and the Web boundary type decode `state_revision` and `print` as optional additive fields. Every fully upgraded tenant REST handler and every event created by an upgraded Hub populates them. A missing value means only that a legacy Hub produced the response or event; it never enables recovery and is not treated as a current empty print state.

### Live `printer_snapshot` events

The repository newly returns the fully rehydrated, post-merge printer and a `live_status_changed` flag from each accepted print report for a known printer. After the transaction commits, the Hub publishes the existing event type when that flag is true, even when the report is not correlated to a Pandar Job and does not change AMS data:

```json
{
  "type": "printer_snapshot",
  "printer": {
    "...": "the ordinary printer response",
    "print": { "...": "the same print object as REST" }
  }
}
```

The handler loads the printer's latest sanitized materials before publishing, so the current whole-printer reducer never turns an omitted materials value into a clear. Job progress events remain unchanged for correlated Pandar Jobs.

`live_status_changed` compares the merged task-scoped values, public generations, and `gcode_state`; a report that only advances `last_seen_at` advances the persisted revision but produces no snapshot. Progress, remaining time, layer, identity, error, job-ID, generation, or state changes do. An explicit `print_error: 0` remains distinguishable from an absent update and publishes the clear. Every event constructor reloads the committed `PrinterWithLiveStatus`, so printer, material, and print-report publishers cannot overwrite enriched state with an older plain `Printer` view.

### Partial merge and task boundaries

Within the same task, an absent report field preserves the stored value. Explicit values overwrite, including progress `0`, error `0`, raw `job_attr: 0`, and an empty printer job ID. The report protocol has presence bits for numeric progress/error/job-attribute fields and printer job ID, but not for the optional string identity fields; an empty string identity is therefore absent and task-scoped identity clearing is driven by an authoritative state boundary.

The Hub stores `task_generation` and treats identity as three typed slots: `task_id`, `subtask_id`, and `gcode_file`. The printer-native `job_id` is an HMS recovery binding and is deliberately not guessed to be task identity. Blank values and task/subtask sentinel `"0"` are not trusted identity, although reported values remain available for display/transport.

This merge table is a Pandar-owned conservative synthesis of the independent fields used by Studio/bambuddy, not a claim that Studio implements one identical helper:

- Compare every slot that is trusted on both the stored and incoming sides. Any same-slot difference proves a task boundary, even when another newly introduced or equal slot exists.
- At least one same-slot equality and no conflict proves continuity; newly supplied other slots enrich that task.
- No incoming trusted identity is a partial update and preserves the task.
- Stored identity absent with new incoming identity is first enrichment and preserves the current task unless a state boundary also proves a new one.
- If both sides contain trusted identities but have no slot in common, identity is ambiguous. Preserve display/progress and `task_generation`, but clear the error receive-time/task/session authorization marker, `printer_job_id`, and raw `job_attr` before merging the incoming frame. Only an explicitly present job ID or job attribute in that frame can restore its respective value. An explicit positive error in that same frame may establish a fresh marker, but it cannot revive either omitted recovery field.

Different namespaces are never compared. The ambiguous rule prevents a stale error, job target, or safety state from authorizing recovery while avoiding an invented task reset that the available wire fields cannot prove. Consequently Resume/Ignore remain unavailable after an ambiguous positive report that omits `job_attr`, and Stop uses the explicit current-frame job ID or Studio's empty-string default instead of a prior task's ID.

For task-generation purposes the exact Studio live set is `PREPARE`, `SLICING`, `RUNNING`, and `PAUSE`; the terminal set is `FINISH` and `FAILED`; the inactive set is `IDLE`; missing or any other value is unknown. `PRINTING` and `PAUSED` may be rendered as coarse UI aliases but never create a native Bambu task boundary.

A row with generation `0` initializes to generation `1` when a non-IDLE report first supplies trusted identity or a live/terminal state, without clearing fields. Thereafter a report increments the generation at most once when any jointly comparable identity slot differs or the stored state is inactive/terminal and the incoming state is live. A proven boundary clears task/subtask IDs, progress, remaining time, current/total layers, file/name, numeric print error, printer job ID, raw `job_attr`, and the internal error marker before applying the report. HMS remains device state and is not task-scoped.

An explicit incoming `IDLE` has highest precedence: it stores `IDLE`, clears every task-scoped field and error marker, ignores stale task/progress/error/job fields carried in that same frame, and does not increment `task_generation`. `FINISH` and `FAILED` retain the final task fields, but only `FINISH` is rendered as a finished task. The migration backfills task generation `1` for an existing row with task evidence, so the first post-deploy partial report does not erase a running task.

The Hub also stores `error_generation` as the latest positive error-occurrence number. It increments once only when a positive occurrence begins or changes: non-positive to positive, one positive code to another, task generation change while positive, an explicitly supplied native target job ID change while positive, or explicit re-observation after the authorization marker is absent or belongs to a different task/session. Before an explicit positive report establishes a marker in any of those absent/different-task/different-session cases, the merge clears stored `printer_job_id` and raw `job_attr`, then applies the current frame; only fields explicitly present in that frame are restored. Clearing to zero/IDLE removes the active condition and marker but does not consume another occurrence number; a later same positive code still increments, makes positive -> clear -> same positive ABA-safe, and cannot reuse recovery context from the earlier occurrence. Repeated explicit copies of the same code/job with the same valid task/session marker do not increment it.

Only `has_print_error` stores a new Hub receive time plus the authenticated session and current task generation in the internal marker. An absent error preserves code, generation, receive time, and marker; an explicit job-ID-only change may advance the occurrence while preserving an otherwise valid same-task/same-session marker. A partial report arriving from a replacement session cannot refresh the previous marker. If that session explicitly re-reports the positive error, it receives a new marker only after the old recovery target and safety state are invalidated as above. `observed_at` remains device telemetry and is never used to authorize recovery.

The gRPC connection token is also persisted as the Agent's current opaque session ID. A stable per-Agent transition lease, owned independently of any one `AgentSession`, linearizes the process registry with the database claim:

1. registration takes the lease, locks the Agent row, persists the new token/status, installs that same token in `SessionRegistry`, then releases;
2. heartbeat, printer snapshot, print report, material snapshot, exact-session offline clear, and Web recovery take the same lease, verify the registry token, then perform their database mutation only if the locked Agent row still has that token;
3. PostgreSQL always locks Agent then printer; SQLite uses an immediate transaction. Old-session offline cleanup is conditional on the exact token.

This replaces the current check-before/check-after gap: an old session cannot commit a printer snapshot, print report, material update, heartbeat, offline clear, or `state_revision` after a replacement becomes current. Across Hub processes, the locked Agent row is the claim authority; a process without the matching local session still fails closed.

### Connection and ordering contract

The WebSocket remains future-only and non-replaying. On every initial connection, reconnect, and bounded 30-second reconciliation interval, the browser runs one serialized reconciliation cycle:

1. on connection, opens the socket; for both connection and periodic reconciliation, begins buffering snapshots from that same socket;
2. fetches the tenant printer list through a token-safe same-origin proxy;
3. unconditionally replaces the current printer inventory with that REST baseline while retaining only browser-local dialog-dismissal state keyed by printer/error generation;
4. replays only buffered shell/print snapshots whose `state_revision` is greater than that printer's REST baseline, while merging materials independently by `materials.observed_at`; an enriched buffered printer absent from REST triggers one more authoritative list refresh rather than being inserted blindly;
5. marks or returns the channel to `live` only after the refresh and replay succeed. While connected, reconciliation starts use a monotonic 30-second start-to-start cadence; an immediate resync trigger may start one earlier and resets that cadence. A second timer or trigger coalesces into the in-progress cycle rather than starting a concurrent fetch.

Every baseline fetch uses an `AbortController` with a hard 10-second deadline. The one permitted unknown-printer confirmation is a separate fetch with its own 10-second deadline, making an ordinary cycle at most 10 seconds and that two-fetch cycle at most 20 seconds. Failure or deadline abort closes the socket, discards that buffer, clears the enriched print view and recovery actions rather than presenting them as current, marks the channel `unavailable`, and enters the existing retry schedule. Both bounds are shorter than the fixed cadence, so a hung fetch cannot overlap or suppress the next scheduled attempt indefinitely. A newly inserted upgraded printer starts at revision `1`; revision `0` is never emitted as an authoritative public printer. During replay or the live phase, a versioned shell/print snapshot for a known printer replaces state only when its revision is greater. An event for an unknown printer schedules the same authoritative list resync: if the printer is now present, that response becomes its baseline and any still-higher buffered event applies; if it remains absent, the event is discarded as stale. This distinguishes a newly created printer from an event buffered before deletion.

Event loss is also a resynchronization boundary. If a tenant `broadcast` receiver reports `Lagged`, the Hub logs the skipped count and terminates that WebSocket instead of continuing; the browser reconnect path above then reloads REST. The control-plane consumer cannot identify which tenant was affected, so a printer-event publish failure, any receive error, and subscriber EOF each invalidate a process-wide internal printer-event epoch. Each active printer WebSocket watches that epoch and terminates when it changes. This internal signal adds no public event discriminator. After EOF or a subscription failure the runtime re-enters its subscription loop after one second, preserving the complete cause chain in logs; an error item invalidates the epoch but the still-open stream continues.

Core NATS is non-replaying and may lose a sibling event during a disconnect window without yielding an item-level error. The 30-second serialized REST cycle is therefore required even on a healthy-looking socket. It applies the exact same replace-and-replay algorithm, so a concurrent newer event wins by `state_revision`. While the document is visible, its scheduler is running, the socket stays connected, and the same-origin fetch plus body read/decode/apply completes within its 10-second deadline, a state committed just after a completed baseline is repaired within the algorithmic 40-second bound: 30 seconds to the next ordinary cycle plus 10 seconds to apply it. The deadline remains armed through response-body reading and decode; a monotonic deadline check immediately before state application rejects a result whose synchronous decode overran it. Timeout clears the enriched view and marks it unavailable instead of presenting it as current.

No wall-clock claim is made while the browser is suspended, background timers are throttled, or the main thread is stalled. `visibilitychange` to visible and `pageshow` each trigger an immediate serialized reconciliation and reset the 30-second cadence; if a cycle is already active, the trigger coalesces and its pending rerun starts immediately after that cycle terminates. This bounds the active-page algorithm independently of control-plane delivery while keeping WebSocket updates immediate in the normal case.

Equal or lower revisions cannot regress a clear. In enriched mode a legacy snapshot cannot add or overwrite shell/print state, though independently newer materials may merge. An authoritative legacy REST baseline replaces the entire prior printer inventory, clears/hides live-print state, and disables recovery; legacy socket updates may then update only the coarse view. Missing fields are never filled from an older browser generation.

The persistent `error_generation`, not browser connection generation or timestamps, keys mismatch occurrences. A clear/reappear cycle missed while disconnected has a higher generation in the REST baseline and opens again; an unchanged positive occurrence stays dismissed after a reconnect. Material data remains independently ordered by `materials.observed_at`.

This requires matching SQLite and PostgreSQL migrations. The printer row adds `state_revision INTEGER/BIGINT NOT NULL DEFAULT 1` with an equivalent `CHECK (state_revision >= 1)` constraint, so an old Hub inserting a printer without the additive column still creates a valid public baseline. `print_task_generation` and `print_error_generation` are non-negative `INTEGER/BIGINT NOT NULL DEFAULT 0` counters. The row also adds nullable raw `print_job_attr`, nullable `print_error_task_generation`, `print_error_session_id`, and `print_error_received_at` markers. The Agent row adds nullable `current_session_id`. SQLite uses `INTEGER`; PostgreSQL uses `BIGINT` for counters/raw attributes, with otherwise identical defaults, constraints, and behavior.

The migration/default gives every existing printer `state_revision = 1`; it backfills `print_task_generation = 1` where task evidence already exists and `print_error_generation = 1` plus the current task generation where a positive error exists. Upgraded inserts explicitly use revision `1`, while legacy-style inserts that omit the column receive the same value from the database. Existing errors deliberately retain null session/time markers and fail closed until the current session explicitly reports them.

The Agent protocol adds presence-preserving `job_attr` to `PrintJobReport` and `AGENT_CAPABILITY_HANDLE_PRINT_ERROR_SEQUENCE_ZERO_PUBACK_ONLY`. A new Agent advertises both the existing `HandlePrintError` capability and the new capability. The Studio plugin continues to require only the existing capability and preserve Studio's transport. The Web recovery route requires the new capability because it depends on current `job_attr` safety state and on sequence-zero operations receiving only bounded QoS1 transport confirmation, never an application-report result.

## Web Printing Progress

The device card keeps its existing status badge and device controls. Its current summary area becomes state-aware:

- `RUNNING`, `PRINTING`, `PAUSE`, or `PAUSED`: show the task name, progress bar and percentage, layers, and remaining time.
- `PREPARE` or `SLICING`: show the task name, with percentage, layers, and remaining time rendered as unavailable.
- `FINISH`: retain the final task details until a later printer state replaces them, matching Studio's useful finished view.
- other states: keep the ordinary device status summary and do not display stale live-print details.

Status precedence is unambiguous: coarse `IDLE`, `OFFLINE`, or `FAILED` suppresses the progress/finished panel; otherwise `print.gcode_state` selects the state-aware view above. An absent or unknown live state falls back to the ordinary coarse status summary. The display name is the first non-empty value of `subtask_name` and the basename of `gcode_file`; otherwise it uses the translated unknown-task label.

The existing `formatProgress`, `formatLayers`, and `formatRemaining` functions and their English/Chinese messages are reused. Missing individual values render as `-` without hiding the other valid values. The AMS/material summary remains available and is not removed by the progress panel.

## Build-Plate Mismatch Prompt

### Detection and text

The dedicated prompt is shown only for numeric `print_error == 83918929` (`0x05008051`, displayed as `0500-8051`). Other print-error codes remain visible in the typed state but do not get an inferred dialog in this change.

Chinese copy:

> 检测到打印板类型与切片 G-code 中不一致。请修改切片参数或者使用匹配的打印板。

Equivalent English copy is added to the English catalog.

### Dialog and inline warning

The devices view owns one dialog coordinator rather than allowing every card to open a modal independently. It selects one unresolved mismatch at a time in stable printer-list order.

An error occurrence is keyed only by printer ID and the server-assigned `error_generation`. Repeated frames for that generation do not reopen a manually closed or successfully submitted dialog. Explicit zero/IDLE closes the occurrence without consuming a number; a later positive, task/native-job transition while positive, different positive code, or session re-observation advances the generation. The same mismatch code in a later occurrence therefore auto-opens again. If several printers have mismatches, closing one advances to the next unresolved occurrence.

Every affected card retains a red inline warning with an action to reopen its dialog until the merged state clears the occurrence. Remote recovery actions are enabled only when persisted native `gcode_state` is exactly `PREPARE`, `SLICING`, `RUNNING`, or `PAUSE`, an authoritative generation is present, and coarse state is not `IDLE`, `OFFLINE`, or `FAILED`. Missing, unknown, terminal, or inactive native state leaves the warning informational and disables every recovery action. Closing a dialog never clears printer state.

While an action is being submitted, all actions are disabled to prevent duplicate commands. A successful HTTP response means only that the live command was sent; the dialog closes, but the inline warning remains until `print_error: 0` arrives. A synchronous failure keeps the dialog open and restores the controls. A later Agent command-result failure uses the existing command-result toast and leaves the inline warning available for retry.

### Native actions

The printer family is the first three characters of `serial_number`, compared as uppercase ASCII. The current printer is in the `20P` action family, whose native Studio action order is:

1. Problem solved and resume (`resume`);
2. Ignore this and resume (`ignore`);
3. Stop printing (`stop`).

Studio's action catalog and runtime guard are applied independently:

- The Studio runtime catalog for `05008051` contains Resume, Ignore, and Stop in that order for families `093`, `094`, `20P`, `22E`, `239`, and `31B`. `26A` and unknown families have no reference-backed actions for this error.
- Studio calls `check_resume_condition()` for every HMS Resume and Ignore regardless of family. Web therefore shows either action only when the persisted `job_state` is present and `<= 1`; missing state or `> 1` disables it and directs the operator to the printer.
- Studio does not apply that guard to HMS Stop. Stop remains available wherever the catalog contains action `5`, including `31B`, independent of `job_state`.

Thus the current `20P` printer shows all three actions for an ordinary FDM job state `0` or `1`, and only Stop when the current job is safety-sensitive or its job state is unknown. The Hub enforces the same catalog-plus-job-state intersection; hiding buttons is not the safety boundary. Stop uses destructive styling, and the error dialog itself is the confirmation surface.

Studio loads its action catalog from the user-data cache, seeds a missing cache from packaged `resources/hms`, and then replaces it when `GetActionImage.php` returns a newer version. The packaged `094` and `239` files in this checkout are older baselines that omit Stop, while the newer official-endpoint snapshot checked into bambuddy contains `[28, 27, 5]` for all six supported families. This focused implementation uses that newer runtime catalog for `05008051`; it does not add Studio's general remote HMS catalog updater.

All three are native `HandlePrintError` actions. They must never be translated to the ordinary queued Resume or Stop operations.

## Tenant Recovery API

Keep the existing endpoint and Operator authorization:

```text
POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/controls
```

The Web submits only semantic intent:

```json
{
  "action": "handle_print_error",
  "error_action": "resume",
  "error_generation": 9
}
```

`error_action` accepts only `resume`, `ignore`, or `stop`. `error_generation` is an optimistic semantic occurrence token, not a printer transport field. The tenant route rejects client-supplied `print_error`, `printer_job_id`, `job_attr`, `job_state`, `task_generation`, or `sequence_id` for this action.

The gRPC connection setup persists its opaque current `SessionToken` on the Agent row when the session becomes online and clears it only when that exact session becomes offline. Each print report is passed the authenticated token that delivered it; an explicit error stores that token and a Hub-generated receive timestamp on the locked printer row. Agent/device clocks do not participate in the recovery decision. There is deliberately no arbitrary 30-second error-age cutoff: Studio permits the operator to consider a paused mismatch for longer than that, while Pandar's existing current-session/heartbeat expiry is the connectivity boundary.

After authorization the route acquires the stable current-session transition lease and requires `HandlePrintErrorSequenceZeroPubackOnly`, then invokes one ownership-checked transaction while retaining that lease through live enqueue. SQLite uses an immediate transaction; PostgreSQL locks the Agent row and then the printer row in the same order as print-report merging. Under that transaction it revalidates all of the following:

- the current error must be exactly `83918929`; a missing, cleared, or different error returns `printer_operation_unavailable`;
- the stored `error_generation` must equal the submitted generation and must belong to the current task generation;
- the error must have a Hub-received timestamp and have been explicitly received in the exact Agent session supplied by the route, and the locked Agent row must still be online with that same current session ID;
- persisted native `gcode_state` must be exactly one of `PREPARE`, `SLICING`, `RUNNING`, or `PAUSE`; missing, unknown, `IDLE`, `FINISH`, or `FAILED` state is unavailable, and coarse `IDLE`, `OFFLINE`, or `FAILED` is an additional veto;
- the requested action must be in the current serial family's Studio catalog; Resume/Ignore additionally require current derived `job_state <= 1`, while Stop does not;
- the current positive `print_error` becomes the native `err` value;
- the current `printer_job_id` is preserved exactly, including an empty string; unknown becomes the same empty string Studio uses as its default;
- Hub uses sequence ID `0`. This is disjoint from Studio's `[20000, 30000)` range and is corroborated by LAN HMS action traffic in `reference/bambuddy`. Single-flight dispatch below prevents two Web recovery operations for the same printer occurrence from sharing it concurrently.

The resulting printer payload stays byte-shape compatible with the implemented Studio path: `print.command` is `resume`, `ignore`, or `stop`; `err` and `job_id` use the locked values; `param` is `"reserve"`; and `sequence_id` is the string `"0"`.

The checked-in references disagree on firmware interoperability. Bambu Studio's direct `command_hms_resume/ignore/stop` code constructs the `err`/`job_id`/`param: "reserve"` shape, while bambuddy documents a live H2D on which that shape was silently rejected and therefore sends a plain command. The user explicitly requested Studio's direct behavior, and Pandar's existing Studio path already implements that shape, so this scope keeps it and adds no inferred per-model fallback. `sent` means dispatched, not physically accepted: the persistent warning remains until the printer explicitly clears the error, making a silent firmware rejection visible rather than reporting resolution.

The transaction constructs the native operation from those locked values and persists a `sent` command and audit entry. The operation then uses the existing live-only dispatcher with the leased session token. Local replacement cannot interleave between validation and enqueue; dispatch still rechecks the exact token and capability. It never wakes or enters the durable outbound queue. The Studio plugin route continues to require the original capability, preserve the sequence and fields supplied by Studio, and rely on Studio's own `jobState_` guard.

Under the same printer-row lock, the transaction queries that printer's `sent` and `acknowledged` `printer_operation` commands and deserializes their typed `PrinterOperationPayload`. Any in-flight `HandlePrintError` blocks another recovery for that printer, which is stronger than occurrence-only exclusion and prevents concurrent cloud-sequence-0 use. A terminal command no longer occupies the slot; if the printer still reports the same generation, the operator may deliberately retry. The plugin and Web persistence paths share this check, so Studio-originated and Web-originated native recovery cannot overlap. Because a native target job-ID change advances `error_generation`, a stale browser generation cannot authorize the newly targeted command.

Sequence `0` is reusable and cannot distinguish two attempts of the same action. `AsyncClient::publish()` alone only enqueues work for rumqttc's EventLoop, and the existing reusable command connection may contain an older queued or inflight QoS1 request. Sequence-zero recovery therefore never uses that connection. `RuntimeBambuMachineGateway` recognizes this exact operation, clones the authenticated printer endpoint, and creates a recovery-only rumqttc client/EventLoop with a unique per-attempt MQTT client ID. It does not subscribe to the report topic and queues exactly one QoS1 PUBLISH.

One five-second deadline covers topic-identity resolution, MQTT connect, enqueue, EventLoop polling, and confirmation. Because the new connection has no prior requests, its single `Outgoing::Publish(packet_id)` belongs to this attempt; the primitive waits only for the matching `Incoming::PubAck(packet_id)`. It ignores application-level MQTT PUBLISH packets while doing so and returns `dispatched` only after PUBACK. The client and EventLoop are dropped on success, cancellation, timeout, queue/poll/connection failure, or protocol error, so an unacknowledged request cannot be replayed or observed by a retry. Each retry creates another unique clean connection, making packet IDs and delayed PUBACKs connection-scoped. Every failure preserves its complete cause chain. PUBACK proves delivery to the printer's MQTT broker, not that firmware accepted the recovery semantics.

On an Agent advertising `HandlePrintErrorSequenceZeroPubackOnly`, sequence-zero `HandlePrintError` uses only that primitive. It never calls `next_report`, registers a reusable application sequence, or decodes an application response as the result. Any later sequence-zero response is unrelated telemetry and can neither succeed nor fail the first operation or a retry; only a later printer state report can clear the persistent mismatch warning. The separate report-forwarding connection continues to deliver that state. Nonzero Studio-plugin operations retain the existing sequence-based application-result correlation.

This is the same `jobState_ > 1` guard used by Studio, not a printer-family inference. A forged Resume/Ignore for unknown or unsafe job state fails closed; a catalog-backed Stop remains independently available.

As with the existing Studio live-error route, pending ownership and cleanup are process-local. Session affinity is not sufficient for arbitrary failover: deployments that enable Web recovery must run exactly one active Hub until cross-replica live-command ownership exists. A non-owning process returns `printer_operation_unavailable`.

The shared dispatcher is renamed/generalized away from plugin-only terminology, but its ownership, capability, session-token, cleanup, and failure semantics do not change.

### Errors and authorization

- Viewer: `403 role_forbidden`.
- Operator or tenant admin: allowed.
- Missing printer: `404 printer_not_found`.
- Invalid, missing, or extra fields: `400 invalid_printer_control`.
- Wrong/cleared/stale generation or error, inactive printer, disallowed family/action, duplicate in-flight recovery, offline/incapable/mismatched Agent session, ownership race, or live-send failure: `400 printer_operation_unavailable`.
- Success: existing command response with status `sent`, never `queued`.

The frontend uses a dedicated server action with `useActionState`; it does not reuse the redirecting `controlPrinter` action or its misleading `printer_control_queued` message. When a successful `command_result` has a typed `PrinterOperationPayload::HandlePrintError`, the Web shows a dedicated translated message, "Recovery command sent; waiting for printer status confirmation" / "恢复指令已发送，等待打印机状态确认", rather than the generic "Printer control completed" toast. A failed result keeps the existing failure toast and full error. The inline mismatch warning remains until explicit printer clear in either case.

## Safety and Data Boundaries

- REST and WebSocket shapes stay typed; no arbitrary JSON or raw MQTT control is exposed.
- The server, not hidden form fields, selects the current error code, printer job ID, and sequence ID.
- Existing tenant isolation, Operator role checks, Agent capability checks, exact-session dispatch, audit logging, and access-code redaction remain authoritative.
- Resume/Ignore authorization derives only from typed `job_attr` using Studio's exact bit extraction; printer family is never used as a substitute for job safety state.
- The UI never interprets HMS as a plate mismatch and never infers an error from paused state.
- Server-side generation/error/job/family/session revalidation and database single-flight remain authoritative across multiple browsers on the required single active Hub; browser pending state is only interaction feedback.
- No physical fault is induced and no real Resume, Ignore, or Stop action is clicked during automated or visual verification.

## Testing

Tests are written before implementation at each boundary.

### Hub and repositories

- tenant printer list and detail return the nested print state without host/access code;
- full `printer_snapshot` serialization carries the same nested print state;
- an uncorrelated print report whose live state changes publishes an enriched `printer_snapshot` with current materials;
- same-task partial reports preserve earlier fields, while explicit zero/empty values overwrite;
- any jointly comparable task/subtask/file conflict and boundary-to-active transition increments `task_generation` and resets absent task fields; equal-plus-new-field enrichment preserves them;
- cross-slot-only ambiguity preserves display state but clears recovery authorization, `printer_job_id`, and raw `job_attr` before merging; tests cover old subtask plus new task/subtask, equal task plus changed file, no-common-slot enrichment, sentinel/absent values, reused IDs after terminal state, and an explicit positive ambiguous report that omits both recovery fields;
- explicit `IDLE` clears task-scoped fields, while `FINISH` retains the finished view and coarse inactive status suppresses stale live display;
- `job_attr` presence/zero/nonzero/absence survives Agent protobuf and partial merge, derives the exact four-bit `job_state`, and clears on task/IDLE boundaries;
- positive/clear/same-positive ABA, different positive codes, task transitions, native target job changes, repeated reports, and same-error re-observation after reconnect prove exact `error_generation` behavior;
- a partial report after reconnect cannot refresh the prior session's error marker; an explicit same-error report from a new session first clears omitted `printer_job_id` and raw `job_attr`, so Resume/Ignore remain unavailable and Stop cannot reuse the old job ID; Agent clock skew does not affect the Hub-received marker;
- Agent session A/B replacement is linearized across database and registry; deterministic interleavings prove old A heartbeat/snapshot/print/material events cannot commit after B becomes current and A disconnect cannot clear B's persisted session ID;
- `state_revision` advances atomically for every accepted non-material printer-row mutation, including last-seen-only reports; last-seen-only reports still publish no snapshot;
- a new printer's first committed/public revision is `1`; an unknown buffered event triggers authoritative resync, adds a newly committed printer, and discards an event for a still-absent/deleted printer;
- SQLite/PostgreSQL migrations enforce `state_revision >= 1` with database default `1`; a direct legacy-style printer insert that omits every new column receives revision `1` on both backends, while task/error generations default to `0` and old positive errors remain unrecoverable until explicit current-session observation;
- SQLite behavior remains covered and the existing configured PostgreSQL live-status exercise continues to pass.

### Recovery route

- catalog-backed Resume, Ignore, and Stop each persist and live-dispatch protobuf tag 25 with the current error and job ID;
- Web recovery uses sequence ID `0` in the command, audit, and MQTT request; it waits only for the matching QoS1 PUBACK and never correlates an application MQTT result, while the plugin route still preserves Studio's supplied nonzero sequence and existing correlation;
- `printer_job_id == ""` is retained;
- client-supplied transport/task fields, unknown actions, missing/extra occurrence fields, and cross-operation fields are rejected;
- Viewer, wrong tenant, missing printer, non-mismatch/cleared/stale generation, missing/unknown/terminal/inactive native state, coarse-state veto, catalog miss, Resume/Ignore with absent or `>1` job state, offline Agent, missing sequence-zero-PUBACK-only capability, session mismatch, ownership/state race, duplicate in-flight command, and dispatch failure preserve the stable errors and never queue the operation;
- session replacement before the recovery lease rejects the request; replacement waits while the lease validates/persists/enqueues, so the command can only enter the exact marked session; partial reports and skewed Agent `observed_at` never refresh authorization;
- concurrent SQLite and configured PostgreSQL requests prove that only one conflicting Web recovery is inserted and sent, plugin/Web overlap is blocked, and a terminal native recovery permits an intentional retry;
- transport tests model rumqttc's enqueue-without-poll behavior and prove each Web attempt uses a unique clean client ID/connection, drives only its EventLoop through its single matching QoS1 PUBACK within the end-to-end five-second deadline, and drops the connection on every terminal path. Fixtures cover an older queued/unacknowledged generic publish on the reusable connection, reconnect replay, timeout then retry, delayed old PUBACK, and application PUBLISH packets before/after PUBACK; none is identified as the recovery publish, retransmitted by a later attempt, or allowed to complete/fail either operation;
- a new Agent advertises both capabilities; new Hub + old Agent rejects Web recovery, while old Hub + new Agent ignores the additive field/capability and Studio plugin behavior remains available;
- the Studio plugin route still preserves its supplied sequence unchanged and requires only the original capability.

### Frontend

- active, preparing, finished, idle, and partially populated print states render correctly in English and Chinese;
- enriched WebSocket snapshots update print state without removing materials or temperatures;
- every socket open/reconnect and serialized 30-second reconciliation buffers events, unconditionally replaces the previous browser generation from REST, replays only greater revisions, and treats a missed clear/reappear cycle as a new generation;
- a lagged tenant event receiver closes its WebSocket; printer-event publish failure, control-plane receive error, or subscriber EOF advances the internal epoch and closes every local printer WebSocket, forcing the same authoritative reconnect flow without a new public event type; EOF/subscription failure also retries subscription;
- REST/WS race tests cover whole-inventory replacement/removal, unknown-event resync for both newly created and deleted printers, clear, clear/reappear, lower/equal/higher revisions, refresh failure, coalesced interval/triggered refreshes, exact active-page 30-second start cadence, an ordinary cycle capped at 10 seconds through body decode/apply, an unknown-printer two-fetch cycle capped at 20 seconds including a stuck second fetch, the active-page 40-second algorithmic silently-lost-state bound, post-decode deadline rejection, immediate `visibilitychange`/`pageshow` reconciliation after simulated suspension, stale-socket callbacks, independent material ordering, a final clear/IDLE lost to tenant broadcast lag, publish failure, and control-plane error/EOF;
- the mismatch dialog auto-opens once per server error generation, stays dismissed across reconnect for an unchanged generation, can be reopened from the inline warning, closes on clear/different error, and handles more than one affected printer deterministically;
- action ordering matches each catalog; Resume/Ignore react to `job_state <= 1`, Stop remains available without that guard where cataloged, and unsupported combinations show printer-only guidance;
- pending state prevents duplicates; synchronous failure retains the dialog; successful native command results use the dedicated "sent, waiting for printer status" translation instead of "completed" and retain the inline warning until clear;
- the server action sends only semantic action plus the occurrence generation and reports `sent`, not `queued`.

### Compatibility

- an existing Web event consumer continues to receive only the three known event discriminators and safely ignores the additive `state_revision` and `print` fields;
- the Android `PrinterEventsDecoderTest` includes an enriched `printer_snapshot` and proves `ignoreUnknownKeys` preserves decoding without a new sealed variant;
- new Rust control-plane decoding accepts legacy events with both fields absent, and an old-shape fixture accepts and drops enriched fields, proving bidirectional rolling decode;
- the new Web tolerates legacy REST/events by clearing the live monitor and disabling recovery, which keeps already-loaded tabs safe during rollback;
- mixed-replica tests publish legacy and enriched snapshots through the control plane without decode failure or duplicate local delivery.

### Verification

- `cargo fmt --all -- --check`;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
- `cargo nextest run --manifest-path Cargo.toml --workspace`;
- targeted PostgreSQL live-status/recovery tests with `PANDAR_TEST_POSTGRES_URL`;
- frontend lint, typecheck, tests, and production build using the repository package scripts;
- a local Hub/Agent/Web smoke check that observes live progress updates and a fixture-driven mismatch prompt without issuing a real printer recovery action.

## Documentation, Deployment, and Rollback

Update `docs/roadmap.md`, `docs/development.md`, `docs/architecture.md`, and `docs/android.md` with the enriched printer response/snapshot contract, Agent protocol/capability addition, state revisions and generations, future-only WebSocket behavior, browser reconnect refresh, single-active-Hub recovery restriction, live monitor, and native recovery UI. The schema migration is additive on both database backends; no credential format change.

The event discriminator and existing capability remain unchanged, and the new protobuf field/capability are additive. Deploy the database migration, then Agents that advertise both the old capability and sequence-zero-PUBACK-only capability, then every Hub replica; an old Hub ignores the new field/capability and keeps Studio plugin support, while a new Hub fails Web recovery closed for an old Agent. Old Web and Android clients ignore enriched printer fields. Old Hubs do not maintain the new revision, so mixed-version decode safety is not feature readiness. Confirm every Hub populates the enriched response, revisions are active, and the target Agent reports the sequence-zero-PUBACK-only capability before deploying the new Web. Web recovery is enabled only where exactly one Hub is active.

Rollback starts by withdrawing the new Web release and its recovery server action; already-loaded new tabs tolerate missing fields and disable recovery after legacy REST/events. If their same-origin refresh proxy is no longer present, they enter `unavailable` and discard buffered/stale recovery state. Before any Hub rollback, wait until every typed `HandlePrintError` command is terminal (`succeeded` or `failed`) and no local live command remains pending, so no process-local owner is abandoned. Then roll back Hubs. The additive columns remain in place. The dual-capability Agent is backward-compatible and may remain deployed; if it is also rolled back, do so last, after no Web recovery is possible. The network plugin needs no rollback.

## Non-Goals

- a general Bambu HMS/error action catalog;
- inferred recovery actions for unknown printer errors;
- changing the Studio plugin error dialog or plugin API;
- redesigning unrelated printer controls, Jobs, temperatures, AMS, or camera UI;
- exposing arbitrary MQTT payloads;
- inducing a mismatch or automatically clicking a physical recovery action.

## Acceptance Criteria

1. A Studio-originated print with no Pandar Job updates task name, percentage, layers, and remaining time in the Web without a page refresh.
2. Idle devices do not show stale details from the prior print.
3. `print_error == 83918929` auto-opens one mismatch dialog and leaves a persistent inline warning.
4. The current `20P` printer shows Resume, Ignore, and Stop in native order when Studio-derived `job_state <= 1`; an unsafe/unknown state retains only its cataloged Stop action.
5. Catalog misses offer no remote action; Resume/Ignore are guarded by current `job_attr` exactly as in Studio, while cataloged Stop is independent. All actions require native `PREPARE`, `SLICING`, `RUNNING`, or `PAUSE`; missing/unknown/terminal state and coarse inactive state fail closed. Server-side catalog/job/session/state checks reject forged or stale submissions.
6. Each allowed action uses the current server-side error state and sequence-zero-PUBACK-only live `HandlePrintError`; it is never durably queued, succeeds only after a bounded matching QoS1 PUBACK, and no application-level sequence-zero response—including a delayed duplicate from the same action—is correlated to any Web attempt.
7. A printer has at most one sent/acknowledged native error recovery across concurrent Web clients and the Studio plugin; terminal commands permit deliberate retry.
8. An explicit printer error clear removes both dialog and inline warning; repeated unchanged frames do not reopen it. While the page scheduler is active and REST/decode/apply meets its deadline, reconnect resynchronization, server-side event-loss invalidation, and bounded reconciliation repair a silently missed mismatch/clear/IDLE within the algorithmic 40-second bound; a baseline deadline clears the enriched view and marks it unavailable, and browser resume/visibility restoration triggers immediate reconciliation rather than claiming a suspended wall-clock SLA.
9. REST, WebSocket, Hub route, frontend, Android compatibility, SQLite, and configured PostgreSQL verification pass without touching unrelated probe directories.
