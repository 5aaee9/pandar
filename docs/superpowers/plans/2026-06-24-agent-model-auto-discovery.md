# Agent Printer Model Auto Discovery Plan

## Goal

Make agent printer refresh discover the printer model from Bambu LAN MQTT and fail visibly when discovery fails.

## Steps

1. Add MQTT model discovery primitives in `crates/pandar-agent/src/machine/mqtt.rs`.
   - Add a `GetVersion` command payload for `info.get_version`.
   - Add a small parser for model strings from `get_version` reports.
   - Accept `get_version` responses whose command appears at `/info/command`.
   - Extract the discovered model from `info.module[]` where `name == "ota"` and `product_name` is non-empty after trimming.
   - Verify with payload and parser unit tests.

2. Change `refresh_printer` ordering in `crates/pandar-agent/src/machine/mqtt.rs`.
   - Subscribe to reports.
   - Publish `GetVersion`.
   - Keep reading reports until a `get_version` response is observed, bounded by one total `report_timeout` deadline, then parse the model.
   - Enforce the total deadline with one outer `tokio::time::timeout(report_timeout, ...)` around the discovery loop so repeated `next_report` calls cannot reset the discovery deadline.
   - On any discovery error, log `serial` plus full error chain and return the error.
   - Publish `RequestPushAll` only after model discovery succeeds.
   - Wait for the pushall/state report.
   - Build the final `MachineSnapshot` from the pushall/state report and attach the discovered model.

3. Update agent MQTT tests in `crates/pandar-agent/src/machine/mqtt/tests.rs`.
   - Extend `FakeMqttTransport` with publish failure injection for `GetVersion` publish failure coverage.
   - Extend `FakeMqttTransport` with an infinite unrelated-report mode that awaits/yields between reports so the timeout test proves the outer total deadline rather than finite fake depletion.
   - Successful refresh should use discovered model, not configured model.
   - Successful refresh should skip unrelated reports before the `get_version` response and still wait for the later pushall/state report.
   - Unrelated reports without a version response should still fail after the total discovery timeout.
   - Missing model should fail before `pushall`.
   - `GetVersion` publish failure, receive timeout, and malformed or blank model responses should fail before `pushall`.
   - Discovery failure log should include the serial and underlying cause.
   - Existing state mapping tests should be updated so raw report normalization has no model and refresh attaches the discovered model.

4. Update gateway-level refresh tests in `crates/pandar-agent/src/machine/tests.rs`.
   - Multi-printer refresh fakes should each receive `get_version` then pushall/state reports.
   - Snapshot assertions should use discovered models.

5. Subagent handoff.
   - Use one implementation owner for MQTT implementation plus MQTT/gateway tests because the write set is tightly coupled.
   - Use a separate docs owner only after implementation behavior is settled.
   - Use a verifier/reviewer lane for final `fmt`, focused tests, clippy, nextest, and spec compliance review.

6. Update docs.
   - Update `docs/development.md` to document refresh-time model discovery and failure behavior.
   - Update `docs/architecture.md` to replace the old configured-model refresh behavior.
   - Update `docs/roadmap.md` with completed work and any remaining follow-up.

7. Verify.
   - `cargo fmt`
   - Focused MQTT tests.
   - `cargo clippy`
   - `cargo nextest run --manifest-path "Cargo.toml" --workspace`

## Risk Controls

- Do not change hub schema or repository behavior.
- Do not remove existing config parsing fields.
- Do not use configured `model` as refresh fallback.
- Preserve full error chains in logs with `{err:#}` formatting.
