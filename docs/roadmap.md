# Pandar Roadmap

## Reference Findings

- `reference/bambuddy` provides the clearest direct implementation reference for Bambu MQTT, file transfer, printer state normalization, and printer connection management.
- `reference/BambuStudio` provides the higher-level product and protocol boundaries: print host upload jobs, network-agent discovery/message APIs, and local print/send-to-SD-card entry points.
- The machine command channel should use MQTT over TLS with `device/{serial}/report` and `device/{serial}/request` topics.
- The machine file channel should start from the reference's implicit FTPS behavior on port 990, even though the high-level product brief says SFTP. Keep Pandar's public boundary protocol-neutral until implementation confirms final naming.
- Print dispatch should be modeled as upload artifact, verify artifact, send MQTT `project_file`, then reconcile state from reports.
- `reference/bambuddy/backend/app/services/bambu_ftp.py` adds details needed for the real runtime: implicit FTPS, username `bblp`, manual 64 KiB `STOR` chunks, post-upload `226`/`SIZE` verification, and model profiles such as TLS 1.2 caps for affected firmware. Pandar intentionally requires protected FTPS data channels and does not copy the reference's A1 clear-data fallback.
- `reference/bambuddy/backend/app/services/bambu_mqtt.py` shows that physical job state must be reconciled from `gcode_state`, `mc_percent`, remaining time, layer counts, `subtask_id`, `print_error`, and HMS-style errors instead of treating MQTT publish success as print completion.
- `reference/bambuddy/backend/app/services/discovery.py` shows Bambu LAN discovery through SSDP multicast `239.255.255.250:2021` with search target `urn:bambulab-com:device:3dprinter:1`, plus bounded subnet scanning and direct-host SSDP when multicast is unavailable.
- Clerk and Logto both support backend API protection through JWT verification against provider JWKS plus issuer, audience, expiration, and optional authorized-party/scope checks. Pandar should treat the identity provider as authentication only; Rust remains the source of truth for user-to-tenant membership and tenant role authorization.
- `reference/open-bamboo-networking` documents and implements the Bambu Studio network plugin ABI surface, including the `bambu_network_*` and `ft_*` dynamic-library exports that a compatible replacement must provide.
- `reference/BambuStudio` drives login through the network plugin ABI: Studio opens `agent->get_bambulab_host() + "/sign-in"` in a WebView, accepts page messages such as `user_login`, `user_ticket_login`, `get_localhost_url`, and `thirdparty_login`, starts its own localhost HTTP server on port `13618`, then calls plugin token/profile ABI methods before applying `change_user(login_info)`.

## Completed

- Added print-speed switching to each printer detail card in the Devices inventory. Active and
  paused prints now expose the four Bambu speed modes (Silent, Standard, Sport, Ludicrous), mapped
  to the existing validated `set_print_speed` control contract as modes 1–4; idle printers keep the
  selector disabled. The shared printer-control intent owns the hidden `speed_mode` field, localized
  English and Chinese labels cover the new controls, and frontend interaction tests verify all four
  choices, the Sport payload, idle-state disabling, pending feedback, and success feedback.

- Compacted the dual-nozzle temperature controls so the nozzle switch shares the responsive
  temperature grid instead of occupying a full row. Each nozzle's diameter and type now render on
  separate lines, keeping values such as `0.4 mm` and `HH05` readable in the narrower card.

- Hoisted the Devices camera viewer into the persistent dashboard layout so an active native
  picture-in-picture stream now survives navigation to Jobs, Agents, and the other dashboard pages.
  The printer-card camera button opens the shared viewer, leaving PiP still tears down the hidden
  stream, and frontend coverage verifies that unmounting the Devices controls does not disconnect it.

- Added native browser picture-in-picture support to the Devices camera viewer. Supported browsers now
  show a PiP action beside fullscreen; entering PiP closes the camera dialog while keeping the same
  MP4 video element mounted so pause/resume, chamber light, and other printer controls remain usable.
  Leaving PiP tears down the hidden stream, unsupported browsers keep the existing fullscreen-only UI,
  and frontend coverage locks the modal-to-page transition and stream lifetime.

- Deepened the network plugin's Studio status projection into one typed, in-process module. Each Hub
  printer-list response is now deserialized and validated once into an ephemeral aggregate that supplies
  both connection observations and firmware observations; malformed known fields reject the whole
  projection while additive unknown fields remain compatible. The aggregate stays in Rust during the
  refresh transaction, while the C++ shim supplies only its existing synchronization and opaque firmware
  session handle. Removed the duplicate validation schema, the raw Rust-to-C++-to-Rust firmware JSON
  handoff, and the test-only printer telemetry FFI export. Projection, firmware, refresh, and compiled
  Studio contract coverage now exercise the production aggregate interface; all 410 network-plugin tests
  pass (1 skipped).

- Deepened the web printer-control seam into `frontend/app/printer-controls.tsx`: the module now
  owns the `PrinterControlIntent` tagged union, the hidden form-field contract
  (`PrinterControlFields`, including per-action field-selection policy such as AMS target
  inclusion and load-only extruder routing), the `printerControlFieldNames` constants for
  user-editable inputs, and the `usePrinterControl` hook. The six control surfaces (axis,
  temperature, nozzle temperature, materials, rack, drying) declare semantic intents instead of
  hand-writing `tenant_id` / `printer_id` / `action` hidden fields, so a payload-contract change
  now lands in one module. `SlotOperationForm` and the camera dialog moved into their own modules
  to keep every touched file under the 400-line module limit.

- Wired the dashboard attention model into production: `computeHealth` / `computeAttention` (plus
  a new `topSeverityOf`) from `dashboard-attention.ts` are now the single source of truth for the
  devices/jobs/agents page clients, replacing three hand-rolled inline health computations that
  had diverged (online vs non-offline printer counting) and the hard-coded `attentionItems={[]}`.
  The previously dormant `NeedsAttention` section now renders real items.

- Moved Bambu Studio shim behavior from C++ into Rust (thin-shim rule). The fail-closed `ft_*`
  file-transfer ABI (Tunnel/Job handle state machines, refcounts, condition-variable result
  wait, callback fan-out) now lives in `pandar-network-plugin/src/file_transfer.rs` +
  `file_transfer/job.rs`; the C++ `shim_file_transfer.hpp` is deleted and all 21 `ft_*` symbols
  are exported from Rust (upstream ft contract fixture passes against the built library).
  Connection delivery policy (claim-before-invoke, per-kind callback selection,
  delivered/undelivered completion) moved from `shim_connection.hpp` into the new Rust module
  `connection/studio/shim_dispatch.rs`; the C++ side keeps only the ABI-legitimate parts — the
  callback gate trampolines and `std::function` invocation — exposed as the flat
  `ShimCallbackBridge` vtable. Full plugin suite: 410 tests pass including the compiled C++ ABI
  probes.

- Completed the project-wide Button unification: every remaining hand-rolled `<button>` and
  Trigger with copy-pasted button classes now renders the shared `Button` primitive. The shared
  component gained a `soft` variant (`bg-primary/10 text-primary`) covering the dense printer
  control grids, which replaced the repeated tone-override strings in the rack/axis/nozzle
  controls (`ConfirmForm` call sites included) with `variant="soft"` + layout-only classes.
  Migrated: sign-in page submits (plugin/mobile, `size="lg"`), printer-card overflow trigger
  (`ghost`/`icon-xs`) and menu items (`ghost` full-width), printer camera/fullscreen and rack/
  drying/materials/nozzle popover triggers and tile controls, error-boundary retry buttons,
  mismatch review action (`link`), tenant switcher/sidebar menu rows, users filter chips
  (`soft`/`outline` toggle) and row actions, theme/language segmented switchers
  (`default`/`ghost` toggle), copy button (`outline`/`xs`), "create another" actions (`link`),
  and the dashboard-inventory link-printer trigger which had carried a full copy of the Button
  classes. The auth sub-app (`frontend/auth`, a separate Next.js app with its own shadcn
  primitives) keeps its local `components/ui/button.tsx`; its passkey complete/sign-out pages
  moved from the deleted `auth-button`/`auth-secondary-button` CSS classes onto that local Button
  (`default`/`outline`), matching the auth login form. The only raw `<button>` left is the
  sidebar resize rail, an invisible layout drag handle with no button styling. Web + auth apps:
  lint, typecheck, builds, and 429 + 11 tests pass.

- Unified dashboard action buttons onto the shared `Button` primitive. The Agents row Delete
  button previously used a hand-rolled class string (`h-9 rounded-md border-destructive/40`), so it
  was visibly taller and differently shaped than the neighboring Discover/Refresh/Settings buttons;
  it now renders the shared `Button` with `variant="destructive" size="sm"` (including the disabled
  HoverCard trigger), matching the row's `outline`/`sm` buttons. The row's Settings entry is real
  navigation, so it stays a semantic `Link` but now takes its classes from the newly exported
  `buttonVariants` helper instead of a copy-pasted class chain. `ConfirmForm` no longer requires a
  raw `buttonClassName` string: it renders the shared `Button` internally and accepts
  `buttonVariant`/`buttonSize` (plus optional extra classes), and all call sites were migrated —
  Users revoke/remove/save buttons onto standard variants (deleting the now-unused
  `actionButtonSm`/`actionButtonSmDanger` constants), and the printer control grids (axis homing,
  stop print, nozzle-rack moves) onto variant + tone overrides that preserve their existing dense
  grid look next to their raw-button siblings. Web lint, typecheck, and 429 tests pass.

- Extended the Devices in-place mutation treatment to the printer-card overflow menu. Refresh AMS
  now submits in place through the same pending/toast pattern (disabled spinner item while in
  flight, "AMS refresh queued" or the translated hub error, no `?status=` redirect), and
  `refreshPrinterMaterials` / `deletePrinter` / `updatePrinter` join `controlPrinter` in returning
  typed `MutationActionState` instead of redirecting. Delete keeps its confirmation dialog but the
  dialog's confirm button now carries the loading state: it spins and disables while the DELETE is
  in flight (blocking repeat confirms), the dialog stays open on failure with an error toast so the
  operator can retry or cancel, and only closes on success. The edit-printer dialog likewise saves
  in place with a spinner submit, toasts "Printer updated" or the error code, and closes only on
  success; both successful delete and edit invalidate the tenant's React Query devices route data
  so the card list reflects the change immediately instead of waiting for the 30s poll. The toast
  wiring is shared behind a generic `useActionStatusFeedback` in `mutation-feedback.ts` (success
  status key + optional onSuccess), which `usePrinterControl` now delegates to, and `ConfirmDialog`
  gained an optional `pending` prop. Coverage: printer-card mutation tests lock the refresh-in-place
  dedupe, delete confirm loading/stay-open-on-failure/invalidate-on-success, and edit
  loading/close-on-success flows; action tests assert the returned states; web lint, typecheck, and
  429 tests pass.

- Made Devices printer-control buttons navigation-free with per-command pending feedback.
  `controlPrinter` no longer ends in a `redirect("?status=printer_control_queued")` (which scrolled
  the page back to the top and only surfaced the outcome through the URL query param); it now
  returns typed `MutationActionState` like the other redirect-free actions. A shared
  `usePrinterControl()` hook wraps `useActionState` plus the existing `useMutationFeedback` toast
  wiring, so every control form on the printer card (stop/pause/resume, chamber light, bed/chamber/
  nozzle temperature presets and custom inputs, nozzle switch, axis moves and homing, AMS slot
  load/unload/RFID reread, drying start/cancel, nozzle-rack moves and confirm/refresh) submits in
  place without losing scroll position. While a command is in flight its own button disables and
  swaps/prepends a spinner, which also blocks re-sending the same instruction until the Hub
  answers; success shows the existing localized "Printer control queued" toast and Hub rejections
  (e.g. `agent_not_connected`) show an error toast with the translated or humanized error code.
  `ConfirmForm` gained an optional `pending` prop for the confirm-gated controls, and the axis
  homing flow now reuses it instead of a hand-rolled dialog. Coverage: action tests now assert the
  returned state (including the hub-rejection path), a new `use-printer-control` suite locks the
  pending spinner/disabled/dedupe behavior and both toast outcomes through `PrinterControlsPanel`,
  and the axis-controls suite was updated for the new action signature; web lint, typecheck, and
  425 tests pass.

- Rebuilt the dashboard Settings page around a compact header plus a sticky scrollspy section
  navigation (Workspace, Appearance, Access & security, Account) instead of the metric hero with
  passive anchor links: `useScrollSpy` tracks the current section against the sticky-header offset
  with a last-section-dominance rule for short pages, and removing the never-scrolling
  `overflow-y-auto` trap on the dashboard `<main>` lets the section nav actually stick while the
  window scrolls. Workspaces can now be renamed in place by tenant administrators through a new
  audited `PATCH /api/v1/tenants/{tenant_id}` Hub endpoint (admin users or no-auth only, tenant
  tokens rejected, empty names rejected, `tenant.rename` audit metadata carries previous and new
  display names, backend-neutral SeaORM update works on SQLite and PostgreSQL); the form saves
  through a server action with `refresh()` so the sidebar, subtitle, and form state update
  immediately, shows inline translated errors, and toasts on success. Non-admin roles see the
  workspace name as a read-only fact and keep the restricted/error access-section states; token
  management, agent enrollment, and audit panels are reused unchanged. Workspace facts (slug,
  created, ID, auth provider) are shown directly instead of behind a disclosure. Coverage: Hub
  route tests for rename success/audit/empty-name/viewer/token rejection, rename-form interaction
  tests, and rewritten settings authorization-state tests; Rust workspace Nextest 1,956 pass,
  web lint/typecheck/422 tests pass, and the rebuilt page was verified in a real browser against a
  live no-auth Hub (scrollspy, sticky nav, rename round-trip, audit event).

- Reworked the link-printer dialog into a confirmed in-place flow. The Devices Add and discovery
  Adopt entry points now use one shared dialog module while retaining their context-specific title,
  description, trigger, defaults, and fixed-Agent behavior. Its shared link form keeps the entered
  access code in component memory across submissions and failed retries while the dialog remains
  open, clears it when the dialog closes, and provides an accessible show/hide control. During
  linking, the submit button switches to a disabled spinner state and the browser polls the command
  every 2s (90s deadline)
  through the existing same-origin command proxy, and the dialog only closes once the agent
  confirms the link (command `succeeded`), followed by a success toast and React Query
  invalidation of the devices/agents route data so the new machine appears immediately. Failures
  stay in the dialog as a `role="alert"` panel that combines a localized human-readable message
  with the machine-readable error code. Unclassified `link_failed` results also show the Agent's
  redacted full error chain, so TLS certificate and other lower-level causes are not hidden behind
  the fallback code. The Agent classifies link failures into stable codes (`invalid_access_code` from
  MQTT connack `BadUserNamePassword`/`NotAuthorized`, `printer_not_found` for discovery misses,
  `printer_unreachable` for transport/timeout errors, `unsupported_printer_type`, `link_failed`)
  and reports them in a typed `printer_link_error` result JSON that survives Hub credential
  redaction, while Hub dispatch rejections (`agent_not_connected`, `agent_not_found`,
  `bad_request`) return as action state from the redirect-free `linkPrinter` server action. The
  obsolete `discovery_command_id` redirect handoff is gone since the dialog no longer navigates.
  Coverage: Agent classification unit tests plus end-to-end link command cases, a Hub redaction
  test proving the error code JSON is preserved, form tests for the loading/success/failure/dispatch
  states, including fallback TLS detail preservation and access-code retention/visibility; web
  lint, typecheck, and 432 tests pass, and
  the full Rust nextest workspace run passes 1,963 tests with 1 skipped.

- Moved the dashboard's current-tenant selection out of the `?tenant=` query param into a
  `pandar.tenant` cookie. Server pages and the dashboard layout resolve the selected tenant from
  the cookie via `resolveSelectedTenant` (falling back to the first effective tenant), so SSR stays
  correct with no hydration flicker; the sidebar tenant switcher is now a button that writes the
  cookie and re-renders the current view, dropping transient `command`/`status` context that belongs
  to the previous tenant. Server actions keep receiving their target tenant through hidden
  `tenant_id` form fields, but their redirect URLs (`statusUrl`, `agentsStatusUrl`, `commandUrl`,
  dispatch/job-history pushes) no longer carry `tenant`; `createTenantFromExternal` and
  `acceptJoinLink` set the cookie to the new tenant before redirecting. Sidebar and settings/agent
  navigation hrefs are tenant-free (`/agents`, `/agents/{id}/settings`), and the standalone
  plugin/mobile sign-in flows intentionally keep their explicit tenant picker param. Web lint,
  typecheck, 414 tests, and production build pass.

- Added AMS filament drying control end to end. The dashboard AMS unit card now offers a Dry
  popover for AMS 2 Pro and AMS HT units (filament select from loaded trays, 45–85°C temperature,
  1–24h duration, rotate-tray toggle) and switches to a live "Drying · remaining time" badge with a
  Cancel action while the firmware reports an active cycle. Agent MQTT gains typed
  `ams_filament_drying` payloads matching Bambu Studio's `DevFilaSystemCtrl` wire shape
  (`mode: 1` start with `cooling_temp: 20`, zeroed `mode: 0` stop), new `AmsStartDrying` /
  `AmsStopDrying` printer operations with 45–85°C / 1–24h validation at both the Hub boundary and
  the Agent proto parser, and a materials refresh after dispatch so the UI state follows the
  firmware push. Agent materials telemetry now extracts per-unit `dry_status` (Studio `info` bits
  4–7) and `dry_time` remaining minutes into the patch document, which flows through the Hub's
  generic JSON merge to `dry_status` / `dry_time_minutes` on dashboard AMS units without Hub schema
  changes. New gRPC `AmsStartDryingOperation` / `AmsStopDryingOperation` fields 31/32 carry the
  command, and Hub audit records the full drying parameter set. Regression coverage locks the MQTT
  payload shapes against the Studio reference, info-bit/dry-time normalization, Agent proto
  validation rejections, Hub route validation (400 on out-of-range temperature or duration), and
  enqueued payload contents for both operations.

- Fixed the Checks workflow audit failures on main. The npm production audit failed because the
  root security override pinned `postcss` to 8.5.20, inside the vulnerable `<=8.5.22` range of
  GHSA-fxqj-rqcc-2cmp; the pin is now 8.5.25, which clears all six moderate findings (postcss,
  next, better-auth, auth, next-intl, vite) with a clean `npm ci`. The Rust audit failed on
  RUSTSEC-2026-0235 against `rkyv 0.7.46`, which enters Cargo.lock only as an unactivated optional
  dependency via sea-orm → rust_decimal; `cargo tree --workspace --all-features --target all`
  proves no workspace target compiles it, and rust_decimal 1.42.1 (latest) still requires
  rkyv ^0.7.46, so there is no upgrade path. A documented `.cargo/audit.toml` ignore now covers
  the advisory with the justification and a revisit note. `cargo audit` exits 0, npm audit reports
  0 vulnerabilities, and the full frontend CI job (lint, 409 web tests, 11 auth tests, typechecks,
  both production builds) passes locally.

- Reworked the Agents page around the agent lifecycle and added discovered-machine adoption. Agents
  are now the primary section: each agent row shows status, linked-printer count, and created date
  with inline Discover (dialog with a 1–15s timeout, disabled while the agent is offline), Refresh,
  Settings, and Delete actions, while agent pairing moved from an always-visible hero into a
  "Pair agent" dialog that keeps the setup steps and one-time environment block. Running discovery
  opens a Discovered printers section that polls the command every 2s until it reaches a terminal
  state, then lists each machine with a Linked badge (matched by serial against tenant printers) or
  an Adopt button; Adopt opens a dialog pre-filled with the discovered host and name that only asks
  for the access code, and the link redirect carries the discovery command so the list stays visible
  for adopting multiple machines in one pass. The diagnostics section keeps per-printer diagnose and
  link/diagnostic command results, manual printer linking now lives only in the Devices page dialog
  (dropping the single-option Type select), and the agent settings page slimmed down to connection
  details since discovery moved to the Agents page. Web lint, typecheck, 409 tests, React Doctor
  100/100, and production build pass.

- Added consistent spacing between the Cards on the Agents page so pairing, printer linking, linked Agents, and diagnostics no longer render flush against each other.

- Raised both `pandar-web` and `pandar-auth` from 58 React Doctor findings to a clean 100/100 without rule suppression. Server actions now authenticate before issuing login tickets, runtime URL handling fails closed, browser timers and asynchronous work own their failures and cleanup, controls carry native accessible semantics, independent server reads run concurrently, and material mapping state lives with its dispatch owner instead of synchronizing upward through an effect. Large job/dispatch and admin modules were split along existing UI boundaries, render-only sidebar adapters and nozzle-temperature helpers moved behind focused modules, stable list identities and one-pass lookups replaced index keys and repeated scans, and dead files/exports were removed. React Doctor 0.9.4 reports zero findings across 276 files; Web lint, typecheck, 399 tests, and production build pass, as do Auth typecheck, 11 tests, and production build.

- Fixed agent-wide LAN discovery on networks that suppress Bambu multicast responses. Discovery now supplements multicast M-SEARCH with bounded unicast SSDP across operational, non-point-to-point private IPv4 interfaces, limits broad networks to the local `/22`, never scans public addresses, and de-duplicates combined responses. Real-LAN verification discovered all three reachable printers that the previous multicast-only path returned as an empty result.

- Aligned A1/A1 Mini FTPS compatibility reporting and operator documentation with the protected-only
  runtime policy. Shared diagnostics and the Web UI no longer advertise a clear-data fallback;
  authoritative docs state that `PROT P` failure is terminal and never downgrades to `PROT C`.
  Automated regressions cover the removed compatibility field and the existing no-downgrade attempt
  order. A read-only opt-in example records firmware module versions and a protected FTPS root
  listing without exposing credentials or file names. Real A1 and A1 Mini execution remains an
  explicit hardware gate because neither device is available in the current environment.

- Implemented a fail-closed Bambu Studio local-camera path for normalized A1, A1 Mini, P1S, and A2L
  models. Agent speaks the native TLS port-6000 `bblp` camera protocol with the existing Bambu
  certificate verifier and keeps the printer host/access code local; Hub gates listing and stream
  open on the exact model whitelist plus an online current Agent session advertising
  `StudioLocalCamera`; the network plugin gives Studio only a random one-use loopback relay URL; and
  `pandar-bambu-source` implements the pinned 21-entry local-media ABI for bounded JPEG samples while
  rejecting arbitrary hosts and direct credentials. Targeted cross-crate tests, the positive compiled
  four-model Studio ABI probe, 26/26 release-smoke, strict workspace Clippy, 1,923/1,923 workspace
  Nextest with one configured skip, six x86_64 Linux Nix package/quality checks, and Linux release
  export inspection pass; the `02.08.01` build exposes 130 network-plugin exports and the companion
  sentinel plus exactly 21 `Bambu_*` exports. Packaged cross-platform evidence, real Studio playback,
  and real camera hardware remain separate acceptance work.

- Completed source-backed AMS Lite routing for A2L (`N9`). Agent material normalization now decodes
  Studio's AMS type nibble into typed AMS, AMS Lite, AMS 2 Pro, AMS HT, and mixed AMS Lite kinds;
  mixed AMS Lite slots use Studio's reserved global tray IDs `24..=27` regardless of the reported
  unit ID, and active routing follows that normalized evidence. Hub terminal usage correlates both
  `ams_mapping` and `ams_mapping2` through the persisted global route, Web dispatch and material
  controls preserve the same IDs even when a tray omits its precomputed global ID, Android preserves
  `unit_kind`, and the network plugin reconstructs type `5`, existence bit `12`, tray bits, and
  `tray_now` for Studio. Empty normalized trays no longer reappear in Studio's tray-existence mask,
  and type-less partial Agent deltas omit routing fields so Hub merge preserves prior mixed AMS Lite
  evidence. Hub terminal attribution also preserves the slicer's flat `24..=27` physical route when
  the corresponding structured mapping lacks a current material-snapshot route instead of reducing it
  to an ordinary AMS ID. Cross-layer regressions cover Agent MQTT normalization, Hub merge and usage,
  Web dispatch/control payloads, Android mapping, and Studio status projection. This is pinned-source and
  automated compatibility evidence, not a real A2L hardware claim.

- Enabled the existing authenticated Web/Android printer-control pipeline for X1C, P1S, and A2L by
  marking only those three additional normalized models as live-control capable. The change preserves
  the prior flow-calibration matrix, keeps X1, X1E, P1P, missing, and unknown models fail-closed, and
  does not bypass per-operation validation or required-device-feature checks. A route-level regression
  proves friendly X1C, P1S, and A2L names can each enqueue a typed Pause command through the shared
  tenant `/controls` endpoint.

- Completed Hub-to-Studio model-resource projection for every printer profile in the pinned Bambu
  Studio `02.08.01.55` snapshot. Friendly names and aliases now emit the exact `N1`, `N2S`,
  `BL-P001`, `BL-P002`, `C11`, `C12`, `C13`, `N6`, `N7`, `N9`, `O1C2`, `O1D`, `O1E`, or `O1S`
  `dev_model_name`; legacy H2C `O1C` canonicalizes to `O1C2`, while unknown future models remain
  unchanged instead of being guessed. A full plugin-printer-list route regression covers every known
  resource family and the unknown-model boundary.

- Added Bambu Studio `02.08.01` as a first-class ABI series pinned to Studio `02.08.01.55` at
  `ba049f6a2e08c3b6033660bb84da80c08722974b`. Its separate plugin carries the exact trailing
  `PrintParams::slicer_uid`, 109-network-plus-21-File-Transfer export surface, and by-value
  `bambu_network_sync_ams_filaments` ABI; AMS cloud sync remains explicitly unsupported with a stable
  redacted failure. The installer and Windows hook resolve `02.08.01.x` to its own artifact, release
  CI derives all platform builds from the catalog, and the pinned upstream native contract passes
  version, bind, print, AMS, and File Transfer modes against Boost `1.84.0`. Current tagged packages
  and platform-specific real-Studio runs still require fresh release evidence.

- Added source-backed, fail-closed H2C FDM nozzle-rack support across Bambu Studio, the network plugin, Hub, Agent, and printer MQTT. Current-session rack telemetry now carries physical nozzle/holder state plus `snow`/`hnow`, while raw `fun2` evidence is retained without being advertised; Studio's bit-60 rack capability is gated on both fresh telemetry and Agent support; typed V0/V1 auto-mapping requires command/sequence correlation and strict successful physical mappings while preserving detailed printer failures; and H2C Studio prints/reprints require the slicer's validated mapping while Web dispatch is rejected instead of guessing. SQLite/PostgreSQL persistence and focused cross-boundary tests cover session fencing and delta-safe state. Signing, rack controls, laser/cut, eMMC/`fun2`, and live-hardware claims remain disabled; see `docs/compatibility/h2c.md`.

- Verified the safe-idle H2C hardware slice on firmware `01.02.00.00`: live MQTT returned the seven-nozzle rack and holder, protected FTPS `PROT P` listing succeeded without writes, direct and isolated Hub/Agent V0/V1 auto-mapping returned physical ID `1`, a correlated unavailable mapping retained printer `errno: 4`, and a replacement Agent kept bit 60 and mapping unavailable until fresh current-session rack telemetry. A follow-up read-only probe proved sequence `2021` belongs to H2C's periodic full-status publisher: reports arrived before any command, `get_version` echoed an arbitrary request ID, and two Studio-shaped `pushall` requests did not change the status sequence. Agent now matches Bambu Studio by treating typed `msg: 0` `push_status` as a full current-session snapshot independently of sequence or an outstanding request, while command responses such as H2C auto-mapping retain exact correlation. No upload, print, rack operation, Studio process, or unsupported H2C behavior was used; split deltas and live Studio/print evidence remain open in `docs/compatibility/h2c-hardware-2026-08-04.md`.

- Moved Pandar's canonical repository to the organization-owned `ProjectPandar/pandar` namespace: Cargo metadata, release discovery, GitHub links, deployment defaults, release documentation, and historical repository commands use the canonical repository path, while GHCR image/chart publishing uses the registry-required lowercase `projectpandar/pandar` path.

- Added Web UI support for H2C nozzle-rack state and operations. The dashboard printer card now renders the current-session nozzle system (mounted hotends, rack slots 16–21 with diameter/type/wear, holder position and calibration) behind the same current-session fence as the Studio projection, and exposes Studio's rack commands as queued printer operations: rack move (centre / A top / B top via `nozzle_holder_ctrl`), per-slot or all `nozzle_info_confirm`, and `holder_nozzle_refresh`. The commands cross the tenant controls API, command persistence, gRPC proto (fields 28–30), and Agent MQTT as Studio-shaped payloads with dynamic `sequence_id` correlation; Hub validates action/id ranges and rejects rack operations for non-H2C printers, while the UI disables them during prints and confirms physical moves. Physical rack movement on hardware remains unverified evidence; see `docs/compatibility/h2c.md`.

- Fixed GitHub Actions after the Studio ABI catalog became a Rust compile-time input: the Nix Rust source filter now carries `studio-abi-profiles.json`, restoring package, Clippy, format, and test derivations. Upgraded Checks and scheduled cache GC to Hestia v3, whose larger manifest bound and fresh cache namespace avoid v2's oversized-manifest failure while old packs expire.

- Reworked Bambu Studio compatibility around ABI series, following the reviewed
  `open-bamboo-networking` build/install model. `studio-abi-profiles.json` now pins separate binaries for
  `02.06.00`, `02.06.01`, `02.07.00`, `02.07.01`, `02.08.00`, and `02.08.01`; each entry keeps an exact upstream
  reference version and commit for contract verification while installed Studio builds match by their
  first three components. `PANDAR_STUDIO_ABI_SERIES` selects a build and defaults to `02.07.01`, so the
  locally installed `02.07.01.62` resolves to that series without treating its fourth component as a new
  ABI. Capability gates cover the five filament exports introduced in `02.06.01`,
  `PrintParams::svc_context` introduced in `02.07.01`, and the `bambu_network_bind` model argument
  introduced in `02.08.00`, plus `PrintParams::slicer_uid` and AMS sync introduced in `02.08.01`.
  Release CI derives all platform/series artifacts from the catalog, with
  macOS jobs running on Apple Silicon and amd64 execution under Rosetta. Adding another supported
  Studio ABI now requires one reviewed catalog entry and only its actual capability differences. The
  catalog stores upstream's source network-agent macro separately from the plugin's reported
  `<abi-series>.99`, covering `02.07.00` where upstream retained the older `02.06.01.50` macro even
  though Studio validates the running plugin against product series `02.07.00`. The first five official
  source commits pass the full native contract on macOS arm64. A current `02.07.01` three-file archive
  also passes release-smoke and loads both dylibs into the installed ARM64 Bambu Studio `02.07.01.62`
  process, which reaches its normal home UI before the original local plugin/config state is restored.

- Made `pandar install-network-plugin` default to the platform-specific network plugin and BambuSource companion shipped beside the CLI in the current unpacked release directory, while preserving `--plugin-file` and `--source-file` overrides for development builds. CLI parsing coverage locks the zero-file-flag path; the next tagged release should rerun native packaged smoke with the simplified install command.

- Fixed the Print Jobs list rendering each `JobRow` list item inside another list item, which produced a React hydration error in production markup. The history list now owns exactly one `li` per job while preserving content-visibility styling, and a focused DOM regression rejects nested list items.

- Published `v0.1.0` from `d50ef4223daf1fe5f45b6adc254ec91a9823bacc`. Release run `30654892795` natively built and smoke-tested the Linux amd64 and Windows amd64 three-file archives plus the Windows Studio hook bundle before publishing all six desktop assets; Checks run `30654892831` and Docker/Helm run `30654892588` passed. Downloaded sidecars, exact archive layouts, the Linux packaged CLI/plugin/full 130-name ABI contract, GHCR manifests, and Helm chart `0.1.0` were independently rechecked after publication. The first tag attempt had exposed that compiling the Linux C++ ABI shim with Zig while probing it with host `libstdc++` corrupted `std::string`; Linux plugins now use the runner's native GNU toolchain while Zig remains scoped to the standalone musl CLI. A real Windows Studio session remains unclaimed evidence.

- Renamed the Windows injection crate and CLI surface from `pandar-studio-dev-hook` to `pandar-studio-hook`, retained the opt-in local log-key patch under `PANDAR_STUDIO_LOG_LOCAL_KEY`, and added a Studio `02.07.01.x` network-plugin download replacement. `pandar install-studio-hook` now fetches the fixed native Windows bundle and sidecar from the latest GitHub Release, verifies SHA-256 and exact ZIP layout, installs the Pandar network plugin plus BambuSource, and caches a Studio-shaped archive. The injected hook replaces only Studio's final `networking_plugins.zip` rename and fails closed when its verified cache is unavailable. Release CI now builds the dedicated bundle natively with MSVC. Local Rust tests and workspace checks cover release selection, checksum/layout rejection, package construction, and CLI parsing; native Windows Studio execution remains the next compatibility-evidence step.

- Rebuilt the dashboard Settings page as a dedicated responsive workspace instead of routing it through the generic dashboard view prop bag: a status overview and anchored section navigation now organize appearance, workspace identity, connected infrastructure shortcuts, access/security administration, recent activity, and the current session. Language and theme controls use larger labeled groups, tenant tokens retain their focused create/rotate/revoke flows, Agent enrollment and audit history remain available to administrators, technical identifiers are progressively disclosed, and English/Chinese copy plus route-level loading states cover the new hierarchy. Workspace agent/printer data now loads separately from protected token/audit data, so non-admin roles can use personal and workspace settings without an admin-only request failing the entire page; protected request failures remain isolated and visible in the access section.

- Added consistent vertical spacing between the Devices fleet-status row, optional attention row, and printer inventory so the printer list no longer sits flush against the status summary.

- Deepened the Hub database dialect so SQLite/PostgreSQL transaction modes, row/table locking, and constraint-violation spellings live in `db.rs` instead of leaking through repository call sites. Unique constraints now use a typed registry; printer-event tickets and join-link consumption use backend-neutral SeaORM statements; legacy-schema test fixtures avoid backend-specific SQL while remaining compatible with partial migration states. Hub migrations now have one human-edited source (`migrations/shared/` plus paired backend overrides), checked-in sqlx inputs regenerated by `scripts/sync-hub-migrations.sh`, and a CI stale-output check that preserves existing migration bytes and checksums. Next: keep genuinely backend-specific runtime SQL (printer snapshot upsert, Studio ID allocation, artifact advisory locking, and cleanup execution) local to those owning modules unless repeated differences justify moving another behavior behind the dialect seam.

- Fixed the GitHub Checks Nix Clippy failure introduced when firmware MQTT reports became typed: `AttemptEvent` now boxes its large `FirmwareMqttReport` variant at the channel boundary and unwraps it for callers, preserving behavior while keeping the event enum compact under Rust 1.96's denied `large_enum_variant` lint. The full-workspace verification also restored `dispatch-form.tsx` below the enforced 400-line production-module limit.

- Deleted the dead server-side route loaders left behind by the client-side fetching migration: `loadDevicesRoute`/`loadJobsRoute`/`loadAgentsRoute`/`loadUsersRoute`/`loadSettingsRoute` and `renderDashboardView` in `frontend/app/dashboard-data.tsx` were exact duplicates of the Route data module's fetch composition with zero callers, kept alive only by their own test file. The loaders, `renderDashboardView`, their orphaned imports, and `dashboard-data.test.tsx` (312 LOC testing dead code) are gone; `dashboard-data.tsx` keeps only the live shell loaders (tenants, identity, membership, auth, sidebar state), leaving `route-data.ts` as the single Route data implementation.

- Routed browser route-data reads back through the Hub proxy, closing the seam leak left by the client-side fetching migration: Route data queryFns now fetch same-origin `/api/tenants/{tenantId}/...` paths served by seven new per-endpoint `hubProxy()` GET routes (`agents`, `jobs`, `users`, `join-links`, `tenant-tokens`, `audit-events`, `commands/{commandId}`), and `hubProxy()` gained a declared `query` config field so `audit-events` carries its `limit=20` in the route file. The shallow `api-client.ts` adapter and the never-set `NEXT_PUBLIC_APP_API_URL` build-time variable (which baked `localhost:8080` into the packaged web image and could not carry the web-origin auth cookie cross-origin) are deleted; server components and server actions keep reading the Hub through `APP_API_URL`. CONTEXT.md's Hub proxy and Route data terms now state that every browser→Hub request crosses the Hub proxy, and `route-data.test.ts` re-mocks from the deleted adapter to same-origin fetch while `hub-proxy-routes.test.ts` locks the new upstream wiring.

- Typed the Agent's Bambu report stream behind one deep module: the new `machine/mqtt/report/` module owns every wire-schema concern for MQTT reports — `MachineReport` decodes a report payload once into typed `print` / `snapshot` / `materials` sections (schema moved out of `mqtt/reports/schema.rs`, `mqtt/snapshot/schema.rs`, `mqtt/firmware/schema.rs`, and `machine/materials/schema.rs` into `report/print.rs`, `report/snapshot.rs`, `report/firmware.rs`, `report/materials.rs`), exposes the firmware views (`firmware_acknowledgement`, `firmware_version_observation`, `firmware_refresh_modules`, `transient_firmware_status`, `firmware_report_matches`) and the pure report predicates (`has_non_firmware_print_telemetry`, `is_feature_only_report`, `device_feature_observation`, `raw_print_payload`) as methods, and privately retains the raw payload only for open-ended diagnostics pass-through. A `MachineReports<T>` adapter wraps `BambuMqttTransport` so refresh flows, the report-forwarding loop, device-feature probing, and the firmware session pump all cross the seam typed; the `FirmwareReportReducer` moved from `machine/firmware/reducer.rs` into `report/firmware.rs` alongside the other firmware report semantics, and the six scattered `parse_*` free functions were deleted. Decode stays lenient per section exactly as before (silent `Option` sections, `Result` only for semantic validation), the job_id decimal→i64 normalization remains below the seam in the two byte pumps, and existing fake-transport tests exercise the decode boundary unchanged; CONTEXT.md records the Machine report term.

- Unified the dashboard's React Query route data behind one deep module: `frontend/app/route-data.ts` now owns the per-view query contract — `routeDataKeys` tenant-scoped key prefixes, per-view query factories carrying the fetch composition and cache policy (`devicesRouteQuery`, `jobsRouteQuery`, `agentsRouteQuery`, `usersRouteQuery`, `settingsRouteQuery`, `agentSettingsRouteQuery`), and the route-data types — so the six page clients call `useQuery(...RouteQuery(tenant.id))` instead of hand-rolled inline definitions, and mutations invalidate through `routeDataKeys` instead of key literals. This fixes dispatch-form's silent no-op invalidation (`['jobs']` matched no live `['route','jobs',tenantId]` query), deletes the dead and drifted `use-route-data.ts` abstraction (zero imports, stale 30s policies), folds `users-query.ts` away (its toast hook moved to `frontend/app/mutation-feedback.ts`), and replaces the `useInvalidateUsers` hook with direct `queryClient.invalidateQueries` at the four users call sites. Key strings are byte-identical so no cache or test surface shifted; `route-data.test.ts` locks key shapes, per-view cache policies, and every queryFn composition, and CONTEXT.md records the Route data term.
- Collapsed the browser-facing Hub API proxies into one deep module: `frontend/app/hub-proxy.ts` (`hubProxy()`) now owns cross-origin mutation rejection, tenant/path id validation and encoding, auth header attachment, request body streaming with a per-route content-type policy, and a uniform response policy (status/statusText and content-type passthrough plus `cache-control: no-store`), so each of the seven routes under `app/api/tenants/[tenantId]/` is per-endpoint config only and the duplicated `responseHeaders` helpers and drifted header handling are gone; reprint, printer-jobs, and metadata-preview responses now consistently send `no-store`. The orphaned printer-events ticket proxy (dead since the browser WebSocket runtime was removed in `46417fd`) was deleted along with the unused `PrinterEventTicket` type and `printerEventWebSocketUrl` builder. Behavior coverage moved to `hub-proxy.test.ts` with a slim route-wiring suite replacing the five per-route test files, and a new root `CONTEXT.md` records the Hub and Hub proxy terms.
- Rebuilt the dashboard Users page as a standalone members-and-invites surface: the route no longer renders through the shared `DashboardViewContent` prop bag, and members are searchable/filterable by role with role-priority sorting, avatars, a You badge, identity provider chips, and a member detail dialog covering role editing, linked identities, and a guarded remove action. Invite links are now a first-class section with active/expired/revoked/used-up status chips, usage progress, relative expiry, and a create-invite dialog that replaces raw TTL seconds with day/week/month presets and per-role descriptions. Role changes, invite revocation, and member removal now return action state and refresh through React Query invalidation with toasts instead of redirecting to Devices, the Hub gains an audited `DELETE /users/{user_id}` endpoint that blocks self-removal and removing the last tenant admin (with SQLite/PostgreSQL-agnostic transactional cascades for identities), and the sign-out panel moved from Users to Settings.
- Fixed Devices incorrectly counting a recently reporting printer as offline when its previous print task state was `FAILED`: the React Query route now uses the established offline/problem connectivity classification instead of requiring the task-derived status to equal `online`, and it restores the dashboard clock so fresh RFC3339 UTC reports render as Online rather than as an absolute UTC time that appears eight hours old in UTC+8. A page-level UTC+8 regression preserves the `FAILED` task label while proving the printer remains 1/1 online with a fresh report.
- Fixed dashboard printer controls such as Pause replacing the page with `Failed to load data`: the shared client error boundary now rethrows Next.js navigation control-flow errors for the App Router to handle while retaining its fallback for ordinary descendant failures. Focused form-action regression coverage reproduces the original `NEXT_REDIRECT` interception and verifies both redirect passthrough and normal error handling.
- Fixed X2D camera streams producing an HTTP 200 response with zero media bytes under FFmpeg 8.1: Agent now keeps the credential-bearing RTSP URL in the protected stdin concat document while scoping `rtsp_transport`, `rtsp_flags`, timeout, buffer, and delay settings to that concat file entry instead of incorrectly applying them to the concat demuxer. Regression coverage locks both camera command variants, and an Agent-local live RTSP/fragmented-MP4 probe produced 7.8 MB in 12 seconds without issuing a printer control or print command.
- Fixed paused-task controls on Devices: the Web control panel now uses the live `gcode_state` instead of only the printer's coarse status, replacing Pause with a localized Resume action for `PAUSE`/`PAUSED` tasks while keeping offline, idle, and failed printers blocked.
- Fixed X2D paused-fault recovery reporting: Agent MQTT print-report decoding now tolerates the numeric legacy `print.state` field emitted alongside canonical `gcode_state`, preserving `print_error`, HMS, job attributes, and pause state so the existing Devices recovery reminder can surface supported faults such as `0500-8062` instead of showing only ordinary paused progress.
- Restored the missing Print Jobs row action for backend-safe dispatch retries: failed jobs now show Retry dispatch only while the source command failed and physical print status is still pending with no start, progress, or layer evidence, reuse the existing audited Hub retry endpoint, return to the Jobs view, and remain hidden once physical evidence makes retry ambiguous.
- Added reviewed Bambu intermediate-chain completion for X2D/N6-V2: Agent TLS verification now combines the bundled `BBL Device CA N6-V2` certificate with any peer-provided intermediates before validating against the existing Bambu roots, so leaf-only BRTC handshakes remain strict without per-device pins. Regression coverage locks the reviewed intermediate fingerprint and proves a leaf-only root/intermediate/leaf chain validates; per-serial SHA-256 pins remain available for unknown issuers.
- Restored Bambu X.509 v1 compatibility without per-device pins: the shared Agent LAN TLS verifier now handles rustls-webpki's v1 rejection through a narrow direct-root path that requires a bundled trusted Bambu CA RSA/SHA-256 signature, matching inner and outer signature algorithms, the expected printer serial in the certificate common name, and a current validity period. Real TLS 1.2/1.3 handshake regressions accept the trusted path and reject forged signatures, serial mismatches, expired leaves, and untrusted self-signed v1 certificates; explicit SHA-256 pins remain required for unknown issuers.
- Completed Phase 32's external-account cutover: removed manual tenant-user creation and identity-linking HTTP methods plus their dormant frontend actions, retained tenant-scoped user/identity reads and local role updates, and preserved all existing user and identity rows. The dashboard now uses the same tenant-access switcher vocabulary and popover interaction as onboarding while keeping route-backed tenant context.
- Migrated Agent MQTT from the locally patched `rumqttc 0.25.1` source tree to the exact-pinned independent `rumqttc-v4-next 0.33.3` fork, preserving MQTT 3.1.1, Bambu-specific rustls configuration, bounded client queues, and the existing event-loop/PUBACK ownership model. Removed the crates.io path patch, the tracked `vendor/rumqttc` tree, and its Nix source-filter exception; the full 477-test Agent suite and 1,843-test workspace suite pass, including raw-broker, firmware-session, recovery, TLS, and pump-ordering coverage, without live printer commands or file uploads.
- Fixed the GitHub Actions Nix failures caused by the patched local `rumqttc` crate being omitted from the filtered Rust package source; `rustSrc` now carries the tracked `vendor/rumqttc` tree so package, quality, and NixOS VM jobs can resolve the workspace lockfile, allowing Docker publishing to pass its exact-SHA Checks gate.
- Completed the repository security hardening pass: login tickets are type-bound and mobile sessions use PKCE plus current-role authorization; external JWT validation requires audience, bounded-lifetime JWKS refresh, HTTPS outside loopback, and no redirects; plugin account revocation revalidates every persisted Hub URL before sending a bearer; printer MQTT/FTPS/BRTC certificate chains fail closed, with explicit per-serial SHA-256 pins for leaf-only printer certificates; Hub gRPC requires TLS off loopback, advertises only HTTP/2, applies handshake/preface deadlines, and bounds setup connections globally/per peer plus established connections per Agent/tenant; camera chunks, retained HTTP camera responses, and command resources are bounded per tenant where applicable, and rejected camera streams no longer spawn untracked gRPC work; Hub/Agent/plugin response reads, artifact reads, and multipart parsing are constrained, with per-tenant/global staging admission, total parse deadlines, cancellation cleanup, private artifact file modes, and pre-storage transactional quota reservations preventing concurrent disk/S3 oversubscription; dashboard/Auth/Android login flows use state, secure cookies, POST token delivery, same-origin mutation checks, an exact custom-scheme callback host, and non-backup mobile storage; observability and deployment defaults are private/least-privilege; and patched Rust/npm dependencies plus digest-pinned container bases and CI inputs are enforced. Remaining work is routine dependency refresh and ongoing security regression review rather than a known open finding from this pass.
- Fixed the GitHub Actions Nix package and quality-check failures after the React Query lockfile update by refreshing the shared Web/Auth npm dependency hash and keeping the two external-checkout Bambu Studio contract gates out of both the hermetic Nix Nextest job and the network-plugin package's duplicate test phase, as required by their verification design; Docker publishing can now pass its exact-SHA Checks quality gate.
- Completed frontend performance optimization Phase 1: added route-level loading states with Skeleton components, implemented code splitting for DispatchForm and DiagnosticsSection via next/dynamic, migrated static content to React Server Components, created DashboardShellProvider/Layout/Registrar/Consumer architecture, added request-memoized server utilities, and restructured routes into (dashboard) route group with atomic cutover. Bundle measurement shows minimal CSS increase (+0.4%) due to new loading states.
- Prepared the `0.1.0` release metadata and operator handoff: all Rust, Web/Auth, Android, npm lockfile, Nix package, Helm, and changelog versions are checked through one tag-aware release gate; Cargo metadata points to the canonical `ProjectPandar/pandar` repository; Next.js and Better Auth use current patch releases with patched Lodash/PostCSS transitive resolutions and a clean production npm audit, while the Nix Auth package preserves npm's workspace-nested runtime dependencies for migration; the changelog, README release entry, installation artifact coordinates, Helm examples, and maintainer runbook describe the exact `v0.1.0` publication and its unsigned/cross-platform limitations; and GitHub Releases generate notes while remaining gated on the tagged commit's successful Checks run. The tag itself is intentionally not created by this preparation change.
- Split the Settings tenant-token list into its own frontend module without changing behavior, restoring the workspace's enforced 400-line production-module limit.
- Refactored Settings and Users admin views: extracted a shared `AdminSectionGuard` for the no-tenant / load-error / restricted states, unified `LanguageSettingsPanel` and `ThemeSettingsPanel` behind a `PreferencePanel` wrapper, and fixed the non-null-assertion risk by switching to a function-based render prop so `children` only evaluates when a tenant exists.
- Visual polish pass on Settings and Users: unified all raw buttons onto the shared `Button` primitive, replaced hard-coded `slate-*` colors with semantic theme tokens in `TenantSettings` and `RuntimeStatusPanel`, added table row hover states and Lucide icons for clarity, and added focus-visible rings to all interactive elements.
- Extracted shared Tailwind class strings into `frontend/lib/utils.ts` (`actionButtonSm`, `actionButtonSmDanger`, `inputSmClasses`, `monoIdClasses`, `rowHoverClasses`, `cardPanelClasses`, `badgeClasses`, `tableScrollClasses`) and applied them across Users, Settings, and admin panels to eliminate repeated long class chains.
- Refined the New/Reprint print-options panel for repeated operator use: it now has a localized accessible group heading, semantic theme surfaces, high-contrast non-color-only radio selection, equal-width responsive controls, and shared primary-button focus/loading behavior with a full-width narrow-screen action.
- Localized the raw Bambu printer states shown by the shared Devices status badge. `PREPARE`, `SLICING`, `PAUSE`/`PAUSED`, and `FINISH` now use the existing English/Chinese token catalog instead of exposing protocol enums.
- Fixed physical print reconciliation for Bambu's numeric `project_file` submission IDs. The Hub now correlates incoming printer telemetry against the persisted successful command result before weaker artifact or filename fallbacks, using one backend-neutral typed-Serde path for SQLite and PostgreSQL. Matching is scoped to the same tenant, Agent, printer, active print state, and successful dispatch, and only a unique match is accepted, so reprints that reuse an artifact update the correct Job while duplicate submission IDs remain uncorrelated. Existing Jobs, including long-running and stalled prints, can recover after Hub or Agent restarts because the mapping comes from durable command results rather than Agent memory.
- Changed Reprint in both Print Jobs and the Devices attention list from an immediate one-click replay into the shared print setup dialog with the existing artifact fixed in place. Reprint now reads stored 3MF metadata without re-uploading, preselects the source printer, and lets operators review or change Plate, current AMS slot mapping, Use AMS, Timelapse, and all model-supported calibration modes before submission. The authenticated Next.js proxy sends typed JSON overrides to the Hub's terminal-only Reprint route, preserving `job.reprint` audit semantics while reusing the stored artifact. Artifact-copy creation now derives the owning Agent from the selected target printer, so changing to a printer on another Agent produces the correct command owner, artifact download path, wake target, and backend-neutral Job record.
- Expanded Web build-plate recovery to the losslessly supported modern `0500` runtime catalog used by Bambu Studio: `8051`, `8061`, `8062`, `808C`, `809B`, and `80A0`. Devices now opens a localized warning for each condition and preserves Studio's exact per-family action order without exposing unsupported actions. The dialog follows the current official HMS runtime catalog rather than the older packaged resources: these errors use Studio's `Warning` title and exact action labels, `8062` is marker detection for every supported family, and only `31B` receives the `808C` offset-or-debris guidance. Hub still trusts only its locked current printer occurrence: it validates the current error code, family, action, generation, task, session, and native state before forwarding that exact server-owned code. Focused frontend and Hub coverage locks the catalog, copy, automatic dialog behavior, and server-authoritative dispatch; broader HMS parity remains future work because Studio actions such as Assistant and defect-acceptance resume are not equivalent to Pandar's current three recovery commands.
- Ran a full frontend optimization pass across Devices, Jobs, Agents, Users, Settings, and Auth surfaces against the instrument-console design system. Added AA-verified `--success`/`--warning` semantic tokens (emerald-700/amber-700 light, emerald-300/amber-300 dark) and replaced hard-coded red/amber/emerald status colors, making state readable in both themes. Fixed WCAG blockers: active AMS tray and active nozzle now carry icon plus accessible text, the chamber-light toggle exposes `aria-pressed` with a visible on/off label, stop-print requires explicit confirmation, the legacy non-modal `ConfirmDialog` was rebuilt on the shared Base UI dialog (focus trap, inert background, focus restore), one-time secrets render in a `role="status"` region with explicit copy buttons and absolute join URLs, and destructive rotate/revoke confirmations name the exact token and what is retained. Replaced hover-only material flyouts with the shared Popover primitive, capped wide popovers for narrow viewports, moved filament identity onto neutral chips with mono hex values (no white-on-white filament text, no guessed color names), and added a skip-to-content link, a single main landmark, a nav landmark with `aria-current`, `aria-pressed` language switching, and reduced-motion skeletons. Dispatch now surfaces transport failures via `role="alert"`, ties file-size and material-mapping errors to their controls (`aria-invalid`/`aria-describedby`), and replaces the OS `window.confirm` mismatch prompt with an in-app confirmation. Performance: internal dashboard navigation uses `next/link`, job/printer/agent lookups are memoized maps, the admin user-identity join is O(n), and the sidebar collapse cookie is honored on load. Copy was normalized in both locales (Sign out, Print jobs, Tenant tokens, sentence case, unified zh terms 打印板/访问码/吊销, and actionable empty states).

- Closed four cross-boundary security gaps: reverse-camera chunks and closes are now authorized against the authenticated Agent that owns the stream; frontend API IDs use one validated/encoded path-segment builder, including camera and internal proxies; Web startup rejects unknown auth providers and any external-auth/static-token combination; and the NixOS module moves database URLs, Agent credentials/printer access codes, Web static tokens, and Auth secrets to root-owned runtime `EnvironmentFile` inputs while rejecting secret-bearing `extraEnvironment` values and Nix-store secret files.
- Closed three security findings: upgraded `quick-xml` to 0.41.0 for the reachable 3MF attribute-parser advisories and added an attribute-heavy regression; replaced plaintext persisted Bambu access codes with tenant/printer-bound, versioned AES-256-GCM envelopes backed by the required `PANDAR_PRINTER_ACCESS_CODE_KEY`, transactional legacy-row encryption, startup key validation, SQLite/PostgreSQL migrations, and deployment documentation; and hardened GitHub publishing so build checkouts do not retain credentials, release build jobs are read-only, write permissions are job-scoped to publishers, and Docker/GitHub Release publication waits for a successful Checks workflow on the exact SHA.
- Fixed Agent command MQTT delivery by giving every rumqttc connection one lifecycle-owned background event-loop pump, so fire-and-forget publishes reach the broker without depending on `next_report`; its bounded report buffer drops the oldest report instead of blocking outgoing traffic, while current connection failures remain available to report callers with their full context. Hardened BRTC against peer-controlled memory and integer failures with 16 MiB frame/chunk limits, warning logs that include the rejected and configured sizes, fallible frame allocation, checked wire-length/offset/chunk/fragment conversions and arithmetic, and focused raw-broker plus boundary regression tests. No live printer command or file upload was issued.
- Implemented the restrained dashboard/Auth motion system: shared easing and duration tokens now drive explicit transform/opacity/color transitions, desktop sidebar collapse is instantaneous while the mobile Sheet remains directional, Base UI dialogs/popovers/hover cards/tooltips/sheets use interruptible lifecycle transitions with opacity-only reduced-motion paths, and printer actions use an anchored dismissible Popover. One-time secrets, metadata-unlocked dispatch controls, and rare Auth feedback receive a single 150 ms state-entry treatment without delaying interaction or announcements; keyed material editors retain reset behavior behind a stable wrapper, reduced motion removes displacement and spinner rotation while preserving useful feedback, and focused tests cover primitive contracts, menu dismissal, reveal boundaries, keyed resets, sidebar paths, and Auth/secret accessibility behavior.
- Redesigned Settings token management as a dedicated full-width credential surface: active, expired, and revoked records are status-sorted with readable scope labels, relative expiry and last-use metadata, creation time, responsive actions, and a compact active/total summary. Token creation and rotation now keep one-time secrets inside focused dialogs, rotation explicitly preserves or changes the current expiry instead of silently creating an immortal credential, and revoke returns to Settings. The shared Day.js pre-hydration fallback, exact timestamp tooltips, and semantic theme colors keep the administration view readable in dark mode and narrow layouts.
- Moved per-Agent Refresh into the Linked agents Actions column with compact localized labels and an Agents-page return path, reusing the existing refresh-printers command while keeping online and offline rows available.
- Moved state-aware Reprint into each terminal Print Jobs card and removed the separate Recovery actions section, keeping the existing reprint server action and job context while simplifying the Jobs page to one task surface.
- Refined Print Jobs task cards into a responsive hierarchy: filename and update time now pair with the Dispatch/Print pipeline, printer and Agent appear as compact identity chips, and the summary promotes the actionable recovery state while hiding unavailable progress placeholders. Real progress receives an accessible bar with only observed layer/time metrics, full errors remain readable, and structured metadata stays in a compact, collapsed two-column Details section on both desktop and narrow screens.
- Added confirmed per-Job deletion to Print Jobs with a compact row action, localized success/error feedback, and a Next.js authenticated proxy. Hub remains authoritative: only terminal or definitively stalled never-started jobs can be deleted, active or ambiguous jobs return a conflict, and the targeted SQLite/PostgreSQL transaction reuses the bulk-clear locks, audit trail, shared-command checks, and orphan-artifact cleanup without affecting other jobs.
- Added a dedicated GitHub Checks frontend quality gate: PRs and `main` pushes now install the root npm workspace with Node.js 24, run both Web and Auth Vitest suites, and execute explicit `tsc --noEmit` checks for each Next.js application. Existing Nix package jobs remain the authoritative Web/Auth production-build coverage.
- Completed Filament Track Switch passthrough to Bambu Studio: Agent material patches preserve each AMS unit's normalized raw `info` bitmap through Hub events and plugin printer responses, while the Rust network-plugin telemetry forwards only valid `0xE` switch routes with input A/B intact. Missing or malformed switch routes are omitted instead of being projected onto the right extruder, and the Studio ABI probe verifies the raw `print.aux` bit 29 state plus both switch-bound AMS bitmaps.
- Corrected X2D Required Materials grouping to match Bambu Studio: the exact conventional dual-AMS topology now renders `AMS(2)` / B slots under Left AMS and `AMS(1)` / A slots under Right AMS even when a partial material snapshot omits bind fields, while an installed Filament Track Switch uses Studio's full-width dynamic panel with its Ext-L / Ext-R sources visible but disabled.
- Completed full Bambu Studio `print.aux` passthrough: Agent material telemetry now validates and normalizes the printer's raw `cfg`, `aux`, and `stat` hex bitmaps, Hub preserves them across partial updates in backend-neutral SQLite/PostgreSQL storage, and the network plugin emits only observed values so Studio's new-print gate sees the real firmware state. The known SD-card bits 12–13, Timelapse Kit bit 26, Filament Track Switch bit 29, and all unknown future bits survive unchanged through the compiled Studio ABI; explicit empty values remain present as a clear, while missing or malformed flags are omitted instead of being synthesized as zero.
- Replaced Required Materials native selects with a Bambu Studio-style color-aware picker grouped by Left/Right AMS and External sources. Per-plate `slice_info.config` `filament_maps` now carries each used filament's sliced nozzle into the preview; N6 Main/Auxiliary routing disables opposite-side, empty, unknown-route, and AMS type-mismatch sources while allowing same-type color changes and side-correct External spools with a pre-dispatch mismatch confirmation. Dispatch emits Studio-compatible `ams_mapping`, `ams_mapping2`, and `ams_mapping_info` identities, nozzle IDs, presets, and `#RRGGBBAA` colors. Agent materials telemetry decodes Filament Track Switch installation from `print.aux`, preserves that nullable state through SQLite/PostgreSQL and dashboard events, enables every routed AMS across both nozzles when installed, and disables External sources; unknown switch state remains fail-closed rather than inferring cross-side routing from incomplete telemetry.
- Stabilized local Windows acceptance for Jobs dispatch: Agent MQTT command/report transports now use role-scoped UUID client IDs so concurrent or restarted sessions no longer disconnect active telemetry through duplicate client-ID takeover, and Next.js uses worker threads so API route workers no longer fail with `spawn EPERM`. Live N6 observation kept one Agent process with its two expected MQTT connections stable for more than 90 seconds and restored fresh telemetry without issuing a print; the optimized Web build and printer API route both passed. Main CI also received the regenerated Nix frontend dependency hash after the shared Day.js lockfile update, with all eight Checks jobs and Docker green.
- Added Agent-owned periodic printer telemetry refresh: each continuous MQTT report session keeps the existing immediate `pushall` and publishes the same request every fixed 60 seconds with `MissedTickBehavior::Delay`. The timer is cancelled with its report task; a periodic publish failure preserves its cause, invalidates firmware generation and cached device features through the existing production path, waits five seconds, and starts a fresh session-owned timer. Deterministic paused-time coverage proves exact deadlines, simultaneous-ready priority, no burst catch-up, cooperative shutdown, and the t=60/t=65/t=120/t=125 retry timeline. Scheduled requests alone never mark a printer online; only qualifying real MQTT reports reach presence updates. No live printer was exercised. Rollback is to revert the delivery commit and restart the Agent; Hub, frontend, database, and protobuf require no rollback.
- Stabilized firmware lifecycle and ABI verification under full-workspace scheduler load without changing production behavior: the Hub prepare-expiry helper now waits for both durable failure and pending completion-ownership cleanup, the Agent's test-only pump-drop pause releases automatically during unwind with bounded five-second positive scheduling windows, and the Studio firmware mock ignores TCP connections cancelled before any HTTP request byte while keeping malformed and partial requests strict. The formerly racy Hub and Agent cases each pass repeated focused runs with green full crate suites, and the focused firmware ABI probe passes.
- Replaced absolute printer last-seen timestamps on Devices cards with localized presence text: reports under three minutes old show Online, while older reports show a Day.js relative last-online age, driven by one shared clock that schedules exact per-printer transition deadlines.
- Kept Linked Agents table actions compact by using a fixed Delete label while retaining the agent name in confirmation and disabled-state guidance.
- Persisted dispatch-complete print jobs as `stalled` once they remain strictly over 15 minutes from dispatch success without start, progress, or layer evidence; periodic zero-progress telemetry does not restart the timer. The existing 15-second Hub maintenance loop advances the backend-neutral SQLite/PostgreSQL state and publishes `job_progress` so open dashboards update immediately. Late matching RUNNING/terminal reports or positive partial progress can still revive the job, and active-file correlation prefers a unique pending/running reprint over its same-file stalled source. Stalled jobs leave active/failure totals, show a localized warning and fixed recovery summary, and remain clearable, deletable, reprintable, correlated, and retention-eligible.
- Cleared the two Next.js development issues on dashboard routes: the theme bootstrap now uses Next.js' tracked `beforeInteractive` script path instead of rendering a raw client script tag, and mobile sidebar detection keeps the server and first client render identical before applying the viewport result after hydration.
- Changed Jobs dispatch to choose Plate only after artifact metadata finishes loading: parsed 3MF files expose their real, potentially non-contiguous plate IDs in a default-aware selector, selected plate changes update the object summary and material Mapping, and metadata-unavailable files receive the numeric fallback only after preview completes. Dispatch stays disabled while plate metadata is unresolved.
- Fixed authoritative AMS cleanup across Agent reassignment by deleting the prior material snapshot under the current session fence by tenant/printer identity instead of the stale material-row owner, and made higher-revision printer events propagate an explicit `materials: null` tombstone to open dashboards while rejecting equal/lower-revision clears.
- Reworked the Jobs dispatch workflow around compact `New` / `Clear` header actions: the upload form now opens in a focused Dialog, previews only the selected plate's used 3MF filaments, and builds an editable Bambu Studio-style AMS mapping from the selected printer's loaded slots using 1-based source IDs, `tray_info_idx`, exact material type, color distance, distinct-slot preference, AMS-HT IDs, and explicit `ams_mapping` / `ams_mapping2` payloads. Tenant administrators can clear terminal history through a confirmed, audited, backend-neutral operation that preserves queued, dispatched, waiting, running, and suspicious outcome-unknown jobs while safely handling commands, shared artifacts, stored files, filament usage, and machine-event references.
- Cleared stale AMS, AMS-HT, external-spool, and active-tray state when an Agent establishes an authoritative printer connection. Agent startup and runtime connection replacement now identify the synchronization boundary, while Hub deletes the previous material snapshot under the exact current-session fence before accepting new incremental material reports, so switching a saved printer endpoint cannot retain material units that do not exist on the newly connected machine.
- Fixed Jobs artifact previews for current Bambu Studio 3MF files by parsing plate index, prediction, and weight from the canonical nested `slice_info.config` metadata entries, restoring the sliced filament type, preset, color, and usage data that drives editable AMS material mapping while keeping JSON object metadata as an order-independent fallback.
- Fixed printer Edit propagation to Agent runtime connections: Hub now persists a targeted reload command and wakes the owning Agent, Agent fetches the latest saved endpoint without embedding credentials in the command, validates the printer TLS identity before replacing the runtime MQTT/report connection, and treats Hub-saved connections as authoritative on restart. Telemetry snapshots can no longer overwrite an edited host/access code; only link/reload snapshots may update connection fields, with matching SQLite/PostgreSQL query coverage and credential-redaction tests.
- Fixed the current main-branch CI failures by replacing firmware prepare-expiry tests' fixed scheduling margin with bounded terminal-state polling, serializing the two network-plugin callback race tests that share a global pause hook, releasing the Studio firmware callback transition lock between empty polls, making the ABI probe's foreground/background mock-Hub choreography deterministic, and pinning Hestia v2 while caching built paths without their runtime closures to reduce future manifest growth against its hard 64 MiB decoded bound. The cache GC and cold-cache Nix quality timeouts now cover the repository's observed workloads.
- Fixed the GitHub Checks Nix quality job by regenerating the NixOS options reference for `services.pandar.agent.hubApiUrl` and making the test-only SQLite printer snapshot transaction reserve its writer before the first read, preventing the concurrent duplicate-serial regression test from failing with `database is locked` under the isolated release nextest build.
- Completed Bambu Studio's native Cloud/tunnel Firmware page through Pandar for printer-main and every printer-reported AMS-family module, with bounded live version refresh, authoritative exact-session/generation/revision status, generation invalidation and reconnect ownership, all four native command shapes, two-phase one-use delivery, at-most-once execute, no automatic retry/replay after ambiguity, fail-closed signed-URL redaction, and an intentionally empty package catalog. Cloud and LAN ABI paths have deterministic protocol coverage, while Studio's own LAN update-button suppression remains unchanged. This release retains one-active-Hub process-local firmware ownership; no external package or live printer firmware command was used, real PostgreSQL firmware tests were skipped because `PANDAR_TEST_POSTGRES_URL` was unset while SQLite and migration parity were covered, and Web/Android remote OTA plus package staging/hosting remain future C work.
- Wired the Task 7 firmware FFI through the thin Bambu Studio C++ ABI adapter for both Cloud and LAN entrypoints. The shim now owns only opaque session/token/string transfer, callback selection, monotonic return handoff, serialized callback invocation, and stop/join/destroy ordering; Rust remains the sole firmware parser, HTTP, catalog, version, status/reset, and acknowledgement-payload authority. Native catalog reads return empty or real main/AMS records from Hub data, `info.get_version` uses the bounded live Rust refresh with the original sequence and every printer-reported module field, all four firmware commands run before the existing non-firmware parser with exact prepare/execute bodies, and delayed acknowledgements are dispatched 1.1–2 seconds after their own originating return. Credential transitions cancel and advance the plugin generation, heartbeat/status/firmware callbacks share one invocation mutex, callback-to-logout reentry is deadlock-free, and destroy wakes and joins both worker threads before releasing callbacks. The compiled Windows probe uses Visual Studio 2022 MSVC `cl.exe` and covers Cloud/LAN values, status overlap serialization, logout/destroy cancellation, and full rejected acknowledgement fields without downloading firmware or issuing a live printer command.
- Implemented the Rust Bambu Studio firmware plugin layer: presence-preserving typed parsing for all four native commands, duplicate top-level `upgrade` rejection through a custom serde presence visitor, the shared 64 KiB input boundary, URL-free prepare plus at-most-once execute with conservative post-attempt ambiguity, live version refresh, exact current/reset/catalog presentation, per-generation cache invalidation, and per-origin callbacks scheduled from their own return handoff at +1.1s through the +2s deadline. Generation validation/enqueue is serialized with invalidation-first session updates, stopped queues cannot issue tokens, execute redirects are disabled, only typed 4xx pre-publish failures are safe while inconsistent 2xx and all 3xx/5xx remain outcome-unknown, and fresh version success requires a nonempty ordered module list. Every raw-pointer FFI export has an explicit unsafe lifetime and allocation-ownership contract. Deterministic mock-Hub, real-export FFI wait/lifecycle, and clock-controlled tests cover redaction, no retry, reset repetition past three seconds, callback independence/cancellation/stop wakeup, allocation freeing, and typed printer batch validation. No package was downloaded and no live firmware command was sent.
- Added the plugin-authenticated Hub firmware HTTP boundary for Bambu Studio: exact-current typed state plus empty typed catalog, fresh `get_version` refresh, URL-free prepare, one-use path-bound execute, explicit pre-publish/rejected/acknowledged/outcome-unknown responses, and the same session/generation projection in the batch printer list. Boundary tests cover tenant/Agent/replica ownership, former-owner local-token races without token consumption, generic 64 KiB request limits, generation replacement, persistence ambiguity, CAS-before-response, URL/cause non-disclosure, and all four closed command variants. Execute now reacquires authoritative Agent ownership before recording `ExecuteSent`, updates the command inside that same backend-neutral transaction, and holds the fence through dispatch, so a sibling claim that wins the validation gap causes a side-effect-free unavailable response. Task 6 route and firmware-service tests are split into topic modules below the 400-line limit, and the completed Rust plugin plus native ABI consume this boundary.
- Implemented the Hub-owned live firmware command lifecycle for fresh version refresh and two-phase control: URL-free SQLite/PostgreSQL-neutral command/audit records, exact session/generation one-shot ownership, one-second prepared-token expiry, at-most-once execute, phase-safe cancellation/results, live-only non-replay guards, refresh CAS-before-resolution, and fail-closed signed-URL redaction. The completed plugin routes and native network-plugin ABI use this lifecycle; no firmware was downloaded or sent to a printer during verification.
- Hardened the Hub firmware lifecycle review boundary: every typed snapshot, result, and failure string is scrubbed against all matching live signed URLs; authoritative Agent session-row fences serialize dispatch/result claims across Hub replicas; late post-publish rejection stays outcome-unknown; every terminalizing claim remains cleanup-visible through durable phase-safe persistence; and cross-replica stale cleanup protects only the exact durable owner session while it remains fresh under the 45-second Agent lease, so restart/replacement commands cannot be shielded by a newer session.
- Closed the remaining Hub firmware lifecycle races: firmware terminal idempotency now requires the exact typed result while generic command duplicate semantics remain unchanged; prepare expiry is armed immediately after durable creation and recovers cancellation before or after local registration; overlapping signed URLs are redacted longest-first; and typed `owner_instance_id` plus exact session ownership lets a remote active Hub shield its commands without allowing a locally abandoned command to survive stale cleanup.
- Added Web and Android X/Y/Z movement controls at `-10`, `-1`, `+1`, and `+10` mm, with Studio-compatible fixed feedrates of 3000 mm/min for X/Y and 900 mm/min for Z, plus confirmed full-axis Home. Exact Web/Android request-shape tests and Android Compose instrumentation cover all movements, Home confirmation, accessibility labels, and in-flight disabling. Verification used local tests and an emulator only; no real printer was moved or homed.
- Added typed, full-width Bambu `print.fun` passthrough and feature-aware Bambu Studio Home/XYZ controls: unknown bits, bit 63, and valid zero survive Agent -> Hub -> plugin, while exact-session capability gates prevent stale modern commands from reaching an old or mismatched Agent. Bit 32 selects `back_to_center`, bit 38 selects strict `xyz_ctrl`, and legacy Studio home/movement semantics remain available without extra axis restrictions or direction inversion. This work has deterministic fake/loopback and compiled-ABI evidence only; no real printer was homed or moved.
- Shipped the Web print monitor for Studio- and printer-originated tasks even when no Pandar Job exists: device cards now show the live task name, percentage, current/total layer, remaining time, finished-task details, and typed HMS diagnostics from enriched printer snapshots.
- Added Pandar-owned build-plate mismatch recovery for the native supported model/action catalog. Hub revalidates the exact error occurrence, task/session marker, printer state, model, catalog guard, and Agent capability, while Web and Studio plugin operations share one native-recovery single-flight. Web sequence-zero dispatch uses a fresh MQTT connection and treats the matching QoS1 PUBACK only as transport confirmation; printer telemetry remains authoritative for whether recovery occurred. The Web warning dialog now reserves enough width for all three English recovery actions, preventing its grid content and footer from overflowing the dialog frame.
- Restored Bambu Studio's native printer-error flow from the reference direct-connection behavior: Agent preserves numeric `print_error` presence and the independent printer `job_id`, Hub persists and exposes both on SQLite/PostgreSQL, Studio telemetry keeps Printing Progress/HMS/AMS live, exact typed `get_version`/`pushall` handling unblocks Sync AMS Filament, and native Resume/Ignore/Stop actions emit the reference `param:"reserve"` MQTT payload instead of falling back to ordinary controls.
- Bound native print-error actions to the exact capable Agent session, session token, and transactionally revalidated printer owner, with exact-session replacement/close/expiry cleanup. SQLite and real PostgreSQL race tests plus the mandatory compiled Studio ABI probe cover this path. Roll out additively as Agent → Hub/migration → plugin; roll back as plugin → Hub → Agent only after every sent/pending `operation.type:"handle_print_error"` command is terminal, leaving the nullable columns in place. A real-printer recovery-action click remains intentionally unperformed unless the operator explicitly authorizes it.
- Preserved raw Bambu MQTT `print` report context inside Agent print-error diagnostic payloads so Hub `machine_events` retain the original rejection fields when a printer immediately reports `FAILED` after a `project_file` command.
- Aligned Bambu Studio N6 Main/Aux nozzle ordering by preserving Bambu Studio's physical dual-nozzle id convention where the Hub's right/Main nozzle is id 0 and left/Deputy nozzle is id 1, sorting `device.nozzle.info` and `extruder.info` by Studio physical id so Sync info resolves the correct nozzle/extruder entries, and normalizing raw Bambu nozzle codes to Studio-friendly four-character codes so Sync info can distinguish High Flow from Standard by the encoded flow character.
- Added legacy top-level Bambu Studio dual-nozzle metadata fields (`nozzle_type2` / `nozzle_diameter2`) alongside the V2 device block so Studio UI paths that still consult top-level nozzle metadata can display both Main and Auxiliary diameters.
- Changed Agent camera streaming to open `ReverseCamera` only on an explicit Hub camera request over the existing control stream, eliminating the idle startup camera tunnel while keeping per-viewer close/replacement behavior.
- Enabled tonic TLS native-root support for `pandar-agent` gRPC client connections and added a local TLS gRPC regression test for the generated Agent control client.
- Wrapped the Nix `pandar-agent` package with a default `PANDAR_FFMPEG_PATH` and ffmpeg runtime `PATH` entry so Agent-mediated camera streaming can spawn the fragmented MP4 transcoder on NixOS deployments.
- Fixed Agent camera connection persistence: printer telemetry snapshots no longer erase saved `host`/`access_code` values when the Agent reports only status data, and the NixOS module now exposes `services.pandar.agent.hubApiUrl` for `PANDAR_HUB_API_URL` so restarted Agents can reload saved printer connections.
- Fixed GitHub Checks failure on the Nix quality checks job: gated the Windows-only `installs_and_restores_swscale_proxy` test in `pandar-studio-hook` with `#[cfg(windows)]` (and its `use super::*` import) so it no longer panics on Linux CI under `--deny warnings`.
- Fixed GitHub Checks failure on the Nix package checks (aarch64-linux) job: on `aarch64-unknown-linux-gnu` rustc links cdylibs with the system bfd linker and auto-emits an anonymous version script that conflicts with `pandar-network-plugin`'s build.rs export map ("anonymous version tag cannot be combined with other version tags"). Added an `aarch64-linux`-only `-C link-arg=-fuse-ld=lld` rustflag (plus the `lld` package in `nativeBuildInputs`) in `nix/pandar.nix`; lld merges the two version scripts and keeps the build.rs exports, matching x86_64-linux.

- Replaced Agent printer-operation report retention with typed `MachineJsonPayload` storage instead of carrying raw MQTT `Value` through the operation dispatch result.
- Replaced Agent printer-operation MQTT result summary extraction with typed dispatch metadata instead of converting `MachineJsonPayload` back through `serde_json::Value` in the command-result serializer.
- Replaced Agent MQTT `get_version` report detection with typed report ownership and a typed helper method instead of passing borrowed raw `Value` reports through model discovery.
- Replaced Agent chamber-light status parsing with a typed serde report method instead of passing raw MQTT `Value` reports into the light-control helper.
- Replaced Agent printer-operation result matching with a typed operation report wrapper instead of repeatedly deserializing raw `Value` helper inputs for sequence IDs and errors.
- Replaced Agent printer-operation command sequence tracking with typed MQTT command metadata instead of re-reading `sequence_id` from serialized JSON payloads.
- Replaced Agent chamber-light operation dispatch with typed MQTT command structs instead of passing raw JSON payload lists through the operation layer.
- Replaced Agent project-file signing nozzle-id mutation with typed serde payload structs instead of mutating the MQTT payload as raw `Value`.
- Replaced Agent print dispatch and printer-operation result payload fields with a shared typed serde JSON enum instead of exposing raw `Value` across the command boundary.
- Replaced Agent material patch normalization output with a typed serde document instead of returning a raw `Value` for production serialization.
- Replaced Agent MQTT print diagnostic payloads with a typed recursive serde payload enum instead of converting typed diagnostics back into raw `Value`.
- Replaced Agent command-result MQTT payload/report serialization fields with typed recursive serde JSON instead of embedding raw `Value` references in result structs.
- Replaced Agent MQTT project-file signing envelope fields with typed recursive serde JSON instead of passing the signed `print` section through raw `Value`.
- Replaced Agent material report nozzle-list detection with a typed serde nozzle entry instead of storing the report entries as raw `Value`.
- Replaced Hub material snapshot repository response fields with typed recursive serde material JSON instead of exposing persisted material state as raw `Value`.
- Replaced Hub printer-event material response fields with a typed recursive serde enum so event material payloads scrub and serialize without exposing raw `Value` fields.
- Replaced Agent BRTC upload chunk test expected payloads with typed serde structs instead of handwritten `json!` objects.
- Replaced Agent fake MQTT command echo parsing with typed command and sequence-id fields instead of `Value` scalar probes.
- Replaced Agent MQTT `project_file` AMS mapping parsing with typed serde mapping structs instead of parsing into `Value` arrays and probing scalar fields manually.
- Replaced Agent MQTT snapshot test report fixtures with typed serde structs and split the fixture definitions into a dedicated module to keep test files under the 400 LOC split threshold.
- Replaced Agent MQTT command payload reference assertions with typed serde fixtures for pushall, get-version, print controls, chamber light, extruder selection, and gcode-line commands.
- Replaced Agent MQTT refresh-flow state reports and expected publish payloads with typed serde fixtures for get-version, pushall, and print state reports.
- Replaced Agent MQTT print-error and HMS diagnostic parsing with typed serde diagnostic enums instead of manually walking `Value` arrays and objects.
- Replaced Agent MQTT diagnostic/HMS unknown payload preservation with a typed recursive serde enum instead of `Value` catch-all fields.
- Replaced Agent printer-operation MQTT result field extraction with a typed recursive serde enum for result, reason, err_code, and errno while retaining the raw report only as a debug payload.
- Replaced Agent MQTT report test fixtures with typed serde structs for progress reports, temperature snapshots, AMS material reports, external spool reports, and raw payload preservation.
- Replaced Agent machine gateway tests' MQTT report and command expectations with typed serde fixtures for refresh, project-file dispatch, light controls, temperature/G-code controls, AMS operations, and raw runtime state reports.
- Replaced the first Agent material-normalization report fixtures with typed serde structs and split shared decoded patch test types into a fixture module under the 400 LOC split threshold.
- Replaced the Hub printer command detail discovery-result fixture with typed serde structs instead of an inline `json!` object.
- Replaced Agent command test MQTT report fixtures with typed serde structs for get-version and AMS-ready reports.
- Replaced Hub printer route test request bodies with typed serde structs for printer updates, printer controls, and link-printer validation payloads.
- Replaced Hub plugin route test request bodies and audit metadata fixtures with typed serde structs for login-ticket, ticket-exchange, and redaction coverage.
- Replaced Hub printer-command route test request bodies with typed serde fixture structs for discovery, diagnostics, and printer-control payloads.
- Replaced Hub job recovery route test request bodies with typed serde fixture structs for retry, reprint, duplicate, and invalid recovery payloads.
- Replaced Hub provisioning workflow test request bodies with typed serde fixture structs for user, identity, tenant-token, agent-pairing, and retired API-token mutations.
- Replaced Hub onboarding route test request bodies with typed serde fixture structs for tenant self-creation, join-link creation, and join-link acceptance.
- Replaced Hub tenant-token route test request bodies with typed serde fixture structs for token creation/rotation, agent creation, user creation, and retired token routes.
- Replaced Hub provisioning agent-pairing request bodies with typed serde fixture structs instead of handwritten `json!` payload objects.
- Replaced Hub print-job AMS mapping request/response validation with typed serde structs for `ams_mapping`, `ams_mapping2`, and `ams_mapping_info`, including multipart parsing, persisted mapping reads, repository validation, and gRPC command conversion.
- Replaced `ams_mapping_info` unknown-field passthrough in Hub and Agent project-file payload types with typed recursive serde enums instead of flattening into raw `Value`.
- Replaced the remaining fixed-shape Rust test `json!` fixtures with typed serde structs for route bootstrap requests, job artifact metadata, PostgreSQL metadata checks, and job material patches, while splitting oversized repository and route test modules under the 400 LOC threshold.
- Replaced Agent printer-operation MQTT result and chamber-light status field probing with typed serde report envelopes while preserving the raw MQTT report in command results.
- Replaced Agent BRTC upload setup/init/chunk request serialization and upload reply parsing with typed serde protocol structs instead of manual `serde_json::Value` field probing.
- Replaced Agent BRTC frame handling with direct typed serde deserialization plus retained raw JSON text for diagnostics instead of routing setup/upload replies through `serde_json::Value`.
- Replaced Agent MQTT `get_version` report detection and model extraction with typed serde structs for the `info` report section.
- Replaced Agent MQTT printer snapshot/progress extraction with typed serde report structs for status, telemetry, nozzle info, lights, print progress, and diagnostic objects.
- Replaced Agent MQTT project-file signing and fake transport command echo field handling with typed serde payload structs.
- Moved Agent material patch normalization behind a typed `MaterialsReport` input so MQTT `Value` handling stays at the parse boundary instead of flowing into material business logic.
- Moved Agent printer snapshot construction behind a typed `SnapshotReport` input so production MQTT report handling parses raw JSON before snapshot business logic.
- Moved Agent print-progress and diagnostic report construction behind a typed `PrintReportEnvelope` input so raw MQTT `Value` parsing remains at the report boundary.
- Replaced Agent printer-operation report payload conversion with direct typed `MachineJsonPayload` construction instead of serializing the parsed report envelope back through `serde_json::Value`.
- Replaced Agent recursive machine JSON payload conversion with direct enum mapping instead of deserializing a `serde_json::Value` back into the same typed payload shape.
- Replaced Agent MQTT typed report helper with direct serde deserialization from `serde_json::Value` instead of stringifying payloads before parsing typed structs.
- Replaced remaining Rust test helper JSON round-trips with direct serde deserialization from `serde_json::Value` for Agent MQTT payloads, material patches, Hub route responses, compatibility payloads, identity fixtures, and material repository fixtures.
- Replaced Agent material scalar string formatting with direct typed enum formatting instead of constructing temporary `serde_json::Value` scalars.
- Replaced Hub material merge state conversion with direct typed material JSON construction instead of serializing through `serde_json::Value` and deserializing it again.
- Replaced Agent MQTT `project_file` command payload construction with a serializable struct instead of manually assembling a JSON object map.
- Replaced Agent print-project test dispatch payload fixtures with typed serde structs and split the oversized print-command test support into a sibling module.
- Replaced Agent MQTT control command payload construction with serializable structs for info, pushall, print controls, light control, AMS RFID/load/unload, and temperature/G-code operations.
- Replaced Agent material snapshot normalization input parsing with typed serde structs for AMS reports, units, trays, external spools, and dual-nozzle/external-slot detection.
- Replaced Agent material snapshot scalar fields with typed serde scalar/color enums so IDs, humidity, temperatures, active trays, colors, and toolhead selectors no longer deserialize as raw `Value` before normalization.
- Replaced Agent material-normalization tray bitmask and power-state test fixtures with typed serde report builders instead of inline JSON objects.
- Replaced the remaining Agent material-normalization inline JSON test inputs with typed serde report builders for invalid AMS shapes, external spool sources, active trays, AMS-HT trays, and credential filtering.
- Replaced fixed-shape Agent link-printer and print-project command result JSON construction with serializable structs.
- Replaced fixed-shape networking plugin ticket/no-auth request bodies and local webserver/callback response JSON construction with serializable structs.
- Replaced networking plugin print-submission AMS mapping passthrough values with typed serde mapping structs before multipart serialization.
- Replaced Bambu Studio network-plugin installer config passthrough fields with a typed recursive serde enum instead of flattening unknown config fields into raw `Value`.
- Replaced Hub printer-operation audit metadata construction with typed serializable metadata structs instead of ad hoc JSON maps.
- Replaced Hub audit-event metadata redaction with a typed recursive serde enum instead of deserializing persisted metadata as raw `Value` and manually matching JSON fields.
- Replaced Hub audit-event actor metadata merge internals with a typed recursive serde enum instead of merging raw `Value::Object` maps.
- Replaced Hub audit-event response metadata serialization with the typed redacted metadata enum instead of converting back through `serde_json::Value`.
- Replaced Hub job and plugin artifact metadata preview/response/persisted-read handling with the typed `ArtifactMetadata` struct instead of serializing and deserializing through `serde_json::Value`.
- Replaced Hub material patch parsing and merge identity extraction with typed serde patch structs for patch documents, AMS units, trays, external spools, and persisted material identities.
- Replaced Hub material repository test patch fixtures with typed serde structs, including absent-vs-null merge coverage, and split material merge tests into a sibling module under the 400 LOC threshold.
- Replaced Hub printer route test JSON response and command-payload assertions with typed serde structs for printer lists/details, material snapshots, printer controls, refresh commands, link-printer commands, and error responses.
- Replaced Hub printer-event material JSON credential scrubbing with a typed recursive serde enum instead of recursively matching raw `Value`.
- Replaced Hub result/error JSON redaction internals with a typed recursive serde enum instead of mutating raw `serde_json::Value`.
- Replaced Hub plugin route test response assertions with typed serde structs for login tickets, plugin sessions, Studio printer devices, plugin print/job responses, and plugin error responses.
- Replaced Hub printer-command route test response, command-payload, audit-metadata, and error assertions with typed serde structs for discovery, diagnostics, printer controls, command details, and invalid request responses.
- Replaced Hub identity verifier test audience-claim and JWKS JSON fixtures with typed serde structs.
- Replaced Hub gRPC printer material/snapshot test material patch fixtures with typed serde structs and split the oversized printer-material test support into a sibling module.
- Replaced fixed-shape `pandar` CLI JSON output construction with serializable structs instead of ad hoc `serde_json::json!` values.
- Matched Bambu Studio/open-bamboo-networking local print upload behavior by naming dispatched artifacts as `*.gcode.3mf`, prioritizing BRTC eMMC upload for supported printer families, falling back to FTPS on BRTC failure, and publishing `project_file` with the actual upload URL plus MD5.
- Aligned Agent `project_file` MQTT dispatch with Bambu Studio/open-bamboo-networking's payload shape, including unconditional `ams_mapping2`, default print option fields, and Studio-compatible task metadata.
- Matched Bambu Studio/open-bamboo-networking's MQTT QoS behavior for `project_file` dispatch by publishing print-start commands with QoS 0 instead of the generic control-command QoS.
- Matched Bambu Studio/open-bamboo-networking's LAN print identity fields by sending `task_id` and `subtask_id` as `"0"` in printer-facing `project_file` commands instead of Pandar internal UUIDs.
- Matched open-bamboo-networking's BRTC upload chunk ABI by omitting `file_md5` from non-final file chunks and sending it only on the final chunk, and persisted print dispatch upload/MQTT details in print-job command result JSON for real-printer debugging.
- Forwarded Bambu Studio's `ams_mapping_info` through the networking plugin, Hub, gRPC, Agent, and printer MQTT `project_file` payload, and matched open-bamboo-networking's signed H2D-family payload behavior when a local Studio slicer key is available.
- Replaced Agent project-file signing `ams_mapping_info[].nozzleId` handling with a typed serde integer field instead of deserializing it as `Value` and probing with `as_i64`.
- Replaced Agent project-file signing unknown-field passthrough maps with a typed recursive serde enum instead of flattening into raw `Value` maps.
- Kept Agent print-report streams alive when printers report non-Pandar job identifiers by ignoring invalid report `job_id` values instead of rejecting the whole reverse stream.
- Added an Agent-authenticated printer connection hydration endpoint and Agent startup restore path so saved LAN printer host/access-code details survive Agent restarts when `PANDAR_PRINTERS` is empty.
- Kept Bambu Studio's native Device page connected by having the Pandar networking plugin keep emitting Studio-style `push_status` heartbeats for selected/subscribed printers instead of sending only a one-shot connection snapshot.
- Forwarded cached printer temperature and light telemetry through the Bambu Studio networking plugin `push_status` payload so Studio's native Device page can render current nozzle, bed, chamber, and lamp state.
- Parsed Bambu's native numeric HMS entries (`attr` / `code`) as typed Agent data, preserved present-empty versus absent snapshots across gRPC, and stored current printer HMS for both SQLite and PostgreSQL.
- Persisted field-merged live print state on the printer before Pandar job correlation so external or otherwise unmatched prints still expose progress, remaining minutes, layers, task metadata, and HMS through the Studio plugin API.
- Replaced the networking plugin's hard-coded idle progress with typed Rust `push_status` fields and refreshed its Hub cache before Studio `pushall` replies and two-second heartbeats, while retaining the last good status when refresh fails.
- Added a Bambu Studio plugin no-auth session path for trusted local Hub development and persisted the plugin token/profile under the Studio config directory so Studio restarts restore Pandar login state without repeating sign-in.
- Allowed Edit printer to update Hub-local printer metadata without requiring a live Agent session, while blank LAN IP/access-code fields preserve existing connection details.
- Fixed the Bambu Studio plugin printer list response so Studio receives top-level `devices` with its native `dev_name` / `dev_online` / `dev_model_name` / `task_status` fields instead of Pandar's tenant API `printers` shape, and allowed trusted local no-auth plugin sign-in to create the Studio plugin token.
- Fixed Bambu Studio localized plugin login URLs by serving `/<locale>/sign-in` from the local plugin webserver, making embedded plugin assets root-relative, and bridging Continue sign-in to Studio's localhost ticket callback.
- Updated the Bambu Studio network plugin operation bridge for the latest Agent printer operations: direct semantic operation submissions now accept select-extruder, targeted hotend, bed/chamber temperature, and AMS RFID/load/unload payloads, while G-code translation covers targeted hotend and bed/chamber temperature commands.
- Split oversized production Rust modules across `pandar-agent`, `pandar-hub`, and `pandar-network-plugin` into focused sibling modules without using `include!`, moved inline tests out of runtime modules, and added a workspace production-module size guard.
- Added `PANDAR_HUB_NO_AUTH=true` for local/trusted no-auth Hub HTTP/WebSocket operation, with startup warning logging, bootstrap and tenant API auth bypass, `no_auth` audit attribution for mutations, and docs that agent reverse gRPC credentials remain required.
- Enabled dashboard live printer-event WebSocket updates in Hub no-auth mode without requiring browser event tickets.
- Updated the frontend design tokens and design-system documentation to the neutral OKLCH light/dark palette, with white as the light root page background.
- Added frontend light/dark theme selection with a system default that follows `prefers-color-scheme`.
- Moved the frontend theme bootstrap script out of the client theme provider so Next.js no longer warns about rendering `<script>` tags from React client components.
- Fixed the frontend sans-serif font token so English pages render with the intended non-serif UI font stack.
- Added a `pandar install-network-plugin --plugin-file <path>` operator command backed by the `pandar-network-plugin` crate; it installs a specified file as the Bambu Studio network plugin and patches `BambuStudio.conf` following the open-bamboo-networking manual-installation flow.
- Fixed Bambu Studio networking plugin startup/login compatibility by starting the plugin local sign-in server during `bambu_network_start`, returning Bambu Studio-compatible token/profile ABI payloads, preserving installed plugin loading through Studio startup, and adding ABI probe coverage for the Studio startup/login shape.
- Fixed Bambu Studio print submission callback ABI parity by reporting `PrintingStageFinished` with success code and the Studio countdown info instead of sending an out-of-range stage value that could crash Studio after a Pandar print submission.
- Split job history, print dispatch, and recovery actions into a dedicated Jobs dashboard page while keeping Devices focused on overview, attention, and printer inventory.
- Reworked the Devices printer inventory into an unframed section with shadcn Empty states, larger desktop empty-state spacing, a dialog-based machine form for linking printers, and per-printer machine cards.
- Allowed the frontend dev server to serve Next.js dev resources from `127.0.0.1` so local browser interactions work on both loopback hostnames.
- Fixed dark-mode contrast for the dashboard header and fleet status strip.
- Linked the overview status strip Agents and Jobs stats to their dashboard pages while preserving the selected tenant.
- Replaced overview status strip grid dividers with straight per-stat separator lines in light and dark themes.
- Added the left status-strip separator and inset all stat separators by 8px.
- Moved status-strip separators outside stat hover backgrounds while preserving the 8px gap.
- Offset status-strip hover bubbles by 8px from the separator while preserving the separator's 8px left inset.
- Delayed the status-strip desktop row, three-column stats, and separator lines to the large breakpoint so medium-width sidebars do not crush stat labels and values.
- Allowed frontend server actions to run in local no-auth mode or with server-configured API tokens instead of requiring a browser auth cookie before submitting mutations such as Link printer.
- Preserved the operator-provided printer name after runtime link by keeping subsequent agent snapshots from overwriting the stored printer display name for the same serial number.
- Moved the printer card agent detail into the top summary status row beside the status badge, showing the agent icon and name without a separate information card.
- Added a printer card actions dropdown with a confirmed Delete printer flow backed by the Hub printer delete API and audit event.
- Moved dashboard language selection from the top bar into Settings and kept tenant selection in the sidebar, now presented through the tenant-access switcher.
- Added Agent-backed AMS refresh: printer refresh now opportunistically refreshes AMS/external-spool snapshots from Bambu MQTT `pushall`, operators can queue per-printer AMS refreshes from the printer inventory, Agent material-only updates sync to Hub over gRPC, and Hub publishes material-aware printer updates to the browser event stream.
- Replaced printer-card filament counts with an AMS/external loading view that shows slot color, remaining estimate, K value, unit temperature/humidity, toolhead assignment, and hover actions for RFID reread, filament load, and filament unload through the Hub-to-Agent printer-operation path.
- Fixed the AMS slot hover menu so moving from a filament slot into its dropdown crosses a hover bridge instead of closing on the spacer gap.
- Rendered AMS remaining estimate `-1` as an unsupported material-sensor state, with a darker gray progress bar and `Unsupported` copy instead of `-1%`.
- Accepted real Bambu-branded model names such as `Bambu Lab X2D` / `Bambu Lab P2S` in compatibility checks so AMS load/unload controls are not rejected before they reach the Agent.
- Matched Bambu Studio's dual-nozzle AMS load behavior by carrying optional `extruder_id` from the printer-card load action through Hub persistence, gRPC, Agent operation parsing, and the final Bambu MQTT `ams_change_filament` payload.
- Made AMS RFID reread produce a refreshed material snapshot after the Bambu `ams_get_rfid` command so the Devices UI receives updated slot data instead of only a command success result.
- Made AMS load/unload operations refresh material snapshots after dispatch so the Devices UI receives updated slot state for the same class of async Bambu material operations.
- Normalized Bambu AMS `humidity_raw` as the displayed humidity percentage and kept `humidity` as the raw 1-5 humidity level so X2D/AMS-HT sensors do not render level values as percentages.
- Accepted decimal-string AMS `temp` sensor readings from Bambu MQTT so the existing Devices AMS header can show temperature beside humidity.
- Humanized active AMS slot labels in printer status details so zero-based machine indexes such as `AMS 0:2` render as operator-facing labels like `AMS A - 3`.
- Matched Bambu Studio's incrementing MQTT `sequence_id` behavior for Pandar's Studio-style printer commands, including `get_version`, `pushall`, print controls, `gcode_line`, `project_file`, and AMS RFID/load/unload controls.
- Kept Studio-style MQTT `sequence_id` generation inside Bambu Studio's `20000..30000` command range by wrapping after `29999` and recovering out-of-range counters back to `20000`.
- Matched printer-operation MQTT reports back to their dispatched `sequence_id`, persisted the machine result/error JSON through Hub command results, broadcast command-result events over the printer WebSocket stream, and surfaced printer-control completion/failure with Sonner toasts in the dashboard.
- Added printer-card temperature telemetry for nozzle, bed, and chamber readings from Bambu MQTT snapshots through Agent, Hub, HTTP/WebSocket payloads, and the Devices UI, with inline Stop/Pause controls beside the temperature row.
- Aligned live temperature parsing with Bambu Studio's V2 device report format so X2D-style bit-packed extruder, bed, and chamber readings populate the Devices UI, and simplified nozzle cards to show current single- or dual-nozzle temperatures without target-temperature noise.
- Suppressed zero or missing target temperatures in printer temperature cards so idle heaters render as a single current reading instead of `current / 0`.
- Moved printer Stop/Pause actions into a dedicated Controls section below the status area instead of sharing the temperature telemetry row.
- Kept printer Stop/Pause actions on a single row inside the Controls section.
- Added a full-width dual-nozzle switch row below the three-column nozzle, bed, and chamber temperature cards, with active nozzle highlighting driven by machine snapshot state and Bambu Studio-style `select_extruder` dispatch. The full-width row keeps both nozzles' diameter/type details inside the control instead of overflowing the former fixed `5rem` column.
- Kept the printer temperature/status controls in a two-row layout at medium viewport widths so the cards do not collapse before the wider desktop breakpoint.
- Added clickable nozzle-temperature controls in the Devices printer cards, with single-nozzle presets/custom input and dual-nozzle left/right controls that highlight the active nozzle.
- Matched Bambu Studio's dual-nozzle temperature behavior by sending targeted hotend changes as MQTT `set_nozzle_temp` with `extruder_index`, while preserving legacy single-nozzle `M104`/`M109` dispatch.
- Added clickable bed and chamber temperature controls in the Devices printer cards, using Bambu Studio-style `M140`/`M190` and `M141`/`M191` temperature dispatch through Hub and Agent.
- Added a printer-card Edit printer dialog for updating display name, LAN IP, and access code through the existing redacted Hub-to-Agent printer-link command path.
- Completed the printer Controls light toggle by matching Bambu Studio's chamber light behavior: Agent sends both `chamber_light` and `chamber_light2` commands and treats the primary light success as the operation result when secondary light reports an unsupported-node failure.
- Added an Agent-mediated camera tunnel for RTSP-capable Bambu models: Hub exposes tenant/printer-scoped fragmented MP4 streaming through a dedicated reverse camera gRPC stream, Agent pulls the printer RTSPS feed directly through ffmpeg, and the Devices Controls panel opens a native video View camera dialog without exposing LAN credentials to the browser.
- Replaced the camera dialog's native video controls with a live-stream display and a single custom fullscreen button so the browser does not show a misleading seek/progress bar for the fragmented MP4 stream.
- Exposed printer LAN IP/access-code metadata to the Bambu Studio plugin device list and status/camera ABI path so Studio receives the correct `X2D` device name plus LAN RTSPS camera URL; verified the installed plugin loads without being overwritten, lists the printer shape, queues a chamber-light operation, and reads a frame from the printer RTSPS camera.
- Filled Bambu Studio-compatible nozzle metadata in both V2 `device.nozzle` and legacy `nozzle_type` / `nozzle_diameter` `push_status` fields so Studio's Printer Parts and print-send nozzle validation can resolve installed nozzle type, diameter, and flow.
- Mapped Bambu Studio send-print requests from Studio serial `dev_id` back to Pandar printer UUIDs before posting plugin print jobs, treated plugin multipart `null` AMS mapping fields as absent, and preserved Hub `invalid_printer_id` errors instead of collapsing them into generic invalid responses.
- Fixed Bambu Studio's persistent `Connecting...` device state by preserving the networking plugin server-connected callback, treating `start_subscribe("app")` as a module subscription, keeping listed printers subscribed for status heartbeats, returning Studio-native model IDs, using the printer serial as Studio `dev_id`, mapping Studio device IDs back to Pandar printer UUIDs for camera/control APIs, and emitting valid `lights_report` JSON in `push_status` so Studio can parse the first status push.
- Fixed Bambu Studio's native Device temperature mapping by emitting the new-protocol `cfg` / `fun` / `aux` / `stat` gates and complete V2 extruder entries (`stat` / `hnow`) in plugin `push_status`, allowing Studio to parse bed, chamber, and multi-extruder temperatures from the `device` status block.
- Fixed the remaining Bambu Studio native Device status mapping by emitting the printer-connected callback during subscription/selection and returning Studio-style `info.get_version` module responses before `push_status`, so Studio initializes the selected machine and renders nozzle, bed, chamber, lamp, and camera status together.
- Added Bambu Studio JSON control parsing for native pause/resume/stop, speed, extruder, nozzle/bed/chamber temperature, AMS RFID/load/unload commands, and forwarded Hub AMS plus external-spool material snapshots into the plugin `push_status` payload.
- Forwarded Bambu Studio native cloud-path printer controls from the networking plugin `send_message` ABI into Hub printer operations so Studio Device controls such as Lamp and hotend temperature mutate the real printer instead of only updating displayed state.
- Emitted Studio-native nozzle metadata in networking plugin `push_status` device blocks so Bambu Studio send-job preflight sees valid nozzle type and diameter information.
- Fixed Bambu Studio nozzle metadata sync by passing the Hub-reported per-nozzle type and diameter through the networking plugin's legacy `nozzle_type` / `nozzle_diameter` fields and V2 `device.nozzle.info` entries instead of hard-coding `XS01` / `0.4`.
- Carried Bambu nozzle diameter/type metadata from Agent snapshots through Hub printer responses and displayed it in the Devices dual-nozzle switch details.
- Moved Bambu Studio `push_status` telemetry, nozzle, AMS, and external-spool payload construction out of the C++ networking plugin shim into typed Rust serde structs, leaving `shim.cpp` focused on the Studio C++ ABI boundary.
- Replaced networking plugin local config, Hub error, G-code, and Bambu Studio control-message JSON field assertions with typed serde request and operation structs/enums, keeping `Value` only for open-ended config or AMS mapping pass-through.
- Replaced additional known-shape JSON handling with typed serde models for 3MF plate metadata, terminal filament-usage material snapshots, and agent printer-operation result serialization.
- Replaced command repository fixed-shape payload and audit metadata test assertions with typed serde structs instead of deserializing to `Value` and indexing fields.
- Replaced print-job repository queued command payload test assertions with the shared `PrintProjectFilePayload` serde type instead of ad hoc `Value` indexing.
- Replaced Hub phase-1 agent delete audit metadata and gRPC printer snapshot material-event tests with typed serde test structs instead of fixed-field `Value` indexing.
- Replaced Hub artifact download route error-response byte comparisons with typed serde response decoding, and split artifact route test support into a focused submodule under the 400 LOC limit.
- Replaced Hub print-job route AMS mapping test payload construction with typed serializable fixtures and split the create-job mapping cases into a focused submodule under the 400 LOC limit.
- Replaced Hub provisioning access route test request bodies with typed serde fixtures instead of handwritten `json!` payload objects.
- Replaced Hub agent route test create-agent request bodies with a typed serializable fixture helper instead of handwritten `json!` payload objects.
- Added a Windows-only Bambu Studio development hook DLL that can proxy `swscale-8.dll` from a copied Studio directory and force new Studio logs onto Bambu's local fallback log key for decryptable development logs.
- Normalized top-level Bambu `vt_tray` / `vir_slot` material reports into external spool snapshots so the Devices Filaments panel can show external materials.
- Completed: Agents page now includes tenant-aware pairing guidance, restricted/no-tenant states, and in-context pairing creation for tenant admins.
- Created the initial Rust workspace with `pandar-core`, `pandar-hub`, `pandar-agent`, and `pandar-app`.
- Added a repository-backed Axum hub with health, summary, tenant create/list, and tenant-scoped agent create/list endpoints.
- Added a minimal agent CLI boundary and a Bambu machine gateway trait for future SFTP/MQTT work.
- Added the first gRPC protocol contract under `proto/pandar/agent/v1/agent.proto`.
- Added a minimal Next.js frontend skeleton using `APP_API_URL`.
- Added `docs/architecture.md` with the target component split and reference-derived machine communication notes.
- Added Phase 1 SQLx persistence for SQLite and PostgreSQL with migrations, repository tests, SQLite durability coverage, and optional PostgreSQL tests behind `PANDAR_TEST_POSTGRES_URL`.
- Pushed the Phase 1 foundation to `main` at commit `1b02636`.
- Added Phase 2 generated gRPC protocol plumbing through build scripts so protobuf Rust output stays under Cargo `target`.
- Added the hub reverse gRPC service, live session registry, command ledger transitions, HTTP+gRPC startup, and the agent reverse client.
- Added SQLite-backed gRPC tests for session lifecycle, command dispatch, acknowledgement, result handling, stale stream protection, and replacement sessions.
- Added Phase 3 agent-side Bambu MQTT models, payload builders, fake/runtime transport boundary, refresh gateway, and `RefreshPrinters` snapshot/result sequencing.
- Added Phase 3 agent-local `PANDAR_PRINTERS` parsing with startup validation and no-network empty config behavior.
- Added Phase 3 machine file-transfer boundary with FTPS-derived constants, request shapes, protected/clear mode policy, success-only cache behavior, and fake no-network tests.
- Added Phase 4 hub printer inventory persistence, tenant-scoped printer HTTP APIs, refresh-printers command dispatch endpoint, future-only printer WebSocket events, and the read-only frontend operations dashboard.
- Added Phase 5 hub print artifacts/jobs persistence, tenant-scoped print job HTTP APIs, print command gRPC dispatch, command/job status coupling, agent artifact-root handling, frontend job history, and HTTP-only print dispatch form.
- Added Phase 6 tenant API token authentication, tenant role authorization, audit events, WebSocket auth, frontend server-side token forwarding, and SQLite/PostgreSQL Docker Compose examples.
- Added Phase 7 staged SeaORM 2.0 migration groundwork with SQLx 0.9 alignment, a shared SeaORM connection accessor, a hand-written `tenants` entity, and SeaORM-backed tenant repository operations.
- Added Phase 9 print report reconciliation with agent MQTT `PrintJobReport` forwarding, hub-side physical print lifecycle persistence, normalized machine events, tenant `job_progress` WebSocket broadcasts, nested `job.print` HTTP responses, and frontend job progress display.
- Added Phase 10 external identity authentication with local `user_identities`, Clerk/Logto-compatible JWT verification through configured JWKS, API-token-first tenant route auth, local tenant role enforcement, local JWKS route tests, and frontend bearer forwarding from request cookies/static tokens.
- Added Phase 11 provisioning/admin boundaries with bootstrap-only cross-tenant APIs, atomic tenant-admin bootstrap, tenant-admin user/token/identity management, API-token revocation, provisioning audit events, agent pairing bundles, and tenant-bound frontend reads.
- Added Phase 12 full SeaORM repository migration coverage for auth, audit, agents, printers, commands, jobs, print reports, machine events, and documented the remaining atomic printer snapshot SQLx adapter.
- Added Phase 13 LAN discovery, printer diagnostics, structured command result persistence, conservative compatibility matrix ownership, hub diagnostic APIs, and frontend diagnostic result rendering.
- Added Phase 14 AMS/external-spool material normalization, tenant-scoped material snapshots, print mapping persistence/dispatch, terminal filament usage derivation, HTTP material responses, and dashboard material summaries.
- Added Phase 15 browser-safe WebSocket tickets, live runtime dashboard event consumption, reconnect status, transition notifications, and token-safe tenant operation references.
- Added Phase 16 tenant-owned token repository/routes, scoped tenant-token bearer authorization, retired user API-token routes, and bootstrap tenant-token issuance.
- Added Nix flake packaging for `pandar-hub`, `pandar-agent`, `pandar-cli`, `pandar-network-plugin`, `pandar-web`, checks, formatter, and development shell; `pandar-cli` installs the unified `pandar hub` / `pandar agent` Rust entrypoint while the frontend remains `pandar-web`.
- Split Nix packaging into a flake-parts root module and `nix/pandar.nix` so package, check, formatter, and dev shell logic stays outside the top-level flake.
- Added a NixOS module exposed as `nixosModules.default` / `nixosModules.pandar` to run `pandar-hub` and `pandar-web` with configurable bind addresses, packages, URLs, and environment.
- Extended the NixOS module with an optional `pandar-agent` systemd service, including hub gRPC URL, identity, credential, printers, artifact root, environment file, and package overrides.
- Generated `services.pandar` NixOS option documentation under `docs/deployment/nixos/options.md` and linked it from the README.
- Added GitHub Actions CI to run `nix flake check --show-trace` on pushes to `main` and pull requests.
- Added Mic92/hestia-backed GitHub Actions caching for Nix flake checks, with a scheduled cache GC workflow.
- Added NixOS VM tests for SQLite and PostgreSQL hub deployments, and split CI into native x86_64/aarch64 package and VM-test matrices.
- Limited aarch64 package CI to the server, agent, CLI, and web artifacts while keeping the Bambu Studio network plugin package check on x86_64, where the current Linux GNU export-map strategy is supported.
- Added tag-driven GitHub Release CI for `pandar` CLI and `pandar-network-plugin` artifacts using `cargo-zigbuild`, covering Linux, Windows, and macOS on amd64 and arm64 with per-target checksums; macOS CLI artifacts are ordinary release Mach-O binaries rather than fully static binaries.
- Verified a real LAN Bambu printer at `10...24` through the agent MQTT path, raised the MQTT packet limit for full `pushall` reports, and confirmed authenticated status refresh returns `IDLE`.
- Added full-chain warning logs for MQTT report receive failures so errors such as `payload size limit exceeded` are visible during printer refresh/report polling.
- Documented the 2026-06-24 Bambu LAN printer probe, including MQTT topics, tested commands, device details, transport findings, verification, and follow-up notes.
- Added `treefmt-nix` flake integration with a standalone `nix/treefmt.nix` configuration for Nix, Rust, GitHub Actions, frontend/Markdown formatting, and EditorConfig validation.
- Added refresh-time printer model discovery through MQTT `info.get_version`; refresh now fails and logs the full error chain when the model cannot be discovered instead of falling back to configured model metadata.
- Added Phase 25 Task 6 Hub-mediated agent artifact downloads: agent bearer auth, agent/artifact ownership checks, storage-backed download responses, agent HTTP artifact fetching through `PANDAR_HUB_API_URL`, and local artifact-reader fallback for legacy command payloads.
- Added Phase 25 Task 8 readiness and cleanup hardening: `/readyz` and Prometheus now report `artifact_storage`, scaled PostgreSQL+NATS filesystem deployments require an explicit shared-filesystem override or object storage, and cleanup execute deletes artifact storage objects before artifact rows while preserving rows on storage delete failure.
- Added Phase 25 scaled artifact storage: browser and Bambu Studio plugin print submission now use multipart artifact uploads, Hub commands carry Hub-mediated `artifact_download_path` values instead of inline base64 artifact payloads, S3-compatible object storage is available for PostgreSQL+NATS deployments, and the scaled smoke harness verifies cross-Hub dispatch/download without a shared local spool.
- Updated deployment, architecture, release, and Docker Compose docs so filesystem storage is documented as the SQLite/single-node default while PostgreSQL+NATS deployments use object storage or an explicit shared-filesystem override.
- Added Phase 26 local HA/failure smoke coverage: the scaled smoke harness now exercises command wake convergence across Hub states, WebSocket `printer_snapshot` and `job_progress` fanout, restart simulation, plugin print pressure, artifact storage put/open/delete failures, and terminal print-report idempotence without Docker or live services.
- Added Phase 26 focused failure observability: Prometheus exports control-plane publish/receive counters, publish failure after durable job/command commit is observable without rolling back state, WebSocket ticket safety is covered across replicas, and storage write/read/delete failure tests pin stable behavior.
- Added Phase 26 operations docs and evidence tracking for SQLite single-node and PostgreSQL+NATS+object-storage deployments, including explicit live soak variables and a `docs/compatibility/phase-26-soak-evidence.md` table for local and live evidence.
- Added a Phase 26 `tools/scaled-artifact-smoke --live` runner entry point for disposable PostgreSQL, NATS, and S3-compatible object storage; disposable local live soak plus explicit NATS and PostgreSQL reconnect evidence are now recorded.
- Refreshed Phase 23/24/26 local evidence after Phase 28: the plugin ABI probe, release-smoke unit coverage, and scaled artifact smoke dry-run are recorded against current code, and the smoke tool now carries the optional artifact metadata field.
- Added Phase 27 live printer-control groundwork: shared model compatibility policy moved into `pandar-core`, Hub now enqueues audited tenant/printer-scoped `printer_control` commands for compatible models, gRPC carries typed printer controls to agents, and agents dispatch typed pause/resume/stop/print-speed MQTT payloads without relying on local model metadata. Local no-network tests cover compatibility, Hub enqueue/route/gRPC behavior, agent command handling, and fake MQTT payload dispatch; real pause/resume/stop/print-speed printer probes are not recorded.
- Added Phase 28 reference-backed slicer metadata: bounded 3MF metadata parsing, SQLite/PostgreSQL `job_artifacts.metadata_json` persistence, tenant preview API, job/plugin response metadata, dashboard upload preview, and compact job/recovery metadata summaries. Local parser, SQLite route/repository/plugin/frontend verification, and disposable PostgreSQL metadata repository verification are recorded.
- Added Phase 29 protocol-level printer operations: Hub now persists and forwards semantic `printer_operation` commands instead of Bambu-specific control strings, tenant `/controls` and plugin `/operations` requests share semantic validation, agents translate operations to Bambu MQTT/G-code locally, and the network plugin parses supported Studio G-code messages into semantic operation JSON before contacting Hub.
- Fixed Nix CI packaging inputs: cargo package/check derivations now provide the CA bundle through `SSL_CERT_FILE` / `NIX_SSL_CERT_FILE`, Rust checks include the plugin-local embedded assets, and `pandar-web` has the current fixed-output npm dependency hash.
- Added Phase 30/31 external account onboarding: Better Auth is accepted as a Clerk/Logto-equivalent JWT/JWKS provider, verified identity data now carries email/display-name claims, external users can inspect `/api/v1/me`, self-create tenants when enabled, and accept tenant-admin join links that create tenant-local user projections.
- Added join-link management with hash-only token storage, optional verified-email restriction, default single-use/seven-day expiry, revoke/list APIs, audited accept/create/revoke events, and frontend onboarding/join-link flows configured through `pandar-web` provider settings.
- Fixed Nix quality CI drift by formatting generated NixOS option documentation with Prettier before comparing it to the checked-in Markdown, and formatted the current onboarding frontend/plan files under treefmt.
- Added Docker publish GitHub Actions workflow for `ghcr.io/projectpandar/pandar/hub` and `ghcr.io/projectpandar/pandar/web`, reusing the existing hub and web Dockerfiles.
- Renamed the hub container build file from `Dockerfile.api` to the root `Dockerfile` and updated Compose and CI references.
- Added a Kubernetes Helm chart under `docs/deployment/kubernetes` for `pandar-hub` and `pandar-web`, with CI lint/render checks and OCI publishing to `ghcr.io/projectpandar/pandar/chart/pandar`.
- Updated GitHub Actions workflow `uses:` references to current released action tags, including Node 24-compatible `actions/checkout` and `azure/setup-helm`.
- Seeded the frontend design system foundation: a product-register `PRODUCT.md` (calm/technical/trustworthy operations console), a seeded `DESIGN.md` ("The Instrument Console" — restrained teal/cyan, Inter + monospace IDs, flat-by-default, state never color-alone), and `.impeccable/live/config.json` for Next.js App Router live iteration.
- Added an exceptions-first dashboard IA over the live WebSocket state: a fleet status strip and pinned "Needs attention" region (grouped by agent, inline recovery actions) replace the vanity count-metric grid, a sticky section nav with deep-link anchors and per-section attention badges replaces the one-long-scroll, sections are reordered by operational priority, the notifications feed gained `aria-live` and severity icons (dropping the severity-agnostic cyan side-stripe), and dispatch dev error codes moved into a Developer disclosure. Backed by `dashboard-attention`/`dashboard-overview`/`dashboard-status` modules; production build clean, `next lint` blocked by a Next 16 CLI quirk unrelated to the change.
- Unified the dashboard status language into a single semantic source of truth: `StatusBadge` now renders a severity icon + label + tinted-background/dark-ink pill (replacing color-only white-on-saturated badges and fixing the white-on-amber-600 contrast failure), a shared `statusMeta`/`statusSeverity` maps any status string → severity+label and also drives the Needs-attention severities and titles, and non-status role chips in tenant admin moved to a label-only `Tag`. `failed`/`offline`/`problem` are now distinguishable by label+icon, not color alone; detector scan clean.
- Closed the remaining status-styling outliers so every pill shares one vocabulary: the diagnostics `CompatibilityRow` one-off white-on-saturated pill and tenant-token scope chips now use `Tag`, the fleet-strip `SeverityDot` was replaced by `StatusIcon`, and `Tag` gained consistent success/warning/neutral/accent tones with shared label prettifying. `notificationSeverity` was intentionally left as the free-text heuristic (folding it onto token-based `statusSeverity` would misclassify prose like "slowed down"); detector scan clean.
- Gated the dashboard's destructive actions behind consequence-explaining confirms: `Queue stop` (stops a running print), `revoke join link`, `revoke tenant token`, and `rotate tenant token` now require an explicit OK in a native confirm that states the consequence before the server action fires; `recovery-actions.tsx` is now a client module and `PrinterControlForm` accepts an optional confirm message. Closes the P0 single-misclick safety hole; "render only state-relevant recovery actions" remains a separate follow-up.
- Replaced the blocking `window.confirm` confirms with an in-app dialog: a new `confirm-dialog.tsx` exports a `ConfirmDialog` (native `<dialog>` modal — top-layer so it escapes the sticky-nav stacking context, focus trap, ESC/backdrop cancel, autofocus on the safe option) and a `ConfirmForm` wrapper; the four destructive actions now open the styled dialog instead of the native browser prompt, with `rotate` preserving its `useActionState` secret-result display. The dialog animates open and close via `@starting-style` + `transition-behavior: allow-discrete` (card fade+lift, backdrop fade), degrades to instant on older browsers, and collapses to ~instant under `prefers-reduced-motion`.
- Landed the Instrument Console type foundation: Inter via `next/font/google` (`--font-inter`) with a mono token and a `@theme` token layer in `globals.css` (replacing the Arial default), and converted all 11 uppercase eyebrow/`thead` labels to sentence-case. De-carded the admin/reference plane via tonal layering (`TenantSettings` + `TenantAdminPanel` recessed to `bg-slate-50`; operational sections stay white/forward, page is slate-100/back) so the tonal boundary maps to the admin IA boundary. Inter is confirmed in the built CSS bundle.
- Added client-side filter + search to the fleet lists and lifted the silent recovery cap: PrinterInventory (search name/serial, status All/Online/Needs-attention) and JobHistory (search filename/job id, status All/Active/Failed/Completed) each gained a `FilterBar` (shared, with `FOCUS_RING` on the input/select) and a distinct "No matches" empty state; filtering runs over the live WebSocket-merged arrays (not URL params). RecoveryActions no longer silently hides jobs beyond the first 8. The list components were split into a new `dashboard-inventory.tsx` module to stay under the 400-LOC limit.
- Closed the keyboard focus gap (WCAG 2.4.11) comprehensively with a global `:focus-visible` rule in `globals.css` (2px cyan-700 outline, 2px offset) so every interactive control shows a visible focus indicator by default, rather than opting in per-element. Existing `FOCUS_RING` elements keep their ring (their `focus-visible:outline-none` cleanly suppresses the global outline, no double indicator); the ~30 previously focus-invisible controls across recovery/dispatch/diagnostics/admin/header now get the indicator. Rule confirmed in the built CSS bundle.
- Made recovery actions state-aware: each job now shows only the controls that apply to its state — live pause/resume/stop/speed only when the physical print is running, Retry dispatch only when the dispatch/command failed, Reprint only when the physical print reached a terminal state, and Duplicate always. Removes the 7-state-blind-controls-per-row sprawl and the misfire risk (e.g. Pause on a completed job, Reprint on a running one).
- Moved developer-shaped copy out of operator surfaces: PrinterInventory no longer renders the literal `POST /api/v1/.../jobs` path — it resolves and shows the managing agent's friendly name instead of its UUID; JobRow dropped the internal-only `Artifact {id}` and `Command {id}` mono lines (keeps the one `Job {id}` reference) and now resolves `Printer {id}` / `Agent {id}` to friendly printer/agent names (JobHistory threads printers+agents). Reduces per-row cognitive load and the "tool built by devs for devs" tell across both primary lists.
- Systematized the control primitives off the 32 plateau: deleted the per-element `FOCUS_RING` constant so the global `:focus-visible` is the single focus system (every control gets one consistent indicator); collapsed three button heights to two tiers (h-9 primary, h-8 in-row — h-7 removed); added `hover:bg-cyan-800` to the cyan primary buttons missing it (View/Discover/Diagnose/Dispatch/admin/onboarding); unified dense controls from bare `rounded` to `rounded-md`. Restores button/focus consistency across the dashboard.
- Added the first inline help layer: a new accessible `HelpTip` (CSS-only tooltip, keyboard-focusable, `role="tooltip"`, integrates with the global focus ring) on the opaque dispatch terms — Use AMS, Flow calibration, Timelapse, and Plate — with plain-language explanations; and humanized the API-facing empty states (PrinterInventory "No tenants" and JobHistory "No jobs") into operator coaching that points to the next UI action instead of the hub API.
- Made the help layer screen-reader-accessible and extended it: `HelpTip` now generates a stable id (`useId`) with `aria-describedby` linking each trigger to its tip and a term-specific `aria-label` (no more four identical unlabeled buttons); `dashboard-ui` is now a client module to host the hook. Extended the same `HelpTip` to the diagnostics compatibility rows — External storage, FTPS TLS 1.2 cap, Clear-data fallback — so the help coverage matches the dispatch surface.
- Distilled JobRow off the density cap: the row now shows four primary columns (artifact filename + single "Updated" time, dispatch/print status pills + errors, printer/agent names, progress/layers/remaining) and relegates everything else — recovery state, project metadata, artifact/material/Job id, file/printer state, and the created/started/finished timestamps — into a `<details>` disclosure. Added `role="list"`/`role="listitem"` + a per-row `aria-label` summary so screen-reader users get a scannable job per row instead of ~20 loose fields. Fuses 4 visible timestamps to 1 primary.
- Humanized the operator-facing copy: `formatJobRecoveryState` now returns plain-language state ("Printing now", "Print completed/failed/cancelled", "Waiting for the agent to come back online", "Could not send the file to the printer", "Printer did not accept the start command", "Could not queue the job at the hub", "Waiting for the print to start") instead of internal taxonomy ("Hub enqueue failure"/"MQTT publish failure"); the print-failed live notification title is now "Print failed"; TenantSettings' raw API paths (agent-pairings, api-tokens) moved behind a "Developer reference" `<details>` and the printer-compatibility card's literal `POST .../diagnose-printer` path replaced with a pointer to the Diagnostics section. The Live-activity panel's raw `liveState` enum is now humanized via `formatLiveState` (idle/connecting/live/disconnected/unavailable/error → Idle/Connecting/Connected/Reconnecting/Unavailable), matching the fleet strip's wording.
- Added bulk operations for farm-scale recovery: a "Refresh all agents" action (`refreshAllAgents`) that fans a refresh-printers POST to every agent in one click, and multi-select bulk retry — failed-dispatch jobs get a checkbox; selecting any shows a "Retry N selected" bar (`retryDispatchJobs`) with a Clear affordance. Two new server actions handle the fan-out with partial-failure status. Lifts H7 (Flexibility) off its no-bulk cap.
- Polished the bulk-ops feature: added a "Select all failed" toggle (selects/deselects all dispatchFailed jobs), `aria-live="polite"` on the bulk bar so screen readers announce the appearing action, `accent-cyan-700` on the checkboxes to tie them to the system, count text ("N of M failed selected"), and humanized partial-failure status codes ("Some retries could not be queued — review the list") via a formatActionStatus message map.
- Added frontend localization (中文 / English) via next-intl in cookie-based non-segment mode. Locale is resolved from the `locale` cookie with Accept-Language negotiation on first visit and mirrored in the zustand `pandar.settings` store; the existing `[locale]/sign-in` Bambu Studio WebView alias is preserved. Translated all user-facing strings across the dashboard, dispatch, recovery, diagnostics, runtime, tenant settings, admin, and the standalone onboarding/sign-in/join pages; dates and numbers are locale-formatted; known machine-status tokens translate with a prettify fallback. A language switcher in the dashboard header and standalone page section headers toggles locale via a server action + `router.refresh()`.
- Fixed GitHub Checks web packaging after localization: refreshed the Nix `pandar-web` npm dependency hash for the updated lockfile and removed `next/font/google` so sandboxed Nix builds do not require Google Fonts network access.
- Added a Lefthook pre-commit formatter hook that runs `nix fmt` before commits, then re-stages only the files that were already staged before formatting; the Nix dev shell now includes `lefthook` for hook installation.
- Rejected explicit `none` external auth providers so deployments must configure a real identity provider value when enabling external identity auth.
- Polished and clarified the auth surfaces against the Instrument Console system: the standalone Better Auth issuer now uses the slate/cyan flat-panel vocabulary instead of a warm one-off card, sign-in/sign-up/sign-out share explicit alert treatment, issuer pages show the session issuer/return target/lifetime, and the Studio plugin sign-in route has operator-facing recovery copy for auth/readiness/tenant failures, callback URL trust-boundary copy and URL validation, visible Studio/default callback state, a header-integrated language switcher, and an explicit tenant/ticket confirmation block.
- Fixed the standalone Better Auth issuer sign-in page by keeping cooldown copy serializable across the Server Component to Client Component boundary, avoiding production `/sign-in` render failures.
- Added WebAuthn conditional passkey autofill to the standalone Better Auth issuer sign-in page: the identifier field now advertises `username webauthn`, the page preloads Better Auth passkey autofill when conditional mediation is available, and the existing manual passkey button remains the visible fallback.
- Replaced the one-page dashboard shell with shadcn `sidebar-08`: `/devices`, `/agents`, `/users`, and `/settings` now share a route-backed sidebar layout, root `/` redirects preserve dashboard query state, action feedback targets the relevant page, tenant switching preserves status and agent command context, and Logout is exposed only when the configured auth provider supplies a sign-out URL. The generated sidebar primitive was split under the frontend module-size limit while preserving the public shadcn import path.
- Fixed the post-sidebar Nix web check by refreshing `pandar-web`'s npm dependency hash for the updated lockfile and removing the reintroduced `next/font/google` dependency so sandboxed `nix build .#pandar-web` stays offline-deterministic.
- Migrated the dashboard shell helper smoke coverage into the root frontend Vitest suite and added React Testing Library coverage for tenant switching, so the route/query/logout contract now runs under `npm --prefix frontend run test` instead of a standalone Node smoke script.
- Reworked external tenant onboarding/joining into an organization-switcher-style flow: verified users now open a compact tenant access switcher, create tenants through a shadcn dialog wired to the existing server action, and join tenants from a matching token form with hash-prefill behavior. Added main-frontend Vitest + React Testing Library coverage for the switcher, dialog fields, and join-token prefill.
- Fixed the GitHub Checks workflow after the passkey autofill frontend lockfile update by refreshing the Nix `pandar-web` npm dependency hash used by package and NixOS module checks.
- Consolidated frontend npm package management at the repository root: `pandar-web`, `pandar-auth`, and `pandar-plugin-local` now share one root workspace lockfile and root build/test scripts; shared dependency versions were aligned, auth reuses the dashboard `cn` helper through a thin re-export, and Nix/Docker/developer docs were updated for root-workspace builds.
- Replaced redirect-backed dashboard action prompts (`status=...`, such as refresh/retry/dispatch queued results) with shadcn Sonner toasts; consumed statuses are removed from the URL and dashboard navigation while durable live notifications, admin secret results, and data-integrity errors remain in-page.
- Completed a React Doctor hardening pass across `pandar-web` and `pandar-auth`: exported server actions now require request-cookie auth before side effects, the dashboard sign-out cookie clear moved to a POST-only route, locale mutation moved from a server action to an explicit route handler, stale-effect/state and accessibility findings were removed, unused frontend surfaces were deleted or moved out of the app tree, and `npx react-doctor@latest --verbose` now reports 100/100 for both frontend projects.
- Added Task 1 agent printer-linking primitives: protocol `LinkPrinter`, redacted Hub `link_printer` command payloads, audited sent-row creation, stale unowned cleanup, durable replay rejection, and Hub-side command error redaction groundwork.
- Updated runtime printer linking from the dashboard Agents page: operators now submit printer type (`BambuLab`), printer IPv4 address, access code, and optional name only; the agent discovers serial/model during Bambu onboarding and reports the completed metadata through snapshots and the link result, while Hub continues to store only redacted command/audit data.
- Fixed manual-IP runtime printer linking so the agent falls back from multicast SSDP discovery to a direct unicast SSDP probe against the submitted printer host, allowing reachable printers to link even when multicast discovery returns no devices.
- Fixed the Better Auth dashboard callback redirect to resolve the final post-login `/` target against `APP_BASE_URL`, preventing internal/bind hosts such as `0.0.0.0:3000` from leaking into the browser after token handoff.
- Updated the dashboard Agents delete control so online agents still show the standard Delete button in a disabled state, with HoverCard and Sonner feedback explaining that online agents cannot be deleted.

## Completed: Phase 30 Better Auth Provider Compatibility

Goal: support Better Auth as a Clerk/Logto-equivalent external auth provider through the existing JWT/JWKS verifier.

- Completed Better Auth deployment guidance through the existing external JWT/JWKS verifier.
- Completed `PANDAR_EXTERNAL_AUTH_PROVIDER=betterauth` documentation with issuer, JWKS URL, audience, and Pandar-side `RS256` verification configuration.
- Completed verified external identity data with profile claims for onboarding: verified email, display name, and username fallbacks.
- Kept tenant authorization unchanged: Pandar still resolves `(tenant_id, provider, subject)` to a local tenant user and role.

## Completed: Phase 31 External Self-Service Tenant Onboarding

Goal: make external account sign-in the primary user entry point while Pandar remains authoritative for tenant membership and roles.

- Completed `/api/v1/me` for external JWT users to inspect their identity and tenant memberships without side effects.
- Completed verified external tenant self-create with the creator as tenant admin and `PANDAR_AUTH_ALLOW_TENANT_SELF_CREATE` as the deployment gate.
- Completed tenant-admin-managed join links with hash-only token storage, optional verified-email restriction, one-time plaintext response, default single-use behavior, and seven-day default expiry.
- Completed join-link acceptance that creates tenant-local user projections and identity links, assigns the link role, and avoids role changes or use-count consumption for existing members.
- Completed provider-configured `pandar-web` onboarding for Clerk, Logto, and Better Auth while preserving the bearer-token boundary to `pandar-hub`.
- Hid manual user creation and manual identity-link forms from the primary frontend path while keeping the API as a transitional/admin-only capability.

## Completed: Phase 32 Remove Manual Pandar User Creation And Linking

Goal: finish the transition from Pandar-managed user provisioning to external-account-backed tenant membership.

- Unified the dashboard's route-backed tenant selector with the onboarding tenant-access vocabulary and popover interaction while preserving view, command, and status URL context.
- Removed manual `POST /api/v1/tenants/{tenant_id}/users`.
- Removed manual `POST /api/v1/tenants/{tenant_id}/users/{user_id}/identities`.
- Kept user listing, identity listing, and tenant-local role updates.
- Kept join links as the supported invite/onboarding path.
- Preserved existing `users` and `user_identities` rows without a data migration.

## Completed: Phase 33 Self-Hosted Better Auth Bundle

Goal: provide an integrated Better Auth deployment option for new Pandar installations.

- Added a `frontend/auth` `pandar-auth` issuer app for email magic-link sign-in, optional post-login passkey binding, Better Auth-owned SQLite state, RS256 JWT issuance, and JWKS exposure.
- Added dashboard callback/sign-out routes so `pandar-web` can receive a self-hosted Better Auth JWT without adding Better Auth dependencies to the provider-neutral frontend.
- Hardened Better Auth signup so existing email accounts must sign in instead of receiving newly registered passkeys, and the dashboard callback checks Better Auth issuer/audience shape before storing the bearer cookie.
- Updated the Better Auth dashboard return flow to use a direct `GET /auth/betterauth/callback?token=...` server redirect, removing the blank HTML/POST bridge while preserving existing JWT validation and cookie semantics.
- Added Nix packaging and a top-level `services.pandar-auth` NixOS module for the issuer, including migration startup and generated option docs.
- Documented the self-hosted issuer deployment wiring, including `PANDAR_EXTERNAL_AUTH_*`, `APP_AUTH_*`, `PANDAR_AUTH_*`, and the `BETTER_AUTH_SECRET` JWKS private-key encryption rotation warning.
- Clerk/Logto migration remains out of scope; self-hosted Better Auth is a new-deployment option.
- Hardened `pandar-web` external-auth entry so source-less Clerk/Logto/Better Auth dashboard requests redirect to the configured sign-in URL, stale dashboard cookies are cleared before provider sign-in, and a Nix `pandar-web-auth-redirect-smoke` check locks the redirect/open-redirect behavior.

## Completed: Phase 34 Self-Hosted Better Auth Email Login

Goal: make the self-hosted Better Auth issuer easier to deploy without requiring passkey enrollment before first login.

- Replaced passkey-first signup/sign-in with email magic-link login and first-time user creation through Better Auth.
- Added Resend and SMTP email delivery configuration, with 30-minute default magic-link expiry and runtime validation for the selected provider.
- Added optional passkey binding immediately after magic-link login with a clear Skip action.
- Restored direct passkey sign-in on the issuer login page for users who already bound a passkey, while keeping email magic links as the default path.
- Redirected `/sign-up` to `/sign-in`; dashboard user management remains later-phase work.
- Updated deployment docs and NixOS options for `services.pandar-auth` email delivery.

## Phase 1: Foundation

- Completed canonical tenant and agent domain IDs/records in `pandar-core`.
- Completed hub repository layer and removed in-memory tenant/agent vectors from HTTP state.
- Completed SQLite and PostgreSQL migrations for Phase 1 tenants, users, agents, printers, and commands.
- Completed repository test harnesses for SQLite by default and optional PostgreSQL via `PANDAR_TEST_POSTGRES_URL`.
- Completed Phase 1 hub HTTP/API wiring against repositories, including startup migration from `PANDAR_DATABASE_URL`.

## Phase 2: Agent Reverse Connection

Goal: establish the durable reverse-control channel between locally deployed agents and `pandar-hub`.

- Expand `proto/pandar/agent/v1/agent.proto` for:
  - agent hello
  - heartbeat
  - printer snapshot
  - hub command dispatch
  - agent command acknowledgement
  - command result
- Completed tonic build/runtime dependencies in the hub and agent crate boundaries that own gRPC.
- Completed hub-side gRPC service for reverse agent sessions.
- Completed hub-side agent session registry with tenant/agent identity, connected status, heartbeat updates, stale-session protection, and replacement-session shutdown.
- Completed persisted agent version, last-seen, and status updates through the existing repository/database boundary.
- Completed `pandar-agent` outbound connection to `pandar-hub` with hello, heartbeat, refresh-printers ack/result, and reconnect/backoff.
- Add tenant binding or registration token placeholder flow sufficient for local development without introducing full auth yet.
- Completed local-development tenant/agent binding through explicit `PANDAR_TENANT_ID` and `PANDAR_AGENT_ID` values.
- Completed integration tests for:
  - agent hello registers a live session
  - heartbeat updates last-seen state
  - disconnected or timed-out agents become unavailable
  - hub command dispatch reaches the connected agent stream
  - command acknowledgement/result updates the command ledger

Exit criteria:

- A local `pandar-agent` can connect outward to a local `pandar-hub`.
- Hub can distinguish offline, connecting, and online agent state from persisted metadata plus live sessions.
- Hub can enqueue a command and receive an acknowledgement/result over the reverse stream.
- No Bambu machine network sockets are opened in Phase 2.

## Phase 3: Bambu Machine Transport

- Completed agent-side MQTT transport boundary based on the reference facts:
  - TLS port 8883.
  - Bambu LAN self-signed certificate policy isolated to the agent MQTT adapter.
  - username `bblp`, password access code.
  - subscribe `device/{serial}/report`.
  - publish `device/{serial}/request`.
  - QoS 1 for publishes.
- Completed state refresh via `pushing.pushall` through the `RefreshPrinters` gateway path.
- Completed basic command payload builders: pause, resume, stop, print speed, raw diagnostics command, and reserved `project_file` shape.
- Completed machine file transfer abstraction based on the reference FTPS behavior:
  - implicit TLS port 990.
  - username `bblp`, password access code.
  - upload, download, list, delete.
  - protected data mode only; a failure never downgrades to `PROT C`.
- Completed targeted tests for command JSON construction, topic naming, fake MQTT refresh, printer config parsing, command event sequencing, and protected-only file-transfer mode selection.

## Phase 4: Printer Inventory And State

- Completed hub persistence for latest tenant-scoped printer state reported by agents.
- Completed tenant-scoped printer list/detail HTTP APIs.
- Completed refresh-printers HTTP command dispatch through the command ledger.
- Completed future-only tenant WebSocket broadcasts for printer snapshots; historical state is loaded through HTTP.
- Completed frontend summary, tenant, and printer inventory dashboard using uncached server-side HTTP reads from `APP_API_URL`.
- Completed browser WebSocket consumption later in Phase 15 after authentication and tenant selection were stronger.

## Phase 5: Print Dispatch

- Completed `JobArtifact` and `Job` core domain models and protobuf `PrintProjectFile` command payload.
- Completed SQLite and PostgreSQL migrations for `job_artifacts` and `jobs`.
- Completed the initial hub filesystem artifact storage with `PANDAR_SPOOL_DIR`, `PANDAR_MAX_ARTIFACT_BYTES`, filename sanitization, and scoped cleanup on repository failure; Phase 25 later moved artifact bytes behind the configured storage boundary.
- Completed tenant-scoped print job HTTP APIs:
  - `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs`
  - `GET /api/v1/tenants/{tenant_id}/jobs`
  - `GET /api/v1/tenants/{tenant_id}/jobs/{job_id}`
- Completed atomic print job creation: artifact metadata, linked command, and job row commit together.
- Completed print command dispatch over the existing agent reverse gRPC stream, including printer id, Bambu serial number, artifact metadata, and print options.
- Completed command/job lifecycle coupling for print jobs through repository-level SQLite/PostgreSQL transactions.
- Completed agent `PANDAR_ARTIFACT_ROOT` handling, safe relative artifact path resolution, missing-artifact failure reporting, and unknown-serial rejection before artifact I/O.
- Completed configured agent gateway composition for uploading a project artifact through `MachineFileTransfer`, then publishing MQTT `project_file` with job identity and print flags; fake tests verify upload-before-publish and no-publish-on-upload-failure behavior without live Bambu sockets.
- Completed frontend print job history, per-printer dispatch API visibility, and the initial HTTP-only dispatch form; Phase 25 later moved browser artifact transport to multipart upload.
- Deferred real printer file-transfer runtime upload and upload verification; the default Phase 5 runtime adapter returns an explicit unavailable error after serial selection until the FTPS implementation is completed.
- Deferred printer-report reconciliation for physical print progress/completion/failure to the next machine-runtime phase.

## Phase 6: Multi-Tenant Product Hardening

- Completed API token authentication for tenant-scoped HTTP and WebSocket APIs.
- Completed tenant role authorization:
  - `tenant_admin` can create agents and perform operator/viewer actions.
  - `operator` can create jobs, refresh printers, and perform viewer actions.
  - `viewer` can read tenant resources and subscribe to printer events.
- Completed SQLite and PostgreSQL migrations for `api_tokens` and `audit_events`.
- Completed backend-neutral auth and audit repositories with SQLite default tests and optional PostgreSQL coverage via `PANDAR_TEST_POSTGRES_URL`.
- Completed audit events for successful agent creation, refresh-printers commands, and print job creation.
- Completed WebSocket authorization and tenant filtering through the same bearer-token boundary as HTTP.
- Completed frontend server-side `APP_API_TOKEN` forwarding and optional `APP_TENANT_ID` tenant binding for tenant printer/job reads and print job creation.
- Completed the original agent-local credential policy documentation; the encrypted Hub persistence work above supersedes the former no-database-storage rule while retaining the ban on frontend environment exposure.
- Completed Docker Compose examples for SQLite and PostgreSQL deployments.

## Phase 7: SeaORM Migration

- Completed the first staged SeaORM 2.0 migration by adding SeaORM `2.0.0-rc.41` behind the existing SQLx pool boundary.
- Completed workspace SQLx `0.9.0` alignment required by SeaORM 2.0.
- Completed hand-written SeaORM entity coverage for `tenants`.
- Completed `TenantRepository` create/list/count migration to SeaORM while preserving the existing repository API and SQLite/PostgreSQL behavior.
- Deferred auth, audit, agents, printers, commands, jobs, and SeaORM migration-system adoption to later phases.

## Phase 8: Real Machine File Transfer Runtime

Goal: replace the Phase 5 unavailable runtime adapter with real agent-side Bambu-compatible file transfer while keeping the public boundary protocol-neutral.

- Completed implicit FTPS on port `990` behind the existing `MachineFileTransfer` trait.
- Completed the Bambu LAN TLS policy for printer-local/self-signed certificates.
- Completed protected data mode selection. Later security hardening made `PROT P` mandatory and rejects any cached or requested clear mode.
- Completed server-side upload size verification before publishing MQTT `project_file`.
- Completed configured gateway wiring so runtime agents use the FTPS adapter for machine file upload.
- Kept tests fake by default with adapter-level coverage for mode policy, verification decisions, and error mapping without requiring live printer sockets.

Exit criteria:

- A configured agent can upload a project artifact to a Bambu printer through the runtime adapter.
- The configured print gateway still publishes MQTT only after upload verification succeeds.
- Upload failures preserve enough context to distinguish auth failure, no FTPS listener, missing SD card/path failure, quota/full card, timeout, TLS/profile mismatch, and partial upload.

## Phase 9: Print Report Reconciliation

Goal: make hub job state represent physical printer progress instead of only dispatch success.

- Completed agent MQTT report normalization beyond the snapshot path to emit `PrintJobReport` events while connected.
- Completed correlation to Pandar jobs using exact job id, artifact/subtask id, and deterministic active-file fallback.
- Completed persistence for printer state, percent, remaining time, current layer, total layers, active file, last valid progress, last valid layer, terminal errors, and normalized `machine_events` in both SQLite and PostgreSQL migrations.
- Completed `gcode_state` transition mapping:
  - `RUNNING` means physical print started or resumed.
  - `FINISH` means completed.
  - `FAILED` means failed, including pre-print failures from preparation states.
  - `IDLE` after `RUNNING` means cancelled or aborted.
- Completed `print_error` and HMS-style structured machine event capture with replay-stable dedupe keys.
- Completed tenant WebSocket `job_progress` broadcasts and nested HTTP `job.print` response fields.
- Completed frontend job history display for dispatch state, physical print state, progress, layers, remaining time, and terminal reason.
- Kept dispatch lifecycle and physical print lifecycle separate in naming and persistence so command success cannot be confused with print completion.

Exit criteria:

- A print job can move from queued/dispatching into running/completed/failed/cancelled from MQTT reports without changing dispatch status semantics.
- Hub restarts and agent reconnects can continue reconciling from latest reports without duplicating terminal events or regressing terminal physical status.
- Frontend users can see physical progress and terminal failure/success reasons for tenant jobs from HTTP job history. Browser live WebSocket consumption is completed in Phase 15; the authenticated hub `job_progress` WebSocket event already exists and is tested.

## Phase 10: External Identity Authentication

Goal: let users sign in with Clerk or Logto while keeping Pandar's tenant membership and role model in Rust.

- Completed equivalent SQLite and PostgreSQL `user_identities` migrations for external provider subject links.
- Completed repository methods for linking and resolving external identities to existing tenant-scoped Pandar users.
- Completed a provider-neutral OIDC/JWT verifier in `pandar-hub` for HTTP and WebSocket bearer tokens.
- Completed Clerk and Logto support through configuration, not provider-specific authorization logic:
  - issuer URL
  - JWKS URL
  - expected audience/API resource
  - accepted algorithms
  - optional authorized parties/origins for Clerk-style session tokens
  - optional scope checks for Logto API-resource tokens
- Completed token validation for signature, `iss`, `aud`, `exp`, optional `nbf`, allowed algorithms, `kid`, optional `azp`, optional scopes, and provider subject.
- Completed API-token-first route authentication so Phase 6 service/automation tokens remain valid when external identity auth is configured.
- Completed local Pandar tenant role enforcement for linked external identities; provider organizations are not trusted as tenant authorization.
- Completed frontend auth integration points so server components/actions forward request-cookie bearer tokens, `APP_AUTH_BEARER_TOKEN`, or `APP_API_TOKEN` to the Rust API.
- Completed tests with local JWKS fixtures for valid token, unknown key, bad issuer, bad audience, expired token, missing membership, insufficient tenant role, print job authorization, and WebSocket authorization.

Exit criteria:

- A signed-in Clerk or Logto user can call tenant-scoped APIs only when Rust has a matching local user and tenant membership.
- A valid identity-provider token without Pandar tenant membership is authenticated but not authorized.
- Tenant role decisions are fully enforced by Pandar repositories and are independent of provider-side organization membership.
- Existing API-token auth remains available for non-browser automation.

## Phase 11: Provisioning And Admin Boundaries

Goal: remove development-only tenant/token ergonomics before production multi-tenant exposure.

- Completed bootstrap-token protection for cross-tenant summary, tenant listing, and tenant creation endpoints using `PANDAR_BOOTSTRAP_TOKEN`.
- Completed first-user/bootstrap flow for creating a tenant, tenant admin, initial API token, and bootstrap audit events in one SQLite/PostgreSQL transaction.
- Completed user invite/linking APIs that bind a verified Clerk/Logto subject to a local Pandar user.
- Completed tenant-scoped user and token management APIs for tenant admins, including role updates and API-token revocation.
- Completed explicit bootstrap authorization for cross-tenant summary/listing endpoints.
- Completed audit coverage for provisioning, token creation/revocation, role changes, and agent pairing actions.
- Completed agent pairing bundle flow that avoids hand-copying persistent database IDs from separate responses, and documented the future token-rotation protocol.
- Completed frontend tenant-bound dashboard reads so `APP_TENANT_ID` deployments do not require cross-tenant bootstrap authority for normal tenant views.

Exit criteria:

- Completed: a fresh deployment can be bootstrapped through documented APIs without test fixtures.
- Completed: tenant users cannot list or summarize other tenants unless they hold the explicit bootstrap authority.
- Completed: provisioning actions are represented in audit events.

## Phase 12: Complete SeaORM Repository Migration

Goal: finish the staged SeaORM 2.0 migration without changing external repository behavior.

- Implemented auth, identity, membership, and audit repository migration.
- Implemented agents/printers migration while preserving live-session and printer snapshot semantics.
- Implemented command/job/artifact repository migration and transaction coupling.
- Completed SQLx escape-hatch audit: repository raw SQL is limited to `crates/pandar-hub/src/repositories/adapters/printers.rs`.
- Kept SQLx migrations as the schema source until there is a separate, explicit decision to adopt SeaORM migrations.
- Maintained SQLite and PostgreSQL parity tests for migrated repository behavior, including the printer snapshot adapter.
- Completed final SDD implementation review and full verification.

Exit criteria:

- Completed: all persistent repository operations use SeaORM query/entity APIs or the explicitly documented printer snapshot upsert adapter.
- Completed: SQLite and PostgreSQL test coverage covers repository behavior and transaction coupling.
- Completed: no mixed SQLx/SeaORM behavior drift remains outside connection/migration plumbing, tests, and the documented adapter.

## Phase 13: Discovery, Diagnostics, And Compatibility Matrix

Goal: make real printer operation debuggable across Bambu printer families.

- Completed agent-side LAN discovery from the reference SSDP behavior on multicast `239.255.255.250:2021`.
- Completed structured diagnostics for configured-printer validation, MQTT reachability/report flow, FTPS reachability, storage write probe, and compatibility.
- Completed command `result_json` persistence and tenant-scoped command detail reads for structured discovery/diagnostic output.
- Completed hub APIs for discovery and diagnostics with operator authorization, tenant scoping, audit events, and wake-agent dispatch.
- Completed a centralized conservative compatibility matrix for model aliases, FTPS TLS/profile policy, external storage, and feature availability. Protected FTPS data channels are a global security policy, not a model capability.
- Completed print-time rejection for unsupported or unknown flow calibration before artifact upload.
- Completed frontend linked-agent controls and command result rendering for discovery rows, diagnostic checks, and compatibility availability without Bambu access-code inputs.

Exit criteria:

- Completed: operators can discover local printers, validate configured credentials indirectly, and see actionable diagnostics before dispatching a print.
- Completed: expected printer or environment problems are successful diagnostic command results with `overall = "problem"` instead of failed hub commands.
- Completed: compatibility rules are centralized and referenced by print command building, FTPS runtime policy, diagnostics, and UI availability.
- Completed: Bambu access codes remain agent-local and are not accepted by hub diagnostic APIs or frontend forms.

## Phase 14: AMS, Filament, And Spool Operations

Goal: promote AMS/external-spool data from raw report details into first-class tenant-visible state.

- Completed agent-side normalization for AMS units, tray IDs, external spool identifiers, active tray, filament type/color/material fields, remaining estimates, credential filtering, and Bambu mapping payloads.
- Completed SQLite/PostgreSQL migrations, SeaORM entities, and repositories for tenant-scoped material snapshots plus derived job filament usage rows.
- Completed preservation of `ams_mapping` and `ams_mapping2` semantics used by `project_file` commands, including strict API shape validation, null-vs-empty persistence, external spool canonicalization, and dispatch to agents.
- Completed terminal job filament usage derivation from persisted mappings and the latest material snapshot with clear `mapped_no_quantity` uncertainty boundaries.
- Completed printer/job HTTP response shapes and frontend dashboard rendering for material summaries and job material rows.
- Kept Spoolman-style external inventory, spool weight tracking, catalog sync, and purchasing out of scope until Pandar's internal state model is stable.

Exit criteria:

- Completed: the printer view exposes current AMS/external-spool state without raw MQTT payload knowledge.
- Completed: print dispatch can persist and show the mapping used for each job.
- Completed: filament usage can be derived from completed or failed jobs with clear uncertainty boundaries.

## Phase 15: Product Runtime UX And Notifications

Goal: turn the operational skeleton into a usable day-to-day cloud replacement surface.

- Completed hub-issued WebSocket tickets:
  - `POST /api/v1/tenants/{tenant_id}/printer-events/tickets` issues tenant-scoped viewer tickets.
  - Tickets are one-use, expire after 60 seconds, and are stored hashed in SQLite/PostgreSQL so sibling Hub replicas can consume them.
  - `GET /api/v1/tenants/{tenant_id}/printer-events` accepts either `Authorization` bearer auth or `ticket` query auth.
- Completed browser-safe ticket bridging through `POST /api/tenants/{tenantId}/printer-events/ticket`; browser code receives auth metadata and opaque tickets only, not `APP_API_TOKEN`, `APP_AUTH_BEARER_TOKEN`, or HttpOnly cookie token values.
- Completed authenticated frontend consumption of `printer_snapshot` and `job_progress` events with live state merging and reconnect delays of 1s, 2s, 5s, and 10s. The UI marks the channel unavailable after 3 failures while continuing retries.
- Completed focused operator notifications for live connection loss and future live transitions:
  - WebSocket subscription failure or disconnect
  - printer offline
  - dispatch/job failure or error
  - physical print failed
  - physical print completed
- Excluded cancellation notifications and historical replay notifications.
- Completed job history/detail improvements for dispatch status, physical progress, artifact details, material details, and command references.
- Completed tenant operation references for agent pairing, API token management, and diagnostics without rendering token values.

Exit criteria:

- Completed: common print monitoring workflows can be performed without refreshing the page.
- Completed: notification and job detail surfaces distinguish hub dispatch/job errors from physical print failures and completions.

## Phase 16: Tenant Tokens And Agent Enrollment

Goal: replace the user-scoped API token model with tenant-owned tokens that can authorize API access and agent registration while preserving the outbound reverse-connection model.

- Completed a new `tenant_tokens` model owned directly by `tenant_id`, not by `user_id`.
- Completed replacing user-scoped API tokens for bearer API authentication; user records remain for human identity and role management, not token ownership.
- Completed hash-only tenant token storage with plaintext token values returned once on creation and rotation.
- Completed `scopes` as the sole token capability source:
  - empty `scopes` means read-only tenant access, equivalent to viewer behavior;
  - `["*"]` means all tenant-scoped API and agent-registration capabilities;
  - `["agent:register"]` means the token can register or rotate agents but cannot read or mutate ordinary tenant API resources.
- Completed nullable `created_by_user_id` audit metadata. Token authorization does not inherit the creating user's role, and later user role changes do not change token capability.
- Completed tenant-token creation, listing, revocation, rotation, and last-used tracking APIs for tenant admins.
- Completed tenant-token-backed agent enrollment credentials for `agent:register` and `*` scopes.
- Completed `pandar-agent` reverse gRPC authentication with tenant-scoped agent credentials instead of trusting only `PANDAR_TENANT_ID` and `PANDAR_AGENT_ID`.
- Completed hash-only agent credential persistence with plaintext credentials returned once for pairing and rotation.
- Completed authenticated-session binding for gRPC command, heartbeat, snapshot, print report, and command result updates.
- Preserved stale-session protection and replacement-session behavior from Phase 2.
- Updated deployment docs so API automation and agent pairing use tenant tokens instead of user-owned API tokens or long-lived bootstrap credentials.

Exit criteria:

- Completed: existing user-scoped API tokens are no longer accepted for bearer API authentication after the migration.
- Completed: a tenant can own multiple active tenant tokens with independent scopes, revocation, rotation, and audit metadata.
- Completed: empty-scope tenant tokens can read tenant resources but cannot mutate them.
- Completed: `*` tenant tokens can perform all tenant-scoped operations.
- Completed: `agent:register` tenant tokens can register or rotate agents but cannot access ordinary tenant API resources.
- Completed: a fresh agent can be enrolled through tenant-token-authorized pairing and connect without manual database identifiers.
- Completed: revoked or rotated tenant tokens and agent credentials cannot open or mutate protected sessions.
- Completed: existing command dispatch and printer/job report tests prove authenticated agent identity is enforced.

## Phase 17: Tenant Admin Product UI

Goal: turn the existing provisioning APIs into a usable tenant-admin surface without moving authorization decisions out of Rust.

- Completed frontend tenant administration for users, roles, external identity links, tenant tokens, agent pairing, linked agents, and recent audit events.
- Completed tenant-admin removal for stale linked agents: `/agents` now exposes a confirmed delete action for non-online agents, the Hub rejects online-agent deletion with `agent_online`, and successful removals are audited while existing agent-owned rows cascade through the database.
- Kept Clerk/Logto as authentication providers only; tenant membership and roles remain Pandar-owned data.
- Completed copy-once handling by discarding plaintext token/pairing responses in browser UI state and avoiding local storage, cookies, and Zustand for secrets.
- Completed compact unavailable rendering when admin resources cannot be read by the current auth context.
- Kept bootstrap-only cross-tenant APIs separate from ordinary tenant-admin UI.

Exit criteria:

- A tenant admin can onboard an operator or viewer, link a Clerk/Logto subject, issue/revoke scoped tenant tokens, and pair an agent from the product UI.
- The UI never displays stored secret values after creation.
- Viewer/operator roles cannot access tenant-admin screens or mutations.

## Phase 18: Command Controls And Recovery UX

Goal: make day-to-day printer operations recoverable from the UI when dispatch or machine state changes unexpectedly.

- Completed tenant-authorized refresh, retry dispatch, reprint, and duplicate-and-print controls.
- Phase 27 later added pause, resume, stop, and print-speed controls; Phase 29 moves customer controls to protocol-defined `printer_operation` commands rather than physical-state mutations.
- Show command state transitions and latest structured result details inline with the affected printer or job.
- Added safe retry affordances for failed dispatch/upload/MQTT operations without creating duplicate physical prints accidentally.
- Kept raw Bambu commands behind diagnostics/admin boundaries; normal operators use typed controls.

Exit criteria:

- Operators can recover common failed or stuck jobs without leaving the dashboard.
- Retrying a failed dispatch is explicit and does not confuse command success with physical print completion.
- Command controls preserve audit events and role authorization.

## Phase 19: Operational Reliability And Observability

Goal: make Pandar easier to operate in long-running self-hosted deployments.

- Completed `/readyz` checks for database, configured artifact storage, gRPC bind configuration, and external-auth JWKS readiness.
- Completed `/metrics` Prometheus output for agent sessions, command lifecycle counts, WebSocket tickets/subscriptions, job outcomes, printer report ingestion, and readiness gauges.
- Completed redaction coverage for bearer tokens, WebSocket tickets, plugin tickets, Bambu access codes, artifact paths, and agent credentials.
- Completed cleanup CLI and retention behavior for terminal jobs/commands, unreferenced artifacts, old machine/audit events, expired/used plugin tickets, and revoked/expired tenant tokens.
- Added backup/restore guidance for SQLite and PostgreSQL deployments.

Exit criteria:

- Operators can distinguish app, database, agent, and printer failures from health/metrics/log evidence.
- Sensitive credentials remain redacted in logs and metrics.
- Self-hosted deployments have documented cleanup and backup paths.

## Phase 20: Artifact And Slicer Workflow Polish

Goal: make print submission closer to a practical Bambu Studio cloud replacement while keeping slicer concerns out of the hub core.

- Completed artifact upload UX with selected filename/size, upload state, displayed max size, and stable backend error-code labels; Phase 25 later replaced browser-side base64 conversion with multipart upload.
- Preserved artifact metadata for operator inspection while keeping slicer files opaque to the hub.
- Completed job duplication and reprint flows that reuse existing artifacts safely.
- Kept backend APIs authoritative for validation.
- Deferred slicer metadata parsing to a future reference-backed parser phase.

Exit criteria:

- Operators can upload, inspect, duplicate, and reprint project artifacts through the UI.
- Material mapping remains explicit and validated.
- The hub still treats slicer files as artifacts unless a future phase adds a reference-backed parser.

## Phase 21: Bambu Studio Network Plugin

Goal: add `crates/pandar-network-plugin` as a Bambu Studio network plugin ABI dynamic-library replacement that connects Bambu Studio to `pandar-hub`.

- Completed `crates/pandar-network-plugin` as a Rust `cdylib` crate with a checked-in C++ ABI shim and export-list test.
- Used `reference/open-bamboo-networking` as the ABI/symbol compatibility reference and `reference/BambuStudio` as the caller-behavior reference.
- Targeted a minimal ABI-compatible shim first, not a full Bambu cloud clone.
- Exported the required `bambu_network_*` and `ft_*` symbols for Bambu Studio loading.
- Kept direct LAN/printer paths as stable no-op/unsupported behavior; the plugin does not connect directly to `pandar-agent` or Bambu machines.
- Implemented login scaffolding around Bambu Studio's existing flow:
  - `bambu_network_get_bambulab_host` starts and returns a plugin-local loopback webserver that serves a Studio-compatible sign-in entry page.
  - The sign-in page is built from the `frontend/plugin-local` monorepo package, embedded in the plugin with `rust-embed`, and lets the user inspect defaults or switch the target web/hub server.
  - The local page then redirects to the configured Pandar frontend authentication flow.
  - The frontend authenticates with Clerk or Logto, selects a tenant through Pandar-managed membership, creates a short-lived one-use plugin login ticket, and returns it through Studio's expected local callback path.
  - The web page uses Bambu Studio's `get_localhost_url` message when available, then sends the browser to Studio's localhost HTTP server with `ticket` and `redirect_url`.
  - Studio calls the plugin's `get_my_token(ticket)` and `get_my_profile(token)` ABI methods; the plugin exchanges the ticket with `pandar-hub` and returns Bambu-shaped token/profile JSON that lets Studio call `change_user(login_info)`.
- Represented the resulting plugin credential as a tenant-owned `["plugin:studio"]` token issued by the hub, not as a user-owned API token.
- Kept Bambu printer access codes and LAN addresses out of the plugin. Those remain agent-local.
- Completed local webserver coverage for embedded assets, default configuration prompts, target-server switching, and ABI handoff through `cargo test -p pandar-network-plugin --test local_webserver` and `--test studio_abi_probe`.
- Added a symbol export test from the local ABI symbol file so missing exports fail before runtime Studio loading.
- Documented Linux, Windows, and macOS replacement paths with packaging/signing explicitly optional.

Exit criteria:

- Bambu Studio can load the Pandar dynamic library through the network plugin path without missing-symbol failures.
- Clicking login in Bambu Studio opens the Pandar sign-in flow, completes Clerk/Logto authentication through the frontend, and returns a tenant-scoped plugin credential through Studio's existing localhost ticket flow.
- The plugin authenticates only to `pandar-hub` and can display user/login state in Bambu Studio through the expected `studio_userlogin`/`studio_useroffline` message shapes.
- No plugin code opens MQTT, FTPS, SFTP, or direct agent sockets.
- Tenant-token revocation or plugin-session revocation prevents further hub access from the plugin.

## Phase 22: Hub Horizontal Scaling Control Plane

Goal: support lightweight single-process Hub deployments and horizontally scaled Hub replicas without changing agent authentication or the reverse gRPC model.

- Completed an explicit Hub control-plane boundary:
  - SQLite and PostgreSQL default to an in-process control plane for single Hub processes.
  - PostgreSQL can use NATS with `PANDAR_CONTROL_PLANE=nats`, `PANDAR_NATS_URL`, and optional `PANDAR_NATS_SUBJECT`.
  - SQLite rejects NATS because it is intentionally scoped to lightweight single-process deployments.
- Completed control messages for agent wake, agent close, and tenant-scoped printer events.
- Kept `pandar-agent` on the existing Hub-authenticated reverse gRPC connection. Agents, browsers, and tenants do not connect to NATS.
- Moved browser WebSocket tickets into SQLite/PostgreSQL-backed one-use storage so browser ticket validation works across Hub replicas.
- Preserved PostgreSQL as the shared fact source for durable tenant, command, job, printer, and ticket state.
- Added cross-instance tests for agent wake/close, WebSocket ticket consumption, and printer event fanout.
- Updated PostgreSQL Docker Compose with an optional NATS profile and documented the deployment split.
- Extended the NixOS module so scaled Hub deployments can use either the local NixOS NATS service or an externally managed NATS URL.
- Added GitHub Actions Nix checks with Hestia caching, package matrices for x86_64/aarch64, and SQLite/PostgreSQL NixOS VM tests.
- Adjusted the NixOS VM tests to run without requiring the Nix `kvm` system feature so GitHub's native arm64 runner can execute them through QEMU fallback when `/dev/kvm` is unavailable.
- Kept the aarch64 package matrix focused on `pandar-hub`, `pandar-agent`, `pandar-cli`, and `pandar-web`; `pandar-network-plugin` remains checked on x86_64 until its C++ ABI export path is reworked for arm64 GNU linking.

Exit criteria:

- SQLite + no broker remains the lightweight single-machine deployment path.
- PostgreSQL + NATS can fan out Hub control messages across replicas while preserving tenant-token authorization at Hub boundaries.
- Print artifacts now use the configured artifact-storage boundary; filesystem storage remains available for single-node deployments, while PostgreSQL + NATS deployments should use object storage or explicitly declare a shared filesystem.

## Phase 23: Real Bambu Studio Plugin Compatibility

Goal: turn the Phase 21 network-plugin scaffold into a verified Bambu Studio integration on real desktop installs.

- Final13 and final14 remain historical regression evidence. Final15/run6 is also non-promotable
  history: Studio selected the single synthetic printer but did not explicitly subscribe it, exposing
  the selected-target ownership gap before any model-task request. Final16 contains that correction
  and is the current completed Linux evidence chain for exact Studio `02.08.01.55`. Real Windows
  Studio and macOS x86_64 remain untested; local macOS arm64 package/ABI and exact-version module-load
  evidence passed, while authenticated macOS behavior remains pending.
- Capture Studio initialization, both library loads, sign-in, token/profile retrieval, printer listing,
  Hub-backed job listing, Hub outage/recovery, logout, and explicit no-hardware unsupported behavior.
  Keep automated print/cancel/command contracts and optional hardware evidence separate.
- Harden `pandar-network-plugin` HTTP behavior beyond symbol exports:
  - preserve useful hub/network error details without exposing bearer tokens, plugin tickets, artifact paths, or local filesystem paths;
  - map Pandar hub responses into stable Bambu-shaped response bodies where Studio expects them;
  - add compatibility probes for Studio versions that call plugin methods in a different order.
- Validate the sign-in loop from Bambu Studio WebView through `frontend/app/plugin-sign-in`, plugin login-ticket exchange, and `studio_userlogin`/`studio_useroffline` callbacks.
- Document known compatible Studio versions, operating systems, plugin replacement paths, and unsupported plugin ABI functions.
- Keep direct LAN/MQTT/FTPS behavior out of the plugin; Studio talks to `pandar-hub`, and Bambu machine credentials remain agent-local.
- Completed local Phase 23 scaffolding: compatibility manifest, smoke runbook, stable plugin error mapping, and a local C++ ABI probe against a mock hub.
- Refreshed local probe evidence on 2026-06-24: `cargo test -p pandar-network-plugin` passed 20 tests against the current code.
- Added a Phase 23 Studio preflight helper that validates local Studio/plugin prerequisite metadata and prints a redacted manifest row template before manual real-Studio testing; it does not claim compatibility without a real Studio run.
- Checked real Studio test prerequisites on 2026-06-24: no local Bambu Studio command and no Windows/macOS host were available, so Phase 23 real Studio rows are blocked until matching Studio installations exist. Matching plugin artifact availability is tracked separately in the Phase 24 release evidence manifest.
- Earlier Task 1 evidence audited the plugin and release path against official Bambu Studio `02.08.01.55`, froze the exact 109-network-plus-21-FT upstream contract, and repaired the target version, bind timezone signature, trailing `PrintParams::slicer_uid`, AMS sync ABI, and complete 130-export surface. That focused run passed all `version,bind,print,ams,ft` modes on native Windows and Linux x64; Linux checker coverage was 16/16, plugin Nextest was 155/155, and the 21-entrypoint FT ownership scope passed 256 cycles under ASan/LSan with `libasan.so.8`. An independent ABI audit returned `VERDICT: APPROVE`. Final12, final11, and final13 are now historical; final13's Windows/PostgreSQL gates had passed before final14 replaced it. Direct LAN and direct `ft_*` machine transfer remain explicitly unsupported.
- Earlier Task 3 evidence completed the truthful Hub-connectivity and printer-presence slice for the pinned Studio contract. Rust owns bounded readiness, authenticated rejection, typed online observations, refresh/cache admission, and generation-scoped delivery tickets. Connection/status/firmware deliveries use the recursive callback gate plus final claims; account events are immutable commit-order records drained FIFO outside business locks, and epoch-owned transition finish keeps reentrant Lost callbacks fenced without requiring the old account to remain visible. Offline/recovery, token/account rotation, local Lost, firmware acknowledgement, and callback reentrancy probes pass through the compiled ABI. That focused Windows run passed 57/57 and full plugin 180/180; an isolated NixOS SSH runner passed strict Clippy, firmware stress 5/5, and full plugin 180/180 with Rust 1.95/GCC 15.2/glibc 2.42 plus target-scoped `lld`, without GitHub Actions. The Linux-only mock-Hub lifetime failure found during verification was fixed in the test fixture rather than by weakening production timeouts, and independent concurrency review returned `VERDICT: APPROVE`.
- Final12 post-freeze Linux pressure exposed a narrower race not covered above. The C++ firmware fixture
  succeeded, but the wrapper failed on `pandar printer status refresh discarded: credentials changed
during request`; a background heartbeat was temporarily clearing `printers_fresh` and could suppress
  a firmware callback. Final13 preserves the last confirmed cache only while a background refresh is in
  flight, while foreground Studio print-info still invalidates immediately and fails closed. Directed
  tests cover both paths. The firmware fixture now uses a callback sentinel handshake and rejects every
  stderr line except the exact stale-generation diagnostic.
- Preserved the full Windows stress diagnosis. Final12's first full run failed with `firmware version
refresh failed` before its exact and full reruns passed. After the callback-sentinel change, stress
  iteration 2 failed with `firmware callback missed handoff deadline`, driving the product repair. Six
  repaired iterations passed; iteration 7 then exceeded the old three-second compound logout watchdog.
  An independent wait-for graph found no ABBA cycle because the dispatcher releases the firmware-
  transition lock before waiting on the callback mutex and callback dispatch does not hold the account-
  queue lock. The test now separates start/logout errors, uses an eight-second internal watchdog, and a
  45-second child bound; this is test-timing correction, not a production lock-order change.
- Aligned the remaining Studio session boundary: Rust now owns selection, subscriptions, initialization, heartbeat planning, virtual-local generations, listener eligibility, request snapshots, and two-phase callback tickets. The selected getter is side-effect-free, status succeeds only after an eligible callback delivery, firmware/status/operation classification is total, trailing-slash Hub identities retain persisted login, and both Studio tunnels expose a Hub-backed virtual printer with `print.device.connection_type:"cloud"`; no direct printer socket or credential path was added.
- Implemented the pinned print/task contract in the working tree: all 45 `PrintParams` fields have typed
  dispositions, delivery stages no longer claim physical start, cancellation/stable ids are explicit,
  and `get_user_tasks` now queries authorized Hub jobs with filters and pagination instead of returning
  synthetic empty success.
- Implemented Task 11's pinned caller-owned model-task contract in the later working tree. The exact
  `StatusPanel.cpp:4145-4162` consumer and eight-field `BBLModelTask` layout are frozen; ordinary
  submissions resolve by tenant plus stable Studio id, return real project/preset names with explicit
  no-rating sentinels, and never invent MakerWorld metadata. One persistent worker performs the Hub
  read asynchronously, writes the same caller pointer and calls back once only on success, fences
  account/configuration races, and interrupts pending GET/POST/DELETE and follower waits during
  destroy. Successful no-auth recovery drains persistence and revocation bookkeeping; server delivery
  before its response remains an unknowable HTTP-create outcome, and cross-process persistence has no
  hard real-time bound. Local workspace run `d8622da6-4458-407d-8ae6-48ee8d0ac27b` passed
  1,800/1,800 with one skip. Linux workspace `67858341-820f-42d7-9a8d-a408b03e6d3d` passed
  1,801/1,801 with one skip, PostgreSQL 16 `f3fef6c4-dcb9-46f1-9812-43040510eca4` passed 7/7,
  and GCC compiled tasks `6fe5e158-4b98-425f-b6ef-578624937801` passed 17/17. Independent review
  returned `VERDICT: APPROVE`. At that point final14 predated this slice, so a new frozen candidate
  and bounded real-Studio evidence were still required; final16 below later satisfied that successor
  callback-evidence gate.
- Completed Task 9 development-session hardening. Rust now owns the bounded serialized no-auth retry,
  retries only proven pre-delivery connection failures, and fences every attempt by Hub generation,
  account epoch, configuration, token, logout, and destroy state. Hub issues a no-auth Studio token
  only inside an exactly-one-tenant SQLite/PostgreSQL transaction; missing or ambiguous tenant states
  create no credential or audit. Printer list and task list/plate/subtask `401`/`410` recovery share
  one no-auth rotation and retry once, while authenticated credentials never fall back to no-auth.
- Serialized every persisted Studio account mutation across processes with
  `.pandar-plugin-account.lock`. Only `MutationDurability::Confirmed` may activate a login candidate
  or count pending/direct revocation intent as staged. `ChangedUnconfirmed` means a namespace change
  was published without confirmed directory durability and fails closed; an ordinary error describes
  the canonical namespace visible at return rather than promising crash durability during an extreme
  rollback failure.
- Hardened requested/passive logout. Requested logout first stages a pending tenant-scoped self-
  revocation and otherwise requires a confirmed direct intent before DELETE. Passive loss does not
  revoke, and a requested race upgrades passive finalization without duplicate callbacks. A retained
  login can be fully restored only before DELETE. Successful or idempotent `401`/`410` revocation
  records a `{hub_url, token_sha256}` completed tombstone before cleanup, blocking stale login loads
  and writes. Duplicate pending cleanup after direct success is best effort. The completed ledger is
  unbounded and may be cleared only after all Studio processes using the directory are stopped and all
  older Hub plugin sessions are invalid or expired.
- Aligned two additional Studio-facing boundaries found after final13. The status projection now clears
  pinned Studio `fun` bit 48 so the external change-assist checkbox is hidden while
  `task_ext_change_assist=true` remains explicitly unsupported. Better Auth carries the exact
  `/plugin-sign-in` tenant and localhost callback through magic-link and passkey completion as a
  bounded canonical base64url value; both issuer and dashboard decoders fail closed, and the JWT is
  not copied into the return target. Real Better Auth 1.6.23 handler coverage freezes the extra-decode
  boundary.
- Historical: froze final14 at source `HEAD 2ba0d1f2755501ea9e7d4babcf176db40638f643`.
  Source archive `pandar-bambu-final14-019f7b10.tar.gz` is 2,782,539 bytes with 1,548 regular members
  and SHA-256 `c422d80d89052732db6b8ae87b68fd1e4145c64f588d8382deafef3345d86681`; canonical-tree,
  member-list, and freeze-evidence SHA-256 values are
  `43a4a577fb90327dad9e59bcb89dc1e91352bad83f27786a32cae34cb62136e5`,
  `5b32472c9372a992c23315d9b33691a0f269248b65db312590ed00556e21aac0`, and
  `70d545770086c6acde271d3181508adf4f0d91fc8213771363ec78b2792f5ec3`. Determinism and all
  unsafe/duplicate/case/reparse/content-diff checks passed.
- Passed the final14 pre-freeze frontend gates: Web 38 files/327 tests and standalone auth 3 files/9
  tests, both typechecks, zero-warning Web lint, both production builds, and the Better Auth callback
  smoke. The immutable final14 source then passed Ubuntu-native fmt, strict workspace Clippy,
  module-size 2/2, release-smoke 21/21, and workspace Nextest run
  `d2231751-1284-46b0-aee6-2e041ca1a203` at 1,781/1,781 with one separately reported skip in 812.413
  seconds. Rust `-D warnings` passed; the retained log still records C++ missing-field-initializer and
  dependency future-incompatibility warnings. All five ABI modes passed the exact
  109-network-plus-21-FT contract, and all 21 File
  Transfer entrypoints passed 256 ASan/LSan cycles without sanitizer errors.
- Produced the final14 three-file Linux package at 24,854,111 bytes with SHA-256
  `4e91f2457197532102544b02d4edac5354dc2982ec55fa707a057cbcba518b68`; its 202,300-byte Linux
  evidence bundle has SHA-256 `db6a464ce6b9b4b5e4689e1f0f21962dd097349056e78beb57a8779e1352cb02`.
  Official-AppImage attempt 1 passed with fixed Bambu Studio AppImage SHA-256
  `e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995`: Studio retained one PID
  and start-ticks identity, both libraries mapped 4/4, loader/certificate error counts were zero, and
  exactly one development no-auth session was observed. The 10,603-byte, 23-member redacted evidence
  bundle has SHA-256 `7eac6abbc7364928147d60dd1c583d084c02debf1552734bc82a4dec59c941be` and records
  `authenticated_session_claim=false`.
- Final14's evidence-document review returned `APPROVE` with no Blocking, Important, or Minor finding,
  but the archive predates the model-task implementation and is now historical.
- Corrected Studio target ownership: a device remains an effective cloud target while selected or
  explicitly subscribed, and heartbeat planning uses that deduplicated union. Removing one ownership
  source retains the target while the other remains; only removing both retires cloud state and Cloud
  tickets, without cancelling or advancing any Local generation or Local ticket.
- Froze final16 for exact Bambu Studio commit
  `ba049f6a2e08c3b6033660bb84da80c08722974b`, version `02.08.01.55`. Its source archive SHA-256
  is `24b45dd30c3509c02b609548409f05fa72490512525621dbc0574a05aa62a039`.
- Passed the complete immutable final16 Linux gate: workspace Nextest 1,808/1,808 with one configured
  skip, fmt, strict workspace Clippy, module-size, ABI tools 22/22, release-smoke tools 25/25,
  packaged tasks 18/18, exactly 109 network plus 21 File Transfer exports, all 21 File Transfer
  entrypoints x 256 ASan/LSan cycles, and PostgreSQL 16.14 at 7/7 with zero runtime skips. The
  three-file release archive SHA-256 is
  `023dcad198674c8ad1c20eb9bc34df9ef9685f49dfeca6e6b5ea58188f3a24a3`; the Linux evidence
  archive SHA-256 is `fe35290675aac4e6ce323a8ebc75bde1c34d373b1df7506f7f8a65b69ffea950`.
- Passed the bounded final16 official-AppImage proof with AppImage SHA-256
  `e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995` and packaged plugin
  SHA-256 `3bcce9085205d6af67dc9671cf58cd6f9fb694d5a587b43d160dc8b6a9b0712f`. The fail-closed
  loopback mock observed exactly one model-task HTTP 200 and four lifecycle events exactly once and
  in order: request started, response accepted, callback started, callback returned. The redacted
  evidence manifest SHA-256 is
  `c6ba9b6282581119d3baec720e26990ad63efc20eb394b0c71dced89081d5fd9`. The deterministic
  245,225-byte redacted official-AppImage evidence archive has SHA-256
  `f07c369ad9e0354ef40142294d9385e9c454fd534a04badce4be000f49c06eca`; an independent second
  generation matched byte-for-byte.
- Final16 is current and its bounded Linux evidence chain is complete. The AppImage proof used a
  synthetic persisted authenticated-shaped session and a fail-closed loopback mock; it makes no
  downstream encrypted-log claim and does not claim real authentication, Hub, Agent, database,
  hardware, print, control, cancel, or firmware behavior. GitHub Actions and Windows Studio were not
  used. Independent final evidence-document and code/evidence reviews found no remaining issue and
  returned `APPROVE`; the code/evidence reviewer independently reran all four selected-target
  regressions successfully. Codex Goal `019f7b10-9262-74e1-aa9c-ba18a29beb2a` is complete.
- Historical: froze final12 at source HEAD `2ba0d1f2755501ea9e7d4babcf176db40638f643`: source archive
  `pandar-bambu-final12-019f7b10.tar.gz` contains 1,543 regular files, is 2,740,698 bytes, and has
  SHA-256 `17371828ef7a26cace73cfbed321d094bf38323670e8fa6ccf69d6cbfd4b7eee`, source-tree SHA-256
  `5aa0038dbc3f0962cc172646876263b0db04e1e6df5fbe571553af1967f242a6`, and member-list SHA-256
  `87a6ad1dfaa404731ed30d7e265303cca64fc4278a478f9c12192c09373eb880`. The ordinal selector includes
  cached and untracked non-ignored existing leaves, excludes `reference/*`, generated plugin
  `probe-*`, and `.superpowers/sdd/progress.md`, and rejects unsafe paths, duplicates, case collisions,
  and reparse points. A second archive matched byte-for-byte.
- Historical: passed the complete final12 Windows clean gate: fmt, strict workspace Clippy, module-size 2/2,
  ABI-tool 21/21, release-smoke-tool 21/21, frontend 37 files/324 tests plus typecheck/lint/build, and
  workspace Nextest final run `5e6f3720-4c1b-4a55-ac34-2250c0cefba7` at 1,776/1,776. The first
  complete attempt's one firmware-probe failure passed in isolation and then in the required complete
  rerun. Clean evidence SHA-256 is
  `a6fc922b5069c78dcbe077f6c4238794777f6b17b62574ee75638c46256fb342`.
- Historical: passed real PostgreSQL 16.14 verification from the same frozen source: evidence run
  `3e00d36c-7fb9-47d3-b71b-d9735ebe0eae` and Nextest run
  `0b708279-6183-4477-9f78-31add8d7f423` completed 55/55 with zero runtime skip markers; evidence
  SHA-256 is `d7f002f5be8708844cce406895503ef7056b634bf04aad068722eb25ef15247e`.
- Added the sentinel-only non-media `pandar-bambu-source` startup companion and three-file release-
  smoke policy. The historical final12 Windows amd64 archive is 21,285,799 bytes with SHA-256
  `b4f6913eef7c1d09da9377fbce36b0ab759add25caac2baa0604c07a595440cb`; its CLI, plugin, and
  companion SHA-256 values are respectively
  `1e57a7cfc2b46717129e7ced227b358eedbaaa74064f2ae2ac5cd44eac576b32`,
  `43be9e73350cacb66ee2dfa991f1a7291175c4d18db2ec917a10a1489f9244d9`, and
  `20805176609ebe891ed45bc7171a34ad0d741351b5dbe8c3c4d9f9b4a5a2a49a`. Native build run
  `4fa89d78-503f-4c51-a4e3-fc788a4f7f03`, ABI run
  `6b71c048-8377-4a61-a750-20c5531df864`, and release-smoke run
  `d808cce0-6e5f-45e7-b4aa-f7b39642d67a` passed; evidence SHA-256 is
  `11c38eb3c198cd07b2f96abbfbf70792b078170389e8869b230badbb98a404d2`. No real Windows Studio
  process was launched.
- Historical: froze final13 at source `HEAD 2ba0d1f2755501ea9e7d4babcf176db40638f643`. Source archive
  `pandar-bambu-final13-019f7b10.tar.gz` is 2,751,227 bytes with 1,543 regular members and SHA-256
  `71080abb1e7392b0440a179b5bca9fd80638de74a614105b8dc11a0f70959c34`; canonical-tree,
  member-list, and freeze-evidence SHA-256 values are
  `db0b7c3385c29ff0cdee1930a66f554a6845b58907373ef543563b829c245761`,
  `87a6ad1dfaa404731ed30d7e265303cca64fc4278a478f9c12192c09373eb880`, and
  `4d132e16f91365795f54c97f608483c34b55726c5f614f5bb8ffaac2ede1fb7f`. Determinism passed and
  all unsafe/duplicate/case/reparse/diff counts were zero. Pre-freeze plugin run
  `da32fbc4-f37e-4198-af5e-c35f73512dcb` passed 368/368 with one separately reported skip.
- Historical: completed the final13 Windows clean gate in first full run
  `90cb6a69-08a5-4421-a661-58e696c374a3`: workspace Nextest 1,778/1,778 with one separately
  reported skip in 1,050.084 seconds, firmware probe 28.858 seconds, fmt, zero-warning strict Clippy,
  module-size 2/2, ABI/release tools 21/21, and frontend 37 files/324 tests plus typecheck, zero-warning
  lint, and production build. `npm ci` recorded six audit vulnerabilities (three moderate, three high),
  preserved as dependency-audit evidence rather than a Studio-parity failure. Clean evidence SHA-256
  is `c1ac8807a427ae4b7003681e9ad343d668dab1d6aa7c143d14bc699fe58b7b89`.
- Historical: completed two final13 PostgreSQL 16.14 runs under harness
  `0c292295-f9ab-459b-89c2-ea74f2c9ff56`: runs `24b49c19-cd07-42b5-a5a3-6d220345bd7e` and
  `1f4b8458-6397-4c0b-8ab3-23d37779c68a` each passed 55/55 with 831 filtered and zero runtime skips.
  Normalized evidence SHA-256 is
  `7e04ae355f7bca3fb409bbc700b5c8f160194c0d2f9ec82df823c859566a2db7`; source read-only and
  cleanup checks passed.
- Historical: completed final13 Windows native package/ABI/release-smoke. Archive
  `pandar-final13-windows-amd64-019f7b10.tar.gz` is 21,285,752 bytes with SHA-256
  `6c50e77a0b4008ce46d86de51411117061c5118e18849ca1fb94f4a3f319db64`; ABI and smoke each passed
  21/21, all five modes passed, `dumpbin` reported 271 total plugin exports, and the companion had one
  Pandar sentinel and zero `Bambu_*` exports. Native evidence SHA-256 is
  `3dab4bffa359e4c46eec77cbfb278ce3a1497f806a1d80343a1735b5a68f025b`; build, ABI, and smoke runs
  were `0430ad0e-7f96-41c5-b9aa-1c6fd690fd16`, `2f27f859-b795-4420-b04a-30410ae7bcbc`, and
  `65ffc0b0-e17e-45da-bd3a-3375f5d88de1`. Six earlier pre-product
  manifest-harness calibrations are infrastructure-only history. No Studio/auth/hardware/Action was
  used.
- Historical: completed final13 Linux native/ASan attempt 2. Nextest run
  `6ec3a215-9430-4ad2-adc7-f692ca156333` passed 1,779/1,779 with one separately reported skip in
  792.687 seconds; firmware passed in 27.315 seconds. Fmt, strict Clippy, module-size 2/2, ABI-tool
  22/22, release-smoke-tool 21/21, all five ABI modes, and 21 File Transfer entrypoints x 256 ASan/LSan
  cycles passed. Archive `pandar-final13-linux-amd64-019f7b10.tar.gz` has SHA-256
  `4166e6012e6c1bf7cdf056ba3bfb28f0fbc9d216c31e5ed2e8620adb8b5fcccc`; evidence-bundle SHA-256 is
  `aa7478fe0f74debcc5f3d1f5ec53a2222d726beafe5224935aa3382c24f6097a`. Attempt 1 run
  `c8a134c4-e775-4f37-b6ed-74ccb1b79123` remains non-promotable harness history. The final evidence-
  document review completed after correcting its sole Minor terminology finding. A pre-final Linux
  tree with manifest SHA-256
  `668f541a8e535018495d8a8969fa6a6d5b70daef49ed848c4c03ab19c40e4f9a` and source-archive SHA-256
  `e8c4d17505e9102b7f9fa3fbce8e653dddc7277b33f02671f603818fc1580b3b` passed the exact firmware
  probe 21/21, but this is non-promotable behavioral stress evidence. Final11/final12 and frozen final5
  results are historical; no GitHub Action, live printer action, or live firmware update was used.
- Historical: completed final13 exact-AppImage attempt 8 with the passed Linux package and official Ubuntu 22.04
  Bambu Studio `02.08.01.55` AppImage SHA-256
  `e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995`. Studio PID `137`/start
  ticks `192688662` remained unchanged across two offline failures and one success/commit after Hub
  became ready. Both libraries mapped 4/4; active/total token count was `1/1`; create/revoke/discard
  counts were `1/0/0`; and loader/certificate error counts were zero. The 7,211-byte, 23-member
  redacted evidence bundle has SHA-256
  `a4453c8dce3829cc1a84a372a772b516812fe1564b310e61db9e9009a11cf9d2`. Attempts 1-7 are retained
  as locale/data-directory/first-run harness calibration history. This passes only exact module load
  and same-process development no-auth recovery; authenticated UI/session, printers/jobs/print/logout,
  unsupported UI, hardware, and live firmware remain untested.
- Historical final13 implementation review returned `APPROVE` with no Blocking, Important, or Minor finding. The
  product diff from final12 changed only four Rust connection files, made no C++ ABI change, and kept
  `connection.rs` at 388 lines. The final evidence-document review is complete.
- Full Studio compatibility remains unverified until every claimed platform has the exact session
  evidence required by `docs/compatibility/bambu-studio-plugin.md`. Authenticated Linux/macOS session
  rows, real Windows Studio, macOS x86_64 load, and hardware actions remain untested as recorded in the
  compatibility manifests.

Exit criteria:

- Every platform explicitly claimed for the pinned build has real Studio load/session evidence for the
  packaged network plugin plus sentinel-only BambuSource companion; unclaimed platforms stay untested.
- A user can sign in through Studio, receive a tenant-scoped plugin credential, and list Pandar
  printers/Hub-backed jobs. Print submission, cancellation, and command semantics have separate
  automated contract evidence; a no-print desktop smoke does not claim hardware execution.
- Plugin failure modes are visible enough to diagnose invalid hub URL, expired ticket, revoked plugin token, offline hub, bad artifact, and unauthorized printer/job access.
- The compatibility evidence is documented from real Studio runs, not only unit tests or export inspection.

## Phase 24: Cross-Platform Release Validation And Packaging

Goal: make release artifacts predictable enough for operators to install without building from source.

- Validate tag-driven GitHub Release artifacts on real Linux, Windows, and macOS hosts, including CLI startup, dynamic-library loadability, checksums, and archive layout.
- Completed the local native release-smoke implementation for exact three-file CLI/network-plugin/
  BambuSource layout, exact target-prefix exports, and companion sentinel/no-`Bambu_*` inspection.
  The current stable contract is 108 network plus 21 FT exports. Historical final13 Windows amd64
  passed native MSVC build, all five pinned Public Beta ABI modes,
  packaged CLI, exact layout/contract exports, and companion inspection with archive SHA-256
  `6c50e77a0b4008ce46d86de51411117061c5118e18849ca1fb94f4a3f319db64`. Historical final16 Linux
  native/runtime/sanitizer gates passed with archive SHA-256
  `023dcad198674c8ad1c20eb9bc34df9ef9685f49dfeca6e6b5ea58188f3a24a3`; its official
  exact-AppImage model-task evidence manifest has SHA-256
  `c6ba9b6282581119d3baec720e26990ad63efc20eb394b0c71dced89081d5fd9`. Final15/run6,
  final14, final13 Linux, and final11/final12/final5 results are historical regression evidence only;
  real authenticated Studio sessions remain a separate unclaimed boundary.
- Refreshed local release-smoke unit evidence on 2026-06-24: `cargo test --manifest-path tools/release-smoke/Cargo.toml` passed 17 tests.
- Historical evidence only: the 2026-06-24 `local-a79bcae` Linux archive used the old two-file layout
  and 129-export check. It remains in the manifest but is not a current Studio candidate.
- Historical Phase 24 work wired the tag-driven workflow to the old release smoke. That obsolete
  GNU Windows/two-file matrix was replaced by same-OS Linux amd64 and Windows amd64 three-file jobs
  using the current native release-smoke; tagged run `30654892795` subsequently passed.
- Added operator release installation docs, a release artifact evidence manifest, and the explicit Phase 24 signing decision: `unsigned-accepted`.
- The workflow-run bullets below are historical two-file/129-export Phase 24 evidence. They are not
  current `02.08.01.55` candidates and are not instructions to use GitHub Actions for this alignment.
- Initial pre-workflow release artifact availability check on 2026-06-24 found no GitHub Releases, no `release.yml` workflow runs, and no tags. Later historical workflow_dispatch runs uploaded artifacts for five target families (see run evidence below), but those artifacts predate the current three-file contract. Tagged `v0.1.0` publication and install validation are recorded separately above.
- Historical run `28098334876` (2026-06-24): old Linux two-file artifacts uploaded; Windows plugin
  packaging and macOS CLI linking failed.
- Historical run `28099917011` (2026-06-24): old Linux two-file/129 checks passed; Windows C++
  runtime linking and macOS export inspection failed.
- Historical run `28102001464` (2026-06-24): old two-file checks passed for Linux amd64/arm64,
  Windows amd64, and macOS amd64/arm64; Windows arm64 export inspection failed. None of those artifacts
  included the current BambuSource companion.
- Historical run `28103772270` (2026-06-24) did not start build steps and produced no artifact
  evidence. It is not part of the current no-Actions alignment path.
- Rechecked release availability on 2026-06-25: no GitHub Releases or remote git tags exist, and run `28102001464` artifacts remain unexpired for linux-amd64, linux-arm64, windows-amd64, macos-amd64, and macos-arm64. Local static follow-up checks passed release-smoke for linux-arm64 and windows-amd64 and checksum/layout/file-type inspection for both macOS artifacts; these do not replace target-host install evidence.
- Real host installation evidence now covers only the `linux-amd64` workflow artifact from run `28102001464`; tagged GitHub Release installs and the other target families remain unverified until `docs/compatibility/release-artifacts.md` records target-family rows from actual release artifact installs.
- Rework the Linux `pandar-network-plugin` export strategy if arm64 plugin releases remain a target, because the current GNU export-map path is known to be fragile around Rust `cdylib` plus C++ shim exports.

Exit criteria:

- A release tag produces downloadable archives whose contents are validated on the target OS family before the release is treated as usable.
- Operators can install the CLI, hub/web services, agent, and plugin from documented artifacts without reading CI internals.
- Any unsupported target is explicit in docs and CI output instead of silently publishing an incomplete artifact.

## Phase 25: Scaled Artifact Storage And Upload Pipeline

Goal: remove shared-local-spool as the limiting factor for horizontally scaled print-job creation.

- Add an artifact-storage boundary with at least:
  - completed filesystem backend for SQLite/single-node deployments;
  - completed S3-compatible object-storage backend suitable for PostgreSQL + multi-Hub deployments.
- Completed metadata persistence in PostgreSQL/SQLite while moving artifact bytes behind the storage backend.
- Completed create-job, duplicate, reprint, plugin print, cleanup, metrics, readiness, and backup/restore docs through the storage boundary instead of assuming `PANDAR_SPOOL_DIR` is local to one Hub process.
- Completed browser and plugin artifact upload transport hardening beyond server-action/base64 submission:
  - multipart uploads avoid browser/server-action base64 body amplification;
  - backend validation and stable error-code labels remain authoritative;
  - storage paths are generated by the Hub, not trusted from browser or plugin callers.
- Completed Hub-mediated agent artifact downloads through bearer-authenticated `artifact_download_path` values, so agents do not need browser/plugin payload bytes or object-store credentials.
- Completed final transport hardening for plugin-side streamed multipart uploads, S3 staged-file streaming, handler-owned upload error labels, same-tenant cross-agent artifact `403` classification, backend download failure classification, and redacted Hub-download failure context.
- Added `tools/scaled-artifact-smoke` to exercise multipart plugin submission on one Hub state, command dequeue on another Hub state, and agent download through a Hub HTTP artifact route without a shared local spool.
- Kept slicer files opaque; this phase changed storage and transport, not slicer parsing.
- Live scaled-deployment evidence is tracked in Phase 26; local dry-run coverage remains the Phase 25 storage/transport baseline.

Exit criteria:

- PostgreSQL + NATS deployments can create print jobs from arbitrary Hub replicas without requiring a shared POSIX spool directory.
- Filesystem storage remains the simple default for SQLite/single-node deployments.
- Cleanup, retry, duplicate, reprint, plugin submission, and audit behavior remain consistent across storage backends.
- Large artifact upload failures preserve actionable cause chains without leaking sensitive paths or tokens.

## Phase 26: Production Soak, HA, And Failure Injection

Goal: prove the scaled Hub and agent model under realistic concurrent use before expanding product surface area.

- Completed local dry-run evidence for concurrent agent-session wake convergence, WebSocket subscribers, plugin clients, print-job creation, restart simulation, storage failures, and terminal print-report idempotence.
- Refreshed scaled smoke evidence on 2026-06-24 after Phase 28 metadata persistence: `tools/scaled-artifact-smoke` now constructs print jobs with explicit `artifact_metadata_json: None`, and `--dry-run --iterations 1 --concurrency 2` passed all local scenarios.
- Fixed and re-verified Phase 26 local concurrent plugin pressure after reproducing a SQLite `database is locked` failure: print-job audit transactions now use SQLite immediate write transactions, and `--dry-run --iterations 2 --concurrency 2` passed all local scenarios with scenario-context diagnostics.
- Checked live soak prerequisites on 2026-06-24: local PostgreSQL binaries were available and `tools/scaled-artifact-smoke --live-preflight` verified required variables, input shape, and disposable safety markers for PostgreSQL/NATS/object-storage; the first pass remained blocked until disposable NATS/object-storage endpoints were configured.
- Added a live runner entry point for artifact, fanout, restart, and terminal scenarios against disposable PostgreSQL, NATS, and S3-compatible object storage. The storage failure scenario remains local-only.
- Completed disposable local live soak on 2026-06-25 using PostgreSQL, NATS, and MinIO containers: `--live-preflight` passed, and `--live --iterations 2 --concurrency 2` passed artifact, fanout, restart, and terminal scenarios twice.
- Fixed and re-verified a live-runner assertion that counted prior persistent commands globally during concurrent plugin pressure; live pressure now counts queued print commands for the current pressure fixtures only.
- Added explicit NATS interruption evidence on 2026-06-25: the live `nats-reconnect` scenario waited after Hub B subscribed, the disposable NATS container was stopped and started, and a subsequent plugin print from Hub A still woke the Hub B agent session and dequeued the persisted command.
- Added explicit PostgreSQL restart/reconnect evidence on 2026-06-25: the live `postgres-reconnect` scenario seeded data before a controlled PostgreSQL stop/start, then fresh plugin print creation, command dequeue, and terminal print-report persistence succeeded through the reused pool.
- Deferred proxy-style artificial SQL latency injection beyond Phase 26; current acceptance uses concurrent pressure plus controlled PostgreSQL restart/reconnect as database fault evidence without adding proxy tooling.
- Exercise failure modes:
  - Hub restart is covered locally through shared database/storage/control-plane reconstruction;
  - NATS disconnect/reconnect is covered by the disposable local `nats-reconnect` live scenario;
  - PostgreSQL restart/reconnect is covered by the disposable local `postgres-reconnect` live scenario; artificial SQL latency injection is deferred beyond Phase 26 unless future incidents require proxy-level delay testing;
  - WebSocket ticket consumption across replicas is covered locally;
  - control-plane subscriber decode failure and continuation are covered by focused tests;
  - artifact-storage write/read/delete failures are covered locally.
- Metrics and logs distinguish app, database, broker/control-plane, storage, agent/session, and printer/report failures through `/readyz`, `/metrics`, and full-chain error logging.
- Recommended deployment topologies and operational runbooks for SQLite single-node and PostgreSQL + NATS scaled deployments are documented.

Exit criteria:

- Local scaled dry-run has repeatable evidence for agent sessions, command dispatch/wake, WebSocket fanout, plugin calls, and print-job creation.
- Operators can identify which subsystem failed from `/readyz`, `/metrics`, logs, and documented runbooks.
- Recovery from local Hub restart simulation does not duplicate terminal machine events or regress physical print state.
- Live PostgreSQL + NATS + object-storage artifact, fanout, restart, terminal, explicit broker interruption, and PostgreSQL restart/reconnect scenarios have disposable local evidence. Artificial SQL latency injection is documented as deferred beyond Phase 26 rather than required Phase 26 evidence.

## Phase 27: Reference-Backed Live Printer Controls

Goal: add typed pause, resume, stop, and related live printer controls only after the command path is audited against Bambu reference behavior.

- Completed reference-backed payload policy for pause, resume, stop, and print-speed dispatch.
- Completed typed agent command builders and gateway methods for supported controls; raw command dispatch remains behind diagnostics/admin boundaries.
- Completed Hub-side compatibility gating so unsupported models or unknown capabilities reject enqueue instead of sending speculative commands.
- Completed command lifecycle, audit event, structured result, and physical print-status separation in local tests.
- Added Phase 27 compatibility documentation with local no-network verification commands and explicit real-printer probe status.
- Frontend controls were updated in this phase and covered by a 2026-06-25 `frontend/` production build; browser-level e2e interaction and real-printer probes are not recorded in this workspace.
- Checked Phase 27 live-control probe prerequisites on 2026-06-24: no `PANDAR_PRINTERS` configuration or printer access code is available in this workspace, so pause/resume/stop/print-speed hardware probes are blocked until an operator supplies safe printer state and agent-local LAN credentials outside source control.
- Real-printer probes for pause, resume, stop, and print speed are not recorded; `docs/bambu-lan-printer-probe-2026-06-24.md` covers other MQTT commands only.

Exit criteria:

- Operators can queue supported live printer controls with tenant role enforcement and audit records.
- UI state distinguishes command dispatch success from physical printer state changes reported later over MQTT.
- Unsupported or unknown printer/control combinations stay unavailable with diagnostic context.

## Phase 29: Protocol Printer Operations

Goal: make customer-facing printer actions device-neutral so non-Bambu agents can translate the same semantic operation contract later.

- Completed `PrinterOperation` protobuf dispatch for pause, resume, stop, chamber-light toggle, speed, home, relative axis movement, and hotend temperature.
- Completed Hub persistence and audit of semantic `printer_operation` payloads; Hub validates ownership, compatibility, ranges, axes, and unknown fields without constructing Bambu MQTT JSON or G-code.
- Completed typed `BambuDeviceFeatures` propagation for the complete unsigned `print.fun` bitmap, including unknown bits, bit 63, and valid zero, with nullable text columns in equivalent SQLite and PostgreSQL migrations. Hub advertises the real bitmap only for an exact current Agent observation session with capability 3 and otherwise sends Studio `"0"`.
- Completed Bambu Agent translation for feature-aware semantic operations: bit 32 enables `back_to_center`, bit 38 enables `xyz_ctrl`, and feature-required commands fail closed without legacy downgrade. Requirement-free legacy translation preserves `G28`, `G28 X`, requested axis order, and the exact seven-line Studio movement envelope without a second Y/Z inversion.
- Completed network plugin parsing of strict modern `back_to_center`/`xyz_ctrl` messages and bounded legacy `gcode_line` wrappers into semantic Hub operations; modern axes are uppercase X/Y/Z with numeric direction -1/1 and mode 0/1.
- Completed semantic-first typed Studio `gcode_line` passthrough: recognized Home, axis, and temperature commands remain semantic, while every other string `param` is preserved after JSON decoding, including empty or multiline strings, LF/CRLF, trailing spaces, final newlines, and final blank lines. Arbitrary unwrapped G-code remains unsupported.
- Limited typed `gcode_line` submission to the authenticated plugin route; the tenant controls route rejects it, over-limit requests retain the existing 64 KiB HTTP 400 `invalid_printer_control` boundary, and exact-current-session Agent capability 4 gates dispatch without printer `fun`, fallback, or downgrade. Queued work may move to a capable replacement, while Hub marks work sent before gRPC delivery and never automatically requeues or replays it. No migration was required.
- Recorded deterministic parser, HTTP, Hub/Agent lifecycle, and compiled Cloud/LAN ABI evidence only. `PANDAR_TEST_POSTGRES_URL` was unset, so real PostgreSQL verification was skipped; no live Studio run or live-printer movement, Homing, or passthrough G-code execution is claimed.
- Real-printer probes for Phase 29 home/move/hotend are not recorded in this workspace.

Exit criteria:

- Hub sends `HubCommand::PrinterOperation` for customer controls.
- Agent-local adapters own all device-specific command conversion.
- Studio plugin live controls remain semantic-first. Only an authenticated typed `gcode_line` wrapper may forward its decoded string unchanged to Hub; this is not a raw MQTT tunnel, and tenant controls reject it.

## Phase 28: Reference-Backed Slicer Metadata

Goal: improve artifact inspection and print defaults by reading safe metadata from project files without turning the hub into a slicer.

- Completed a narrow parser boundary for Bambu/3MF project metadata needed by Pandar:
  - plate count and selected plate defaults;
  - model/project display name;
  - material mapping hints;
  - estimated filament/time fields when safely available.
- Completed reference-derived fixtures and bounded parsing; the hub does not parse or execute arbitrary slicer logic.
- Completed optional advisory persistence in both SQLite and PostgreSQL migrations. Backend validation and operator-selected print settings remain authoritative.
- Completed metadata preview, dashboard display, job responses, and plugin responses.
- Completed disposable PostgreSQL metadata verification for create/list/get hydration and reprint/duplicate reuse through the repository boundary.
- Preserved opaque artifact handling for unknown, unsupported, or malformed files.
- Added Windows MSVC build compatibility for the network plugin shim by passing the MSVC C++17 compiler flag.
- Fixed filesystem artifact storage key validation on Windows so rooted paths such as `/tmp/escape` are rejected consistently.
- Made the Phase 21 network plugin export verification locate Visual Studio `dumpbin.exe` on Windows when it is not on `PATH`.
- Added Bambu Studio sign-in route aliases so Studio's localized `/en/sign-in` WebView entry reaches the plugin sign-in page instead of a Next.js 404.
- Polished auth recovery UI after Impeccable critique: plugin sign-in failures now keep actions and developer details in separate stable rows, passkey setup previews the browser/device confirmation prompt, auth buttons have explicit spacing, and sign-out shows visible progress or retry controls before returning to the dashboard.
- Completed the follow-up auth hardening pass from the 34/40 Impeccable critique: plugin failure states now include an explicit action-required status marker, the standalone auth issuer resolves English/Chinese copy from the same locale cookie/headers as the dashboard, auth trusted origins include the issuer base URL by default, sign-out inspects Better Auth client errors before redirecting, and Studio plugin sign-in skips redundant tenant selection when exactly one tenant is available.
- Added local camera tunneling from Agent to Hub and changed the dashboard camera viewer to use a native video element backed by fragmented MP4 instead of multipart image rendering.
- Fixed Bambu Studio network plugin no-auth session recovery so a stale persisted plugin token is refreshed before printer listing, preventing Studio from falling back to `No printer` after local Hub token rotation.
- Fixed dual-nozzle Bambu Studio AMS mapping by preserving 254/255 external-slot semantics and defaulting two AMS units with missing `info` to right/left toolhead bindings when the report exposes dual external slots.
- Fixed the Studio network plugin JSON field extractor so numeric fields do not consume the next string key, and expanded the ABI probe fixture to cover dual AMS units with all tray materials.
- Fixed dual-AMS Bambu Studio binding by emitting AMS `info` as the hex string format Studio parses, so the left toolhead's AMS is not misread as an invalid extruder binding.
- Replaced Hub audit metadata construction for agents, printers, commands, jobs, auth provisioning, and admin bootstrap with typed serde metadata structs instead of fixed-shape `json!` maps.
- Replaced Agent material patch envelope, empty-tray clears, active-tray references, and signed project-file payload headers with typed serde structs while keeping open-ended printer report fields as `Value` at the protocol boundary.
- Replaced remaining fixed-shape compatibility, fake MQTT, material-snapshot test JSON construction with typed serde structs so tests assert serialized contracts through serde instead of ad hoc `json!` field probing.
- Replaced Agent command result tests for printer-link and printer-operation responses with typed serde deserialization instead of `serde_json::Value` field indexing.
- Replaced Agent diagnostics/print-project result tests and Hub gRPC link-printer redaction result tests with typed serde deserialization or typed string maps instead of `Value` object indexing.
- Replaced representative Agent material normalization tests with typed serde patch structs instead of direct `Value` indexing for fixed-shape assertions.
- Replaced the remaining Agent material normalization assertions, Hub material repository snapshot assertions, MQTT project-file payload checks, MQTT material patch checks, and selected machine operation report checks with typed serde test structs instead of direct `Value` indexing.
- Replaced Bambu Studio network plugin installer, local webserver, and operation parser test assertions with typed serde structs/enums instead of direct `Value` field indexing.
- Replaced network plugin HTTP/ABI operation request assertions and small Hub agent-printer/no-auth route response checks with typed serde structs/enums.
- Replaced Hub bootstrap route request/response assertions with typed serde structs instead of direct `Value` field indexing.
- Replaced Hub readiness route response assertions with typed serde structs instead of direct `Value` field indexing.
- Replaced Hub tenant-token route response and audit metadata assertions with typed serde structs instead of direct `Value` field indexing.
- Replaced Hub onboarding and join-link route response assertions with typed serde structs instead of direct `Value` field indexing.
- Replaced Hub agent route response and delete-audit metadata assertions with typed serde structs instead of direct `Value` field indexing.
- Replaced Hub job create/read route response assertions with shared typed serde structs instead of direct `Value` field indexing.
- Replaced Hub job auth-validation response assertions with shared typed serde structs instead of direct `Value` field indexing.
- Replaced Hub multipart job validation and metadata-preview response assertions with typed serde structs instead of direct `Value` field indexing.
- Replaced Hub job recovery response and audit metadata assertions with shared typed serde structs instead of direct `Value` field indexing.
- Replaced Hub provisioning workflow and agent-pairing response/audit assertions with typed serde structs instead of direct `Value` field indexing.
- Replaced Hub printer-events ticket/error/WebSocket event assertions with typed serde structs/enums instead of direct `Value` field indexing.
- Replaced remaining Hub plugin print metadata, plugin-token audit metadata, and audit-events route assertions with typed serde structs instead of direct `Value` field indexing.
- Replaced Hub plugin multipart print-created response assertions with typed serde structs instead of direct `Value` field indexing.
- Replaced Bambu Studio network-plugin installer config patching with typed serde config structs while preserving unknown config fields through flatten maps.
- Fixed the Bambu Studio network-plugin installer to accept Studio's mixed string/boolean plugin flags and write boolean flags back in Studio's native JSON form.
- Historical baseline: an earlier UI-hang workaround returned an immediate synthetic empty
  `get_user_tasks` page. The `02.08.01.55` alignment supersedes that behavior with the authorized
  Hub-backed jobs route, bounded request behavior, filters/pagination, stable ids, and explicit errors;
  it also keeps Cloud and virtual-local callback delivery generation-scoped without duplicate status.
- Preserved Studio HMS and print progress when optional Hub telemetry such as `chamber_light_on` is JSON `null`, defaulting only that missing light state instead of rejecting the entire printer status.
- Prevented Bambu Studio 2.7.1.62 from crashing while parsing Pandar extruder status by always emitting `filam_bak`, and stopped unchanged connection refreshes and selected-machine updates from replaying the server/printer/status callback loop.
- Restored Bambu Studio's single-machine cloud initialization by emitting `tunnel/<device>` before cloud status, retrying at heartbeat cadence until Studio requests `get_version`, and then suppressing later notifications while keeping focus subscriptions side-effect-free.
- Changed the Studio plugin's local Hub default to `127.0.0.1` so IPv6-first `localhost` resolution cannot consume the bounded heartbeat refresh timeout before reaching an IPv4-only Hub listener.
- Changed SQLite print-report persistence to reserve its writer transaction before reading, preventing Studio token-auth writes from invalidating the report snapshot and disconnecting the Agent with `database is locked`.
- Replaced the shared Hub route-test tenant fixture helper's response id extraction with typed serde deserialization instead of direct `Value` field indexing.
- Replaced BRTC control JSON wrapping with a typed flattened serde wrapper and changed fixed redaction test assertions to typed map deserialization.
- Replaced Hub job-create invalid material mapping test case splitting with explicit typed test cases instead of extracting fields from temporary `Value` objects.
- Replaced Hub material snapshot merge internals with typed serde unit/tray/external-spool state structs, leaving only open-ended material attributes in flattened maps.
- Replaced Hub material repository test fixtures with typed serde material patch structs instead of fixed-shape `json!` fixture builders.
- Replaced the Hub material repository outcome test's wrong/valid patch inputs with typed serde fixtures instead of constructing fixed-shape `Value` bodies.
- Replaced Hub plugin multipart error-response assertions with typed serde response structs instead of direct `json!` object equality.
- Replaced the remaining simple Hub route fixed error/status response assertions with typed serde response structs.
- Replaced the Hub printer delete test's empty list response assertion with typed serde response decoding.
- Replaced fixed Bambu MQTT/version runtime report test fixtures with typed serde-serializable structs.
- Replaced fixed agent startup/command report and material patch test fixtures with typed serde-serializable structs.
- Replaced fixed Hub printer material snapshot test fixtures with typed serde-serializable structs.
- Replaced Hub material patch unknown-field redaction and merge-state fields with a typed recursive serde enum instead of recursively filtering `serde_json::Value` maps.
- Replaced Agent material patch output assembly with typed serde patch structs for AMS units, trays, external spools, and active-tray references instead of hand-building `serde_json::Value` objects.
- Replaced the remaining Hub job route test flatten-capture maps with typed recursive serde enums instead of `BTreeMap<String, serde_json::Value>`.
- Replaced Agent print-report HMS diagnostic extraction with the same typed serde envelope used for print progress, avoiding a second raw `Value` deserialize path for the same MQTT report.
- Replaced Agent printer-operation MQTT result handling with a single typed serde envelope that preserves unknown fields through flattened maps instead of cloning raw `Value` for a second payload conversion.
- Replaced Agent project-file MQTT signing with typed command payload structs, so signing no longer deserializes a prebuilt `serde_json::Value` back into a duplicate project-file shape.
- Moved Agent chamber-light MQTT report decoding behind a typed helper so the light-control loop consumes `PrinterReport` instead of raw `serde_json::Value`.
- Replaced Agent MQTT sequence-id test helpers with typed serde envelopes instead of `BTreeMap<String, ...>` lookups by JSON section name.
- Replaced Hub gRPC link-printer redacted result assertions with typed serde structs instead of dynamic string maps.
- Replaced the Hub printer snapshot material event assertion with direct typed `PrinterEventMaterialJson` comparison instead of a `serde_json::Value` round-trip.
- Replaced core compatibility serialization tests with typed `DiagnosticCompatibility` string round-trips instead of `serde_json::Value` round-trips.
- Replaced Hub material snapshot fixture decoders and Agent machine operation report helpers with typed serde string round-trips instead of `serde_json::Value` round-trips.
- Replaced Hub JWT verifier test audience/JWKS decoders with typed serde string round-trips instead of `serde_json::Value` conversion helpers.
- Replaced Agent machine and MQTT test payload inspectors with typed serde string decoders instead of cloning `serde_json::Value` into fixed-shape structs.
- Replaced Agent fake MQTT payload matching helpers with typed serde decoders over borrowed payloads instead of cloning `serde_json::Value` into fixed-shape structs.
- Replaced Agent material normalization tests with direct typed patch decoding instead of converting normalized patches through `serde_json::Value`.
- Added a shared Hub route-test typed JSON decoder and removed direct `serde_json::from_value` usage from Hub route tests.
- Centralized Agent machine report decoding behind a typed serde helper and removed direct `serde_json::from_value` calls from Rust crates.
- Aligned the Bambu Studio network plugin's dual-nozzle Studio mapping so Pandar `R` nozzle status maps to Studio Main id `0`, Pandar `L` maps to Deputy id `1`, and AMS/external material bindings follow Studio's 255 Main / 254 Deputy virtual-slot convention.
- Aligned Agent project-file dispatch identities with Bambu Studio/Bambuddy behavior by generating a fresh non-zero int32-range `project_id`/`task_id`/`subtask_id` per submission instead of sending `"0"`.
- Added Bambu LAN X.509 v1 certificate compatibility for MQTT, FTPS, and BRTC while preserving TLS handshake signature verification, with local handshake regressions for valid and mismatched keys.
- Resolved X2D MQTT topic identity from the printer certificate common name while preserving its distinct inventory serial in Hub data, with retrying background report subscription.
- Displayed the Hub-provided dispatch or print failure cause directly in each Devices "Needs attention" job row, with localized reason labels and wrapping text so operators can diagnose failures without opening the Jobs view.
- Matched Bambu Studio print dispatch options by model, carrying Timelapse plus paired Auto/On/Off bed-leveling, flow-dynamics, and nozzle-offset values through Web, Hub, gRPC, Agent, MQTT, and the Studio plugin ABI; migrated existing queued commands and corrected N6/X2D capability handling.
- Restored Next 16 frontend linting with ESLint, extended the 400-line production-module guard to C/C++ and TypeScript/TSX, split the oversized Studio shim and dashboard/action modules, loaded dashboard resources by view with batched user identities, and hardened Docker/Helm defaults for non-root read-only workloads without ServiceAccount tokens.
- Added current and active-target readouts to the nozzle, bed, and chamber temperature dropdowns; chamber targets now flow from Bambu Studio's legacy `ctt` and V2 packed MQTT reports through gRPC, SQLite/PostgreSQL persistence, and dashboard events, while zero targets remain represented as off.
- Moved Bambu Studio account/profile/token JSON, cross-process-locked atomic persisted-login I/O,
  directory-durability decisions, pending/direct/completed revocation state, runtime URL policy,
  `401`/`410` session decisions, and versioned unsupported ABI dispositions into Rust. The thin C++
  shim now retains all eight callback registrations, rejects Debug Studio STL mode, and reports local-
  print-with-record and other unimplemented cloud surfaces explicitly instead of silently succeeding.
- Added a dedicated settings page for each linked Agent, moved printer discovery and its timeout
  control out of the Agent list, and kept discovery command results on the selected Agent's page.

Exit criteria:

- Completed locally: operators can inspect practical project metadata before dispatching a print.
- Completed locally: metadata parsing failures do not block upload or dispatch unless the artifact itself is invalid.
- Completed locally: parsed values never override explicit user settings or compatibility rules.
- Completed locally: disposable PostgreSQL repository verification covers persisted metadata hydration and artifact reuse.

## Optional Later: Virtual Printer And Proxy

- Decide whether virtual-printer/proxy behavior from `reference/bambuddy` is in scope.
- If accepted, isolate it as a separate local-agent feature because it changes LAN behavior, port ownership, MQTT/FTPS proxying, and discovery semantics.

## Immediate Next

- After deploying the updated Web frontend, validate camera picture-in-picture with a live stream in
  current Chrome/Edge and Safari, including returning to Devices controls while PiP is active and
  confirming that closing PiP releases the camera stream. Firefox lacks the standard video PiP API
  exposed to page controls and should retain the fullscreen-only UI.
- Run the `verify_a1_protected_ftps` read-only firmware gate against one real A1 and one real A1 Mini,
  record each exact main-module firmware version, and prove a root directory listing succeeds after `PROT P`.
  Do not upload, delete, print, or issue printer controls during this gate, and do not claim either
  model as hardware-verified until both results are captured.
- Build a new `02.08.01` three-file candidate and validate the exact camera callback and one-use
  loopback URL behavior on a real Studio host. With operator approval, test each model's live camera
  while confirming no printer
  host, access code, or Hub bearer appears in the Studio URL, logs, or evidence bundle. Do not broaden
  the whitelist or claim hardware compatibility from source-backed tests alone.
- Finish the open [GitHub issue #2](https://github.com/ProjectPandar/pandar/issues/2) H2C acceptance rows: with separate operator approval, passively capture nozzle-only and holder-only deltas during a manufacturer-supported rack action; run a real Studio callback/submission; and inspect the exact physical mapping in `project_file` before any small print. Safe-idle full telemetry, protected FTPS listing, V0/V1 mapping, correlated failure delivery, and replacement-session bit-60 fencing are recorded in `docs/compatibility/h2c-hardware-2026-08-04.md`. The Web UI now exposes Studio-shaped rack move/confirm/re-read operations, but no physical rack action has been exercised on hardware yet; validating one of the UI-issued rack commands against a real printer is part of this gate. Do not enable signing, laser/cut, eMMC/`fun2`, or new physical IDs from this evidence.
- After the next `main` push, verify GitHub Actions can publish Hub/Web images and the Helm chart under the `ghcr.io/projectpandar/pandar` package namespace.
- Added macOS desktop publishing for both amd64 and Apple Silicon: both tag-workflow rows use the
  Apple Silicon `macos-26` runner; arm64 runs natively, while amd64 cross-compiles and runs its CLI,
  ABI probe, and release-smoke under Rosetta 2. The jobs reject AppleDouble archive entries. Local
  Apple Silicon release build and packaged smoke passed the current 108 network and 21 File Transfer
  export contract; the official `02.07.01.62` stable app loaded both exact dylibs and reached its normal UI.
  A local x86_64 Mach-O cross-build and Rosetta packaged-smoke preflight also passed; the pinned amd64
  ABI workflow run, real Intel Studio load, and authenticated Studio behavior remain separate next
  steps.
- Deploy the updated Web frontend and confirm a printer with a fresh RFC3339 report and a retained `FAILED` task state shows 1/1 online plus a fresh Online presence label in a UTC+8 browser, while the task status remains Failed.
- Deploy the updated Web frontend and confirm Pause/Resume and the other printer server-form controls navigate to their status feedback without logging `NEXT_REDIRECT` or replacing the dashboard with the data-load fallback.
- Deploy the camera-option fix to the local Agent, restart only `pandar-agent`, and rerun the authenticated Hub camera probe to confirm the production route changes from HTTP 200 with zero bytes to a non-empty fragmented MP4 stream before checking browser playback.
- After deploying the updated Web and Agent, confirm the paused X2D `0500-8062` report surfaces the existing Devices recovery reminder, then use its operator-approved action to continue the print; keep additional real file uploads outside automated validation.
- Track stable `rumqttc-v4-next` releases and security advisories; keep the Agent on the MQTT 3.1.1 package, and rerun raw-broker, PUBACK, firmware-session, reconnect, TLS, and native package gates before any fork upgrade.
- Treat real Better Auth WebView/ticket/session, real Hub/Agent/database integration, and all hardware,
  print, control, cancel, and firmware behavior as separate unclaimed follow-up evidence. Final16's
  fail-closed loopback proof is not a substitute for those rows.
- Run the same authenticated checklist with a newly frozen native archive in real Windows Bambu
  Studio. Historical native MSVC, PE, ABI, and release-smoke evidence does not itself prove Studio
  behavior; no Windows Studio process was launched for final16.
- Run the macOS amd64 job on the Apple Silicon GitHub Actions runner and record exact-version Studio
  authenticated-session evidence on both macOS architectures; local Apple Silicon arm64 package
  evidence alone does not prove those remaining boundaries.
- Run live-printer validation only with an explicitly safe printer state and agent-local credentials:
  chamber target readout, pause/resume/stop/print-speed, home/move/hotend, print/cancel, and other
  hardware-dependent behavior remain unclaimed. Any live firmware-update validation remains a
  separately authorized hardware gate; none was run for final16.
- After a current three-file `v0.1.0` archive is published, record target-host checksum, install, CLI,
  library-load, and real Studio evidence without treating historical two-file workflow artifacts as
  current candidates.

## Completed: Android App

- Added a GitHub Actions workflow that builds and uploads the Android release APK with `git rev-list --count HEAD` injected as its `versionCode`.
- Added a Jetpack Compose + Material 3 Android app under `mobile/android/` (package `zip.iptables.pandar.android`) that monitors printers/jobs and controls Bambu machines via the pandar-hub HTTP/WebSocket API.
- Replaced Android direct OIDC configuration with a Hub browser login flow: Android now asks only for the Hub URL, opens `/mobile-sign-in`, receives a `zip.iptables.pandar.android://auth/callback` one-use ticket, exchanges it with Hub mobile login-ticket APIs, and stores the returned tenant token for normal tenant HTTP/WebSocket calls while keeping the Bambu Studio plugin callback validator loopback-only.
- Added a printers dashboard, per-printer detail (pause/resume/stop, X/Y/Z movement, confirmed full-axis Home, chamber light, set hotend/bed/chamber temperature, AMS load/unload/reread RFID), and a jobs screen with retry-dispatch and reprint, all updated live over the tenant `printer-events` WebSocket.
- Authored JVM unit tests covering status→severity mapping, hub DTO JSON shapes, the WebSocket event decoder, strict control-request body shapes (including a no-polymorphic-discriminator guard), and settings mapping. Build and instrumented tests run in Android Studio (see `docs/android.md`); no Rust crate or Next.js frontend code was modified.
- Verified the Android app on a local `Pixel_8_API_36_1` emulator: fixed Gradle/SDK dependency drift, restored the Gradle wrapper JAR needed for Windows builds, fixed AndroidX/AppAuth API compatibility issues, and corrected `AppContainer` initialization order so `zip.iptables.pandar.android/.MainActivity` stays foreground without a startup crash.
