# Phase 27 Live Printer Controls

This note records the Phase 27 compatibility policy for typed live printer controls. It is reference-backed and locally verified without opening live printer sockets.

## Reference Payloads

Publish all live-control commands to `device/{serial}/request` with MQTT QoS `1`.

| Control | Hub action | MQTT payload |
| --- | --- | --- |
| Pause | `pause` | `{"print":{"command":"pause","sequence_id":"0"}}` |
| Resume | `resume` | `{"print":{"command":"resume","sequence_id":"0"}}` |
| Stop | `stop` | `{"print":{"command":"stop","sequence_id":"0"}}` |
| Print speed | `set_print_speed` with `speed_mode` 1-4 | `{"print":{"command":"print_speed","param":"<speed_mode>","sequence_id":"0"}}` |

Print-speed modes outside `1..=4` are invalid. Non-speed actions must not carry `speed_mode`.

## Compatibility Policy

The Hub is authoritative for compatibility and authorization. The frontend can mirror this policy for disabled controls, but the Hub must still reject unsupported or unknown models.

| User-facing model | Normalized model | Aliases | Live controls |
| --- | --- | --- | --- |
| A1 | `A1` | `BambuLab A1` | Supported |
| A1 Mini | `A1_MINI` | `A1M`, `A1MIN`, `BambuLab A1 Mini` | Supported |
| P2S | `P2S` | `N7` | Supported |
| X2D | `X2D` | `N6` | Supported |
| Missing or unknown model | none or normalized unknown key | none | Unknown; reject control enqueue |

Phase 27 dispatches only typed controls: pause, resume, stop, and print speed. Raw MQTT or arbitrary printer commands remain outside the operator control path.

## Lifecycle Boundary

Live printer controls are dispatch commands, not physical print-state mutations.

- Enqueueing a control creates a durable `printer_control` command and audit record.
- Sending the command over gRPC or publishing it to MQTT updates the command lifecycle only.
- A successful control result means the agent dispatched the typed MQTT payload.
- Physical print state remains report-derived from later printer MQTT reports and existing print reconciliation.

This separation prevents a successful MQTT publish from being treated as proof that a printer paused, resumed, stopped, or changed speed.

## Local Verification

These commands are no-network checks. They use compatibility tests, fake MQTT transports, command handler tests, and Hub route/repository/gRPC tests.

```sh
cargo test -p pandar-core compatibility
cargo test -p pandar-agent configured_control_printer
cargo test -p pandar-agent printer_control
cargo test -p pandar-hub printer_control
```

Observed locally on 2026-06-24:

- `cargo test -p pandar-core compatibility`: 5 passed.
- `cargo test -p pandar-agent configured_control_printer`: 3 passed.
- `cargo test -p pandar-agent printer_control`: 6 passed.
- `cargo test -p pandar-hub printer_control`: 10 passed.

## Real-Printer Probe Status

Not run for Phase 27 live controls.

`docs/bambu-lan-printer-probe-2026-06-24.md` records real-printer evidence for MQTT connectivity, `pushall`, `gcode_line`, and `get_version` on an X2D. It does not record pause, resume, stop, or print-speed probes, so this document makes no hardware compatibility claim for those controls.
