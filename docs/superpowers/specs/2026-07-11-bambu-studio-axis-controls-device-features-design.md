# Bambu Studio Axis Controls and Device Features Design

## Goal

Make Bambu Studio homing and X/Y/Z movement work through Pandar while preserving the printer's actual protocol capabilities. Pandar must parse the printer's `print.fun` bitmap into a typed device-feature value, propagate the complete bitmap without dropping unknown bits, expose it back to Studio, and send the modern homing or axis-control command to the printer when the corresponding printer feature is present.

## Reference Behavior

Bambu Studio treats `print.fun` as a hexadecimal 64-bit feature bitmap. The relevant reference behavior is:

- bit 32 enables MQTT homing;
- bit 38 enables MQTT X/Y/Z control;
- when bit 32 is set, Home sends `print.command = "back_to_center"`;
- when bit 32 is clear, Home sends `gcode_line` containing `G28 X` while printing and `G28` otherwise;
- when bit 38 is set, an axis click sends `print.command = "xyz_ctrl"` with an uppercase `axis`, `dir` of `-1` or `1`, and `mode` of `0` for 1 mm or `1` for 10 mm;
- when bit 38 is clear, Studio sends a `gcode_line` containing, in order, `M211 S`, `M211 X1 Y1 Z1`, `M1002 push_ref_mode`, `G91`, one `G1` movement, `M1002 pop_ref_mode`, and `M211 R`;
- legacy Studio movement uses feedrate 3000 for X/Y and 900 for Z;
- Studio performs its own Y/Z direction adjustment for non-CoreXY printers before constructing either the modern command or the legacy movement line.

Pandar must not add a new home-axis restriction, collapse `G28 X` into full homing, or invert Y/Z a second time.

## Scope

In scope:

- Parse nested MQTT `print.fun` into a typed Bambu device-feature set in Agent.
- Preserve all 64 bits, including bits Pandar does not currently name.
- Propagate the typed feature set through Agent snapshots, protobuf, Hub persistence, and the plugin printer response.
- Emit the feature set as Studio's hexadecimal `print.fun` value.
- Add modern `back_to_center` and `xyz_ctrl` parsing in the network plugin.
- Parse the actual Studio `gcode_line` wrapper and legacy axis-control envelope.
- Preserve Studio's axis-specific legacy homing behavior.
- Make Agent choose a modern printer MQTT command from its local device-feature cache when the semantic operation is exactly representable by that command.
- Add an optional typed device-feature requirement to semantic Home/Move operations so a stale modern Studio request can never be silently translated into a different legacy action; raw Studio JSON and raw G-code must not cross Hub.
- Add unit, repository, protocol, and compiled ABI regression coverage for modern and legacy paths.

Out of scope:

- `fun2` or a speculative generic feature-negotiation framework.
- Top-level `fun` fallback for firmware that does not report nested `print.fun`.
- Naming unrelated `fun` bits that are not consumed by this feature.
- New Web or mobile axis-control UI.
- Removing or broadening existing Hub authorization, ownership, or public-operation value validation.
- A general-purpose G-code parser.
- Live movement or homing against real hardware without separate operator authorization.

## Typed Device Feature Model

Add a dedicated `pandar-core` model, separate from model-derived `CompatibilityFeatures`:

```rust
pub enum BambuDeviceFeature {
    MqttHoming = 32,
    MqttAxisControl = 38,
}

pub struct BambuDeviceFeatures(u64);
```

`BambuDeviceFeatures` provides:

- hexadecimal parsing of exactly 1 through 16 ASCII hexadecimal digits after trimming outer ASCII whitespace; signs, `0x` prefixes, separators, trailing text, and empty input are rejected;
- canonical uppercase hexadecimal formatting;
- `bits()` for protobuf conversion;
- `contains(BambuDeviceFeature)` for named capability checks;
- string serde using the canonical hexadecimal representation.

The complete `u64` is authoritative. Named feature checks do not reconstruct the bitmap, so unknown and future bits survive a parse/serialize round trip. Leading zeroes and input letter case are canonicalized, but bit content is unchanged. Canonical zero is `"0"`. An absent value remains distinct from the valid value `0`.

`CompatibilityFeatures` remains unchanged because it expresses model-derived product compatibility with `Supported`, `Unsupported`, and `Unknown`; `BambuDeviceFeatures` expresses a firmware-reported runtime bitmap.

## Data Flow

### Printer to Agent

`SnapshotPrint` accepts nested `print.fun` through a field-scoped typed deserializer. The deserializer distinguishes a string from an invalid JSON type without failing deserialization of the enclosing `print` object. Snapshot conversion parses a valid string into `Option<BambuDeviceFeatures>` on `MachineSnapshot`.

- Missing `fun` produces no feature update.
- Valid `"0"` produces a present zero-bit feature set.
- A non-string value, malformed string, or out-of-range value produces a contextual parse issue naming `print.fun`; Agent logs the serial and full issue chain, does not replace a previously known feature set, and does not discard valid sibling telemetry from the same report.

Agent maintains a shared, per-serial runtime feature cache. Refresh, link, and continuous report paths update the cache only when a parsed feature value is present, including a present zero bitmap. Missing and invalid fields leave the cache unchanged.

The cache is deliberately current-process state, not a copy of Hub persistence:

- it starts unknown for every serial after Agent restart;
- a successful refresh/link or continuous report containing `fun` primes it;
- replacing a printer endpoint or losing/restarting that printer's report-forwarding connection invalidates it;
- a later valid value, including zero after a firmware downgrade, replaces the prior value.

Before dispatching `Home` or `MoveAxes`, an unknown cache entry triggers a bounded feature probe on the printer command transport: subscribe to the report topic, publish the existing typed `pushall` command, and wait up to the existing report timeout for a report containing a valid nested `print.fun`. The probe updates the same cache used by dispatch. If the printer cannot provide a valid value before the deadline, Agent logs the complete cause, invalidates the current-session advertisement, and never guesses support from the model or Hub's persisted value. A feature-required modern operation then fails without an MQTT operation publish; a requirement-free operation retains its legacy semantic translation. This makes a retained Hub value safe across Agent restart and prevents a stale modern message from being reinterpreted as a different legacy action.

### Agent to Hub

Extend `PrinterSnapshot` additively:

```proto
message PrinterDeviceFeatures {
  fixed64 bambu_fun_bits = 1;
}

message PrinterSnapshot {
  // existing fields 1..12
  PrinterDeviceFeatures device_features = 13;
}

message PrinterDeviceFeaturesSnapshot {
  string serial = 1;
  // absent means invalidate the current-session observation
  PrinterDeviceFeatures device_features = 2;
}

message AgentEvent {
  // existing variants 10..16
  PrinterDeviceFeaturesSnapshot printer_device_features_snapshot = 17;
}

enum AgentCapability {
  // existing values
  AGENT_CAPABILITY_REQUIRED_DEVICE_FEATURES = 3;
}
```

Message presence distinguishes missing telemetry from a valid zero bitmap. Refresh and link populate the optional feature message on their full `PrinterSnapshot`. A feature-only message with absent `device_features` is an explicit invalidation, not a zero bitmap.

Continuous MQTT handling must not turn a `fun`-only report into a synthetic full printer snapshot: the existing full snapshot contract would otherwise overwrite status and temperature fields with `unknown` or empty values. When a continuous report has `fun` but does not meet the existing full-telemetry snapshot predicate, Agent emits the new feature-only event. Hub handles that event with an exact current-session and tenant/agent/serial ownership check and updates only the feature column. When a report already produces a real full snapshot, that snapshot carries the optional feature message and no duplicate feature-only event is emitted.

### Hub Persistence

Add nullable `bambu_fun_bits TEXT` and `bambu_fun_session_id TEXT` columns with equivalent SQLite and PostgreSQL migrations. A text bitmap avoids both databases' signed 64-bit integer boundary while preserving the full unsigned value. Hub stores canonical uppercase hexadecimal bits and the exact Agent session that observed them.

Add `Option<BambuDeviceFeatures>` to the printer domain model and snapshot upsert boundary. Snapshot conflict updates use:

```sql
bambu_fun_bits = COALESCE(excluded.bambu_fun_bits, printers.bambu_fun_bits)
```

Therefore an absent incremental field preserves the last known value, while present zero overwrites it. Hydration parses the stored canonical value and preserves the full error context if the database contains an invalid value.

The feature-only repository method updates only the bitmap and its observation-session marker by tenant, current Agent owner, and printer serial. A present feature value writes the bits plus the current session id. An explicit feature invalidation clears only the session marker and leaves the last-known bits available for diagnostics. It does not change `status`, `model`, `last_seen_at`, nozzle JSON, temperatures, active nozzle, light state, or the ordinary full-snapshot state revision. Tests seed all those fields, apply both feature values and invalidation, and prove every seeded field remains unchanged on SQLite and PostgreSQL.

### Hub to Studio

The plugin printer-list response exposes the canonical feature bitmap only when `bambu_fun_session_id` exactly matches the printer owner's current connected Agent session and that session advertises `AGENT_CAPABILITY_REQUIRED_DEVICE_FEATURES`. If there is no current session, the marker differs, the current Agent lacks that capability, or the current session explicitly invalidated its observation, Hub returns `fun: "0"`. Returning zero actively clears Studio's prior bit 32/38 state while keeping last-known bits in storage for diagnostics. The network plugin's typed Rust status input includes the Hub-provided string in `StudioTelemetry`. The C++ ABI shim stops hardcoding `"fun":""`; it only embeds the Rust-produced telemetry object.

Pandar never advertises bit 32 or 38 from model guesses or from another Agent session. On Agent connection/reconnection, Runtime registers the current event sender, clears each per-serial cache entry, queues a feature invalidation for each configured printer, probes `pushall`, and queues the observed value or keeps the invalidation before it begins consuming Hub commands. Report-forwarder reconnect and endpoint replacement repeat invalidation before probing. Consequently a current-session feature advertisement can only appear after the same runtime cache has been primed with that observation.

## Printer Command Selection

Hub continues to persist, validate, and dispatch only semantic `Home` and `MoveAxes` operations. Agent owns all Bambu-specific selection and payload construction.

Add a typed optional requirement to those semantic operations:

```proto
enum DeviceFeature {
  DEVICE_FEATURE_UNSPECIFIED = 0;
  DEVICE_FEATURE_BAMBU_MQTT_HOMING = 32;
  DEVICE_FEATURE_BAMBU_MQTT_AXIS_CONTROL = 38;
}

message PrinterOperation {
  string serial_number = 1;
  repeated DeviceFeature required_device_features = 2;
  // existing operation oneof
}
```

The persisted semantic JSON and plugin request use `required_device_features`, with values `bambu_mqtt_homing` and `bambu_mqtt_axis_control`. Hub accepts only the homing requirement on an empty-axis Home and only the axis-control requirement on a one-axis, 1-or-10-mm, feedrate-free Move. Web/mobile and legacy Studio operations omit the list. Existing queued operations deserialize with an empty list.

Hub fails closed before dispatching any non-empty requirement list. Under the same exact-session transition lease used for live printer operations, it re-reads the printer owner, verifies that the current Agent session advertises `AGENT_CAPABILITY_REQUIRED_DEVICE_FEATURES`, verifies `bambu_fun_session_id` equals that session, and verifies the stored exact bitmap contains every required bit. A session change, an older/non-capable Agent, invalidation, or a missing bit marks the command failed and sends no protobuf command. This prevents an older Agent from ignoring additive protobuf field 2 and executing the operation. The current-session Agent repeats the cache check immediately before MQTT publish to close the observation-to-publish race.

Before publishing, Agent verifies every required feature against the current-process cache. An unknown cache triggers the bounded probe. If the probe reports the required bit, Agent publishes the modern payload. If the probe returns a valid bitmap that lacks the required bit, Agent queues that exact bitmap, returns a cause-preserving command failure, and publishes no operation MQTT command. It queues `"0"` only when the exact observed bitmap is zero. A non-string, malformed, missing, or timed-out observation queues invalidation instead. It must never reinterpret a feature-required modern operation as legacy: after Studio has normalized `back_to_center` or `xyz_ctrl`, the printing-state-dependent `G28 X` choice and the legacy 3000/900 feedrate can no longer be reconstructed.

An operation without a feature requirement retains its exact semantic legacy meaning. Agent may use a modern command only when a known cached feature makes it exactly equivalent; it never blocks or changes an axis-specific Home or a feedrate-bearing Move.

### Homing

Agent sends the exact modern JSON `print.command = "back_to_center"` with a generated Studio sequence id only when:

- the cached feature set contains `MqttHoming`; and
- the semantic home operation has an empty axis list, matching Studio's modern full-home request; and
- a feature-required request has passed its current-cache check.

An operation without the modern requirement emits `gcode_line` with `G28` plus every requested axis in semantic order unless a known current feature makes empty-axis Home exactly equivalent to `back_to_center`. Empty axes produce `G28`; `[X]` produces `G28 X`. This removes the previous Bambu adapter behavior that collapsed all home operations to bare `G28`.

### Axis Movement

Agent sends the exact modern JSON `print.command = "xyz_ctrl"` with uppercase `axis`, numeric `dir`, numeric `mode`, and a generated Studio sequence id only when:

- the cached feature set contains `MqttAxisControl`;
- exactly one of X/Y/Z is present;
- the absolute movement is exactly 1 mm or 10 mm; and
- no feedrate was supplied; and
- a feature-required request has passed its current-cache check.

Those conditions exactly cover Studio's modern messages. Agent maps the axis to uppercase, the sign to `dir`, and 1/10 mm to mode 0/1. It does not invert Y or Z.

All operations without a modern requirement that are not exactly eligible for modern delivery use the legacy Studio command sequence. Agent emits one `G1` line containing the requested axes and optional feedrate between the same `M211` and `M1002` commands Studio uses. This preserves existing non-Studio semantic operations that cannot be represented by `xyz_ctrl` instead of rejecting or rounding them. The cache/probe and payload choice live in Agent; Hub still transports only semantic operation data.

## Network Plugin Parsing

The typed Studio JSON parser adds:

- `back_to_center` -> `Home { axes: [], required_device_features: [bambu_mqtt_homing] }`;
- `xyz_ctrl` -> one-axis `MoveAxes` with delta `dir * 1` or `dir * 10`, no feedrate, and `required_device_features: [bambu_mqtt_axis_control]`;
- `gcode_line` -> parse its string `param` through the bounded G-code parser with no modern feature requirement.

Modern parsing accepts only Studio's actual schema:

- axis is exactly `"X"`, `"Y"`, or `"Z"`;
- direction is numeric `-1` or `1`;
- mode is numeric `0` or `1`;
- required fields must be present.

Malformed modern messages return the existing stable `unsupported_printer_operation` result and do not post an operation to Hub.

The bounded G-code parser keeps its existing simple command support and additionally recognizes exactly this ordered seven-command Studio legacy movement envelope after whitespace/comment normalization:

1. `M211 S`
2. `M211 X1 Y1 Z1`
3. `M1002 push_ref_mode`
4. `G91`
5. one `G1` containing X/Y/Z movement and the optional feedrate
6. `M1002 pop_ref_mode`
7. `M211 R`

It extracts only the `G1` movement after verifying every surrounding command. A missing, reordered, altered, or additional command rejects the entire message. It does not accept arbitrary macros or forward raw G-code.

No additional printer-state restriction is added in Pandar. Studio retains its own RUNNING/SLICING/PREPARE enablement behavior, and existing Hub boundary validation remains unchanged.

## Tests

### Core

- Parse and format the combined bits 32 and 38 value `4100000000`.
- Query each named feature independently.
- Preserve unnamed bits and bit 63 through parse/format.
- Distinguish absent data from valid zero at consuming boundaries.
- Reject empty, malformed, negative, and greater-than-64-bit hexadecimal input.

### Agent

- Parse nested `print.fun` from a real MQTT-shaped byte payload into the typed snapshot.
- Keep sibling temperature/status telemetry when `fun` is a non-string, malformed, or out-of-range value, and prove the contextual warning includes the serial, `print.fun`, and the underlying reason.
- Prove refresh/link and continuous-report protobuf mappings carry `fixed64` bits.
- Prove a `fun`-only report emits only `PrinterDeviceFeaturesSnapshot` and updates the local cache.
- Prove absent reports preserve cache state and valid zero replaces it.
- In one deterministic test using the same shared cache, ingest a capability report, dispatch Home/Move, and inspect the exact MQTT payload; repeat after replacing the cache with zero.
- Prove a cold cache performs the typed `pushall` feature probe before dispatch and chooses modern after a supported response.
- For feature-required Home/Move, prove a valid nonzero bitmap missing the required bit publishes no operation MQTT payload, returns a cause-preserving failure, and queues the exact observed bitmap including other named, unnamed, and bit-63 features. Prove exact zero queues zero; invalid/missing/timeout queues invalidation.
- Prove report-forwarder reconnect and endpoint replacement invalidate cached features.
- Prove run/session startup queues invalidation and feature probing before consuming any Hub operation, and the cache value that authorizes a modern command is the same observation sent to Hub.
- Prove bit 32 plus empty axes produces `back_to_center`.
- Prove missing bit 32 preserves `G28 X` and bare `G28` behavior.
- Prove bit 38 plus X/Y/Z modern inputs produces exact `xyz_ctrl` axis, direction, and mode payloads without a second inversion.
- Prove unsupported movement shapes use the ordered Studio legacy envelope without rounding or rejecting the semantic request.

### Hub

- Prove SQLite and PostgreSQL migration text is equivalent and both new columns are nullable.
- Prove repository create, missing-value preservation, valid-zero overwrite, nonzero overwrite, bit-63 hydration, and typed domain output.
- Seed a populated printer, apply a feature-only event, and prove status, model, last-seen time, nozzle JSON, temperatures, active nozzle, light state, and state revision remain byte/value identical on both backends.
- Run the same merge assertions against PostgreSQL when `PANDAR_TEST_POSTGRES_URL` is configured; otherwise report the skip explicitly.
- Prove gRPC snapshot mapping stores the current session marker; plugin printer-list serialization returns the canonical value only for an exact current-session match and returns `"0"` for disconnect, session mismatch, and explicit invalidation.
- With stale stored bits 32/38 and a new cold Agent session, prove Studio cannot receive those bits until the new session reports them; zero/invalid/timeout convergence returns `"0"` while preserving last-known diagnostic bits.
- Prove required feature values persist as typed semantic JSON, convert to protobuf, reject invalid operation/requirement combinations, and default to empty for existing operation payloads.
- Queue a required-feature operation, replace the Agent session with an older/non-capable session before dispatch, and prove Hub fails the command without sending protobuf; repeat for a capable new session whose observation marker or bitmap does not match.

### Network Plugin

- Prove status telemetry emits the exact Hub-provided `fun` and missing/null input does not erase sibling telemetry.
- Prove `back_to_center` and representative X/Y/Z `xyz_ctrl` payloads map to semantic operations with their exact required device feature; legacy wrappers have no requirement.
- Reject invalid or missing modern fields.
- Prove actual Studio `gcode_line` wrappers for `G28`, `G28 X`, and the seven-command movement envelope map to semantic operations.
- Reject legacy envelopes with each surrounding command missing, reordered, altered, or followed by an extra command.
- Update the compiled ABI probe fixture so the mock Hub returns `8000004100000020` (bits 63, 38, 32, and unnamed bit 5), the emitted `push_status` preserves the complete bitmap, modern home/move calls reach the mock Hub as semantic JSON, and separately submitted legacy `gcode_line` wrappers for home and movement also reach Hub as semantic JSON. No operation request may contain raw Bambu commands.
- Carry the same `8000004100000020` value through Agent protobuf, Hub storage/API, and compiled ABI telemetry, proving unnamed and bit-63 preservation at each boundary.

## Documentation Impact

After final implementation review:

- update `docs/roadmap.md` to record typed device-feature passthrough and Studio XYZ/homing support;
- update `docs/development.md` with the typed feature path and Agent command selection;
- update `docs/compatibility/bambu-studio-plugin.md` with modern and legacy Studio behavior;
- record that no real-printer movement or homing probe was performed unless separately authorized.

## Acceptance Criteria

- Studio receives the printer-reported full `fun` bitmap rather than hardcoded feature bits or an always-empty value.
- Unknown `fun` bits, including bit 63, survive Agent -> Hub -> plugin -> Studio at the bit level.
- A printer reporting bit 32 receives `back_to_center` for Studio's modern home request.
- A printer reporting bit 38 receives exact `xyz_ctrl` for Studio's modern X/Y/Z requests.
- A printer lacking those bits continues through Studio's legacy messages, including `G28 X` while printing and the ordered seven-command axis envelope.
- A stale Studio modern request whose required feature cannot be confirmed publishes no printer command; it never degrades to bare `G28` or a speedless legacy move, and Hub's Studio advertisement converges to `"0"`.
- A valid nonzero bitmap missing one required bit is preserved exactly; convergence never erases unrelated named, unnamed, or bit-63 features.
- Pandar does not add axis-homing restrictions or a second Y/Z inversion.
- Hub never stores or forwards raw Studio JSON or raw G-code.
- A feature-only report cannot erase any existing full printer snapshot field, and SQLite and PostgreSQL expose equivalent behavior.
- Agent restart, report reconnect, endpoint replacement, and a valid zero downgrade cannot cause Hub or Agent to treat a last-known bitmap as current-session capability.
- Focused tests, formatting, Clippy, the production-module size guard, workspace nextest, independent review, and the compiled ABI probe pass.

## Rollout and Rollback

Roll out Hub migration/session-fresh API support and Agent typed snapshot/feature-probe/required-feature command support before exposing the new plugin artifact. Old agents omit the new Agent capability and additive protobuf events, so Hub returns `fun: "0"` and refuses to dispatch any feature-required operation to them; they never cause or execute a modern shape they cannot safely enforce. Hub can retain last-known diagnostic bits across an updated Agent restart, but the session marker no longer matches, so Studio sees zero until the updated Agent queues invalidation, probes the current printer, and reports a current-session value. Only then can the updated plugin expose modern bits.

For rollback, deploy the previous plugin first so Studio returns to legacy messages, then drain or fail every queued/sent operation with a non-empty `required_device_features` list before rolling back Agent and Hub. Requirement-free semantic operations remain valid. Leave the nullable feature columns in place; they are inert for older code and avoid destructive schema rollback.
