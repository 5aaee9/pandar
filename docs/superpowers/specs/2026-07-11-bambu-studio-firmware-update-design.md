# Bambu Studio Native Firmware Update Design

## Goal

Make Bambu Studio's native Firmware page work through Pandar for both the printer's main firmware
and every AMS-family module reported by the printer, including AMS, AMS 2 Pro, and AMS-HT. Studio
must see real firmware versions and upgrade state, send its native firmware commands without Pandar
inventing policy, and receive the printer's matching acknowledgement and progress reports.

Pandar is a typed, session-bound transport in this design. The printer remains authoritative for
which firmware is available and whether it supports a command. Pandar does not manufacture package
URLs, versions, release notes, success, or completion.

## Scope

In scope:

- Capture all modules from the printer's `info.get_version` report, including `ota`, `ams/*`,
  `n3f/*`, `n3s/*`, and future module names without dropping them.
- Capture the known `print.upgrade_state` fields and the AMS firmware-switch state while preserving
  absent-field semantics across partial MQTT reports.
- Propagate firmware telemetry through Agent, protobuf, Hub persistence, and the network plugin.
- Replace the plugin's hardcoded `get_version` report and empty
  `bambu_network_get_printer_firmware` response with Rust-built payloads derived from real printer
  telemetry.
- Satisfy each Studio page/post-finish `get_version` request with a bounded fresh printer query.
- Parse and pass through Studio's `upgrade_confirm`, `consistency_confirm`, explicit `start`, and
  `mc_for_ams_firmware_upgrade` commands.
- Preserve Studio's exact firmware-command `sequence_id` through the whole path.
- Deliver firmware commands only to the exact current capable Agent session, without queued replay
  after reconnect or process restart.
- Return a matching printer acknowledgement quickly enough for Studio's AMS firmware selector and
  forward ongoing `upgrade_state` progress through the normal Studio status stream.
- Add deterministic protocol, dual-backend, lifecycle, plugin HTTP, and compiled ABI coverage.

Out of scope:

- Web or Android firmware controls, package browsing, and remote OTA orchestration. Those are the
  separately deferred C scope.
- Downloading, uploading, mirroring, caching, or hosting firmware packages in Pandar.
- Bambu account credentials or calls to authenticated Bambu cloud firmware APIs.
- Scraping the public download page. The public version-history pages are readable, but the package
  catalog is Cloudflare-protected and is not a reliable unauthenticated package source.
- Fabricated package URLs, release notes, version ordering, or module-name allowlists.
- A generic raw MQTT or arbitrary JSON tunnel.
- Live flashing or any real-printer firmware command during automated verification.
- Cross-replica forwarding of the existing process-local Agent session and live secret/result
  ownership; this release retains the documented one-active-Hub deployment restriction.

## Reference Behavior

Bambu Studio refreshes the Firmware page by sending `info.command = "get_version"` and calling the
network plugin's `bambu_network_get_printer_firmware` ABI
(`reference/BambuStudio/src/slic3r/GUI/UpgradePanel.cpp:1656-1662`). The ABI response is parsed as:

```json
{
  "devices": [{
    "dev_id": "<serial>",
    "firmware": [{"version": "...", "url": "...", "description": "..."}],
    "ams": [{"firmware": [{"version": "...", "url": "...", "description": "..."}]}]
  }]
}
```

Studio marks that catalog request valid even when both arrays are empty. It only adds catalog
entries whose URL contains a filename
(`reference/BambuStudio/src/slic3r/GUI/DeviceManager.cpp:3979-4054`). Therefore Pandar returns the
exact envelope and only emits a catalog entry when it has a real non-empty URL. Current versions and
printer-advertised new versions come from printer telemetry, not invented catalog records.

Studio's `get_version` parser also retains `hw_ver` and `flag`, uses the flag bits for beta labels,
and treats `visible` plus `new_ver` as an alternate new-version encoding
(`reference/BambuStudio/src/slic3r/GUI/DeviceManager.cpp:2700-2721`). Pandar carries those fields
without adding its own visibility or beta policy.

Studio's normal update button sends:

```json
{"upgrade":{"command":"upgrade_confirm","sequence_id":"...","src_id":1}}
```

Consistency repair sends `consistency_confirm`. A retry or explicit catalog entry sends:

```json
{"upgrade":{"command":"start","sequence_id":"...","url":"...","module":"ota|ams","version":"...","src_id":1}}
```

These payloads are constructed in
`reference/BambuStudio/src/slic3r/GUI/DeviceCore/DevUpgradeCtrl.cpp:8-53`. The normal page path uses
`upgrade_confirm`; an explicit `start` therefore remains useful when Studio already has a real URL,
but Pandar does not need to find or create one.

The AMS firmware selector sends:

```json
{"upgrade":{"command":"mc_for_ams_firmware_upgrade","sequence_id":"...","src_id":1,"id":2}}
```

and waits for sequence-correlated data on a short control deadline
(`reference/BambuStudio/src/slic3r/GUI/DeviceCore/DevFilaAmsSettingCtrl.cpp:7-27`). Studio parses the
result under `print.upgrade_state.mc_for_ams_firmware`, including `firmware[]`,
`current_firmware_id`, `current_run_firmware_id`, and `status`
(`reference/BambuStudio/src/slic3r/GUI/DeviceCore/DevFilaAmsSetting.cpp:28-65`).

Studio parses the following general upgrade fields from `print.upgrade_state`:

- `status`, `progress`, `message`, `module`, and `err_code`;
- `new_version_state`, `consistency_request`, `force_upgrade`, and `dis_state`;
- `ota_new_version_number`;
- `new_ver_list[]` entries containing `name`, `cur_ver`, and `new_ver`;
- nested `mc_for_ams_firmware` state.

The reference implementation maps download, pre-flash, flash, success, and failure statuses into the
native UI and requests `get_version` again after completion
(`reference/BambuStudio/src/slic3r/GUI/DeviceCore/DevUpgrade.cpp:22-105`). Pandar forwards these
values; it does not synthesize the state machine.

Studio stores `progress` as a string, consumes the legacy `ams_new_version_number` and
`ahb_new_version_number` fields, and uses bit 2 of `print.cfg` as a force-upgrade compatibility
signal (`reference/BambuStudio/src/slic3r/GUI/DeviceCore/DevUpgrade.h:55-65` and
`reference/BambuStudio/src/slic3r/GUI/DeviceManager.cpp:3316-3327`). Pandar preserves those exact
wire shapes.

`reference/open-bamboo-networking/src/agent.cpp:737-826` demonstrates the important telemetry
sources without depending on Bambu cloud: the full `info.get_version.module[]` list and
`print.upgrade_state.new_ver_list[]`. Pandar follows those sources with typed Rust models. It does
not copy that project's synthetic descriptions or empty-URL catalog entries.

## Considered Approaches

### Selected: typed telemetry plus typed live firmware commands

Agent parses the known printer envelopes, keeps a merge-safe per-printer cache, and sends typed
snapshots to Hub. Plugin commands become a closed typed firmware-command enum and are delivered to
the exact current capable Agent. This preserves protocol behavior, lets Hub enforce tenancy and
session ownership, and keeps opaque package URLs out of durable state.

### Rejected: raw Studio JSON tunnel

Passing arbitrary JSON from the plugin to MQTT would be smaller, but it would bypass the existing
typed operation boundary, make capability/lifecycle auditing impossible, and create a general raw
printer-control surface. Only the four observed firmware command shapes are accepted.

### Deferred: Pandar-managed package staging

Downloading a package and uploading it to the printer could eventually support a Web/Android OTA
experience, but it is a different product path. It requires a trustworthy package source,
compatibility and integrity metadata, storage lifecycle, and explicit operator UX. It is not needed
for Studio's native `upgrade_confirm` flow and belongs to C, not B.

## Typed Firmware Model

Add shared core types for telemetry that has stable meaning:

```rust
pub struct PrinterFirmwareModule {
    pub name: String,
    pub software_version: Option<String>,
    pub software_new_version: Option<String>,
    pub new_version: Option<String>,
    pub visible: Option<bool>,
    pub product_name: Option<String>,
    pub serial_number: Option<String>,
    pub hardware_version: Option<String>,
    pub firmware_flag: Option<i32>,
}

pub struct PrinterUpgradeState {
    pub status: Option<String>,
    pub progress: Option<String>,
    pub message: Option<String>,
    pub module: Option<String>,
    pub error_code: Option<i64>,
    pub new_version_state: Option<i32>,
    pub consistency_request: Option<bool>,
    pub force_upgrade: Option<bool>,
    pub display_state: Option<i32>,
    pub ota_new_version_number: Option<String>,
    pub ams_new_version_number: Option<String>,
    pub ahb_new_version_number: Option<String>,
    pub new_versions: Option<Vec<PrinterFirmwareVersion>>,
    pub ams_firmware: Option<AmsFirmwareSwitchState>,
}
```

The exact Rust field types are finalized against reference fixtures during implementation, but the
wire rules are fixed:

- module names are opaque non-empty strings; Pandar never restricts the cache to today's known
  prefixes;
- known scalar values use typed fields, not manual `serde_json::Value` extraction;
- `hw_ver` and `flag` are retained because Studio consumes the hardware version record and uses the
  flag bits for current/new beta labels;
- `sw_new_ver`, `visible`, and `new_ver` remain distinct typed fields and are re-emitted under their
  original keys. Studio itself applies `visible` plus `new_ver` as its alternate effective
  new-version form; Pandar adds no visibility policy;
- the AMS firmware list is typed as `id`, `name`, and `version` records;
- optional collection presence and the push-status envelope kind are retained. Agent first mirrors
  Studio's JSON preprocessing: a `push_status` delta (`msg = 1`) merges into the prior reconstructed
  printer object, while a full/base-reset report (`msg = 0` or legacy no-`msg`) replaces that object.
  The extracted typed state therefore preserves a delta-absent `new_ver_list`, but represents a
  genuinely absent list in a full report as absent; Studio then performs its own scalar-preserve and
  absent-list-clear behavior. A present empty collection always stays present and empty;
- unknown fields may be retained only in a bounded, open-ended pass-through map if a real Studio
  fixture proves they are needed; known fields remain typed;
- malformed firmware fields are isolated to firmware telemetry and do not discard valid sibling
  printer state from the same report;
- strings are size-bounded at the Agent MQTT boundary and plugin HTTP boundary using the existing
  request/report limits; no extra product policy is added.

The Agent cache is keyed by printer serial and contains:

- the latest ordered `info.get_version` module list;
- the presence-preserving merged `PrinterUpgradeState`;
- the latest printer `print.cfg` string used by Studio's force-upgrade compatibility path;
- an observation generation tied to the active printer report stream;
- independent monotonic module and status revisions within that generation.

Each report-stream start allocates the next per-serial process generation under a transition lease,
emits invalidation for that generation, and only then permits snapshots or firmware commands bound
to it. Endpoint replacement and reconnect take the same lease. An old report producer or in-flight
command may finish after a reconnect, but its old generation is carried on every event/result and
cannot mutate the new generation.

All startup and Studio-triggered `get_version` requests pass through one per-serial refresh
coordinator, which assigns module revisions in completion order. Only the long-lived report task
applies delta/full reconstruction and assigns status revisions. Fresh command clients can return
transient state to the initiating Studio callback but never mutate the shared status reducer.

`info.get_version` replaces the ordered module list as one coherent observation. Duplicate names
remain in their reported order; Pandar neither rejects nor deduplicates printer telemetry. For
status, Agent stores the firmware fields extracted from the reconstructed printer object: delta
reports merge before extraction and full/no-`msg` reports replace before extraction. Explicit zero,
false, empty string, and empty lists remain present. Report-stream reconnect, printer endpoint
replacement, and Agent restart invalidate the reconstructed object before new telemetry is
accepted, so a stale in-progress update is not shown as current.

## Agent and Printer MQTT Flow

### Observation

Agent extends its existing typed `get_version` parser to retain `sw_ver`, `sw_new_ver`, `visible`,
`new_ver`, `product_name`, `sn`, `hw_ver`, and `flag` for every reported module. The same report still
supplies model discovery; firmware parsing does not create a second MQTT request.

Continuous report processing extracts `print.upgrade_state` with a field-scoped typed deserializer.
Each accepted change updates the shared per-serial cache and emits a dedicated firmware event. A
firmware-only report never becomes a synthetic full printer snapshot, so it cannot overwrite
unrelated status, temperatures, materials, or job state.

On report-stream startup or reconnect, Agent sends `info.get_version` and the existing status probe,
then emits the fresh module observation. If upgrade state has not yet been reported, it remains
absent rather than being invented. An explicit invalidation event establishes the new generation
before new snapshots are emitted or commands can acquire a generation lease.

### Command execution

Add a new protobuf `FirmwareCommand` oneof and an Agent capability dedicated to firmware control.
The command carries the serial, Hub-authorized expected report generation, exact Studio
`sequence_id`, `src_id`, and only the fields required by its typed variant:

- `UpgradeConfirm`;
- `ConsistencyConfirm`;
- `Start { url, module, version }`;
- `SwitchAmsFirmware { id }`.

The same capability covers a read-only `RefreshFirmwareVersion { serial, sequence_id, generation }`
Hub command used only to satisfy Studio's native `info.get_version` request with fresh printer data.
Agent publishes `info.command = "get_version"` with that exact Studio sequence id. It is not
translated to an `upgrade` command and carries no package URL.

Agent reconstructs the exact nested MQTT `upgrade` envelope. It does not regenerate the Studio
sequence, normalize the module, alter the URL, or translate one command into another. `Start`
requires non-empty URL/module/version strings at the plugin boundary because they are external
input; no hostname allowlist or speculative compatibility check is added.

The generic Agent reverse-command loop must not await firmware I/O serially behind unrelated long
commands. Read-only refresh work is spawned into a per-printer single-flight refresh coordinator;
different printers can proceed independently. A refresh uses one fresh MQTT client and makes at
most three `get_version` publish/wait attempts inside its bounded command result. This retry is
Pandar-owned because Studio does not retry an explicit `result = "fail"` response.

Every spawned refresh/reservation task belongs to the active reverse-session task set. Stream end,
session replacement, or shutdown cancels and joins that set before clearing the session sender, so
old tasks cannot emit into a later session.

Mutating control uses a two-phase live reservation so no queued command can publish after Hub has
already returned a pre-publish failure:

1. Hub sends `PrepareFirmwareControl { command_id, serial, expected_generation }` without the action
   or URL.
2. Agent immediately tries to reserve that printer under the generation transition lease. Busy,
   stale-generation, or ending-session cases fail before publish. A successful reservation returns
   `FirmwarePrepared` and expires unless execution arrives within one second.
3. Only after claiming that exact prepared reservation does Hub send
   `ExecuteFirmwareControl { command_id, expected_generation, command }`, including the transient URL
   when the variant is `Start`.
4. Agent resolves the reservation directly rather than queueing behind another same-printer action,
   reacquires the transition lease, and compares the expected generation again immediately before
   MQTT publish. Mismatch/expiry/session cancellation fails without publish.
5. Agent reports `FirmwarePublished` immediately after MQTT publish and then returns either the
   printer acknowledgement or `PublishedWithoutAcknowledgement` after the two-second printer wait.

Hub may time out the prepare phase safely because it has not sent execute. After execute is sent, it
never reports a pre-publish failure solely because a Hub timer elapsed: it waits for the Agent's
bounded terminal result or session/generation cancellation. A post-publish missing acknowledgement
is an explicit unknown printer-response outcome, returned to Studio as ABI transport success and
never retried. This is not mistaken for completed flashing.

Every refresh/control attempt creates a clean-session MQTT client with a unique bounded id such as
`pandar-agent-fw-<serial>-<uuid>`; it must not reuse the persistent command or report client id. The
client subscribes, establishes a receive ordinal immediately before publish, and accepts only
reports received after that barrier. It therefore cannot disconnect the persistent clients or match
a response already queued before the current publish.

The fresh control client waits at most two seconds for the printer's top-level `upgrade`
acknowledgement with both the exact expected command and Studio sequence id. The acknowledgement is parsed into a typed
URL-free record containing the reported `command`, `sequence_id`, `result`, `err_code`, `reason`, and
message fields. `upgrade_state` seen on this client may accompany the transient command result for
the delayed Studio callback, but it never writes the durable firmware cache. The single long-lived
report stream is the sole writer of durable upgrade status, avoiding cross-client progress
reordering. Unrelated reports are ignored for command completion.

A delayed acknowledgement from an older request with the same command and same reused sequence that
arrives only after the new publish is indistinguishable on Bambu's wire protocol; Pandar documents
the same limitation Studio has and does not claim to eliminate it. The fresh client/barrier prevents
pre-existing queued matches, while command-plus-sequence rejects different-command collisions.

Printer rejection, timeout, subscribe, publish, and decode failures retain their complete cause
chains. A result with a matching acknowledgement means only that the printer responded; a
`PublishedWithoutAcknowledgement` result means only that MQTT publish completed. Flashing completion
is represented solely by later authoritative long-lived-stream `upgrade_state` telemetry.

## Protobuf Boundary

Extend the existing protocol additively with:

- `AGENT_CAPABILITY_FIRMWARE_CONTROL`;
- `PrinterFirmwareModulesSnapshot` carrying serial, stream generation, module revision, and a full
  ordered module replacement, where an empty repeated list is an intentional empty replacement;
- `PrinterFirmwareStatusSnapshot` carrying serial, stream generation, status revision, the full
  currently reconstructed optional upgrade state, and proto3-optional `cfg`;
- `PrinterFirmwareInvalidated` carrying serial and the newly established generation;
- `PrepareFirmwareControl` and `ExecuteFirmwareControl` in `HubCommand`, with the closed
  `FirmwareCommand` payload present only on execute;
- read-only `RefreshFirmwareVersion` in `HubCommand` with a typed ordered-module result;
- typed `FirmwarePrepared` and `FirmwarePublished` Agent events plus the normal terminal
  `CommandResult` lifecycle, with an additive optional typed `FirmwareCommandResult` carrying
  generation and either ordered version modules plus their module revision, a URL-free top-level
  upgrade acknowledgement, or `published_without_acknowledgement`.

Status snapshots are replacements, not patches; Agent performs delta reconstruction before emitting
them. Message presence distinguishes absent state from zero/empty values. `new_ver_list` and the AMS
firmware array use optional wrapper messages around repeated records so absent and present-empty are
different on the wire. Scalar fields use proto3 `optional` presence. Repeated module records stay
ordered and duplicate names are preserved so Pandar does not add a printer-report restriction.
Existing field numbers and generated wire bytes remain unchanged.

Hub derives the Agent session marker from the authenticated reverse stream; Agent does not supply or
choose it. The numeric report generation is Agent-supplied and only accepted within that session.
The first invalidation from a new authenticated Agent session establishes that session's initial
generation; later invalidations in the same session must be strictly newer. Snapshot and
command-result updates use compare-and-set on exact session plus exact active generation and a
strictly newer field-specific revision, so either ordering of a late event, refresh, and reconnect
cannot restore stale firmware state.

Agent advertises the firmware capability only after both typed observation and typed command
handling are present in the same binary. Older Agents therefore cannot receive firmware commands.

## Hub Persistence and API

Add equivalent nullable firmware-state storage for SQLite and PostgreSQL. A single typed JSON column
for modules and a single typed JSON column for upgrade state are acceptable because the Hub does not
query individual firmware fields; they are always serialized and deserialized through the shared
typed structs. Store typed `cfg`, the authenticated observation session id, the active report
generation, and module/status revisions next to them so Plugin data is exposed as current only when
it belongs to the printer owner's exact connected Agent session and generation and that session
advertises `AGENT_CAPABILITY_FIRMWARE_CONTROL`.

Repository behavior:

- a full module observation replaces only ordered firmware modules for the exact active generation
  and a strictly newer module revision;
- a firmware-status snapshot replaces the reconstructed upgrade-state/`cfg` record only for the
  exact active generation and a strictly newer status revision;
- invalidation atomically advances the active generation and clears current firmware data without
  modifying any other printer column, resetting both revisions for the new generation;
- malformed stored typed JSON returns the full parse cause instead of being treated as empty;
- all create, hydrate, update, invalidation, and conflict behavior is byte-equivalent on SQLite and
  PostgreSQL.

Expose a plugin-authenticated firmware-state endpoint for the selected printer. It returns the
current typed module list, optional upgrade state, optional `cfg`, and optional real catalog records.
Initially the catalog list is empty because Pandar has no package source; the endpoint and ABI must
nevertheless use the exact Studio envelope. A future C implementation can add real catalog records
without changing Studio command transport.

Studio's `info.get_version` request uses a separate plugin-authenticated live refresh endpoint, not
an indefinitely cached response. Hub sends an exact-current-session `RefreshFirmwareVersion`
command bound to the active report generation and waits for Agent to issue a fresh printer
`info.get_version`. Agent returns the typed ordered module list with the Studio request sequence id;
Hub compare-and-set persists that same generation before resolving the HTTP response. This path is
used on initial/cold page load and after Studio observes a finished upgrade, so the post-flash
version cannot remain stuck at the pre-flash cache value. If the live refresh fails, Rust emits a
typed `info.get_version` failure response with the original Studio sequence id after Agent's bounded
three-attempt refresh; Studio does not retry an explicit failure, so Pandar does not rely on it. The
path does not return an empty successful module list or silently label cached data as fresh. Refresh
creates an action/serial/sequence-only live command record and the same exact
session/generation one-shot waiter used by control; it contains no secret and is still rejected by
every durable replay path.

Expose plugin-authenticated firmware prepare and execute endpoints. Splitting the Plugin-to-Hub
phase lets the plugin know whether an ambiguous HTTP failure happened before any mutating execute
request was attempted. Prepare performs the existing plugin ticket, tenant, printer ownership, and
agent ownership checks, then:

1. resolves the printer owner's current local Agent session under the live-command transition
   boundary;
2. requires `AGENT_CAPABILITY_FIRMWARE_CONTROL`;
3. writes a command/audit record that contains action, serial, sequence id, module, version, and AMS
   id as applicable, but never the `start.url` value;
4. creates a process-local prepared entry and one-shot completion waiter keyed by command id, exact
   session token, and
   active report generation;
5. sends URL-free `PrepareFirmwareControl` to that exact session and waits at most one second for
   the exact reservation;
6. returns an opaque one-use prepared command token to the plugin without any URL.

The plugin then calls execute exactly once, supplying the prepared token and the original command;
only this transient request contains `start.url`. Execute atomically claims the current prepared
entry, inserts the URL into process-local pending memory, and sends `ExecuteFirmwareControl`. It
waits for the Agent's bounded terminal result or session/generation cancellation, distinguishing
pre-publish failure, printer acknowledgement/rejection, and published-without-acknowledgement, then
returns only the typed URL-free outcome and optional transient firmware state after it is
   redacted and durably recorded.

Prepared tokens are at-most-once, expire with the Agent reservation, and cannot be recreated from
the command record. The plugin never automatically retries execute.

The existing immediate `{command_id,status:"sent"}` printer-operation response is not reused. The
firmware endpoints own a dedicated in-memory result waiter. Command-result handling resolves it only
after claiming the exact pending command under the live-command transition boundary. Prepare
timeout/busy/stale-generation paths fail before execute and remove the prepared entry/waiter. After
execute, printer rejection and acknowledgement timeout retain the published/unknown distinction;
replacement, disconnect, generation invalidation, or shutdown cancel an unexecuted reservation,
remove any pending URL, and record outcome-unknown if publish may already have occurred. There is no
durable polling or replay path.

Firmware commands are live-only. A replacement Agent may not receive an old firmware command, even
if the old session disconnected before acknowledging it. A Hub restart cannot reconstruct the URL
or replay the command. On session close/replacement, pending firmware commands are marked failed.
This is deliberately stricter than durable semantic printer operations because replaying a flash
request is unsafe and Studio itself issues the request interactively.

Firmware refresh/control kinds are explicitly rejected by every durable Hub-command conversion and
fallback path. Startup stale-command cleanup, late inbound results, and session-close draining treat
them as live-only kinds alongside (but separately from) link-printer/live-operation special cases.
A late result may finish redacted durable bookkeeping only when it still claims the exact in-memory
pending session/generation entry; otherwise it is ignored and cannot reconstruct, redispatch, or
resolve a newer waiter.

The refresh and control result waiters are process-local like the current SessionRegistry. This
release therefore retains Pandar's documented one-active-Hub restriction: a plugin request reaching
a replica that does not own the Agent session returns the stable unavailable result; cross-replica
forwarding and distributed secret/result ownership are out of scope. Tests cover non-owner behavior.

URL redaction is fail-closed:

- raw URLs may exist only in the transient Studio input, plugin-to-Hub request body, process-local
  pending entry, live protobuf command, and printer MQTT payload required for exact `start`
  passthrough; they never enter durable command payload JSON, audit metadata, result JSON, tracing
  fields, metrics, panic strings, or HTTP error bodies;
- Agent and Hub errors are redacted against the exact pending URL before persistence/logging;
- query strings and URL-embedded credentials are not logged;
- tests use a unique sentinel URL and assert it is absent from every durable/readback surface.

## Network Plugin Behavior

The C++ shim remains a thin ABI adapter. All request parsing, Hub HTTP calls, Studio telemetry
construction, catalog construction, command validation, and redaction live in Rust behind flat C
FFI functions.

### Firmware page and `get_version`

`bambu_network_get_printer_firmware` calls the Rust firmware-state client and returns HTTP 200 with:

```json
{"devices":[{"dev_id":"<serial>","firmware":[],"ams":[]}]}
```

until real catalog records exist. If a real record is present, Rust maps printer main records to
`firmware[]` and AMS-family records to the first `ams[].firmware[]` collection expected by Studio.
It never emits an empty URL as a pretend selectable package.

When Studio sends `info.get_version`, Rust synchronously requests the bounded live refresh described
above and builds a printer-shaped `info` response from that fresh ordered module list, preserving the
original Studio sequence and every exact module field. A refresh failure emits a typed failure
response with the same sequence instead of a successful empty list. The existing hardcoded fake
versions in `shim.cpp` are removed. Both Cloud and LAN ABI entrypoints receive the same typed data
and command transport behavior.

Studio itself maps upgrade-available state to unavailable for a true LAN-mode `MachineObject` and
therefore hides the LAN update button. Pandar does not override that Studio policy. Native available
update/button behavior is accepted end-to-end through Pandar's Cloud/tunnel device path; LAN ABI
coverage proves exact versions, status, and command passthrough only, without claiming Studio will
show a button it intentionally suppresses.

### Status telemetry

Rust's Studio status builder includes the current typed `print.upgrade_state` object without
changing unrelated push-status fields. Missing firmware state omits `upgrade_state`; a present zero
or empty field remains present. The complete `mc_for_ams_firmware` structure is included when
reported by the printer. It emits `cfg` only when the current reconstructed state contains it; the
shim's hardcoded empty `cfg` is removed.

Omission is not enough when a previously current session/generation becomes invalid, because Studio
retains its existing firmware objects. On the plugin cache transition from current to invalid, Rust
builds this exact local-unavailable reset envelope so no Studio-preserved scalar or AMS status can
survive:

```json
{
  "info": {
    "command": "get_version",
    "sequence_id": "0",
    "result": "fail",
    "module": []
  },
  "print": {
    "command": "push_status",
    "msg": 0,
    "cfg": "",
    "upgrade_state": {
      "status": "",
      "progress": "",
      "message": "",
      "module": "",
      "err_code": 0,
      "new_version_state": 0,
      "consistency_request": false,
      "force_upgrade": false,
      "dis_state": 0,
      "ota_new_version_number": "",
      "ams_new_version_number": "",
      "ahb_new_version_number": "",
      "new_ver_list": [],
      "mc_for_ams_firmware": {
        "firmware": [],
        "current_firmware_id": -1,
        "current_run_firmware_id": -1,
        "status": ""
      }
    }
  }
}
```

This is identified in code/tests as a Pandar session reset, never persisted or represented as
printer telemetry. The plugin emits it immediately and on heartbeats until either fresh
current-generation state arrives or at least one reset has been emitted after three seconds, so an
in-flight Studio AMS guard cannot permanently reject the reset.

The existing batch plugin-printer response is extended with current session/generation firmware
telemetry, so every normal two-second cache refresh also refreshes firmware progress; the heartbeat
then emits it through the ordinary status callback. Progress therefore converges without requiring
another firmware command or an N-per-printer polling loop.

Command acknowledgement uses a stricter asynchronous path. Studio creates the AMS sequence guard
only after `publish_json`/the ABI call returns, ignores callbacks for the next one second, and expects
the matching sequence before three seconds. Therefore the plugin must never invoke the callback
synchronously before returning. Each firmware send call obtains its own opaque Rust return token.
The result envelope is attached to that token but is not yet eligible. As the final epilogue action
after all potentially blocking work, the originating C++ call performs a return handoff containing
its monotonic timestamp; no unrelated send participates in that token. Rust anchors the envelope's
not-before and deadline at handoff +1.1 seconds and +2 seconds respectively. A dedicated serialized
callback dispatcher emits only a handed-off ready token. C++ only performs the final handoff, waits
for Rust's next-ready opaque envelope, and invokes the STL callback under an ABI-owned invocation
mutex, so the shim gains no firmware policy. The compiled probe must prove a deliberately delayed
originating return and an overlapping unrelated send cannot move this envelope before one second or
after two seconds relative to its own return handoff.

The callback contains the printer's exact top-level `upgrade` acknowledgement and, when the
refreshed cache has it, `print.upgrade_state` in the same JSON object. The top-level sequence unlocks
Studio's guard; the nested state updates the selector. If state is not available yet, the
acknowledgement still unlocks the guard and the normal heartbeat supplies later state. Heartbeat,
status-request, and command-triggered invocations use the same callback mutex, preventing concurrent
calls into Studio.

`PublishedWithoutAcknowledgement` queues no synthetic top-level callback. The ABI still reports
transport success because MQTT publish occurred; Studio's own request guard times out normally and
authoritative heartbeat state may still show progress.

The queue is owned per plugin Agent/login generation. Logout cancels pending firmware envelopes.
`bambu_network_destroy` stops and joins the dispatcher and heartbeat threads, drains/cancels Rust
queue entries, and only then releases callbacks and the C++ Agent object; no delayed callback may
outlive its ABI handle.

### Command parsing

The Rust parser recognizes only a top-level `upgrade` object with one of the four exact command
names. It requires a string `sequence_id`, numeric `src_id`, and the variant-specific fields using
typed serde enums/structs. Unknown fields are tolerated for Studio forward compatibility, but
unknown command names, wrong types, missing fields, empty `start` fields, and AMS ids that do not fit
the printer's signed integer wire type return the existing stable unsupported-operation error and
never contact Hub.

Firmware parsing runs before the existing printer-operation parser in both
`send_message` and `send_message_to_printer`. Non-firmware messages retain current behavior. No new
printer-state, model, `fun`, or module-name restriction is introduced in the plugin.

Rust submission calls prepare first while retaining any URL only in plugin memory. Prepare transport
or decode failure is safely pre-publish and returns ABI failure. Once a prepared token exists, Rust
calls execute exactly once and never retries it. Only a typed execute response explicitly marked
`pre_publish_failure` returns ABI failure. A connection reset, malformed/5xx response, or persistence
failure without that phase stamp after execute was attempted is conservatively
`firmware_outcome_unknown`: return ABI transport success, queue no synthetic acknowledgement, retain
a URL-free diagnostic, and rely on heartbeat telemetry. This may report success when execute never
arrived, but it can never encourage Studio to replay a command that may have published. Tests cut the
HTTP connection and inject persistence failure both before prepare completion and after execute.

## Error and Lifecycle Behavior

- Never-observed firmware telemetry produces a valid empty Studio catalog and omits firmware status.
  A transition from current to stale emits the explicit Pandar local-unavailable reset; neither path
  fabricates printer telemetry.
- A disconnected, stale, or non-capable Agent produces a stable
  `firmware_control_unavailable` plugin error and no command record containing secrets.
- An Agent replacement or Hub/Agent restart invalidates current firmware telemetry before it is
  exposed again, cancels every unexecuted reservation, records outcome-unknown for a command that
  may already have published, and never replays either.
- A printer-reported nonzero `err_code` is returned as a failed command result with the exact URL
  removed from all durable/error surfaces. The plugin still returns ABI transport success after
  scheduling that real acknowledgement, as direct Studio publish does; Studio receives and displays
  the printer error from the delayed callback. Proven pre-publish failures return ABI failure;
  published-without-acknowledgement returns ABI success without inventing a callback, allowing
  heartbeat telemetry to converge while preventing replay.
- Decode or persistence failures retain lower-level context in logs after secret redaction.
- Studio's sequence id is opaque text for transport and matching. Pandar does not require it to be
  globally unique or regenerate it.
- No step marks an update complete merely because publish or acknowledgement succeeded.

## Tests

### Core and Agent

- Parse a representative `get_version` report containing `ota`, classic `ams/0`, AMS 2 Pro
  `n3f/*`, AMS-HT `n3s/*`, and an unknown future module; preserve every exact name and version.
- Prove a new full module list replaces old modules and duplicate names/order survive unchanged.
- Prove `hw_ver`, `flag`, and both `sw_new_ver` and `visible` plus `new_ver` forms survive the
  Agent-to-Studio round trip.
- Parse every known general and AMS nested `upgrade_state` field from MQTT-shaped bytes.
- Prove `progress` remains a string; `ams_new_version_number`, `ahb_new_version_number`, and `cfg`
  preserve exact values.
- Prove a `msg = 1` delta merges before extraction, including preserving an absent
  `new_ver_list`; prove a full/no-`msg` report replaces the reconstructed object, distinguishes an
  absent list from a present-empty list, and retains explicit zero/false/empty values.
- Prove malformed firmware fields do not discard valid sibling printer telemetry.
- Prove report-stream reconnect and endpoint replacement establish a new generation before fresh
  state, and both late-old-before-new and new-before-late-old orderings reject the old event/result;
  prove lower/equal module and status revisions cannot overwrite newer same-generation state.
- Prove exact protobuf round trips, message presence, capability advertisement, and unchanged legacy
  wire fixtures; prove every refresh result carries the module revision required by Hub CAS.
- Prove cold and post-terminal Studio refreshes use a fresh printer `get_version`, preserve the
  Studio sequence, perform at most three internal attempts, and never return a successful
  empty/cache-only response after refresh failure or rely on Studio to retry explicit failure.
- For every firmware command, inspect the exact MQTT JSON, including the unchanged Studio sequence
  id and `src_id`.
- Prove every refresh/control uses a fresh clean MQTT client, subscribes and establishes the receive
  barrier before publish, rejects pre-barrier queued and post-barrier wrong-command responses,
  accepts only the post-publish exact command-plus-sequence acknowledgement inside the two-second
  firmware bound, uses a unique client id, leaves persistent command/report clients connected, and
  retains full error chains. Document rather than falsely test away the same-command/same-sequence
  delayed-response ambiguity.
- Block an unrelated Agent command and prove prepare is handled without generic command-loop delay;
  prove same-printer busy rejects before execute, different-printer concurrency, reservation expiry,
  execute-generation recheck, and session cancellation all publish nothing.
- Prove Hub prepare timeout never sends execute and no late Agent work can publish; after execute,
  prove both acknowledged and published-without-ack terminal outcomes without replay.
- Deliver command-client state and long-stream progress in opposite orders and prove only the
  long-lived report stream mutates durable status while transient state remains callback-only.

### Hub

- Prove SQLite/PostgreSQL migration parity and nullable typed firmware columns.
- Prove create/hydrate/module replacement/status replacement/generation invalidation and malformed
  stored JSON.
- Seed every unrelated printer field, apply firmware events, and prove all unrelated fields remain
  unchanged on SQLite and real PostgreSQL.
- Prove exact current-session and capability delivery, stale-session rejection, replacement during
  dispatch, disconnect failure, Hub restart non-replay, and pending-command cleanup.
- Prove the process-local one-shot resolves only for the exact command/session/generation result;
  prepare timeout/busy never sends execute, and replacement, generation change, disconnect,
  shutdown, and a non-owning Hub replica preserve pre-publish versus outcome-unknown distinctions.
- Prove separate plugin prepare/execute tokens are one-use, execute is never automatically retried,
  and persistence failures before prepare completion versus after execute retain safe phase labels.
- Prove refresh/control records cannot be reconstructed by durable command conversion, startup
  cleanup, late-result handling, or any queued fallback.
- Use a unique signed-URL sentinel to prove URL absence from Pandar durable commands, audits, result
  JSON, captured logs, and API readback while proving the exact value exists only in the transient
  execute command and reaches the fake Agent once.
- Prove tenant, printer, and Agent ownership boundaries for state and commands.
- Run real PostgreSQL tests when `PANDAR_TEST_POSTGRES_URL` is configured and report the exact skip
  otherwise.

### Network Plugin

- Typed parser coverage for all four commands and every missing/null/wrong-type/unknown-command
  rejection shape.
- HTTP tests for firmware state, fresh version refresh, empty catalog, real catalog mapping, command
  redaction, unavailable/non-owner Agent, matching acknowledgement, and URL-free result shape.
- Cut the plugin execute response before and after Hub receives it and prove only typed explicit
  pre-publish failure returns ABI failure; all ambiguous post-attempt failures return URL-free
  outcome-unknown success without retry or synthetic acknowledgement.
- Status tests for omitted versus present-empty fields, the complete AMS switch structure, and the
  exact local-reset JSON from a fully populated upgrading/force/consistency/AMS-SWITCHING state,
  including clearing stale module/main/AMS state across an active three-second Studio guard.
- `get_version` tests prove cold and post-finished requests use fresh modules, failure carries the
  original sequence, all real module fields are emitted, and no hardcoded version remains.
- Compiled MSVC ABI tests call Cloud and LAN entrypoints, open the native Firmware page paths,
  request `get_version`, inspect main/AMS/AMS-HT telemetry, send every command shape, and verify
  exact sequence/value forwarding to a fake Hub.
- A timing/compiled-ABI test proves no firmware callback fires before `send_message` returns or in
  Studio's first one-second guard window, then proves the exact top-level acknowledgement and nested
  state arrive on the serialized callback between 1.1 and 2 seconds relative to the originating
  return handoff even when that call's return is delayed and an unrelated send overlaps; another
  test destroys/logs out the Agent with a pending callback and proves cancellation/join before
  callback/object release.
- A concurrency test overlaps heartbeat and command-triggered firmware emission, proves callback
  serialization, and proves every heartbeat refresh includes current firmware progress.

### Completion

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- focused Core, Agent, Hub, plugin HTTP/parser, migration, lifecycle, and compiled ABI tests
- module-size guard with every touched/new production Rust file below 400 LOC and no `include!`;
  pre-existing `shim.cpp` is explicitly exempt from the Rust-only guard and receives ABI-adapter
  deltas only, with no new business logic
- `cargo nextest run --manifest-path Cargo.toml --workspace`
- no external package download and no live printer firmware command

## Documentation

Update the Bambu Studio compatibility document, development protocol notes, and roadmap with:

- supported printer-main and AMS-family Studio firmware behavior;
- authoritative telemetry and live-only command lifecycle;
- URL redaction and no-package-hosting boundary;
- deterministic versus live-hardware verification evidence;
- the existing one-active-Hub requirement for local Agent session, URL, and result-waiter ownership;
- C as the future Web/Android remote OTA scope.

## Rollout and Rollback

Roll out in this order:

1. Agent with additive telemetry, command handling, and capability advertisement;
2. Hub schema/protobuf handling and plugin endpoints;
3. network plugin with native Firmware page behavior.

The plugin must not expose controls until the current Agent session advertises the capability.

Roll back in reverse order. Before rolling Hub or Agent back, drain the owning Hub's local firmware
URL/result waiters by allowing commands to reach a terminal acknowledgement or explicitly failing
them; never transfer or replay them to a replacement process. Nullable firmware columns may remain
after rollback. A printer already flashing continues under printer control; Pandar rollback does
not attempt to cancel it or assert its outcome.

## Acceptance Criteria

1. Bambu Studio's native Firmware page displays printer-reported main and AMS-family current
   versions without hardcoded data, and cold/post-finished page refreshes read fresh printer
   `get_version` data.
2. Printer-advertised new-version and progress state reaches Studio with correct absent/zero/empty
   semantics.
3. `upgrade_confirm`, `consistency_confirm`, a real-URL `start`, and
   `mc_for_ams_firmware_upgrade` reach the exact current capable Agent and printer with the original
   Studio sequence id and values.
4. A matching acknowledgement is delivered asynchronously inside Studio's 1-to-3-second AMS guard
   window and later printer `upgrade_state` remains the only source of progress and completion.
5. A current-to-invalid session/generation transition emits the exact local reset and clears every
   previously populated Studio main/AMS upgrade field.
6. Plugin/Hub prepare failure is known pre-publish; any ambiguous failure after execute is attempted
   becomes outcome-unknown transport success, is never automatically retried, and never fabricates
   a printer acknowledgement.
7. Signed package URLs are never persisted or logged by Pandar and firmware commands are never
   replayed after disconnect, replacement, or restart. Studio's own pre-plugin logging is outside
   Pandar's control and is not claimed as verification evidence.
8. Pandar adds no model, state, `fun`, URL-host, or module-name policy beyond external shape/size
   validation.
9. The C++ shim contains only ABI adaptation; Rust owns firmware business logic and typed serde
   parsing.
10. SQLite and PostgreSQL behavior is equivalent, all focused/ABI/workspace checks pass, and no real
   firmware update is executed during verification.
