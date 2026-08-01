# Bambu Lab H2C compatibility

Pandar supports the H2C FDM workflow used by current Bambu Studio builds while keeping unverified hardware behavior disabled. The implementation treats H2C as a nozzle-rack machine, not as an H2D-style fixed dual-nozzle printer.

## Supported boundary

- Printer model aliases `O1C` and `O1C2` normalize to H2C. Studio receives the printer's observed model identifier, including canonical rack profile `O1C2`.
- Agent MQTT telemetry decodes `print.device.nozzle`, sibling `print.device.holder`, and extruder `snow` / `hnow` routing. Nozzle-only and holder-only delta reports merge independently.
- Hub stores the normalized nozzle system and validated raw `print.fun2` evidence behind the current Agent session fence for both SQLite and PostgreSQL. A replacement session cannot advertise a prior session's rack state, and the plugin deliberately omits `fun2` so storage evidence cannot enable eMMC behavior.
- Studio receives the observed nozzle inventory, holder state, and extruder routing. `print.fun` bit 60 is visible only when the current Agent session advertises H2C auto-mapping support and current-session rack telemetry contains a physical rack nozzle.
- Studio `get_auto_nozzle_mapping` V0 and V1 requests cross the plugin, Hub, gRPC, Agent, and printer MQTT boundaries as typed messages.
- Mapping replies must match both `print.command` and `sequence_id`. Successful replies additionally require the requested protocol version and a non-empty mapping containing only `-1` or physical nozzle IDs `0`, `1`, and `16` through `21`.
- Correlated printer failure replies retain their `reason`, `errno`, detail, and unknown fields even when their version is absent, future, or malformed. Timeouts and missing, uncorrelated, or invalid success replies fail terminally.
- Studio FDM submissions preserve the slicer's `nozzle_mapping`, `ams_mapping2`, `ams_mapping_info`, and nozzle metadata through dispatch. H2C Studio submissions and reprints require a validated physical nozzle mapping. Web uploads without Studio mapping metadata are rejected with `h2c_nozzle_mapping_required` instead of guessing a rack slot.

## Deliberately unsupported

Pandar does not infer H2C behavior from H2D or resource-profile similarity. The following remain disabled until captured hardware behavior and an explicit implementation prove them safe:

- H2C command signing or nozzle-ID rewriting.
- Nozzle-rack movement, replacement, maintenance, or calibration controls outside Studio's mapping request.
- Laser and cutting workflows.
- eMMC printing and other `fun2`-gated behavior.
- Any physical nozzle ID outside `0`, `1`, and `16` through `21`.

The plugin does not send `nozzles_info` to printer MQTT; this follows the existing captured Studio print contract. The field remains typed Studio metadata only.

## Source basis

The implementation is based on the tracked Bambu Studio source and resources:

- `reference/BambuStudio/resources/printers/O1C.json`
- `reference/BambuStudio/resources/printers/O1C2.json`
- `reference/BambuStudio/resources/profiles/BBL/machine/Bambu Lab H2C.json`
- `reference/BambuStudio/resources/profiles/BBL/machine/Bambu Lab H2C 0.4 nozzle.json`
- `reference/BambuStudio/src/slic3r/GUI/DeviceCore/DevMappingNozzle.cpp`
- `reference/BambuStudio/src/slic3r/GUI/DeviceCore/DevNozzleRack.cpp`
- `reference/BambuStudio/src/slic3r/GUI/DeviceCore/DevNozzleSystem.cpp`
- `reference/BambuStudio/src/slic3r/GUI/DeviceManager.cpp`
- `reference/BambuStudio/src/slic3r/GUI/SelectMachine.cpp`
- `reference/BambuStudio/src/slic3r/GUI/Jobs/PrintJob.cpp`

These sources establish the rack topology, bit-60 capability gate, mapping request versions, response correlation, and physical mapping handoff. They do not establish the unsupported behaviors listed above.

## Hardware acceptance still required

Tracked by [GitHub issue #2](https://github.com/ProjectPandar/pandar/issues/2). A safe H2C hardware session should verify, without enabling unsupported controls:

1. Capture a full `push_status` plus separate nozzle-only and holder-only deltas and compare the projected Studio device state.
2. Run both mapping request versions from Studio and record correlated success and printer-declared failure replies.
3. Confirm Studio receives the exact response and retains physical IDs in the subsequent FDM print submission.
4. Inspect the final MQTT `project_file` payload before authorizing a small FDM print, confirming the slicer-provided mapping is unchanged.
5. Reconnect the Agent and confirm bit 60 stays hidden until fresh rack telemetry arrives for the replacement session.

Until this checklist is completed on real H2C hardware, Pandar claims source-backed protocol compatibility and fail-closed dispatch behavior, not live-print validation.
