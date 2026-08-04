# H2C hardware probe — 2026-08-04

This note records a safe-idle H2C probe against the current Pandar source. The printer host and certificate identity are redacted, and the access code was supplied ephemerally. No access code, Hub token, Agent credential, or unredacted raw report is stored in the repository.

## Device and safety boundary

- Private host: `10.1.61.x`
- MQTT TLS: reachable on port `8883`
- FTPS TLS: reachable on port `990`
- Certificate issuer profile: `BBL Device CA O1C2-V2`
- Reported product: `Bambu Lab H2C`
- Main firmware: `01.02.00.00`
- State during the probe: `IDLE`

The operator declined an actual print and a physical nozzle-rack operation. The probe therefore did not upload, delete, print, move the rack, issue rack maintenance/control, sign H2C commands, use laser/cut, use eMMC/`fun2`, or introduce physical nozzle IDs outside the implemented boundary.

## Read-only transport evidence

A 30-second authenticated MQTT capture received 26 reports, including 25 full `print.push_status` reports. Every observed full report used `msg: 0` and the firmware-fixed `sequence_id: "2021"` rather than echoing Pandar's `pushing.pushall` sequence.

The full rack report contained:

- Physical nozzle IDs `0`, `1`, `16`, `17`, `19`, `20`, and `21`.
- Diameter observations `0.4`, `0.4`, `0.6`, `0.8`, `0.4`, `0.4`, and `0.4` mm for those IDs.
- `src_id: 16`, `tar_id: 18`, holder `stat: 0`, `pos: 1`, and `info: 1`.
- Two extruder observations with current physical `hnow` IDs `0` and `1`.
- `print.fun: "14027FF18FFF9CB7"`, whose bit 60 is set.
- A present raw `fun2` field. The Studio printer projection omitted `fun2` as required, and the probe did not use it to enable behavior.

No nozzle-only or holder-only delta occurred during the passive observation window. Because the operator declined a physical rack action and Pandar deliberately does not expose unverified rack controls, split-delta hardware capture remains open.

A protected FTPS root listing completed through `PBSZ 0` plus `PROT P` and returned zero visible entries. The probe did not write or remove a file.

## Auto-mapping evidence

Direct Agent MQTT and the isolated Hub plugin HTTP boundary (`plugin HTTP -> Hub -> gRPC -> Agent -> printer MQTT`) produced the same correlated results:

| Request | Correlation | Printer result | Version | Physical mapping / failure |
| --- | --- | --- | --- | --- |
| V1, sequence `820001` | command and sequence matched | `success` | `1` | `[1, -1 x31]` |
| V0, sequence `820002` | command and sequence matched | `success` | omitted, interpreted as V0 | `[1, -1 x31]`; unknown `PA_used` retained |
| V1 unavailable group, sequence `820003` | command and sequence matched | `fail` | `1` | `errno: 4`, no mapping |

Pandar accepted both successful responses only after validating the exact requested version and physical IDs. The correlated printer-declared failure remained a valid terminal response and preserved its observed `errno`, even though this firmware supplied no `reason` or `detail`.

After the presence fix described below, a new V1 request with sequence `830001` again returned the exact correlated success mapping through the isolated Hub/Agent/printer chain.

This verifies the live printer boundary and the HTTP endpoint used by the network plugin. It is not evidence that a real Bambu Studio process received the callback or retained the mapping in a sliced FDM submission.

## Session fence and fixed-sequence finding

The Agent was replaced with a capable session that could not reach the printer. Before any fresh rack telemetry from that replacement session:

- Studio projection returned `fun: "0"` and no nozzle system.
- H2C auto-mapping failed closed with HTTP 400 `h2c_auto_nozzle_mapping_unavailable`.

After reconnecting to the real printer, current-session rack telemetry restored the seven-nozzle system and bit 60. The probe also exposed a presence bug: rack state recovered, but Studio still reported the printer offline because H2C's full reports always used sequence `2021`; the Agent previously treated only a report echoing its current `pushall` sequence as authoritative.

The Agent now accepts the exact H2C fixed-sequence shape as authoritative only while one of its own `pushall` requests is outstanding:

- normalized endpoint model is H2C;
- `print.command == "push_status"`;
- `msg == 0`;
- `sequence_id == "2021"`.

Other models and other H2C sequence IDs retain the strict request-sequence rule. A regression first reproduces offline presence plus a non-authoritative unrelated sequence, then proves the exact H2C report restores `IDLE` presence. With the fixed Agent, a real replacement session recovered in the first observed poll to `dev_online: true`, the original `fun` bitmap, seven nozzles, and holder state without an explicit refresh command.

Focused regression command:

```sh
cargo test -p pandar-agent h2c_fixed_sequence_full_report_recovers_presence_after_timeout -- --nocapture
```

## Remaining hardware acceptance

The following claims remain open:

- A real nozzle-only delta and holder-only delta captured from a manufacturer-supported physical rack action.
- A real Bambu Studio process receiving the exact V0/V1 response and retaining its physical IDs in the subsequent FDM submission.
- Inspection of an actual final `project_file` payload produced from an H2C-sliced `.gcode.3mf` before authorizing a small print.
- A small FDM print and physical outcome.

Until those steps are separately authorized and completed, Pandar can claim live H2C status, rack inventory, protected FTPS listing, auto-mapping, correlated failure, and Agent-session fencing evidence—not live Studio submission or print validation.
