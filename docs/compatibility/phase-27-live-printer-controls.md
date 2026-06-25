# Phase 27 Live Printer Controls

This note records the Phase 27 compatibility policy for typed live printer controls. It is reference-backed and locally verified without opening live printer sockets.

## Reference Payloads

Publish all live-control commands to `device/{serial}/request` with MQTT QoS `1`.

| Control     | Hub action                              | MQTT payload                                                                   |
| ----------- | --------------------------------------- | ------------------------------------------------------------------------------ |
| Pause       | `pause`                                 | `{"print":{"command":"pause","sequence_id":"0"}}`                              |
| Resume      | `resume`                                | `{"print":{"command":"resume","sequence_id":"0"}}`                             |
| Stop        | `stop`                                  | `{"print":{"command":"stop","sequence_id":"0"}}`                               |
| Print speed | `set_print_speed` with `speed_mode` 1-4 | `{"print":{"command":"print_speed","param":"<speed_mode>","sequence_id":"0"}}` |

Print-speed modes outside `1..=4` are invalid. Non-speed actions must not carry `speed_mode`.

## Compatibility Policy

The Hub is authoritative for compatibility and authorization. The frontend can mirror this policy for disabled controls, but the Hub must still reject unsupported or unknown models.

| User-facing model        | Normalized model               | Aliases                            | Live controls                   |
| ------------------------ | ------------------------------ | ---------------------------------- | ------------------------------- |
| A1                       | `A1`                           | `BambuLab A1`                      | Supported                       |
| A1 Mini                  | `A1_MINI`                      | `A1M`, `A1MIN`, `BambuLab A1 Mini` | Supported                       |
| P2S                      | `P2S`                          | `N7`                               | Supported                       |
| X2D                      | `X2D`                          | `N6`                               | Supported                       |
| Missing or unknown model | none or normalized unknown key | none                               | Unknown; reject control enqueue |

Phase 27 dispatched only typed controls: pause, resume, stop, and print speed. Phase 29 moves the command kind to protocol-defined `printer_operation` and expands the semantic operation set to home, relative move axes, and hotend temperature. Raw MQTT or arbitrary printer commands remain outside the operator control path.

For Bambu printers, Phase 29 intentionally collapses every home operation to bare `G28` in the agent adapter. Axis-specific home intent may exist in the Pandar protocol for future device families, but the Bambu adapter must not publish `G28 X`, `G28 Y`, or `G28 Z`.

## Lifecycle Boundary

Live printer controls are dispatch commands, not physical print-state mutations.

- Enqueueing a control creates a durable `printer_operation` command and audit record.
- Sending the command over gRPC or publishing it to MQTT updates the command lifecycle only.
- A successful control result means the agent dispatched the typed MQTT payload.
- Physical print state remains report-derived from later printer MQTT reports and existing print reconciliation.

This separation prevents a successful MQTT publish from being treated as proof that a printer paused, resumed, stopped, or changed speed.

## Local Verification

These Phase 29 commands are no-network checks. They use compatibility tests, fake MQTT transports, command handler tests, network plugin parser tests, and Hub route/repository/gRPC tests.

```sh
cargo test -p pandar-core compatibility
cargo test -p pandar-agent printer_operation
cargo test -p pandar-agent configured_operate_printer
cargo test -p pandar-network-plugin
cargo test -p pandar-hub printer_control
```

Observed locally on 2026-06-25 after Phase 29:

- `cargo fmt -- --check`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`: 570 passed.
- `npm run build` in `frontend/`: passed with Next.js 16.2.9, including production compilation and TypeScript checks for the live-control UI/server-action path.

Earlier Phase 27 evidence used the older `printer_control` command kind; Phase 29 preserves the tenant `/controls` HTTP route name but persists and dispatches `printer_operation`.

## Real-Printer Probe Status

| Date       | Printer                                                         | Controls                                                    | Result    | Evidence                                                                                                                                                                                                                                                                                                       |
| ---------- | --------------------------------------------------------------- | ----------------------------------------------------------- | --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-06-24 | Bambu Lab X2D from `docs/bambu-lan-printer-probe-2026-06-24.md` | pause, resume, stop, print speed                            | `blocked` | The prior real-printer probe used an interactively supplied LAN access code and recorded MQTT connectivity, `pushall`, `gcode_line`, and `get_version` only. The current workspace has no `PANDAR_PRINTERS` configuration or printer access code, so typed live-control probes cannot be repeated safely here. |
| 2026-06-25 | none configured                                                 | pause, resume, stop, print speed, Phase 29 home/move/hotend | `blocked` | Fresh environment check found no `PANDAR_PRINTERS` configuration. No printer access code or operator-selected safe printer state is available in this workspace, so live-control hardware probes were not attempted.                                                                                           |

`docs/bambu-lan-printer-probe-2026-06-24.md` records real-printer evidence for MQTT connectivity, `pushall`, `gcode_line`, and `get_version` on an X2D. It does not record pause, resume, stop, or print-speed probes, so this document makes no hardware compatibility claim for those controls.

Do not run Phase 27 live-control probes against a real printer unless the operator has selected a safe machine state and supplied agent-local LAN credentials outside source control. Record failed and blocked attempts here because they are compatibility evidence.
