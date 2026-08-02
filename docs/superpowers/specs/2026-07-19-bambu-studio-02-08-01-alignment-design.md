# Bambu Studio 02.08.01 Alignment Design

## Status And Baseline

- Status addendum: the working tree now implements a constrained Studio local-camera path for
  normalized A1, A1 Mini, P1S, and A2L printers. Eligibility requires an online current Agent session
  advertising `StudioLocalCamera`; the Hub repeats that gate when opening MJPEG. Agent alone knows
  the printer host/access code and connects to the native TLS port-6000 camera protocol. Studio
  receives only a random one-use `bambu:///local/127.0.0.1?...` relay URL, and the replacement
  BambuSource implements exactly the pinned 21-entry local-media ABI for bounded JPEG samples. Every
  other model and all cloud/TUTK/Agora, recording, and direct-printer media paths remain fail-closed.
  Automated protocol/route/relay/export evidence and the positive four-model compiled Studio ABI
  probe pass; packaged cross-platform evidence, real Studio playback, and real camera hardware remain
  pending.
- Status: the core ABI, command, print/task, firmware/AMS, account, and no-auth implementation remains
  in place. Final12 passed its Windows clean, PostgreSQL
  16.14, and Windows MSVC native package/ABI gates, then failed promotion when Linux full validation
  exposed a background-refresh/firmware-callback race. Its compiled fixture succeeded, but its wrapper
  rejected the run after `pandar printer status refresh discarded: credentials changed during
  request`. Final12 hashes and passed gates are historical evidence only.
- Status: historical final13 repairs the discovered race. A background heartbeat uses stale-
  while-revalidate only while its refresh request is in flight; foreground Studio print-info still
  invalidates immediately and fails closed. Directed tests freeze both policies. The firmware fixture
  uses a callback sentinel handshake, and its wrapper permits only the exact expected stale-generation
  diagnostic. The final13 Windows clean, PostgreSQL 16.14, and Windows MSVC package/ABI/release-smoke
  gates passed. Corrected Linux attempt 2, ASan/LSan, and exact-AppImage module-load/development no-auth
  recovery also passed; the final13 evidence-document review is complete. Final13 is not the current
  candidate because it predates the working-tree Studio `fun` bit 48 mask and Better Auth plugin
  return-intent repair.
- Status: final14 is the current verified Linux candidate and contains both post-final13 repairs. It is
  frozen at `HEAD 2ba0d1f2755501ea9e7d4babcf176db40638f643`; immutable source archive
  `pandar-bambu-final14-019f7b10.tar.gz` is 2,782,539 bytes with 1,548 regular members and SHA-256
  `c422d80d89052732db6b8ae87b68fd1e4145c64f588d8382deafef3345d86681`. Canonical-tree,
  member-list, and freeze-evidence SHA-256 values are
  `43a4a577fb90327dad9e59bcb89dc1e91352bad83f27786a32cae34cb62136e5`,
  `5b32472c9372a992c23315d9b33691a0f269248b65db312590ed00556e21aac0`, and
  `70d545770086c6acde271d3181508adf4f0d91fc8213771363ec78b2792f5ec3`. Determinism and all
  unsafe/duplicate/case/reparse/content-diff checks passed.
- Status: final14 pre-freeze frontend verification passed Web 38 files/327 tests and standalone auth 3
  files/9 tests, both typechecks, zero-warning Web lint, both production builds, and the callback smoke.
  From the immutable source, Ubuntu-native fmt, strict workspace Clippy, module-size 2/2,
  release-smoke 21/21, and workspace Nextest run `d2231751-1284-46b0-aee6-2e041ca1a203` passed
  1,781/1,781 with one separately reported skip in 812.413 seconds. The exact 109-network-plus-21-FT
  contract passed all five ABI modes, and 21 File Transfer entrypoints x 256 ASan/LSan cycles reported
  no sanitizer errors. Rust `-D warnings` passed; the retained Clippy log still contains C++ build and
  dependency future-incompatibility warnings.
- Status: final14 Linux package `pandar-final14-linux-amd64-019f7b10.tar.gz` is 24,854,111 bytes with
  SHA-256 `4e91f2457197532102544b02d4edac5354dc2982ec55fa707a057cbcba518b68`; its 202,300-byte evidence
  bundle has SHA-256 `db6a464ce6b9b4b5e4689e1f0f21962dd097349056e78beb57a8779e1352cb02`.
  Official-AppImage attempt 1 passed against AppImage SHA-256
  `e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995`: Studio retained one PID/
  start-ticks identity, both libraries mapped 4/4, loader/certificate error counts were zero, and one
  development no-auth session was observed. The 10,603-byte, 23-member redacted evidence bundle has
  SHA-256 `7eac6abbc7364928147d60dd1c583d084c02debf1552734bc82a4dec59c941be` and explicitly records
  `authenticated_session_claim=false`. This is not Better Auth WebView/ticket/session evidence.
- Status: final14's fresh evidence-document review returned `APPROVE` with no Blocking, Important, or
  Minor finding. The Goal remains active because the pinned model-task overload, authenticated Better
  Auth Studio UI flow, real Windows Studio, macOS, hardware actions, and live firmware remain open.
- Historical status: immutable source archive `pandar-bambu-final13-019f7b10.tar.gz` is 2,751,227 bytes with
  1,543 regular members and SHA-256
  `71080abb1e7392b0440a179b5bca9fd80638de74a614105b8dc11a0f70959c34`. Canonical-tree,
  member-list, and freeze-evidence SHA-256 values are
  `db0b7c3385c29ff0cdee1930a66f554a6845b58907373ef543563b829c245761`,
  `87a6ad1dfaa404731ed30d7e265303cca64fc4278a478f9c12192c09373eb880`, and
  `4d132e16f91365795f54c97f608483c34b55726c5f614f5bb8ffaac2ede1fb7f`. Determinism passed and
  unsafe/duplicate/case/reparse/diff counts were zero.
- Historical status: Windows run `90cb6a69-08a5-4421-a661-58e696c374a3` passed 1,778/1,778 with one separately
  reported skip; clean evidence SHA-256 is
  `c1ac8807a427ae4b7003681e9ad343d668dab1d6aa7c143d14bc699fe58b7b89`. PostgreSQL harness
  `0c292295-f9ab-459b-89c2-ea74f2c9ff56` produced two 55/55 runs with zero runtime skips and normalized
  evidence SHA-256 `7e04ae355f7bca3fb409bbc700b5c8f160194c0d2f9ec82df823c859566a2db7`.
  Windows archive `pandar-final13-windows-amd64-019f7b10.tar.gz` is 21,285,752 bytes with SHA-256
  `6c50e77a0b4008ce46d86de51411117061c5118e18849ca1fb94f4a3f319db64`; native evidence SHA-256
  is `3dab4bffa359e4c46eec77cbfb278ce3a1497f806a1d80343a1735b5a68f025b`.
- Historical status: final13 Linux attempt 2 run `6ec3a215-9430-4ad2-adc7-f692ca156333` passed 1,779/1,779
  with one separately reported skip, all five ABI modes, the exact three-file package/runtime audit,
  and 21 File Transfer entrypoints x 256 ASan/LSan cycles. Package and evidence-bundle SHA-256 values
  are `4166e6012e6c1bf7cdf056ba3bfb28f0fbc9d216c31e5ed2e8620adb8b5fcccc` and
  `aa7478fe0f74debcc5f3d1f5ec53a2222d726beafe5224935aa3382c24f6097a`. Attempt 1 run
  `c8a134c4-e775-4f37-b6ed-74ccb1b79123` remains non-promotable harness history because its wrapper
  expected 21 exports from the FT-only invocation while the checker reported all 130 library contract
  exports.
- Historical status: final13 exact-AppImage attempt 8 passed with official Ubuntu 22.04 `02.08.01.55` AppImage
  SHA-256 `e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995` and the passed Linux
  package. Studio PID `137`/ticks `192688662` remained unchanged through two offline failures and one
  success/commit after Hub became ready; both libraries mapped 4/4, active/total token count was `1/1`,
  create/revoke/discard counts were `1/0/0`, and loader/certificate error counts were zero. Redacted
  23-member evidence SHA-256 is
  `a4453c8dce3829cc1a84a372a772b516812fe1564b310e61db9e9009a11cf9d2`. Attempts 1-7 remain
  locale/data-directory/first-run harness calibration history. This proves exact module load and same-
  process development no-auth recovery, not an authenticated session, Studio UI, hardware, or firmware.
- Historical status: a pre-final Linux tree with manifest SHA-256
  `668f541a8e535018495d8a8969fa6a6d5b70daef49ed848c4c03ab19c40e4f9a` and source-archive SHA-256
  `e8c4d17505e9102b7f9fa3fbce8e653dddc7277b33f02671f603818fc1580b3b` passed the exact firmware
  probe 21/21. This is non-promotable behavioral stress evidence, not a final13 freeze or full gate.
  Authenticated desktop-session rows, real Windows Studio, macOS, hardware actions, and live firmware
  remain untested. No GitHub Action or real printer action was used. Final implementation review
  returned `APPROVE` with no Blocking, Important, or Minor finding; the final evidence-document review
  completed after correcting its sole Minor terminology finding.
- Codex Goal: `019f7b10-9262-74e1-aa9c-ba18a29beb2a`.
- Pandar baseline: `7ff83a64fb6171effcde622536785d67b1e6a44b`.
- Studio contract target: official Bambu Studio `master` commit
  `ba049f6a2e08c3b6033660bb84da80c08722974b`, Studio version `02.08.01.55`,
  network-agent version `02.08.01.52`.
- Handshake family: `02.08.01.x`. The exact audited Studio build is `02.08.01.55`; sharing the first
  eight version characters does not make another patch build compatible. Every exact Studio build
  requires an upstream diff, contract probe, and its own real evidence row before Pandar claims it.

The Studio commit was resolved from the official repository on 2026-07-19. The ignored local
`reference/BambuStudio` checkout is 141 commits behind that target and must not be treated as the
current contract without an explicit commit comparison.

## Context

Pandar is a self-hosted Bambu Studio cloud alternative. Its network plugin is a Hub-backed adapter:
Bambu Studio calls the plugin ABI, the plugin talks only to `pandar-hub`, and `pandar-agent` owns LAN
printer credentials, MQTT, and machine file transfer.

At the recorded Pandar baseline, the plugin could not claim compatibility with the target Studio
family because:

- it returns `02.07.01.00`, while Studio compares the first eight version characters and refuses to
  create the network agent for a different family;
- `bambu_network_bind` omits Studio's `timezone` argument, shifting the following `bool` and callback
  across the C++ ABI boundary;
- the target Studio adds `PrintParams.slicer_uid` and the
  `bambu_network_sync_ams_filaments` export;
- Windows release CI currently cross-compiles the plugin on Ubuntu for the GNU Windows ABI, while the
  official Studio build uses MSVC C++17 and passes `std::string`, `std::vector`, and `std::function`
  across the plugin boundary;
- an offline Hub device can receive a synthetic plugin heartbeat that makes Studio mark it online;
- configured Hub URL state is reported as server connectivity without a successful network check;
- unsupported cloud printer commands may return success without dispatching an operation;
- Wi-Fi, SD-card, binding, chamber, and camera fields include hard-coded or incompatible values;
- print options are only partially forwarded, the callback reaches `PrintingStageFinished` after the
  initial Hub response, and Studio task history/detail calls return empty success values;
- the target Studio emits `wtm_upgrade`, which the firmware command parser does not recognize.

Those bullets are historical baseline gaps, not a description of the current working-tree
implementation. The current contract is pinned to 109 network exports plus 21 File Transfer exports
(130 unique Studio target-prefix exports) and Boost `1.84.0`. Final12 source-freeze, Windows clean,
real PostgreSQL, and Windows native results are historical because Linux stress exposed the additional
race. Final13 is historical after completing its Windows, PostgreSQL, Linux native/ASan, and exact-
AppImage module-load/development no-auth recovery gates. Final14 is the current verified Linux
candidate containing the bit 48 and Better Auth return-intent repairs. Its official-AppImage attempt 1
proves only exact module load and development no-auth same-process behavior; it explicitly sets
`authenticated_session_claim=false`. Earlier export tests, compiled ABI fixtures, final5/final11/
final12 results, and pre-final final13 stress remain regression evidence and do not substitute for the
open authenticated desktop-session gate.

## Goal

Make Pandar a safe and truthful Hub-backed network-plugin replacement for the exact audited Bambu
Studio `02.08.01.55` contract, using the `02.08.01.x` handshake family without treating that family as
a blanket compatibility claim.
The result must:

1. pass Studio's version gate and exactly match the pinned ABI symbols, signatures, callbacks, and
   by-value C++ type layouts;
2. preserve the Hub-only architecture and keep printer credentials and machine transports in Agent;
3. report connectivity, online state, telemetry, and capabilities from authoritative observations;
4. handle, explicitly reject, or explicitly classify every Studio command without silent success;
5. preserve every supported print option and expose honest print progress and task history;
6. record real Studio evidence before adding a compatibility claim.

### Final5 Regression, Final12 Evidence, And Final13 Hardened Resolution

The 2026-07-21 final5 no-auth/no-hardware process run remains a historical regression fixture: when
Studio started before Hub, its initial bootstrap connection failed and that Studio process did not
retry after Hub recovered. Restarting Studio was then required. Final11 first demonstrated same-PID
recovery and is retained as historical evidence. Final12 preserves that retry contract and hardens the
adjacent credential-persistence boundaries under these exact constraints:

1. Only a Rust-proven pre-delivery connection failure may retry. The bounded schedule allows at most
   five attempts including the initial request, starts at two seconds, caps at 30 seconds, and is
   fenced by token, account, and Hub-configuration generations. HTTP responses, ambiguous timeout or
   response loss, and a changed generation never retry.
2. No-auth bootstrap requires exactly one tenant. Multiple tenants return HTTP `409` with
   `ambiguous_no_auth_tenant` and create no server credential or audit.
3. Every persisted Studio account mutation is serialized across processes by
   `.pandar-plugin-account.lock` in addition to the process-local locks. A login, pending revocation,
   or direct intent is accepted only after `MutationDurability::Confirmed`. A
   `ChangedUnconfirmed` result means the namespace change was published but directory durability could
   not be confirmed, so the candidate is not made current. An ordinary error describes the canonical
   namespace visible when the call returns; it is not a promise of crash durability during an extreme
   rollback failure.
4. Requested logout first attempts a confirmed pending self-revoke. If that cannot be confirmed, it
   must confirm a direct-revocation intent before issuing the tenant-scoped DELETE. Passive account
   loss never revokes, and a concurrent requested transition upgrades passive finalization without
   duplicate callbacks. A retained account may be fully restored only on a safe failure before DELETE;
   it is never restored after DELETE is attempted because the remote result may be successful or
   ambiguous.
5. Successful DELETE and idempotent `401`/`410` first record a completed-revocation tombstone containing
   the canonical Hub URL and only the token SHA-256. That tombstone blocks stale login loads and writes
   before direct/pending cleanup. Duplicate pending cleanup after direct success is best effort. The
   completed ledger is intentionally unbounded for now and may be cleared only after every Studio
   process using the data directory is stopped and every older Hub plugin session is invalid or expired.
6. Task list, detail, plate, and subtask requests share one `401`/`410` no-auth rotation and at most one
   retry. Authenticated sessions never fall back to no-auth.
7. A periodic background printer refresh preserves the last confirmed cache only during its in-flight
   replacement request. Its terminal failure still invalidates freshness. A foreground Studio print-
   info request invalidates at admission and cannot read the old cache while waiting. This narrow
   stale-while-revalidate policy prevents heartbeat scheduling from suppressing an unrelated firmware
   callback without making foreground reads optimistic.

The final11 and final13 official-AppImage runs historically proved same-PID development no-auth
recovery. Final14 official-AppImage attempt 1 independently proves the current Linux package's exact
module load and same-process development no-auth behavior, with both libraries mapped 4/4 and no
loader/certificate errors. Automatic no-auth bypasses WebView ticket creation and exchange; the
evidence therefore records `authenticated_session_claim=false` and is not authenticated sign-in
evidence.

## Compatibility Boundary

This goal defines a **Hub-backed Studio compatibility profile**, not a full Bambu cloud clone.

Required behavior:

- dynamic-library load, version acceptance, agent creation, callback registration, sign-in, token and
  profile exchange, Hub connectivity, printer list, subscriptions, truthful status, Hub-backed print
  submission, Hub task history, logout, and explicitly supported printer operations;
- ABI-safe behavior for every symbol loaded by the pinned Studio, including intentionally unsupported
  functions;
- stable, redacted Studio-facing errors for unavailable and unsupported behavior.

Intentionally unsupported unless a separate architecture decision reopens them:

- direct LAN discovery, bind/unbind, certificate ownership, and direct printer sockets inside the
  plugin;
- plugin-owned MQTT, FTPS, SFTP, or Agent connections;
- direct printer camera credentials, cloud/TUTK/Agora media, and camera recording (the sole camera
  exception is the authenticated Hub/Agent-mediated loopback MJPEG path described above);
- direct `ft_*` media browsing/upload, Send to SD card, local print, and SD-card print;
- MakerWorld publishing, ratings, recommendations, and a complete Bambu cloud settings clone;
- firmware package staging or hosting.

Studio's LAN-shaped connect/message entrypoints are retained only as a Hub-backed virtual/local ABI
proxy. An authorized `dev_id` is the sole target authority; host/IP, username, password, and SSL
arguments are ignored and scrubbed, and the plugin opens no direct machine socket. Local status and
messages still travel plugin -> Hub -> Agent -> printer.

Unsupported surfaces must retain the exact ABI shape, return an explicit stable failure, and avoid
advertising capabilities that make Studio offer an unusable action. They must never become empty or
silent success paths merely to make a local probe pass.

## Contract Decisions

### Pinned Studio source and version policy

The pinned Studio commit is the source of truth for this goal. The implementation must record:

- Studio version and network-agent version;
- every `get_network_function(...)` symbol requested by Studio;
- every `ft_*` symbol loaded by `InitFTModule`/`FileTransferUtils`, plus its callback, option, result,
  and handle signatures;
- every function-pointer signature used by the supported flow;
- every STL-owned type passed by value or reference across the ABI;
- the source commit used to construct each compiled fixture.

The contract gate must consume an actual checkout of the official repository at the pinned commit.
Windows runs use an exact-HEAD local source path. Linux runs use a fresh temporary checkout on the
operator-authorized SSH host and must not mutate its long-lived Pandar
checkout. GitHub Actions are not part of this alignment verification path.
The gate verifies the remote URL and commit, reads the versions and requested symbols from those
upstream files, and compiles its contract caller against the upstream headers. A hand-copied minimal
header, symbol list, or its own SHA is not independent compatibility evidence.

The caller must also use the pinned source dependency that makes those headers complete. For this
target, that is Boost `1.84.0` from the URL and SHA-256 in upstream `deps/Boost/Boost.cmake`. The
checker verifies `BOOST_VERSION == 108400`; the native runner verifies the archive SHA-256, stages the Boost header
tree, and passes that root explicitly. It must not shadow `ProjectTask.hpp`, Boost.Log, or another
transitive upstream header with a Pandar-authored substitute.

For the pinned contract, `bambu_network_get_version` must return the target network-agent version
`02.08.01.52`, and may do so only after the symbol, signature, layout, and version-gate tests pass. A
version string is a compatibility claim, not a workaround for Studio's gate. Supporting another
first-eight-character family requires a separate fixture and real evidence. A patch build inside the
same family still requires a source diff, refreshed contract probe, and exact real evidence. There is
no legacy fallback that lies about one implementation supporting an unaudited build.

### ABI safety

The first implementation slice must:

- add the missing `timezone` parameter to `bambu_network_bind`, even while bind remains unsupported;
- add `slicer_uid` in the exact target `PrintParams` position;
- export `bambu_network_sync_ams_filaments` with the exact target signature and a documented supported
  or explicit-unsupported result;
- compare the complete requested symbol set with the pinned Studio contract;
- compile-check the complete target File Transfer ABI even though its operations remain explicitly
  unsupported;
- execute every target File Transfer typedef through an isolated safety scope, validate callback
  payload cookies, boundary canaries, values, and cardinality across repeated retain/release cycles,
  and run the current C++ File Transfer ABI/ownership boundary plus its native caller with ASan/LSan
  on Linux so a use-after-free, double release, leak, or callback overwrite fails independently of
  the intentional version/layout/export RED cases. Linker flags that load `libasan` into the Rust
  `cdylib` are not evidence that Rust instructions were sanitizer-instrumented; this slice makes no
  such claim. If File Transfer ownership moves into Rust, the evidence must add real Rust sanitizer
  instrumentation before retaining the memory-safety claim;
- maintain one declaration header for Pandar's C++ exports, include it when compiling the production
  definitions, and compile a separate contract translation unit that includes the real upstream
  `NetworkAgent.hpp` and checks those declarations against the upstream function-pointer typedefs;
- compile a dynamic target caller against the real upstream `PrintParams` and callbacks, populate
  sentinel values, and invoke the built plugin so by-value layout drift is observable;
- verify relevant `sizeof`, `alignof`, enum values, and callback shapes on every supported
  compiler/standard-library ABI; verify STL-owned field order with a target-compiled sentinel call,
  using offsets only for types where the C++ standard defines `offsetof`.

`shim.cpp` and its C++ headers remain a thin adapter for C++ entrypoints, STL-owned arguments, callback
storage/invocation, and the minimum synchronization needed by those callbacks. Rust owns account and
persisted-login policy, session/selection/subscription state, virtual-local generations, heartbeat
eligibility, message classification, status/capability construction, camera selection, HTTP behavior,
and Pandar policy behind flat C FFI exports.

### Platform C++ ABI and release artifacts

The compiled plugin must use the same platform C++ ABI family as its Studio host because the boundary
passes Standard Library-owned types by value and embeds `std::function` callbacks:

- Windows x86-64 plugin artifacts use the MSVC Rust target and MSVC C++ compiler on a Windows runner.
  The CLI may keep a separate cross-compiled artifact, but a GNU/MinGW plugin is not compatible with
  an MSVC Studio host.
- macOS plugin artifacts use Apple Clang/libc++ on the matching macOS architecture.
- Linux plugin artifacts must document and match the official Studio distribution's compiler,
  libstdc++ ABI mode, architecture, and minimum runtime baseline.
- An architecture without an official matching Studio host and a runnable ABI probe does not receive
  a claimed-compatible plugin artifact.

Local and authorized SSH checks must probe the packaged artifact itself on the native target runner.
Passing a locally built development library cannot validate a differently compiled release archive.
Artifact evidence records the Rust target, C++ compiler/version, C++ runtime/ABI mode, architecture,
and SHA-256.

### Connectivity and online-state semantics

These signals remain distinct:

- configured Hub URL means only that a connection can be attempted;
- server connected means a bounded request to the configured Hub's unauthenticated health/readiness
  boundary has succeeded and no later transport/readiness failure has invalidated that observation;
- plugin authenticated means the current token is accepted by an authenticated plugin route;
- Agent connected means the Hub owns a current authenticated Agent session;
- printer online means a current printer observation says the printer is reachable;
- receiving a synthetic plugin timer tick is never evidence that the printer is online.

The plugin must preserve `dev_online` from the Hub. It may emit periodic status only for a device whose
last confirmed observation is online in the current session/epoch. A background refresh may retain
that confirmed observation only while the replacement request is in flight; a terminal failure
invalidates it. A foreground Studio print-info request invalidates immediately. An offline transition
must stop online-producing heartbeats and reach Studio through the reference-backed offline path. A
failed refresh must not replay stale `push_status` as fresh presence.

One Rust classifier resolves generic Studio messages in the strict order firmware -> status ->
semantic operation -> unsupported. A status request succeeds only after an eligible current callback
actually receives the payload. Missing listeners, missing subscriptions, stale refresh, ineligible
targets, or a superseded final delivery claim return `BAMBU_NETWORK_ERR_CONNECT_FAILED` (`-2`), not a
silent success.

`connect_server`, `is_server_connected`, server callbacks, and refresh behavior must be driven by a
real bounded Hub request. A plugin-route `401/403` proves the Hub responded but invalidates or rejects
authentication; it must not be collapsed into a transport-disconnected result. The complete lower-
level error cause must remain available in redacted diagnostics.

### Command disposition

Every target Studio command must appear in one versioned disposition table with exactly one outcome:

- `handled`: parsed into a typed semantic operation and dispatched once;
- `explicitly_unsupported`: no dispatch, stable `unsupported_printer_operation`, non-success ABI code;
- `benign_noop`: success is required by observed Studio behavior, has no machine effect, and is named
  individually with a regression test.

Unknown commands use `explicitly_unsupported`; they do not inherit `benign_noop`.

This decision deliberately reopens and supersedes the cloud `Unsupported noncandidate -> SUCCESS`
row in `2026-07-09-studio-native-print-error-design.md` for exact Studio `02.08.01.55`. The native
Resume/Ignore/Stop candidate rules remain unchanged. Tests that currently freeze blanket cloud
success must be replaced by version-targeted disposition tests.

The initial table must classify at least:

- pause, resume, stop, error recovery, print speed, Home/XYZ, typed `gcode_line`, temperatures,
  extruder selection, chamber light, and basic AMS load/unload/RFID;
- `skip_objects`, `set_fan`, `set_airduct`, buzzer, printing options, calibration families, camera
  control, advanced AMS commands, and target-version firmware commands;
- status requests such as `info.get_version` and `pushing.pushall`, which are transport/status
  requests rather than printer operations.

Adding a supported machine operation requires a typed Core/Hub/Agent contract, exact-current-session
capability gating where applicable, SQLite/PostgreSQL parity for persistence changes, and
reference-backed Agent translation. Raw Bambu JSON must not become a general Hub API.

### Truthful telemetry and capabilities

Known status shapes use typed serde models. For each Studio field the projection must identify an
authoritative source and unknown behavior. Unknown or unavailable capability is omitted, false, or an
explicit unavailable state according to Studio's parser; it is never replaced with a favorable fake.

The first status slice covers:

- `dev_online` and offline transitions;
- Wi-Fi signal and device signal;
- SD-card presence and health;
- connection type, binding state, and secure-link state;
- chamber support, current temperature, and target temperature;
- camera availability, local/remote protocol advertisement, and callback URL shape.

Rust owns the connection state machine, heartbeat eligibility, typed status projection, capability
decisions, and camera disposition. Existing C++ helpers that currently construct JSON or decide these
states are reduced to flat-FFI invocation and Studio callback adaptation as part of the same slice;
the fix must not add more policy to `shim_state.hpp` or `shim_status.hpp`.

Capability projection is consumer-specific. Hub and Agent may retain the complete observed feature
bitmap, but the Studio projection clears a known bit when Pandar cannot honor the UI path it enables.
Pinned `DeviceManager.cpp:4393` maps `fun` bit 48 to external change assistance, and the pinned print
flow forwards that checkbox as `task_ext_change_assist`; Pandar therefore clears bit 48 until a typed
downstream encoding exists while preserving supported and unknown bits.

Hub already supplies chamber target temperature, so the plugin must preserve it through the typed
input and Studio payload. Camera callbacks must return a URL shape Studio accepts; when no compatible
path exists, camera capability is unavailable rather than a normal HTTP URL paired with an `rtsps`
claim.

### Print submission and task surfaces

Every target `PrintParams` field must be classified as:

- forwarded to a typed Hub print request and persisted/dispatched;
- consumed by a documented plugin-only concern; or
- rejected before submission with a stable field-specific error because Pandar cannot honor it.

No selected field may be silently dropped. `slicer_uid`, nozzle metadata/mapping, vibration and layer
inspection, timelapse variants, bed type, calibration modes, change assistance, eMMC behavior, and
service context all require explicit entries.

The implementation plan must trace Studio's callback contract before choosing the terminal boundary.
`PrintingStageFinished` must correspond to a documented durable/delivery state, not merely the first
HTTP success if later failure is still part of the same Studio submission. Intermediate callbacks and
`OnWaitFn` usage must reflect states Studio can safely display, and cancellation must not create an
unowned Hub/Agent operation.

`get_user_tasks` must use the existing authenticated Hub jobs route. Plate, JSON subtask-info, and
slice detail functions return real known data or an explicit unavailable error; empty success values
are not valid substitutes. The separate model-task overload has a pinned consumer in
`StatusPanel.cpp:4145-4162`: `bambu_network_get_subtask(BBLModelTask*, callback)` must eventually fill
the caller-owned model from authorized persisted task metadata and invoke the callback exactly once
only on success. Its current explicit unavailable/no-callback result is honest but remains an open
alignment gap. SQLite and PostgreSQL must behave identically for any new query or persisted field.

### Authenticated Better Auth return intent

When `/plugin-sign-in` sends a signed-out user to the self-hosted Better Auth issuer, the selected
tenant and Studio localhost callback must survive magic-link verification, optional passkey setup,
direct passkey sign-in, and the dashboard callback. The intent is a bounded canonical base64url value,
not a nested query: Better Auth 1.6.23 decodes its callback value during verification. Both apps decode
fail-closed and accept only the same-origin `/plugin-sign-in` path and query. Absolute URLs, `//`,
backslashes, fragments, other paths, invalid UTF-8, and non-canonical encodings are rejected. The JWT
is consumed by the dashboard callback and stored only in the HttpOnly cookie; it is never copied into
the return target.

### Firmware and AMS delta

The target firmware command matrix includes `wtm_upgrade`. Pandar must either translate it through the
existing typed prepare/execute protocol with the same ownership, redaction, at-most-once, and no-replay
invariants, or return a capability-driven explicit unsupported result before Studio can start it.

The intentionally empty firmware package catalog remains valid. This goal does not invent download
URLs, stage packages, relax the one-active-Hub ownership boundary, automatically retry an ambiguous
publish, or perform a live flash without separate explicit operator authorization.

Catalog, refresh, and send operations claim an immutable request-generation snapshot before any I/O.
Completion is accepted only for that snapshot, so an older A/B request cannot publish or overwrite a
newer state. Errors remain request-owned rather than sharing a mutable cross-request `last_error`.

The new AMS sync ABI export is required for ABI alignment. Its behavior is separately classified as
handled or explicitly unsupported; adding the export does not authorize a Bambu cloud filament clone.

### BambuSource startup gate

Pinned Studio `GUI_App.cpp:3639-3676` creates the network agent only after
`NetworkAgent::get_bambu_source_entry()` loads `data_dir/plugins/BambuSource.dll` on Windows or the
corresponding `libBambuSource.so`/`libBambuSource.dylib` on Linux/macOS. The exact `02.08.01.55`
archives do not contain that library, so a network-plugin-only install cannot reach agent creation.

Pandar release archives therefore include a tiny same-target `pandar-bambu-source` companion. It
exports only the Pandar sentinel `pandar_bambu_source_sentinel` and deliberately exports no
`Bambu_*` camera/media entrypoints. This satisfies Studio's library-load startup gate while leaving
Studio's existing safe `Fake_Bambu_Create` fallback in control; it does not implement or claim camera,
media, or BambuSource feature parity. The installer requires both artifacts, copies the companion to
Studio's exact platform filename, and preserves the existing config patch and backup behavior.

Native release smoke must require all three packaged files, hash the companion, require its sentinel,
reject every `Bambu_*` export, and continue to ABI-probe only the staged network plugin.

## Workstreams And Dependencies

1. **Contract lock:** pin source/toolchain evidence and add failing version/symbol/signature/layout tests.
2. **ABI repair:** fix version, bind signature, target types, callbacks, and missing exports.
3. **Truthful state:** repair Hub health, offline propagation, heartbeat, telemetry, and capabilities.
4. **Command matrix:** remove blanket success and implement or explicitly reject target commands.
5. **Print and tasks:** classify parameters, correct progress/cancellation, and expose Hub job history.
6. **Firmware/AMS delta:** handle WTM and the target AMS ABI without weakening existing safety.
7. **Real evidence:** run target Studio artifacts and update the existing manifest/runbook.

Contract lock and ABI repair are sequential and block all real Studio claims. Truthful state, command
matrix, and print/task work may proceed as separately reviewed slices after ABI repair. The plugin is
deployed last for any slice that adds a Hub/Agent operation; rollback disables the plugin producer
first and drains or explicitly fails nonterminal work before older Hub/Agent components return.

## Verification And Evidence

Automated gates:

- a pinned-source symbol diff with zero unexplained requested exports;
- pinned-source `ft_*` symbol/signature coverage with explicit unsupported runtime results;
- compiled target-signature and layout probes on every supported platform ABI;
- native-runner checks of the packaged plugin artifact built with the Studio-compatible compiler and
  C++ runtime family;
- native inspection of the packaged BambuSource companion proving its SHA-256, sentinel export, and
  zero `Bambu_*` exports;
- deterministic tests for version acceptance, bind invocation, callback registration/reentrancy,
  Hub connection loss/recovery, offline subscriptions, stale refresh rejection, and logout;
- directed tests proving background refresh preserves the last confirmed cache only while in flight
  and foreground Studio print-info invalidates immediately;
- compiled firmware-fixture synchronization that arms a callback sentinel before command assertions,
  rejects every unexpected stderr line, and permits only the exact stale-generation diagnostic;
- repeated native stress with the internal logout watchdog separated from the request-start assertion,
  an eight-second internal bound, and a 45-second child-process bound;
- deterministic no-auth tests proving retry is limited to pre-delivery connection failures and cannot
  create duplicate server credentials or audits; fail-closed single-tenant selection; server-token
  disposition on logout; and consistent `401`/`410` task recovery;
- table-driven tests proving every known command disposition and unknown-command rejection;
- typed telemetry fixtures for missing, offline, and capability-specific fields;
- print-field classification, progress, cancellation, task pagination/detail, authorization, and
  SQLite/PostgreSQL parity tests;
- `cargo fmt --all -- --check`;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`;
- `cargo test -p pandar-core --test module_size`;
- applicable frontend production checks when a user-facing control changes.

Real evidence gates:

- use a built release artifact, not an in-tree library path accidentally loaded by a helper;
- record exact Studio version, OS, architecture, artifact checksum, date, current `HEAD`, and the
  SHA-256 of the exact dirty source snapshot used for the build. Do not invent a Pandar commit for an
  uncommitted working tree;
- verify load/version acceptance, agent creation, sign-in, ticket/token/profile, printer and task
  lists, Hub outage/recovery, and logout in the exact-Studio desktop smoke;
- keep automated print-field, lifecycle, cancellation, task, and command-contract evidence separate
  from the no-print desktop smoke. Live hardware print/cancel/command evidence is an optional later
  row and is not required to call the no-print desktop smoke complete;
- record unsupported surfaces as `unsupported`, not `passed`;
- run safe live-printer probes only with an authorized printer state and agent-local credentials;
- do not issue a live firmware update as part of ordinary compatibility verification;
- update `docs/compatibility/bambu-studio-plugin.md`; local probes alone never add a `passed` row.

If database behavior changes, real PostgreSQL verification is required in addition to SQLite. An unset
`PANDAR_TEST_POSTGRES_URL` is an explicit blocked/skip result, never implied coverage.

## Documentation Impact

Implementation slices update existing canonical documents instead of creating a second manifest:

- `docs/compatibility/bambu-studio-plugin.md` for version/platform evidence and unsupported surfaces;
- `docs/compatibility/bambu-studio-plugin-smoke.md` for the real-host procedure;
- `docs/architecture.md` for durable architecture and ownership decisions;
- `docs/development.md` for operator/developer behavior and rollout/rollback;
- `docs/roadmap.md` for Phase 23 status and immediate next work.

## Completion Criteria

This Goal is complete only when all of the following are true:

1. Exact Studio `02.08.01.55` loads the packaged network plugin plus the non-media BambuSource
   companion and creates the Pandar agent without version fallback or missing symbols; other
   `02.08.01.x` builds remain unclaimed until separately diffed and evidenced.
2. All target symbols, function signatures, callbacks, and passed C++ types have compiled ABI proof.
3. The bind call is safe even though direct bind remains explicitly unsupported.
4. Offline devices, unavailable Hub state, stale cache, and unknown capabilities cannot appear healthy.
5. Every target command is handled, explicitly unsupported, or an individually justified benign no-op.
6. Supported print fields are preserved, unsupported fields fail before submission, and Studio task
   history reflects Hub jobs.
7. WTM and AMS target deltas have safe, explicit dispositions without weakening firmware invariants.
8. Required Rust, frontend, dual-backend, module-size, and workspace gates pass.
9. The existing compatibility manifest contains real exact-version evidence for every platform Pandar
   claims to support, using a packaged artifact built with the host-compatible C++ ABI.
10. Roadmap and release documentation describe the Hub-backed compatibility profile and its remaining
    unsupported surfaces without claiming full Bambu cloud parity.
11. The no-auth development profile either recovers an initial Hub/bootstrap failure in-process and
    has deterministic tenant, logout-token, and task-expiry semantics, or documents each deliberately
    accepted limitation without calling that profile complete.
12. Better Auth magic-link and passkey flows preserve only the validated Studio plugin return intent
    without an open redirect or bearer-token leak.
13. The pinned model-task `bambu_network_get_subtask` consumer receives real authorized metadata or an
    explicit failure; the Goal cannot close while it remains an unconditional 501/no-callback path.

Current completion status: active. Final14 contains the post-final13 capability and auth repairs and
has passed its immutable freeze, Ubuntu-native workspace/package/ABI/sanitizer gates, and official-
AppImage exact-load/development no-auth attempt. Its fresh evidence-document review returned
`APPROVE`; the model-task overload, authenticated Better Auth desktop-session checklist, real Windows
Studio, and macOS remain open. Hardware and live-firmware evidence remain separately authorized,
unclaimed gates.

## Post-Approval Erratum: Selected-Target Ownership and Final16 Evidence

This 2026-07-23 erratum is appended to the approved design; it does not rewrite the approved body.
It supersedes the final14 current-status language above.

### Selected-target ownership correction

For Studio session delivery, the effective cloud target is a device that is selected or explicitly
subscribed. Heartbeat planning uses the deduplicated union of those ownership sources. Removing one
source does not retire the target while the other remains. Only when both sources are absent may the
plugin retire cloud state and Cloud delivery tickets. That retirement never cancels or advances a
Local generation or Local ticket.

This correction matches the pinned Studio single-device path: Studio may select a device without
adding an explicit subscription. Final15/run6 is therefore non-promotable historical evidence; it
selected the synthetic printer but did not deliver the fixture transition or request the model task
before this ownership correction.

### Final16 evidence and claim boundary

Final16 is the current completed Linux evidence chain for exact Bambu Studio commit
`ba049f6a2e08c3b6033660bb84da80c08722974b`, version `02.08.01.55`. The exact source archive
SHA-256 is `24b45dd30c3509c02b609548409f05fa72490512525621dbc0574a05aa62a039`.
Its immutable Linux gates passed workspace Nextest 1,808/1,808 with one configured skip, fmt, strict
workspace Clippy, module-size, ABI tools 22/22, release-smoke tools 25/25, packaged tasks 18/18,
exactly 109 network plus 21 File Transfer exports, all 21 File Transfer entrypoints x 256 ASan/LSan
cycles, and PostgreSQL 16.14 at 7/7 with zero runtime skips. The three-file release archive SHA-256 is
`023dcad198674c8ad1c20eb9bc34df9ef9685f49dfeca6e6b5ea58188f3a24a3`; the Linux evidence
archive SHA-256 is `fe35290675aac4e6ce323a8ebc75bde1c34d373b1df7506f7f8a65b69ffea950`.

The official-AppImage proof used AppImage SHA-256
`e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995` and packaged plugin
SHA-256 `3bcce9085205d6af67dc9671cf58cd6f9fb694d5a587b43d160dc8b6a9b0712f`. A fail-closed
loopback mock observed exactly one model-task request with HTTP 200 and exactly four ordered lifecycle
events: request started, response accepted, callback started, callback returned. The redacted
evidence manifest SHA-256 is
`c6ba9b6282581119d3baec720e26990ad63efc20eb394b0c71dced89081d5fd9`. The deterministic
245,225-byte redacted official-AppImage evidence archive SHA-256 is
`f07c369ad9e0354ef40142294d9385e9c454fd534a04badce4be000f49c06eca`; an independent second
generation matched byte-for-byte.

The AppImage run used a synthetic persisted authenticated-shaped session. It makes no downstream
Studio encrypted-log claim and does not claim real authentication, Hub, Agent, database, hardware,
print, control, cancel, or firmware behavior. GitHub Actions and Windows Studio were not used. Those
real-system and other-platform rows remain unclaimed follow-up evidence rather than part of the
bounded final16 completion claim. Independent final evidence-document and code/evidence reviews found
no remaining issue and returned `APPROVE`; Codex Goal `019f7b10-9262-74e1-aa9c-ba18a29beb2a` is
complete.
