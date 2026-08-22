# Bambu Studio 02.08.01.55 Command Disposition Matrix

This document records the command-disposition audit derived from Bambu Studio commit
`ba049f6a2e08c3b6033660bb84da80c08722974b` (Studio `02.08.01.55`, network agent
`02.08.01.52`). `02.08.01` is an active cataloged ABI series; real-Studio compatibility claims still
require the platform-specific evidence recorded in `bambu-studio-plugin.md`.

## Contract Summary

Pinned Studio has 66 finite top-level envelope/command pairs. Pandar assigns exactly one outcome to
each pair:

| Outcome                  | Count | Contract                                                                                                                                                         |
| ------------------------ | ----: | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `handled`                |    21 | Parse a known typed shape and deliver it through the named status, semantic operation, or firmware path.                                                         |
| `explicitly_unsupported` |    45 | Return `BAMBU_NETWORK_ERR_INVALID_RESULT` (`-19`) with `{"error":"unsupported_printer_operation"}` and make no Hub printer-operation request or Studio callback. |
| `benign_noop`            |     0 | No target command has evidence that a success no-op is required.                                                                                                 |

Malformed shapes of handled commands, unknown commands, unknown top-level envelopes, and messages
containing more than one top-level envelope are `explicitly_unsupported`. An empty Cloud `dev_id` is
an invalid ABI target and returns `BAMBU_NETWORK_ERR_CONNECT_FAILED` (`-2`) without reusing the
selected printer, refreshing the printer cache, sending a Hub request, or invoking a callback. A LAN
message also returns `-2` unless its `dev_id` has a current active local generation.

Every ordinary Studio producer below calls `MachineObject::publish_json`. Pinned
`DeviceManager.cpp:2428-2461` selects `send_message_to_printer` for a LAN-mode printer and
`send_message` otherwise, so every row has the same `Cloud/LAN` caller surface. The plugin preserves
that tunnel only for a current target; it does not fall back between Cloud and LAN.

The LAN-shaped surface is a Hub-backed virtual tunnel, not a direct LAN implementation.
`connect_printer` uses only an authorized `dev_id`. It ignores and scrubs the passed host/IP, username,
password, and SSL flags, records only a fresh generation-scoped Hub target, and opens no direct printer
socket. Local status and commands continue through Pandar's Hub/Agent path. Discovery, bind,
certificate handling, MQTT, FTPS, and File Transfer remain outside the plugin; Studio sees
`connection_type:"cloud"` for either tunnel.

Rust classifies one generic message with strict precedence firmware -> status -> semantic operation ->
unsupported. Invalid firmware-shaped input remains a firmware error and is not retried as another
class. A handled status request returns success only when its current eligible callback actually
receives the payload; a target that is neither selected nor explicitly subscribed, a failed refresh,
a missing callback, or a stale final claim returns `BAMBU_NETWORK_ERR_CONNECT_FAILED` (`-2`).

The source paths in the tables are relative to `src/slic3r/GUI/` at the pinned commit. `AUTO` means
an automatic lifecycle request, `CORE` a normal device control, `FUN` a `fun`/`fun2`/`cfg` or device
telemetry gate, `ACTION` an error-action catalog entry, `AMS` AMS state/configuration, `CAL`
calibration UI, `CAM` camera/storage state, `NOZZLE` nozzle-system state, `MODEL` a model/protocol
branch, `FW` firmware UI, and `DORMANT` no caller found beyond the bounded builder.

## Camera Envelope

|   # | Command                                | Pinned producer                      | Gate                            | Disposition              | Pandar path or alternative                                                                                |
| --: | -------------------------------------- | ------------------------------------ | ------------------------------- | ------------------------ | --------------------------------------------------------------------------------------------------------- |
|   1 | `camera.ipcam_cap_pic_set`             | `DeviceCore/DevPrintOptions.cpp:543` | CAM, `cfg` bits 38-39           | `explicitly_unsupported` | Pandar clears the camera capability in Studio; use the Pandar monitor outside the plugin when configured. |
|   2 | `camera.ipcam_delete_oldest_timelapse` | `DeviceManager.cpp:2156`             | CAM, internal timelapse/storage | `explicitly_unsupported` | No Studio plugin alternative.                                                                             |
|   3 | `camera.ipcam_get_media_info`          | `DeviceManager.cpp:2145`             | CAM, internal timelapse/storage | `explicitly_unsupported` | No Studio plugin alternative.                                                                             |
|   4 | `camera.ipcam_record_set`              | `DeviceManager.cpp:2104`             | CAM, IP camera and SD card      | `explicitly_unsupported` | No Studio plugin alternative.                                                                             |
|   5 | `camera.ipcam_resolution_set`          | `DeviceManager.cpp:2126`             | CAM, advertised resolution list | `explicitly_unsupported` | No Studio plugin alternative.                                                                             |
|   6 | `camera.ipcam_timelapse`               | `DeviceManager.cpp:2115`             | DORMANT                         | `explicitly_unsupported` | No caller evidence permits a success no-op.                                                               |

## Information And Pushing Envelopes

|   # | Command            | Pinned producer                                                  | Gate                              | Disposition              | Pandar path or alternative                                                                                                         |
| --: | ------------------ | ---------------------------------------------------------------- | --------------------------------- | ------------------------ | ---------------------------------------------------------------------------------------------------------------------------------- |
|   7 | `info.get_version` | `DeviceManager.cpp:1282`                                         | AUTO, connection/firmware refresh | `handled`                | Typed status request; emit the current version response on the same claimed tunnel.                                                |
|  53 | `pushing.pushall`  | `DeviceManager.cpp:1318`                                         | AUTO, status refresh              | `handled`                | Typed status request; emit current fresh status on the same claimed tunnel.                                                        |
|  54 | `pushing.start`    | `DeviceManager.cpp:1339`; caller `DeviceCore/DevManager.cpp:115` | AUTO, stale-push watchdog         | `explicitly_unsupported` | A fresh online full status reports MQTT liveness without claiming this unsupported subscription-control command; no success no-op. |
|  55 | `pushing.stop`     | `DeviceManager.cpp:1339`                                         | DORMANT                           | `explicitly_unsupported` | Correct support would require tunnel-scoped push suspension; no caller evidence permits a no-op.                                   |

## Print Envelope

|   # | Command                                    | Pinned producer                                                                   | Gate                             | Disposition              | Pandar path or alternative                                                                                                                                    |
| --: | ------------------------------------------ | --------------------------------------------------------------------------------- | -------------------------------- | ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
|   8 | `print.ams_change_filament`                | `DeviceManager.cpp:1615`                                                          | AMS tray/extruder state          | `handled`                | Exact typed load/unload shapes become semantic AMS operations.                                                                                                |
|   9 | `print.ams_control`                        | `DeviceManager.cpp:1737`                                                          | AMS/ACTION                       | `explicitly_unsupported` | No independent hide gate; use the printer's native controls.                                                                                                  |
|  10 | `print.ams_filament_drying`                | `DeviceCore/DevFilaSystemCtrl.cpp:28,45`                                          | FUN, `fun2` bit 5 and AMS type   | `explicitly_unsupported` | `fun2` is not advertised; use the printer's native controls.                                                                                                  |
|  11 | `print.ams_filament_setting`               | `DeviceManager.cpp:1685`                                                          | AMS material editor              | `explicitly_unsupported` | No independent hide gate; material telemetry remains available.                                                                                               |
|  12 | `print.ams_get_rfid`                       | `DeviceManager.cpp:1715`                                                          | AMS/protocol selection           | `handled`                | Exact typed reread request becomes a semantic AMS operation.                                                                                                  |
|  13 | `print.ams_reset`                          | `DeviceCore/DevFilaSystemCtrl.cpp:14`                                             | AMS reorder capability           | `explicitly_unsupported` | Reorder capability is not advertised.                                                                                                                         |
|  14 | `print.ams_user_setting`                   | `DeviceManager.cpp:1647`                                                          | AMS settings                     | `explicitly_unsupported` | No independent hide gate; use the printer's native controls.                                                                                                  |
|  15 | `print.auto_stop_ams_dry`                  | `DeviceManager.cpp:1748`                                                          | ACTION                           | `explicitly_unsupported` | No independent hide gate; use the printer's native controls.                                                                                                  |
|  16 | `print.back_to_center`                     | `DeviceCore/DevAxisCtrl.cpp:13`                                                   | FUN, homing support              | `handled`                | Typed Home operation, gated by exact current printer feature bit 32; legacy `G28` remains separate.                                                           |
|  17 | `print.buzzer_ctrl`                        | `DeviceManager.cpp:1531`                                                          | ACTION                           | `explicitly_unsupported` | Unsupported action entries must not be synthesized.                                                                                                           |
|  18 | `print.calibration`                        | `DeviceManager.cpp:1884`                                                          | CAL/model; no single bridge gate | `explicitly_unsupported` | No independent hide gate; a future typed calibration slice is required.                                                                                       |
|  19 | `print.clean_print_error`                  | `DeviceManager.cpp:1349`                                                          | ACTION dialog cleanup            | `explicitly_unsupported` | No independent hide gate; no Hub operation is created.                                                                                                        |
|  20 | `print.close_air_filt`                     | `DeviceManager.cpp:1482`                                                          | ACTION/FUN bit 46                | `explicitly_unsupported` | Pandar clears bit 46 from the Studio projection; action entries are not synthesized.                                                                          |
|  21 | `print.extrusion_cali`                     | `DeviceManager.cpp:1759,1902`                                                     | CAL/FUN bit 7                    | `explicitly_unsupported` | Pandar clears the calibration capability from the Studio projection.                                                                                          |
|  22 | `print.extrusion_cali_del`                 | `DeviceManager.cpp:1982`                                                          | CAL/FUN bit 7                    | `explicitly_unsupported` | Pandar clears the calibration capability from the Studio projection.                                                                                          |
|  23 | `print.extrusion_cali_get`                 | `DeviceManager.cpp:2001`                                                          | CAL/FUN bit 7                    | `explicitly_unsupported` | Pandar clears the calibration capability from the Studio projection.                                                                                          |
|  24 | `print.extrusion_cali_get_result`          | `DeviceManager.cpp:2021`                                                          | CAL/FUN bit 7                    | `explicitly_unsupported` | Pandar clears the calibration capability from the Studio projection.                                                                                          |
|  25 | `print.extrusion_cali_sel`                 | `DeviceManager.cpp:2031`                                                          | CAL/FUN bit 7                    | `explicitly_unsupported` | Pandar clears the calibration capability from the Studio projection.                                                                                          |
|  26 | `print.extrusion_cali_set`                 | `DeviceManager.cpp:1785,1944`                                                     | CAL/FUN bit 7                    | `explicitly_unsupported` | Pandar clears the calibration capability from the Studio projection.                                                                                          |
|  27 | `print.flowrate_cali`                      | `DeviceManager.cpp:2053`                                                          | CAL/FUN bit 6                    | `explicitly_unsupported` | Pandar clears the calibration capability from the Studio projection.                                                                                          |
|  28 | `print.flowrate_get_result`                | `DeviceManager.cpp:2094`                                                          | CAL/FUN bit 6                    | `explicitly_unsupported` | Pandar clears the calibration capability from the Studio projection.                                                                                          |
|  29 | `print.gcode_file`                         | `DeviceManager.cpp:1878`                                                          | MODEL, old-X1 calibration branch | `explicitly_unsupported` | This is not the typed `gcode_line` path; no fallback.                                                                                                         |
|  30 | `print.gcode_line`                         | `DeviceManager.cpp:3729`                                                          | CORE/legacy protocol             | `handled`                | A string `param` is dispatched once as typed G-code; known Home/axis/temperature forms first become semantic operations. Unwrapped raw G-code is unsupported. |
|  31 | `print.get_auto_nozzle_mapping`            | `DeviceCore/DevMappingNozzle.cpp:68,195`                                          | NOZZLE                           | `handled`                | Typed H2C V0/V1 request/reply path with exact command/sequence correlation, strict successful physical mappings, and detailed correlated printer failures.    |
|  32 | `print.holder_nozzle_refresh`              | `DeviceCore/DevNozzleRackCtrl.cpp:101,190`                                        | NOZZLE/FUN bit 60                | `explicitly_unsupported` | Bit 60 is exposed only for a current capable H2C rack session, but this distinct maintenance command remains rejected.                                        |
|  33 | `print.idle_ignore`                        | `DeviceManager.cpp:1433`                                                          | ACTION                           | `explicitly_unsupported` | No independent hide gate; no Hub operation is created.                                                                                                        |
|  34 | `print.ignore`                             | `DeviceManager.cpp:1459`                                                          | ACTION                           | `handled`                | Only the exact native print-error shape with `param:"reserve"`, valid error/job/sequence fields is handled; ordinary or malformed shapes are unsupported.     |
|  35 | `print.nozzle_holder_ctrl`                 | `DeviceCore/DevNozzleRackCtrl.cpp:35,51`                                          | NOZZLE/FUN bit 60                | `explicitly_unsupported` | Current-session H2C rack visibility does not authorize holder movement or control; the command remains rejected.                                              |
|  36 | `print.nozzle_info_confirm`                | `DeviceCore/DevNozzleRackCtrl.cpp:83,92`                                          | NOZZLE/FUN bit 60                | `explicitly_unsupported` | Current-session H2C rack visibility does not authorize nozzle confirmation or maintenance; the command remains rejected.                                      |
|  37 | `print.pause`                              | `DeviceManager.cpp:1409`                                                          | CORE print state                 | `handled`                | Typed semantic Pause operation.                                                                                                                               |
|  38 | `print.print_option`                       | `DevPrintOptions.cpp:438,482,494,504,521,552`; `DeviceManager.cpp:1816,1828,1842` | FUN/cfg/AMS options              | `explicitly_unsupported` | Live option mutation is unsupported; Task 6 separately classifies print-submission fields.                                                                    |
|  39 | `print.print_speed`                        | `DeviceManager.cpp:1805`                                                          | CORE print state                 | `handled`                | Valid speed mode becomes a typed semantic operation.                                                                                                          |
|  40 | `print.refresh_nozzle`                     | `DeviceManager.cpp:1592`                                                          | NOZZLE/ACTION                    | `explicitly_unsupported` | Refresh capability/action is not advertised.                                                                                                                  |
|  41 | `print.resume`                             | `DeviceManager.cpp:1421,1445`                                                     | CORE/ACTION                      | `handled`                | Ordinary Resume and the exact valid native print-error form are handled; malformed native candidates are unsupported.                                         |
|  42 | `print.select_extruder`                    | `DeviceCore/DevCtrl.cpp:63`                                                       | FUN/extruder topology            | `handled`                | Valid extruder id becomes a typed semantic operation.                                                                                                         |
|  43 | `print.set_against_continued_heating_mode` | `DeviceCore/DevPrintOptions.cpp:474`                                              | FUN bit 62/cfg option            | `explicitly_unsupported` | Pandar clears bit 62 from the Studio projection.                                                                                                              |
|  44 | `print.set_airduct`                        | `DeviceCore/DevFan.cpp:112`                                                       | FUN, `device.airduct`            | `explicitly_unsupported` | Pandar does not advertise `device.airduct`.                                                                                                                   |
|  45 | `print.set_bed_temp`                       | `DeviceManager.cpp:1543`                                                          | FUN bit 39/new protocol          | `handled`                | Valid target becomes a typed semantic bed-temperature operation.                                                                                              |
|  46 | `print.set_ctt`                            | `DeviceCore/DevChamberCtrl.cpp:10`                                                | FUN/chamber                      | `handled`                | Valid target becomes a typed semantic chamber-temperature operation; truthful chamber visibility is Task 5.                                                   |
|  47 | `print.set_extrusion_length`               | `DeviceManager.cpp:1854`                                                          | MODEL, new protocol              | `explicitly_unsupported` | No independent hide gate; Studio's legacy G-code path is distinct.                                                                                            |
|  48 | `print.set_fan`                            | `DeviceCore/DevFan.cpp:80`                                                        | FUN, new airduct protocol        | `explicitly_unsupported` | Pandar omits new fan capability, so Studio uses the handled typed `gcode_line`/legacy `M106` path.                                                            |
|  49 | `print.set_nozzle_temp`                    | `DeviceManager.cpp:1582`                                                          | CORE/extruder                    | `handled`                | Valid target and extruder become a typed semantic hotend-temperature operation.                                                                               |
|  50 | `print.skip_objects`                       | `DeviceManager.cpp:1378`                                                          | FUN bit 49                       | `explicitly_unsupported` | Pandar clears bit 49 from the Studio projection; no stop-command substitution.                                                                                |
|  51 | `print.stop`                               | `DeviceManager.cpp:1388,1398,1470`                                                | CORE/ACTION                      | `handled`                | Ordinary Stop and the exact valid native print-error form are handled; malformed native candidates are unsupported.                                           |
|  52 | `print.xyz_ctrl`                           | `DeviceCore/DevAxisCtrl.cpp:32`                                                   | FUN axis control                 | `handled`                | Exact uppercase axis/direction/mode shape becomes a typed MoveAxes operation gated by current feature bit 38.                                                 |

## System Envelope

|   # | Command                  | Pinned producer                    | Gate                         | Disposition              | Pandar path or alternative                                                                                  |
| --: | ------------------------ | ---------------------------------- | ---------------------------- | ------------------------ | ----------------------------------------------------------------------------------------------------------- |
|  56 | `system.get_access_code` | `DeviceManager.cpp:1291`           | AUTO after printer-connected | `explicitly_unsupported` | Machine credentials remain outside the Studio plugin control path; the automatic call receives non-success. |
|  57 | `system.ledctrl`         | `DeviceCore/DevLampCtrl.cpp:39,53` | FUN/light node               | `handled`                | Only `chamber_light` or `chamber_light2` with mode `on`/`off` becomes a typed chamber-light operation.      |
|  58 | `system.print_cache_set` | `DeviceManager.cpp:4864`           | FUN/eMMC cache               | `explicitly_unsupported` | Cache capability is not advertised.                                                                         |
|  59 | `system.set_door_stat`   | `DeviceManager.cpp:4842`           | FUN bit 12                   | `explicitly_unsupported` | Pandar clears bit 12 from the Studio projection.                                                            |
|  60 | `system.uiop`            | `DeviceManager.cpp:1360`           | ACTION dialog cleanup        | `explicitly_unsupported` | No independent hide gate; no Hub operation is created.                                                      |

## Upgrade Envelope

|   # | Command                               | Pinned producer                           | Gate                 | Disposition              | Pandar path or alternative                                                                                                                                                                  |
| --: | ------------------------------------- | ----------------------------------------- | -------------------- | ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
|  61 | `upgrade.consistency_confirm`         | `DeviceCore/DevUpgradeCtrl.cpp:20`        | FW                   | `handled`                | Typed prepare/execute firmware control with current generation, redaction, and at-most-once delivery rules.                                                                                 |
|  62 | `upgrade.mc_for_ams_firmware_upgrade` | `DeviceCore/DevFilaAmsSettingCtrl.cpp:16` | FW/AMS               | `handled`                | Typed AMS firmware-switch control with the same delivery rules.                                                                                                                             |
|  63 | `upgrade.start`                       | `DeviceCore/DevUpgradeCtrl.cpp:33,46`     | FW                   | `handled`                | Exact nonempty URL/module/version shape uses typed prepare/execute; no retry or replay after an ambiguous publish.                                                                          |
|  64 | `upgrade.upgrade_confirm`             | `DeviceCore/DevUpgradeCtrl.cpp:11`        | FW                   | `handled`                | Typed prepare/execute firmware confirmation.                                                                                                                                                |
|  65 | `upgrade.wtm_upgrade`                 | `DeviceCore/DevNozzleRackCtrl.cpp:276`    | FW/NOZZLE/FUN bit 60 | `explicitly_unsupported` | H2C mapping support does not extend to rack firmware upgrades. An exact envelope is rejected before Hub publish; no firmware token, live fallback, replay, or synthetic package is created. |

## XCam Envelope

|   # | Command                 | Pinned producer                      | Gate                                | Disposition              | Pandar path or alternative                                              |
| --: | ----------------------- | ------------------------------------ | ----------------------------------- | ------------------------ | ----------------------------------------------------------------------- |
|  66 | `xcam.xcam_control_set` | `DeviceCore/DevPrintOptions.cpp:459` | FUN bits 42-45/cfg detection option | `explicitly_unsupported` | Pandar clears the detection capability bits from the Studio projection. |

## Visibility And Capability Enforcement

Pandar must preserve the authoritative device bitmap in Hub storage but apply a plugin-transport mask
when projecting it to Studio. The required mask clears bits 6, 7, 8, 9, 10, 12, 13, 28, 40, 42-46,
49, and 62 because this matrix leaves their commands unsupported. Bit 60 remains masked unless a
current Agent session both supports H2C auto-mapping and has current-session physical rack telemetry.
Implemented axis bits 32 and 38 and unrelated unknown bits must be preserved. Snapshot `cfg` bits 38-39 must also be cleared. This
mask is about what the Pandar Studio bridge can execute, not a rewrite of the printer's observed
hardware state.

Pandar does not advertise `device.airduct`, new fan protocol fields, `fun2`, nozzle-refresh support,
remote storage, advanced calibration configuration, or AI-monitor configuration. The Studio camera
path is explicitly unavailable in both status and camera ABIs. SD-card availability is true only when
authoritative `aux` bits 12-13 encode state 1; missing, malformed, or other states remain unavailable.
Chamber support is true only when both current and target temperatures were observed, and fresh online
full status is the only source of MQTT liveness. Network, chamber, storage, push-liveness, camera
protocol, and camera URL fields therefore agree, closing the capability-visibility gate.

Some target commands have no independent Studio hide gate. In particular, basic AMS presence is also
required for handled RFID/load/unload operations, and real nozzle/extruder/error state cannot be
deleted merely to hide advanced AMS, extrusion, or error-action commands. Those rows remain visible
where Studio has no finer gate, but every invocation has the same explicit `-19`, stable body, zero
Hub-operation, and zero-callback contract. Pandar does not claim that a missing Studio gate is a
supported capability.

## Dynamic Error Actions

Pinned `DeviceManager.cpp:1488-1525` has one producer outside the finite list: it copies a command
name from a printer error-action document into `print.command`. This does not authorize arbitrary
success. Pandar accepts only its already typed exact native Resume, Ignore, and Stop shapes. Any other
copied command or malformed native candidate is `explicitly_unsupported` under the same stable-error
contract.
