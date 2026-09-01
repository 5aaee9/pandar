# Pandar Roadmap

This document is intentionally limited to current priorities, the latest delivery window, and explicit unscheduled gaps. GitHub issues are the source of truth for actionable work. Released history belongs in [CHANGELOG.md](../CHANGELOG.md), implementation history remains in Git, and detailed compatibility evidence belongs under [`docs/compatibility/`](compatibility/).

## Current Priorities

### [#7: Validate no-USB H2D eMMC printing through Pandar](https://github.com/ProjectPandar/pandar/issues/7)

Software delivery and the Web hardware path are complete:

- Printable artifacts probe the BRTC/eMMC tunnel on port 6000 before protected FTPS fallback.
- The BRTC client supports the printer's static-RSA TLS 1.2 profile while preserving the shared serial/CN, pinned-leaf, legacy-v1, and CA-chain trust rules.
- The current Agent session's `print.fun2` bit 0 is projected to Studio, and successful jobs persist and expose `command.uploaded_url`.
- A Web-submitted print on an H2D with `sdcard = false` reached `RUNNING` from built-in storage through a `brtc://emmc/...` upload.

Remaining acceptance work:

- Deploy matching current Hub, Agent, and `02.08.02` Studio plugin builds.
- Submit a small print from real Bambu Studio and prove `try_emmc_print=true`, a `brtc://emmc/...` upload, and an accepted or printing machine state.
- Record redacted evidence that no full-artifact FTPS fallback or 30-second FTPS timeout occurred and that no printer credential or bearer token leaked.
- Update the issue and compatibility evidence before closing the hardware gate. This does not claim direct-LAN plugin support.

### [#8: Fix sustained MQTT report pump overflow/resync churn](https://github.com/ProjectPandar/pandar/issues/8)

The software scope is complete:

- Agent overflow warnings identify the printer serial and dropped report count while preserving loss-triggered full resynchronization.
- Hub snapshot validation remains ordered and fail-fast, while current-session aggregate application and event fanout run with bounded concurrency.
- Command results, print reports, and other non-snapshot events retain their ordered fail-fast behavior.
- Hermetic regressions cover warning attribution, slow snapshot fanout, and current-session fencing.

Remaining acceptance work:

- After deploying the current Hub and Agent, run a sustained print on one printer with the other linked printers idle.
- Confirm the Agent records zero pump-overflow warnings. If any remain, audit and remove the per-printer firmware transition-mutex blocking point.
- Record the live result in issues #7 and #8 before closing #8.

## Recently Completed

- Fixed the create-invite dialog so a successfully submitted non-Viewer role remains selected after React resets the form.
- Published `v0.2.0`, including the Bambu Studio `02.08.02` ABI series, generated Hub clients, personal-preset synchronization, the Studio printer-event stream, and the current release artifact matrix.
- Completed the software scopes for BRTC/eMMC transport (#5), session-fenced Studio eMMC capability projection (#6), transport URL persistence, and bounded concurrent Hub snapshot application (#8).

## Unscheduled Evidence And Scope Gaps

These are not active roadmap items until they have an open GitHub issue and an explicitly safe test environment:

- Additional authenticated, exact-version Studio evidence on Windows and both macOS architectures; see [`compatibility/bambu-studio-plugin.md`](compatibility/bambu-studio-plugin.md).
- Hardware-dependent camera, FTPS, H2C rack, print, cancel, firmware, and live printer-control validation; see the relevant files under [`docs/compatibility/`](compatibility/).
- Virtual-printer or LAN proxy behavior. It remains out of scope because it changes discovery, port ownership, and MQTT/file-transfer routing.

## Maintenance Rule

Keep only open milestones and the latest delivery window here. When work is released or superseded, move durable release notes to `CHANGELOG.md`, keep detailed test evidence in `docs/compatibility/` or the originating GitHub issue, and remove the old roadmap narrative instead of accumulating another historical phase log.
