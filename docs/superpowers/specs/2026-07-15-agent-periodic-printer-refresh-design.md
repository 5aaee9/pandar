# Agent Periodic Printer Refresh Design

## Context

Pandar currently keeps one MQTT report forwarder per configured Bambu printer while an Agent-to-Hub
session is active. The forwarder subscribes to device/{serial}/report, publishes one typed
pushing.command = "pushall" request after the subscription is established, and then forwards
unsolicited printer reports as normalized PrintJobReport, PrinterSnapshot, device-feature, and
material events.

The forwarder waits up to 10 seconds for each report. That timeout is only an idle receive timeout:
it logs the idle condition and waits again. It does not request a new printer snapshot. A non-idle
MQTT failure exits the forwarder, and the existing runtime owner invalidates transient feature state,
waits 5 seconds, then subscribes again and sends the existing initial pushall.

The dashboard's printer presence is deliberately separate from the Agent heartbeat. A printer is
shown as online only when its last_seen_at is less than three minutes old. Agent heartbeats prove
that the Agent-to-Hub session is alive; they do not prove that a LAN printer is reachable. An idle
printer can therefore appear stale when its MQTT session remains connected but it does not emit a
qualifying report for several minutes.

Bambu Studio provides two useful reference constraints. It rate-limits ordinary pushall requests to
no more than one every three seconds, and its selected-printer keep-alive requests a full status
after roughly five minutes. Pandar needs a shorter cadence because its UI intentionally changes from
Online to Last online after three minutes.

## Goal

Keep each active printer report subscription event-driven while also requesting a full printer
status every 60 seconds. The periodic request must use the existing typed RequestPushAll payload,
request topic, QoS, report parser, Hub event path, and error/retry ownership.

The periodic timer path must update printer presence only after a real MQTT report is received and
accepted by the existing normalization path. A timer tick alone must never synthesize a snapshot,
print report, or last_seen_at update.

## Non-goals

- Do not change the 15-second Agent-to-Hub heartbeat or use it as printer presence.
- Do not change the frontend's three-minute online threshold or display clock.
- Do not add a Hub scheduler, durable refresh command, database write, migration, or wire contract.
- Do not make the 60-second period configurable in this change.
- Do not replace the continuous MQTT subscription with polling.
- Do not run info.get_version on every periodic refresh; model discovery remains an explicit
  refresh/link concern.
- Do not change explicit RefreshPrinters or RefreshPrinterMaterials command behavior.
- Do not change the authoritative configured-printer snapshot sent at Agent session startup. That
  baseline currently advances Hub last_seen_at without a live printer report and is a separate
  presence-semantics concern; this task only forbids synthetic events from the new periodic tick.
- Do not add adaptive intervals, jitter, cross-client throttling, or a feature flag.
- Do not claim that a sent pushall proves the printer is online; only a qualifying response does.
- Do not require or perform a live-printer test during automated verification.

## Considered Approaches

### Periodic request inside the existing report forwarder - selected

Each per-printer report forwarder owns one Tokio interval in the same task that owns the subscription
and report receive loop. It sends the existing immediate pushall, schedules the first periodic tick
60 seconds later, and then selects between session closure, a periodic tick, and the next report.

This keeps subscription, timer, MQTT publishing, cancellation, and retry ownership in one task. It
adds no detached timer and cannot outlive report-task replacement or Agent session teardown.

### Refresh through a Hub scheduler - rejected

A Hub scheduler would create durable command and audit churn, depend on the Agent session dispatcher,
and put a LAN transport keep-alive policy in the wrong process. It would also complicate multi-Hub
ownership for behavior that belongs to one Agent-local MQTT session.

### Refresh printer presence from the Agent heartbeat - rejected

This would report a printer as online whenever its Agent is connected, even if the printer is powered
off, unreachable, has rejected the access code, or has a failed MQTT session.

### Send pushall only after an idle receive timeout - rejected

This is not a fixed refresh cadence. Frequent partial or feature-only reports could indefinitely
defer a full state request, while the timer must guarantee a bounded full-refresh attempt regardless
of unsolicited traffic.

## Runtime Behavior

### Subscription startup

The current ordering remains:

1. Subscribe to the resolved printer report topic.
2. Start the existing firmware report processor when applicable.
3. Publish one immediate RequestPushAll to the resolved request topic with QoS 1.
4. Start the periodic schedule with its first deadline 60 seconds after initialization.

There is no immediate interval tick in addition to the existing startup request.

### Fixed periodic schedule

The production interval is a non-configurable Duration::from_secs(60) constant. It belongs to the
report-forwarder generation, not to the last unsolicited report. Incoming printer reports do not
reset or postpone the next periodic request.

Use tokio::time::interval_at and set MissedTickBehavior::Delay. If the machine is suspended or the
task is delayed across multiple deadlines, the forwarder sends at most one catch-up request and then
waits a complete 60 seconds before the following request. It must not burst one request for every
missed minute or issue two near-neighbor requests after resuming.

Use a biased select ordered as sender closure, periodic tick, then next report. Sender closure is a
cooperative early-exit signal when the event channel closes independently; it is not the authoritative
session teardown mechanism. If closure and a tick are both observable at selection time, closure wins
and no new periodic request starts. Tick precedence also prevents continuously ready unsolicited
reports from starving a due refresh.

Runtime replacement and session teardown remain authoritative: they abort and then join the owned
report task. Aborting drops the interval and receive future and may cancel an MQTT publish that is
already in flight, so no detached periodic work survives teardown.

The scheduled request reuses:

- BambuMqttTopics::for_serial(...).request after normal topic-identity resolution;
- BambuMqttCommand::RequestPushAll.payload();
- BAMBU_MQTT_QOS;
- the existing non-retained MQTT publish behavior.

The periodic path does not send get_version, create a second MQTT client, or call the explicit Hub
refresh command.

### Report handling and presence

Unsolicited and pushall response reports use the same existing report-processing path. There is no
timer-specific event type or parser:

- qualifying non-firmware print telemetry may emit PrintJobReport;
- temperature or chamber-light telemetry may emit PrinterSnapshot;
- device-feature and material reports keep their current specialized event behavior;
- malformed, firmware-only, or non-qualifying reports do not gain new presence semantics.

For the new periodic path, Hub continues to update last_seen_at only when its existing snapshot or
print-report repository path accepts a real Agent event. If a printer ignores every scheduled request,
the timer emits no event and the dashboard continues to age the previous timestamp. The existing
session-start configured-printer baseline remains unchanged as stated under non-goals.

### Errors and retry

An MQTT idle receive timeout retains its current behavior: log the complete timeout cause and keep the
same subscription and fixed schedule alive.

A periodic publish failure returns from the direct forwarder with both periodic-refresh context and
the lower-level MQTT publish cause. Errors must not be swallowed or reduced to their outer display
string.

The production forward_print_reports_with_firmware_retry wrapper remains authoritative. It starts a
new firmware generation and emits the existing firmware invalidation, invalidates and emits transient
device-feature state, waits its existing 5-second retry delay, and then re-enters the forwarder. The
new generation subscribes again, starts the firmware processor's get_version observation, sends the
existing immediate pushall, and starts a fresh 60-second interval.

Explicit refresh commands use their existing command transport and may coincide with a scheduled
request. This design adds no shared cross-client throttle. A 60-second scheduled cadence remains far
above Bambu Studio's three-second ordinary-request floor, and coordinating all independent transports
would introduce broader ownership for no required behavior.

## Ownership and Concurrency

There remains exactly one runtime report task per configured printer serial in the existing
report_tasks map. The periodic interval is a local value inside that task:

- replacing, unlinking, and Agent session teardown abort and then join owned report tasks, dropping
  their intervals;
- session teardown does not rely on sender closure and joins report tasks before clearing the stored
  sender;
- sender.closed() is only an auxiliary cooperative stop when the event channel closes independently;
- aborting the task may cancel an in-flight publish before the transport enqueue completes;
- a non-idle failure exits into the existing retry loop instead of spawning another timer;
- report parsing and scheduled publishing are serialized by one task;
- command and report MQTT clients retain their current separate client IDs.

No new lock, shared mutable schedule, background supervisor, or Hub-side ownership is introduced.

## Module Boundaries

crates/pandar-agent/src/machine/mqtt/reports.rs is already 380 lines, close to the repository's
400-line production limit. The implementation must not append the timer loop in place.

Move the existing forwarding functions and temperature-snapshot predicate into
crates/pandar-agent/src/machine/mqtt/reports/forwarding.rs. Keep report decoding, normalized value
construction, and event constructors in reports.rs, and re-export the same forwarding functions so
callers retain their current API. Put deterministic timer tests in
crates/pandar-agent/src/machine/mqtt/reports/forwarding/tests.rs.

Use a test-only controlled transport in the new test module rather than growing the existing
357-line FakeMqttTransport or the 1,300-line aggregate MQTT test module. Enable Tokio's test-util
feature only for pandar-agent tests so paused-time tests exercise the real 60-second production
constant without changing production features.

Every touched production Rust module must remain at or below 400 lines. Do not use include!.

## Tests and Acceptance Criteria

Deterministic tests must prove all of the following without a live broker:

1. Startup publishes exactly one immediate pushall with the existing topic, payload, and QoS, and the
   production constant is exactly 60 seconds.
2. No second request is published before the 60-second deadline.
3. The deadline publishes exactly one additional pushall even when no report arrives.
4. A qualifying unsolicited report is forwarded immediately before the deadline and does not reset
   the fixed deadline.
5. Advancing across multiple missed deadlines produces one catch-up request, not a burst; the next
   request waits a complete 60 seconds.
6. When sender closure and a periodic tick are simultaneously ready, the closure branch wins and no
   periodic publish starts.
7. When the periodic tick and a continuously ready report source are simultaneously ready, the
   periodic branch wins; report traffic cannot starve refreshes.
8. A successful periodic publish with no printer response emits neither PrinterSnapshot nor
   PrintJobReport and cannot itself advance Hub presence. Firmware or device-feature invalidations
   caused by a separate failure remain valid non-presence events.
9. Closing the Hub event receiver cooperatively stops the forwarder and prevents a later scheduled
   request, independently of runtime abort-and-join teardown tests.
10. A direct forwarder whose second, periodic publish fails returns an error chain containing the
    periodic request-topic context and the lower-level transport cause.
11. The production forward_print_reports_with_firmware_retry test covers firmware and device-feature
    invalidations, the five-second delay, re-subscription, get_version observation startup, and the
    new generation's immediate pushall after a periodic publish failure.
12. Existing report, firmware observation, device-feature, material, retry, and session-lifecycle
    tests remain green.

Required verification:

- new deterministic forwarding timer tests;
- existing MQTT report and runtime retry/lifecycle tests;
- all pandar-agent tests;
- cargo fmt --all -- --check;
- cargo clippy --workspace --all-targets -- -D warnings;
- the production module-size test;
- cargo nextest run --manifest-path Cargo.toml --workspace.

No frontend or database behavior changes, so no new frontend or PostgreSQL-specific test is required.
The full workspace command remains mandatory.

## Documentation

Update docs/architecture.md to distinguish the 30-second MQTT protocol keep-alive, 10-second idle
receive timeout, 15-second Agent heartbeat, and new 60-second printer full-status request. Update
docs/roadmap.md with the completed Agent behavior and its real-response-only periodic presence
semantics.

Maintain .superpowers/sdd/progress.md as the local review and verification ledger. It remains an
ignored workflow artifact and is not part of the delivery commit.

## Operational Cost and Safety

The steady-state additional load is one QoS-1 pushall request and its printer-generated full report
per configured printer per minute. Reports remain bounded by the existing 256 KiB MQTT packet limit.
There is no Hub command row per tick. Delayed missed-tick handling prevents resume bursts.

The 60-second period is chosen against the 180-second UI threshold: after a qualifying response it
normally provides refresh opportunities at 60 and 120 seconds before the UI becomes stale, plus a
third request at the threshold boundary. This tolerates two missed responses without pretending that
a request guarantees reachability; continued missing or slow responses still let the UI become stale.

The cost is 60 scheduled requests and at most 60 corresponding full reports per configured printer
per hour. The period is twenty times Bambu Studio's three-second ordinary-request floor. It is more
frequent than Studio's roughly five-minute selected-printer keep-alive because Pandar has a tighter
three-minute product presence threshold.

The rollout is Agent-only and backward-compatible with existing Hub, frontend, database, and protobuf
versions. During a rolling restart, older Agents retain event-only behavior and updated Agents add the
periodic request.

## Rollback

No migration or durable contract is added. Revert the delivery commit and restart the Agent to return
to startup-only pushall behavior. Hub and frontend require no rollback. Existing explicit refresh
commands remain available throughout rollout and rollback.
