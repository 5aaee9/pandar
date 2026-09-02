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

Live deployment follow-up: with the concurrent Hub and attributed warnings deployed, every printer still overflowed on a ~45 s cycle while job reports flowed normally — the attributed serials plus zero report-consumer restarts identified the actual source as each printer's command-role MQTT connection, which subscribes to `device/{serial}/report` during operations and never unsubscribes, leaving a queue nobody reads. The prescribed firmware transition-mutex audit was performed and cleared (those locks are only held across channel sends while the shared channel drains). The pump overflow policy is now per role: report-role connections keep the fail-the-consumer-for-resync contract, while command-role queues shed the oldest entry on overflow so a fresh command response is never discarded.

Remaining acceptance work:

- After deploying the current Hub and Agent, run a sustained print on one printer with the other linked printers idle.
- Confirm the Agent records zero pump-overflow warnings.
- Record the live result in issues #7 and #8 before closing #8.

## Recently Completed

- Removed two orphaned `pandar-auth` smoke scripts (`smoke-email-env.mjs`, `smoke-dashboard-token-redirect.mjs`) that no nix check, CI job, or npm script referenced; their contracts live in the nix jwt smoke check and the passkey sign-in vitest suite. Also removed the unused `--chart-1..5` palette family from the dashboard theme and the matching rows in the design-token documentation.
- Removed four dead-code candidates from the simplification audit: the unused `spawn_control_plane` wrapper (production only uses `spawn_control_plane_ready`), the orphaned Android `LoginViewModel` superseded by the hub-browser login flow, four unused dashboard type declarations (`Summary`, `UserIdentityList`, `PrinterEvent`, `RuntimeNotification` plus its now-unreferenced local `TextKey` copy), and the unreferenced `PLUGIN_NAME` constant from the network plugin.
- Removed the dead `AppState` constructor chain: `connect_with_config` and `connect_with_auth_config` had no callers, so `connect` now reads its environment configuration and reaches `connect_with_config_values` directly.
- Collapsed the command-transition test seam: all test-only unfenced command/job status mutators (mark_sent, mark_acknowledged, mark_succeeded, the print-command variants, mark_for_command, and the plain enqueue wrappers) are now compiled for tests only, the session-less ack/result entries are explicit test fixtures, and `CommandRepository::mark_failed` is the single documented unfenced transition reserved for hub-side terminal sweeps. Production transitions were already fully session-fenced; the shared UPDATE cores are unchanged.
- Dropped the retired user API-token feature end to end: a new shared migration deletes the `api_tokens` table, the 410-tombstone provisioning routes are removed, and the dead repository, entity, and `authenticate_bearer` paths are gone. External-identity sign-in now builds `AuthenticatedUser` directly; tenant tokens, bootstrap, and the mobile/plugin client contracts are unchanged.
- Removed 23 retired `pandar_plugin_*` FFI exports from the network plugin (superseded account load/persist/clear wrappers, hub-action and ticket-admission policy probes, and the legacy plugin-jobs HTTP path). The shipped plugin export table shrank while every Studio-loaded `bambu_network_*` symbol and all shim→Rust contracts are unchanged; the redaction-policy tests moved onto the live print-submission path.
- Deleted two unused type re-export shim files from the dashboard; consumers already import the same aliases from `dashboard-types` and the generated Hub client.
- Removed the obsolete `suppaftp_api_compile_proof` helper and stale `allow(dead_code)` markers from the agent FTPS module; the test-only TLS config helper is now compiled for tests only.
- Removed 82 dead translation keys from the dashboard message bundles; no user-facing strings changed.
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
