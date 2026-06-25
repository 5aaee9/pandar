# Bambu LAN Printer Probe - 2026-06-24

This note records the real-printer probe that validated Pandar's agent-side Bambu LAN MQTT path and the follow-up MQTT packet/logging fixes.

## Device

- Host: `10...24`
- MQTT port: `8883`
- FTPS port: `990`
- MQTT username: `bblp`
- Serial: `20...74`
- Product: `Bambu Lab X2D`
- Firmware: `01.01.01.00`

The printer access code was supplied interactively for the probe. Do not commit it to source, docs, tests, fixtures, or logs.

## MQTT Topics

- Report topic: `device/20...74/report`
- Request topic: `device/20...74/request`

The printer accepted TLS MQTT connections on port `8883` with username `bblp` and the LAN access code as the password.

## Commands Tested

Requesting a full status report:

```json
{ "pushing": { "command": "pushall" } }
```

The authenticated status refresh returned a report whose normalized printer state was `IDLE`.

Sending a Home command through the raw MQTT command path:

```json
{ "print": { "command": "gcode_line", "param": "G28", "sequence_id": "90001" } }
```

The printer reported `gcode_line` acknowledgements and then returned to `IDLE`, confirming that direct command publish through `device/{serial}/request` works.

Querying version/details:

```json
{ "info": { "command": "get_version", "sequence_id": "90002" } }
```

The response identified the machine as `Bambu Lab X2D` with firmware `01.01.01.00`.

## Transport Findings

- SSDP discovery did not return a response during this probe.
- MQTT over TLS was reachable and usable.
- FTPS on port `990` was reachable, but a writable storage probe timed out during transfer mode attempts.
- The full `pushall` report exceeded rumqttc's previous effective packet limit for this printer report shape.

## Code Changes From Probe

The agent now builds runtime Bambu LAN MQTT options through `bambu_lan_mqtt_options` and sets the MQTT packet limit to `256 * 1024` bytes for both inbound and outbound packets.

The agent also logs full MQTT report receive error chains at warning level. This matters for errors such as `payload size limit exceeded`: the root cause now appears in logs instead of only being returned up the refresh/report path.

The background print-report loop now uses the generic message `printer report receive failed` instead of labeling all receive failures as timeouts.

## Verification

Commands run after the fix:

```sh
cargo fmt --check
cargo test -p pandar-agent
cargo clippy -p pandar-agent --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Observed results:

- `cargo test -p pandar-agent`: 101 tests passed.
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`: 424 tests passed.

## Follow-Up Notes

- Keep LAN credentials local to `pandar-agent`; do not store printer access codes in the hub or frontend.
- Revisit FTPS runtime behavior separately. This probe confirmed listener reachability but not successful upload/delete behavior.
- This probe did not exercise Phase 27 typed pause, resume, stop, or print-speed controls; those require a separate operator-approved run in a safe printer state.
- If MQTT report receive fails again, inspect warning logs for the full `{err:#}` chain before changing protocol behavior.
