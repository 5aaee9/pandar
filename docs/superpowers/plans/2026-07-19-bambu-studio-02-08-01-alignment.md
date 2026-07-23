# Bambu Studio 02.08.01 Alignment Implementation Plan

Execution status: in progress on an uncommitted working tree. Tasks 1-7 and Tasks 9-11 have completed
their reviewed implementation slices; Task 8's authenticated and remaining platform desktop rows
remain open.
Final12 passed its Windows and PostgreSQL gates but is historical after Linux exposed a background-
refresh/firmware-callback race. Historical final13 contains that repair, is frozen, and passed the complete Windows
clean, real PostgreSQL 16.14, Windows native package/ABI/release-smoke, corrected Ubuntu-native, and
C++ File Transfer ASan/LSan gates. Linux attempt 1 remains non-promotable outer-harness history;
exact-AppImage attempt 8 passed module load and same-process development no-auth recovery. The
authenticated desktop-session checklist, real Windows Studio, macOS, and hardware remain untested. Final13 implementation
review returned `APPROVE` with no Blocking, Important, or Minor finding; the persistence review remains
`VERDICT: APPROVE` with two documented Minor limitations. The final evidence-document review completed
after correcting its sole Minor terminology finding. Final13 predates the bit 48 capability mask and
Better Auth Studio return-intent repair. Final14 contains those repairs and is now the verified Linux
candidate after immutable freeze, Ubuntu-native workspace/package/ABI/sanitizer, and exact-AppImage
development no-auth gates. Its fresh evidence-document review returned `APPROVE` with no Blocking,
Important, or Minor finding. The working tree now has deterministic compiled model-task coverage, but
the authenticated Better Auth Studio UI flow, real Studio model-task invocation in a newly frozen
candidate, real Windows Studio, macOS, and hardware remain open.
No GitHub Action, live firmware update, or real printer action was used.

> **For agentic workers:** Execute one slice at a time with a fresh implementer and a fresh independent
> reviewer. Keep each slice uncommitted until its focused tests and review pass. Do not let a later
> slice hide a failed earlier gate.

**Goal:** Make Pandar a safe and truthful Hub-backed network-plugin replacement for exact Bambu Studio
`02.08.01.55` using its `02.08.01.x` handshake family, while keeping Direct LAN, plugin-owned file
transfer, and Bambu cloud services as explicitly unsupported boundaries.

**Architecture:** Bambu Studio continues to call the thin C++ ABI adapter. Rust owns account/session
policy, selection, subscriptions, virtual-local generations, heartbeat/status delivery, typed message
classification, HTTP, and compatibility decisions. The C++ side owns only ABI/STL adaptation,
callback invocation, and required synchronization. The plugin connects only to
`pandar-hub`; Hub persists and authorizes semantic work; `pandar-agent` owns Bambu MQTT and machine
file transfer. Compatibility is proven against pinned Studio source plus real Studio runs, not by
Pandar's self-authored fixture alone.

**Tech Stack:** Rust 2024 workspace, C++17/MSVC and platform C++ ABI probes, serde, reqwest, axum,
SeaORM/SQLite/PostgreSQL, tonic/prost, Tokio, cargo-nextest, Bambu Studio `02.08.01.x`.

## Reviewed Inputs

- Design: `docs/superpowers/specs/2026-07-19-bambu-studio-02-08-01-alignment-design.md`.
- Existing Phase 23 design and plan:
  `docs/superpowers/specs/2026-06-24-phase-23-real-studio-plugin-compatibility-design.md` and
  `docs/superpowers/plans/2026-06-24-phase-23-real-studio-plugin-compatibility.md`.
- Existing evidence and runbook: `docs/compatibility/bambu-studio-plugin.md` and
  `docs/compatibility/bambu-studio-plugin-smoke.md`.
- Studio source target: official Bambu Studio commit
  `ba049f6a2e08c3b6033660bb84da80c08722974b`.
- Previous native recovery design: `docs/superpowers/specs/2026-07-09-studio-native-print-error-design.md`.
- Existing firmware design: `docs/superpowers/specs/2026-07-11-bambu-studio-firmware-update-design.md`.

## Global Constraints

- Follow the alignment design exactly. Any change to the compatibility boundary requires design and
  plan re-review before production edits continue.
- Preserve the Hub-only plugin architecture. Do not add direct Agent, MQTT, FTPS, SFTP, discovery, or
  printer connections to the plugin.
- Keep Direct LAN bind/discovery and `ft_*` behavior ABI-safe but explicitly unsupported. Do not make
  them success no-ops.
- Do not raise the advertised Studio version before the target symbol, signature, type-layout, and
  version-gate tests pass.
- Treat `02.08.01.x` only as the handshake family. A different patch build needs an upstream diff,
  refreshed contract run, packaged-artifact probe, and exact evidence row.
- Build every plugin with the Studio host's native C++ ABI family. In particular, never publish a
  GNU/MinGW Windows plugin for an MSVC Studio host.
- `shim.cpp` and C++ shim headers contain only ABI/STL adaptation. Parsing, status construction, HTTP,
  and Pandar policy belong in Rust behind flat C FFI.
- Use typed serde structs/enums for known JSON shapes. Do not replace schema modeling with manual
  `serde_json::Value` extraction.
- Preserve complete lower-level error context and redact tokens, tickets, access codes, signed URLs,
  artifact paths, and filesystem paths.
- Any persistent behavior must support SQLite and PostgreSQL with equivalent migrations and tests.
- Unknown capability fails closed. Do not add a legacy fallback or favorable synthetic default.
- Every production Rust, C/C++, and frontend module must remain at or below 400 LOC. Do not use
  `include!`.
- Do not run a live firmware update. Safe live printer actions require explicit operator authorization,
  a known printer state, and agent-local credentials outside source control.
- A local fixture, export test, or fake Hub never creates a real compatibility evidence row.
- Do not add or run GitHub Actions for this effort. Run Windows checks locally and Linux checks only
  through the user-authorized SSH host in an isolated temporary directory; never mutate the
  remote long-lived Pandar checkout.
- Update `docs/architecture.md`, `docs/development.md`, `docs/compatibility`, and `docs/roadmap.md` only
  after the corresponding implementation slice passes independent review.

## Baseline And Delivery

- Pandar baseline: `7ff83a64fb6171effcde622536785d67b1e6a44b`.
- Studio baseline: `ba049f6a2e08c3b6033660bb84da80c08722974b`.
- `.superpowers/sdd/progress.md` is the ignored local ledger. Maintain it after every gate and never
  stage it.
- Create one Conventional Commit per independently reviewed deployable slice; do not create RED,
  GREEN, or reviewer-fix micro-commits.
- Before each commit, audit the exact diff and exclude `crates/pandar-network-plugin/probe-*` and all
  unrelated workspace changes.
- Fetch and rebase if `origin/main` advances; never force-push.
- The Codex Goal remains active until the design completion criteria and real evidence gates pass.

## Planned Documentation And Test Assets

- Create `tools/studio-abi-contract` as a read-only checker that requires an official Bambu Studio
  checkout at the exact pinned commit, extracts versions plus network and File Transfer requested
  symbols from that checkout, requires upstream's exact Boost `1.84.0` header root, and compiles/runs
  target contract callers against the real upstream headers without a shadow header.
- Create `crates/pandar-network-plugin/src/shim_exports.hpp` as the single declaration surface included
  by production definitions and the upstream contract translation unit for all 109 network and 21 FT
  target exports. Do not duplicate function declarations in the test fixture.
- Create `crates/pandar-network-plugin/tests/fixtures/studio_upstream_contract.cpp` plus the focused
  `studio_upstream_ft_contract.hpp`; the translation unit includes the real upstream
  `NetworkAgent.hpp` and `FileTransferUtils.hpp` from the supplied checkout. Task 1 uses it as a
  target-compiled dynamic caller; after Task 2 creates the shared export declarations, the same
  translation unit also checks those declarations against upstream typedefs.
- Run the contract checker against an exact pinned checkout on local Windows and the authorized Linux
  SSH runner. A copied header/symbol fixture is not a substitute.
- Build any delivery candidate with the native target toolchain and probe the exact packaged artifact;
  this plan does not add or use GitHub Actions.
- Create `docs/compatibility/bambu-studio-command-matrix.md` when Task 4 establishes the target command
  dispositions. This is a command contract, not a second compatibility evidence manifest.
- Continue using `docs/compatibility/bambu-studio-plugin.md` as the only version/platform evidence
  manifest.

---

### Task 1: Freeze The Studio Contract And Add RED Drift Tests

**Files:**

- Create: `tools/studio-abi-contract/{Cargo.toml,Cargo.lock,src/{main.rs,source.rs,plugin.rs,native.rs,types.rs,http_probe.rs}}`
- Create: `crates/pandar-network-plugin/tests/fixtures/{studio_upstream_contract.cpp,studio_upstream_ft_contract.hpp}`
- Modify: `crates/pandar-network-plugin/tests/exports.rs`
- Modify as needed: `crates/pandar-network-plugin/tests/studio_abi_probe/{compiler.rs,run.rs}`

**Acceptance boundary:** This task adds contract evidence and intentionally failing tests only. It does
not change production symbols, version, status, commands, or Hub behavior.

- [x] Require an upstream source path, verify its `origin` is the official repository and `HEAD` is the
  pinned commit, and read Studio/network-agent versions from that checkout. Reject tracked changes to
  the contract source files or a wrong commit; unrelated untracked build output is not contract drift.
- [x] Extract all target `get_network_function(...)` names directly from upstream `NetworkAgent.cpp`
  and all symbols loaded by `InitFTModule`/`FileTransferUtils.cpp`. Compare the built plugin against
  both sets rather than the historical Phase 21 list.
- [x] Compile the target contract source against the actual upstream headers and upstream's pinned
  Boost `1.84.0` dependency. Verify its archive SHA-256 and `BOOST_VERSION`; do not copy or shadow an
  upstream ABI or transitive dependency header with a Pandar-authored minimal header.
- [x] Add target calls including an unsupported bind invocation with timezone/callback, print invocation
  with sentinel values including `slicer_uid`, and AMS sync lookup.
- [x] Compile-check File Transfer callback/options/result/handle declarations from upstream
  `FileTransferUtils.hpp`, then call every loaded `ft_*` entrypoint through the target typedef and
  verify stable unsupported results without a socket or crash. Run 256 create/retain/release cycles
  with callback cookies, boundary canaries, exact values, and exact cardinality, plus an isolated Linux
  ASan/LSan scope that instruments the current C++ FT implementation and native caller and fails on
  callback corruption, use-after-free, double release, or ownership leak. Verify an
  `__asan_report_load*`/`__asan_report_store*` reference and the final `libasan` dependency; do not
  describe the Rust code as instrumented merely because its linker flags load the sanitizer runtime.
  Apply the runtime-link flags to the explicit Linux target only, so host build scripts are neither
  mislabeled as instrumented nor made dependent on a late-loaded ASan runtime.
- [x] Add explicit version-gate coverage using Studio's first-eight-character comparison.
- [x] Record the target platform/toolchain matrix: Studio compiler, Rust target, C++ compiler/version,
  Standard Library ABI/runtime, architecture, and which native runner executes the probe. The current
  Ubuntu/Zig Windows GNU plugin path must be recorded as RED for an MSVC host.
- [x] Prove RED for the current code: version family mismatch, normalized bind signature mismatch,
  target `PrintParams` layout/sentinel mismatch, and missing `bambu_network_sync_ams_filaments`.
  Record each expected failure in the ledger with the exact upstream source location.
- [x] Have a fresh reviewer inspect the checker output and target-compiled caller against the pinned
  upstream source and return
  `VERDICT: APPROVE` before Task 2.

Focused verification:

```powershell
cargo test --manifest-path tools/studio-abi-contract/Cargo.toml
cargo build -p pandar-network-plugin
cargo run --manifest-path tools/studio-abi-contract/Cargo.toml -- --studio-source <exact-upstream-checkout> --plugin <native-plugin-artifact> --boost-archive <boost-1.84.0.tar.gz>
cargo run --manifest-path tools/studio-abi-contract/Cargo.toml -- --studio-source <exact-upstream-checkout> --plugin <asan-native-plugin-artifact> --boost-archive <boost-1.84.0.tar.gz> --ft-safety-only --address-sanitizer
cargo test -p pandar-network-plugin --test exports
cargo test -p pandar-network-plugin --test studio_abi_probe
```

The overall task is RED when the named new assertions fail for the audited production gaps; unrelated
existing probe cases must remain green.

**Completion evidence (2026-07-20):** the checker is pinned to official commit
`ba049f6a2e08c3b6033660bb84da80c08722974b` and verified Boost `1.84.0`; it extracted 109 network and
21 File Transfer declarations. The pre-repair version, bind, `PrintParams`, AMS-sync, and export-set
assertions failed for the intended reasons, while the unrelated probe cases stayed green. A fresh
contract reviewer returned `VERDICT: APPROVE` before Task 2 production changes were accepted.

---

### Task 2: Repair Version, Signatures, Layouts, And Required Exports

**Files:**

- Modify: `crates/pandar-network-plugin/src/shim_types.hpp`
- Create: `crates/pandar-network-plugin/src/shim_exports.hpp`
- Create: `crates/pandar-network-plugin/src/shim_file_transfer_types.hpp`
- Create: `crates/pandar-network-plugin/src/studio_abi.rs`
- Modify: `crates/pandar-network-plugin/src/{lib.rs,shim.cpp,shim_file_transfer.hpp}`
- Modify: `crates/pandar-network-plugin/build.rs`
- Modify: `crates/pandar-network-plugin/src/shim_abi_user.hpp`
- Modify: `crates/pandar-network-plugin/src/shim_abi_operations.hpp`
- Modify: `crates/pandar-network-plugin/src/shim_abi_content.hpp`
- Create: `crates/pandar-network-plugin/tests/studio_target_abi.rs`
- Modify: `crates/pandar-network-plugin/tests/fixtures/studio_abi_probe.cpp`
- Modify: `tools/studio-abi-contract/src/{source.rs,types.rs}` and the target contract fixtures for
  declaration-map and AMS layout checks.
- Modify Task 1 fixtures/tests only for legitimate production-result assertions, never to weaken the
  pinned contract.

**Acceptance boundary:** Studio accepts the target version and every loaded ABI entrypoint is safe.
Direct bind and AMS cloud sync may still be explicitly unsupported, but their ABI shapes are exact.

- [x] Introduce one Rust-owned target network-agent version value, exactly `02.08.01.52`, exposed
  through flat FFI; the C++ export only returns the adapted string.
- [x] Declare every target export once in `shim_exports.hpp`, include it in the production shim, and
  make the upstream contract translation unit compare each declaration with the real upstream
  function-pointer type. Production definitions that diverge from the declaration must fail compile.
- [x] Make `build.rs` derive the macOS export allowlist from that reviewed declaration map and assert
  that it contains the complete 109+21 target set; do not keep using the historical 129-symbol Phase
  21 floor as the target allowlist.
- [x] Add the bind timezone parameter in the exact target position and preserve an explicit bind
  failure without invoking or corrupting the callback.
- [x] Add `slicer_uid` in the exact target `PrintParams` layout.
- [x] Add `bambu_network_sync_ams_filaments` with the exact target by-value signature. Until its later
  disposition is implemented, return the target AMS-sync failure and a stable redacted body.
- [x] Make all Task 1 RED cases GREEN, including target function-pointer calls and struct layout checks.
- [x] Run the plugin test suite under the default Nextest profile and confirm the existing eight heavy
  ABI cases retain their exclusive scheduling configuration.
- [x] On Windows, build and run the contract probe with `x86_64-pc-windows-msvc` and MSVC C++17. Do not
  use the current GNU/MinGW release artifact as proof.
- [x] Obtain fresh independent ABI review approval before advertising exact Studio `02.08.01.55`
  support in docs or artifacts.

Focused verification:

```powershell
cargo nextest run -p pandar-network-plugin -E 'binary(exports) | binary(studio_abi_probe) | binary(studio_target_abi)' --no-tests=fail
cargo nextest show-config test-groups -P default
cargo test -p pandar-core --test module_size
cargo build -p pandar-network-plugin --target x86_64-pc-windows-msvc
cargo run --manifest-path tools/studio-abi-contract/Cargo.toml -- --studio-source <exact-upstream-checkout> --plugin target\x86_64-pc-windows-msvc\debug\pandar_network_plugin.dll --boost-archive <boost-1.84.0.tar.gz>
cargo fmt --all -- --check
cargo clippy -p pandar-network-plugin --all-targets -- -D warnings
```

Run the explicit `x86_64-pc-windows-msvc` build and complete contract invocation from a Windows MSVC
developer environment. It must turn every intentional Task 1 RED into GREEN against the same pinned
Studio checkout and verified Boost root; the lighter Rust/fixture tests are not a substitute.

**Completion evidence (2026-07-20):** the repaired MSVC x64 plugin exposes exactly 130 target exports
and the pinned caller passed `version`, `bind`, `print`, `ams`, and `ft`. Windows checker coverage was
15/15 and the plugin suite was 155/155. The user-authorized native Linux SSH runner independently
passed checker 16/16, the same five caller modes, 155/155 plugin tests, and the 21-entrypoint ×
256-cycle ASan/LSan File Transfer scope with concrete sanitizer imports. No GitHub Action was used.
The independent ABI reviewer returned `VERDICT: APPROVE`.

---

### Task 3: Make Hub Connectivity And Printer Presence Truthful

**Files:**

- Reduce to ABI/callback glue: `crates/pandar-network-plugin/src/{shim_state.hpp,shim_status.hpp,shim_abi_user.hpp}`
- Create: `crates/pandar-network-plugin/src/connection.rs`
- Modify: `crates/pandar-network-plugin/src/{lib.rs,printer_refresh.rs,http.rs}` for the Rust-owned
  connection state machine, typed outcomes, flat FFI, and request ownership.
- Modify: `crates/pandar-network-plugin/src/studio_status/{input.rs,list.rs,request.rs}`
- Modify focused tests under `crates/pandar-network-plugin/tests/{printer_refresh.rs,status_request.rs,studio_status.rs,studio_abi_probe}`.

**Acceptance boundary:** A configured URL, stale cache, or timer cannot create server/printer online
state. Recovery requires a fresh successful observation.

- [x] Write deterministic RED coverage for `200 online -> dev_online=false -> timeout/500 -> 200
  recovery`, including cloud callbacks and the two-second heartbeat window.
- [x] Make `connect_server` perform one bounded request to the public Hub health/readiness boundary.
  Transport failure, timeout, or not-ready response changes server connectivity; authenticated
  plugin-route `401/403` instead changes authentication state while preserving proven reachability.
- [x] Make `refresh_connection` incapable of recovering from a nonempty URL alone; callback only on
  an actual state transition.
- [x] Preserve `dev_online` in typed cache state. Remove offline devices from online-producing
  heartbeat targets, emit the reference-backed offline transition, and reject stale epoch/session
  refreshes.
- [x] A failed refresh keeps diagnostic cache only; it does not emit stale `push_status` or connected
  signals as current state.
- [x] Preserve the last confirmed cache during an in-flight background heartbeat refresh, while keeping
  foreground Studio print-info invalidation fail closed. Freeze the distinction with
  `background_refresh_preserves_last_confirmed_cache_while_in_flight` and
  `foreground_refresh_invalidates_cache_while_in_flight`.
- [x] Move connectivity decisions and heartbeat eligibility out of C++ into `connection.rs`; C++ keeps
  only ABI-owned values, callback invocation, and required synchronization.
- [x] Verify reentrant callback, logout, subscribe/unsubscribe, current local-device, and generation
  invalidation behavior remains deadlock-free.
- [x] Obtain fresh independent review approval.

Focused verification:

```powershell
cargo nextest run -p pandar-network-plugin -E 'binary(printer_refresh) | binary(status_request) | binary(studio_status) | binary(studio_abi_probe)' --no-tests=fail
```

**Completion evidence (2026-07-20):** Rust now owns bounded Hub readiness, authenticated rejection,
typed printer presence, request/cache admission, and generation-scoped delivery tickets. Status,
connection, printer, local-tunnel, and firmware deliveries use the recursive callback gate and their
Rust-owned final claims. Account callbacks are immutable transition events queued in commit order and
drained FIFO outside account/refresh/queue locks; they do not promise that the committed account
remains unchanged while external callback code runs. Account-transition Lost work keeps request
admission fenced through delivery, and an epoch-owned finish cannot release a newer transition. Windows focused
Nextest run `b5cf5c89-80d6-44e5-8838-cc2dcf0ae438` passed 57/57, full plugin run
`043a8ed4-0e9d-4202-82aa-3aad32764584` passed 180/180, and the post-Clippy regression run
`71e597c1-5155-41fa-a3e0-c657130e016e` passed 20/20; fmt and strict Clippy passed. On the isolated
NixOS SSH runner, Rust 1.95.0 plus GCC 15.2.0/glibc 2.42 and target-scoped `lld` passed strict Clippy
and full plugin run `dec4a7a0-0bfd-4257-b6af-43769f4c1f24` 180/180 without GitHub Actions. A
Linux-only mock-Hub lifetime failure was reproduced by `ce52318f-7a2c-490b-975a-ff81542b65b0`, fixed
in the test fixture without changing production timeouts, then passed stress run
`225b6844-8042-4b09-aa93-3259bd1a4bd3` 5/5 with the rotated-token assertion enabled. Independent concurrency review returned
`VERDICT: APPROVE`.

**Final13 addendum (2026-07-22):** Final12 Linux full validation later showed that the compiled
firmware fixture could finish successfully while its Rust wrapper failed on
`pandar printer status refresh discarded: credentials changed during request`. A periodic background
refresh was clearing `printers_fresh` at admission, so ordinary scheduling could temporarily suppress
an unrelated firmware callback. The final13 repair applies stale-while-revalidate only to the in-flight
background path; failure still invalidates, and foreground print-info still invalidates immediately.
The two directed tests above lock those semantics. Final12 is therefore historical. Final13 is frozen;
its Windows, PostgreSQL, corrected Linux native/ASan, and exact-AppImage load/no-auth recovery gates passed.

---

### Task 4: Establish The 02.08.01 Command Disposition Matrix

**Files:**

- Create: `docs/compatibility/bambu-studio-command-matrix.md`
- Modify: `crates/pandar-network-plugin/src/gcode/{operation.rs,studio_json.rs}`
- Modify: `crates/pandar-network-plugin/src/shim_abi_operations.hpp`
- Modify tests: `crates/pandar-network-plugin/tests/{operation_parser.rs,native_print_error.rs}` and
  compiled ABI operation fixtures.
- Modify Core/Hub/Agent files only in separately reviewed child slices for commands selected as
  `handled`.

**Acceptance boundary:** Every observed target Studio command is handled, explicitly unsupported,
invalid, or an individually justified benign no-op. There is no fallthrough success.

- [x] Build the target command inventory from pinned Studio call sites and record command envelope,
  capability/UI gate, Cloud/LAN caller, Pandar disposition, and alternative workflow.
- [x] Add RED tests for no selected device, empty `dev_id`, unknown envelopes, `skip_objects`,
  `set_fan`, `set_airduct`, camera controls, buzzer, calibration, and advanced AMS commands.
- [x] Change blanket cloud unsupported success to stable non-success with
  `unsupported_printer_operation`, explicitly superseding the prior native-print-error table row.
- [x] Preserve the exact valid/invalid native Resume/Ignore/Stop candidate behavior.
- [x] For each command marked `handled`, create a separate reviewed implementation brief covering
  typed Core/Hub/Agent contracts, capability gates, dispatch lifecycle, and reference-exact MQTT.
- [x] For each command left unsupported, ensure Studio is not told the capability is present and no
  Hub/Agent request is made.
- [x] Obtain fresh independent review approval for the complete matrix and parser behavior.

Focused verification:

```powershell
cargo nextest run -p pandar-network-plugin -E 'binary(operation_parser) | binary(native_print_error) | binary(studio_abi_probe)' --no-tests=fail
```

Completion evidence (2026-07-20): the pinned inventory contains 66 finite pairs with 21 existing
typed handled paths, 45 explicit unsupported paths, and no justified success no-op. The complete
contract is recorded in `docs/compatibility/bambu-studio-command-matrix.md`. Parser RED run
`262db3ed-92ce-407a-95d2-e333f7c1dbbd` failed on mixed operation/status/firmware envelopes, and
compiled ABI RED run `a16c7526-4e62-4f3b-8d06-ceb7020c12f0` failed on the empty Cloud target.
After the fix, parser run `fedf734b-8ac7-4e07-ba2b-fd52d68a978c` passed 26/26 and compiled MSVC ABI
run `d6f4a2b7-8905-484a-97b1-2d795b754d3a` passed 1/1. Focused strict Clippy also passed. No new
command was promoted to `handled`, so no new Core/Hub/Agent implementation brief was required.
Expanded table-driven run `0bf4661a-53fe-4c8d-95e4-e0d052280c64` passed 38/38 and accounts for all
45 explicit-unsupported pairs plus existing coverage of all 21 handled pairs.
Task 5 closes capability visibility by masking the unsupported `fun`/`cfg` gates, deriving SD-card
availability only from the authoritative `aux` state, and making the unavailable Studio camera path
agree across status and both camera ABIs. Independent Task 4 review re-ran both focused suites
successfully and returned `VERDICT: APPROVE` for the complete matrix and parser behavior.

---

### Task 5: Remove Synthetic Device Capabilities And Preserve Known Telemetry

**Files:**

- Modify/split: `crates/pandar-network-plugin/src/shim_status*.hpp`
- Create: `crates/pandar-network-plugin/src/studio_status/{capabilities.rs,payload.rs}`
- Modify: `crates/pandar-network-plugin/src/{studio_abi.rs,studio_status.rs}` and
  `studio_status/{input.rs,device.rs,list.rs,request.rs}`
- Modify only when a real source field is missing: Hub plugin response types/routes and Agent/Core
  telemetry contracts, with SQLite/PostgreSQL parity if persistence changes.
- Modify: `crates/pandar-network-plugin/tests/studio_status.rs` and compiled Studio parser fixtures.

**Acceptance boundary:** Every advertised network, storage, chamber, bind, and camera capability comes
from an authoritative current observation. Unknown data stays unknown/unavailable.

- [x] Add RED fixtures for no capabilities, partial telemetry, complete telemetry, offline state, and
  stale partial updates.
- [x] Remove fixed `100%` signals, `sdcard:true`, `connect_type:lan`, `bind_state:free`, and universal
  chamber support.
- [x] Carry Hub chamber target temperature through the typed input and packed Studio payload without
  defaulting a known target to zero.
- [x] Define camera protocol and callback availability together. Return a Studio-accepted URL only
  when that path is usable; otherwise advertise no live view and return an explicit unavailable result.
- [x] Move status envelope construction, capability decisions, and camera selection into Rust. Leave
  `shim_status.hpp` with timer/callback adaptation only; it must not build JSON or choose policy.
- [x] Preserve nozzle, bed, AMS, `cfg`, `aux`, `stat`, feature-bitmap, firmware-generation, and partial
  update semantics.
- [x] Run a target Studio parser fixture proving unavailable SD-card/chamber/camera controls stay hidden.
- [x] Obtain fresh independent review approval.

Implementation evidence (2026-07-20): Rust typed status projection now owns the complete
`push_status` envelope, capability masks, SD-card derivation, chamber V1/V2 fields, online MQTT
liveness, local-connect payload, and explicit camera-unavailable result. C++ retains only
cache/synchronization/callback/result adaptation. The status suite passed 18/18 in run
`10c25192-737b-447e-96eb-e5de56513575`; compiled structure, camera, axis/status, and success ABI runs
were `51b96b0e-aa4f-47ac-8602-50b486721e0e`, `458ba55e-c3b7-4662-93de-8199271074dc`,
`2f613744-e8b2-412e-aa40-20b3cc5ae1e5`, and `7dda0040-8173-40f6-9d14-8ce0091f2ca1`.

Agent/Hub telemetry now carries an independent `telemetry_authoritative` bit. Partial reports update
only present fields, while an explicitly requested full refresh may clear stale fields. Focused Agent
and Hub runs `d93f2094-64d2-4e20-bdf5-da69096f2cbd` and
`4f8622e8-0777-4bc5-b81e-b10bde780cbb` passed; snapshot regressions passed 45/45 and 14/14 in runs
`fa50f269-5a22-4255-8cb6-58ca140fb5d2` and `57885994-6ca0-4c15-91d5-b7d4a4badfde`.
The shared repository contract also passed against a real disposable PostgreSQL 16 container on the
user-authorized Linux SSH runner in run `b48cf2e9-ffc5-4f86-b1e6-69d4c550b670`; the container and
isolated `/tmp` source copy were removed afterward. No GitHub Actions were used.

Final Task 5 review also closed the six device-status findings carried forward from the Studio-source
audit: credential/IP redaction, tri-state chamber-lamp projection (including `flashing` as on),
sequence-authoritative `pushall`, compiled pinned Studio status consumption, current-session MQTT
liveness, and model-presence preservation. Fresh focused Agent, Hub, and plugin runs passed 5/5, 5/5,
and 23/23 respectively; the post-review chamber-lamp regression passed 1/1. The independent reviewer
returned `VERDICT: APPROVE` on 2026-07-21.

---

### Task 6: Align Print Parameters, Progress, Cancellation, And Tasks

**Files:**

- Modify: `crates/pandar-network-plugin/src/{lib.rs,shim_firmware.hpp,shim_abi_operations.hpp,shim_abi_content.hpp}`
- Create focused typed modules under `crates/pandar-network-plugin/src/` before any touched production
  file exceeds 400 LOC.
- Modify: Hub plugin routes/responses, job repository/entities, protobuf/Core/Agent print contracts,
  and equivalent SQLite/PostgreSQL migrations only for fields required by the reviewed disposition
  table.
- Modify focused plugin/Hub/Agent tests for print submission, task listing, task detail, and lifecycle.

**Acceptance boundary:** Every target `PrintParams` field has an enforced disposition, Studio progress
does not overstate delivery, and Studio tasks reflect authorized Hub jobs.

- [x] Create the complete field disposition table and RED tests for every nondefault target field.
- [x] Preserve supported fields end to end with typed contracts. Reject unsupported nondefault fields
  before artifact submission; do not silently discard them.
- [x] Trace the target Studio `OnUpdateStatusFn`, `OnWaitFn`, and cancellation call sequence. Document
  the exact Pandar milestone represented by each Studio stage before changing production callbacks.
- [x] Emit `PrintingStageERROR` for pre-terminal Hub/artifact/task failures and never imply physical
  printer start from a mere HTTP acceptance.
- [x] Implement `get_user_tasks` through `/api/v1/plugin/jobs`, honoring device/status/offset/limit,
  tenant authorization, bounded refresh, and stable errors.
- [x] Return real plate/subtask/slice metadata when stored; otherwise return explicit unavailable, not
  empty success.
- [x] Test queued/running/succeeded/failed states, downstream Agent failure, cancellation races,
  pagination, cross-tenant access, and Hub outage.
- [x] Run equivalent SQLite and real PostgreSQL tests. An unset `PANDAR_TEST_POSTGRES_URL` blocks this
  slice's completion claim.
- [x] Obtain fresh independent review approval.

Current Task 6 implementation status: `get_user_tasks` is Hub-backed and no longer returns a synthetic
empty success; lifecycle, cancellation, stable ids, filters/pagination, and typed plate/subtask/slice
behavior are implemented. Final12's Windows clean and real PostgreSQL results are historical; final13
reran both successfully from its frozen input.
The final evidence-document review passed under Task 8.

- Historical frozen final13 source identity: `HEAD` `2ba0d1f2755501ea9e7d4babcf176db40638f643`, archive SHA-256
  `71080abb1e7392b0440a179b5bca9fd80638de74a614105b8dc11a0f70959c34`, canonical tree SHA-256
  `db0b7c3385c29ff0cdee1930a66f554a6845b58907373ef543563b829c245761`, and member-list SHA-256
  `87a6ad1dfaa404731ed30d7e265303cca64fc4278a478f9c12192c09373eb880`.
- Real PostgreSQL 16.14 harness `0c292295-f9ab-459b-89c2-ea74f2c9ff56` ran
  `24b49c19-cd07-42b5-a5a3-6d220345bd7e` and `1f4b8458-6397-4c0b-8ab3-23d37779c68a`; each passed
  55/55 selected cases, with 831 filtered and zero runtime skip markers. Their log SHA-256 values are
  `b123f495e09de3c57c2c175000a37cc1fa7395dd0a9c52f1c2f72426c2f4dc08` and
  `b3e233f50fe1be9df43867e34307fd6193f09a2dc00940318bdfb8827f0a8d54`; normalized evidence SHA-256
  is `7e04ae355f7bca3fb409bbc700b5c8f160194c0d2f9ec82df823c859566a2db7`.
- Windows complete workspace run `90cb6a69-08a5-4421-a661-58e696c374a3` passed 1,778/1,778 with one
  separately reported skip. Clean-gate evidence SHA-256 is
  `c1ac8807a427ae4b7003681e9ad343d668dab1d6aa7c143d14bc699fe58b7b89`.
- Final13 implementation and evidence-document reviews are approved.

---

### Task 7: Close Target Firmware And AMS ABI Deltas

**Files:**

- Modify: `crates/pandar-network-plugin/src/firmware/{parser.rs,model.rs,ffi.rs,http.rs}`
- Modify: `crates/pandar-network-plugin/src/{shim_firmware.hpp,shim_abi_content.hpp}`
- Modify typed Core/Hub/Agent firmware contracts only if WTM cannot use the existing module-generic
  prepare/execute shape.
- Modify firmware parser, FFI, HTTP-boundary, lifecycle, and compiled ABI fixtures.

**Acceptance boundary:** `wtm_upgrade` and AMS sync have explicit target-version behavior without
weakening firmware ownership, secrecy, or replay invariants.

- [x] Add RED target fixture coverage for `wtm_upgrade` and the AMS sync ABI call.
- [x] Select the capability-driven explicit-unsupported WTM branch allowed by the design: clear pinned
  Studio `fun` bit 60 in the plugin projection and reject an injected exact `wtm_upgrade` envelope
  before Hub publish. Do not create a firmware token, package, fallback, or replay path.
- [x] Capability/session mismatch fails before publish with no fallback because the WTM capability is
  never advertised and an injected command is rejected at the typed parser/FFI boundary.
- [x] Claim the immutable firmware request snapshot generation before catalog, refresh, or send I/O,
  and fence completion so an older A/B request cannot publish or overwrite newer state. Keep request
  errors request-owned; do not share a mutable `last_error` across concurrent calls.
- [x] Either implement AMS sync through a reviewed Hub-backed typed contract or preserve the exact ABI
  while returning the documented AMS sync failure. Do not add filament cloud CRUD implicitly.
- [x] Keep the package catalog empty and do not add a live firmware test.
- [x] Make the compiled fixture arm a version-heartbeat callback sentinel and wait for its commit before
  firmware command assertions. The wrapper may accept only the exact stale-generation diagnostic and
  must reject every other stderr line.
- [x] Run focused firmware suites, full plugin/Hub/Agent suites, and fresh independent review.

The selected Task 7 boundary passed a pinned focused review on 2026-07-21. The compiled Studio
consumer proves that raw `fun` bit 60 exposes the nozzle-rack capability and the Pandar projection
hides it; exact injected WTM JSON fails before Hub I/O; and the by-value AMS ABI returns stable `-32`
with a redacted body. Focused Nextest passed 3/3 in run
`ff59e38f-ca9d-4fa9-89f0-367655947cde`, and the independent reviewer returned
`VERDICT: APPROVE`. The compiled A/B barrier additionally proved generation ownership before I/O for
catalog, refresh, and send, with no shared `last_error`. Historical final12 passed its Windows clean
gate after the initially failing firmware probe passed both in isolation and in the required complete
rerun, but its later Linux run exposed the Task 3 background-refresh race. Final13 Linux verification
is complete through native, sanitizer, and exact-AppImage load/no-auth recovery. The historical final12 logout/ABI review found no issue;
the separate persistence review approved the code with two documented Minor limitations rather than a
zero-Minor cross-cutting claim.

The pre-final stress sequence must remain visible rather than being collapsed into a generic flake.
Final12 Windows full first failed with `firmware version refresh failed`, then passed the exact probe
and required full rerun. Final12 Linux later ran the C++ fixture successfully but failed its wrapper on
the exact stale-generation diagnostic. After the callback-sentinel fixture change, Windows stress
iteration 2 failed with `firmware callback missed handoff deadline`, which drove the background stale-
while-revalidate product repair. After that repair, six iterations passed and iteration 7 reported
`status callback logout deadlocked against firmware dispatcher`. The independent wait-for graph found
no ABBA cycle: the dispatcher releases the firmware-transition lock before waiting for the callback
mutex, and callback dispatch does not hold the account-queue lock. The old three-second compound
watchdog left only about 1.6 seconds after its fixed 1.4-second callback delay, so the test now separates
the start and logout assertions, waits eight seconds internally, and gives the child 45 seconds. These
are diagnostic/stress facts, not final13 gate completion.

---

### Task 8: Real Studio Evidence, Canonical Docs, And Delivery

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/pandar-bambu-source/Cargo.toml`
- Create: `crates/pandar-bambu-source/src/lib.rs`
- Modify: `crates/pandar-app/src/main.rs`
- Modify: `crates/pandar-network-plugin/src/installer.rs`
- Modify: `docs/compatibility/bambu-studio-plugin.md`
- Modify: `docs/compatibility/bambu-studio-plugin-smoke.md`
- Modify: `docs/architecture.md`
- Modify: `docs/development.md`
- Modify: `docs/roadmap.md`
- Modify as needed: `tools/release-smoke/src/` and its tests
- Maintain but do not stage: `.superpowers/sdd/progress.md`

**Acceptance boundary:** Documentation reflects reviewed implementation and real evidence exactly. No
unsupported platform or function is promoted to passed.

- [x] Add the same-target `pandar-bambu-source` companion required by pinned Studio's startup gate.
  Require it in `install-network-plugin`, install it under Studio's exact platform filename, export one
  Pandar sentinel and no `Bambu_*` camera/media entrypoint, and preserve Studio's fake-source fallback.
- [x] Define the native candidate build policy for CLI, network plugin, and companion. Build the Windows x86-64
  libraries on a Windows
  runner with `x86_64-pc-windows-msvc`/MSVC; use native Apple Clang for macOS; document and enforce
  the official Studio-compatible compiler/libstdc++ ABI for Linux. Do not publish a claimed-compatible
  plugin for an unverified architecture.
- [x] Make release smoke inspect and execute the packaged plugin artifact on its native runner, not a
  separately compiled development library. Require the companion in the exact archive layout, hash it,
  require its sentinel, and reject `Bambu_*` exports while ABI-probing only the network plugin. Record
  Rust/C++ toolchains, runtime ABI, and SHA-256 for both libraries.
- [ ] Run the no-print desktop smoke on every platform Pandar will claim for exact Studio
  `02.08.01.55`, recording exact Studio/Hub/Agent versions, current `HEAD`, the exact dirty source
  snapshot SHA-256, artifact hashes, and redacted evidence.
- [ ] Verify load/version/agent creation, login/ticket/token/profile, printers, Hub-backed tasks, Hub
  outage/recovery, logout, and explicit unsupported results that require no machine action.
- [x] Keep automated print-field/lifecycle/cancellation/command evidence separate. A safely authorized
  hardware print, cancel, or command run is optional later evidence and is not a completion condition
  for the no-print desktop smoke. Never flash firmware in this verification.
- [x] Update the existing manifest rows. A platform without evidence remains `blocked` or `untested`.
- [x] Update architecture/development with the final command/status/print ownership and exact rollout/
  rollback sequence. Update Phase 23 and Immediate Next without inventing a new roadmap phase.
- [x] Run the final evidence-document review. The reviewer checked the design, plan, pinned contract,
  previous findings, exact diff membership, and completed evidence without promoting untested
  authenticated Linux, real Windows Studio, macOS, hardware, or live-firmware surfaces. Its sole Minor
  terminology finding was corrected.
- [x] Freeze final13 and rerun the complete Windows workspace, PostgreSQL, and Windows native gates from
  that immutable input.
- [x] Complete corrected Ubuntu-native attempt 2 and promote sanitizer results only from the wholly
  successful harness. Preserve attempt 1 as non-promotable infrastructure history.
- [x] Run the exact-AppImage gate from the same immutable input and passed final13 Linux archive.

### Task 9: No-Auth Session Recovery And Credential Hygiene

**Files:**

- Modify: `crates/pandar-network-plugin/src/shim_no_auth.hpp`
- Modify as needed: `crates/pandar-network-plugin/src/shim_status_heartbeat.hpp`
- Modify: `crates/pandar-network-plugin/src/shim_tasks.hpp`
- Modify as needed: `crates/pandar-hub/src/routes/plugin.rs`
- Modify matching focused tests in `crates/pandar-network-plugin/tests/` and
  `crates/pandar-hub/src/routes/tests/plugin/`
- Modify: the compatibility, development, and roadmap documents named in Task 8

**Acceptance boundary:** Repair only the confirmed development-session gaps. Do not turn no-auth into
an authenticated-session claim, and do not add machine access or hidden fallback behavior.

- [x] Preserve the final5 official-AppImage failure/success pair as historical regression evidence.
  With both libraries loaded, starting Studio before Hub and then recovering Hub left
  login/token/audit counts at zero for 30 seconds; restarting only Studio created exactly one redacted
  `plugin:studio` session. Historical redacted evidence bundle SHA-256:
  `7f103873d222b8b51e1209c4836f2acc2579515cff9729dd89c4271032e801b0`. Final11 later proved same-PID
  recovery, but that package is historical. Final13 attempt 8 independently proved its then-current
  package's same-PID development no-auth recovery and is now historical; final14 attempt 1 supplies the
  corresponding current Linux package evidence with `authenticated_session_claim=false`.
- [x] Add bounded in-process retry only for a connection-stage failure that the Rust HTTP boundary can
  prove occurred before request delivery. Serialize it with account mutation; fence logout, destroy,
  Hub-configuration, token, and account-epoch changes; never retry an ambiguous timeout/response-loss
  or HTTP response. Prove repeated wakeups leave exactly one server token, token-create audit, persisted
  profile, and Studio callback.
- [x] Require exactly one tenant for no-auth bootstrap. The repository list is already ordered; do not
  silently turn its oldest item into the development-session tenant. Return a stable conflict and
  create no credential or audit when multiple tenants exist.
- [x] Define and implement the server-token disposition for Studio logout. If self-revocation is the
  contract, make it tenant-safe, idempotent, redacted, and covered on SQLite and PostgreSQL.
- [x] Serialize every persisted account mutation across processes with
  `.pandar-plugin-account.lock`. Accept login, pending-revocation, and direct-intent changes only after
  `MutationDurability::Confirmed`; treat `ChangedUnconfirmed` as published but not durability-proven.
- [x] Before an unstaged requested DELETE, persist a direct-revocation intent. On success or idempotent
  `401`/`410`, record a completed tombstone with canonical Hub URL plus token SHA-256 before clearing
  matching login/intent state, so stale processes cannot restore the revoked login.
- [x] Apply the same `401`/`410` no-auth refresh-and-single-retry semantics to Studio task list/detail
  requests that printer refresh already uses, with stale-account rejection.
- [x] Rerun focused probes, the complete Windows workspace gate, and both database backends where
  persistence changes. Final13 Windows run `90cb6a69-08a5-4421-a661-58e696c374a3` passed 1,778/1,778;
  both PostgreSQL 16.14 runs passed 55/55. Fresh independent code review returned `APPROVE` with no
  Blocking, Important, or Minor finding.
- [x] Rerun the exact-AppImage no-auth recovery sequence with the final13 packaged Linux candidate.
  Attempt 8 passed; final11 is retained only as historical regression evidence. The final evidence-
  document review passed under Task 8.

Task 9 completion semantics are intentionally narrow. No-auth retries only a proven pre-delivery
connection failure, with at most five attempts including the initial request, a two-second initial
backoff, a 30-second cap, and token/account/configuration generation fences. Multiple tenants fail
closed with HTTP `409` `ambiguous_no_auth_tenant`.

Persisted login, pending, direct-intent, and completed-revocation files are serialized by a process-
local lock plus `.pandar-plugin-account.lock` across Studio processes. Only
`MutationDurability::Confirmed` may activate a candidate or count an intent as staged.
`ChangedUnconfirmed` means the namespace change was published but directory durability could not be
confirmed, so callers fail closed. An ordinary error describes the canonical namespace visible at
return and does not promise crash durability during an extreme rollback failure.

Requested logout first attempts a confirmed pending self-revoke. If that cannot be confirmed, a
confirmed direct intent is required before DELETE. Passive account loss does not revoke; a concurrent
requested transition upgrades passive finalization without duplicate callbacks. A retained account
can be fully restored only on a safe failure before DELETE, never after DELETE is attempted. Successful
DELETE and idempotent `401`/`410` record a `{hub_url, token_sha256}` completed tombstone before cleanup,
which blocks stale login loads and writes. Duplicate pending cleanup after direct success is best
effort. The completed ledger is currently unbounded and may be cleared only after every Studio process
using the data directory is stopped and every older Hub plugin session is invalid or expired. Task
list/detail/plate/subtask share at most one `401`/`410` rotation/retry, while authenticated accounts
never fall back to no-auth.

### Task 10: Post-Final13 Capability Honesty And Better Auth Return Intent

**Files:**

- Modify: `crates/pandar-network-plugin/src/studio_status/capabilities.rs`
- Modify: pinned Studio projection and ABI tests under `crates/pandar-network-plugin/tests/`
- Modify: `frontend/app/plugin-sign-in/` and `frontend/app/auth/betterauth/`
- Modify: `frontend/auth/` return helpers, sign-in/completion pages, and tests
- Modify: compatibility, architecture, development, release, and roadmap documents

**Acceptance boundary:** Hide only the pinned Studio capability whose downstream print parameter is
still rejected, and preserve only the exact plugin return intent across Better Auth. Do not advertise
change-assist support, widen the callback allowlist, place a JWT in `return_to`, or promote final13 as
evidence for code it does not contain.

- [x] Clear pinned Studio `fun` bit 48 in the Studio projection while preserving other supported and
  unknown bits; keep `task_ext_change_assist=true` rejected before Hub I/O.
- [x] Pin and compile the `DeviceManager.cpp:4393` consumer and prove Cloud/LAN projection plus all 45
  print-field dispositions.
- [x] Carry tenant plus the exact Studio localhost callback as a bounded canonical base64url value
  through magic-link, direct passkey, optional passkey completion, and dashboard callback hops.
- [x] Fail closed on malformed, overlong, non-canonical, invalid-path, cross-origin, backslash, and
  fragment values; keep the JWT out of the return target.
- [x] Exercise the real Better Auth 1.6.23 magic-link handler and cross-app codec interoperability.
- [x] Obtain independent implementation review approval.
- [x] Freeze the post-final13 successor and rerun required workspace, frontend, native Linux, and
  exact-AppImage gates before naming a current candidate.
- [x] Complete a fresh evidence-document review after the successor results are recorded. The final14
  review returned `APPROVE` with no Blocking, Important, or Minor finding after correcting warning and
  candidate-identity wording.

### Task 11: Pinned Model-Task `get_subtask` Consumer

**Files:**

- Modify: `crates/pandar-network-plugin/src/shim_abi_content.hpp`
- Modify: Rust model-task FFI and typed task projection under
  `crates/pandar-network-plugin/src/studio_print/`
- Modify as needed: backend-neutral Hub task repositories/routes plus paired SQLite/PostgreSQL tests
- Add: pinned compiled consumer coverage under `crates/pandar-network-plugin/tests/`
- Modify: print-contract, plugin compatibility, plan, and roadmap documents

**Acceptance boundary:** This is not `bambu_network_get_subtask_info`. Use real authorized task
metadata to fill Studio's caller-owned `BBLModelTask`; do not invent model/design/profile identifiers,
return empty success, or invoke the callback after failure.

- [x] Pin `StatusPanel.cpp:4145-4162`, the target `BBLModelTask` layout, and the NetworkAgent forwarding
  boundary from exact Studio `02.08.01.55`.
- [x] Add a compiled RED consumer proving the current 501/no-callback gap.
- [x] Define the minimum real task metadata and backend-neutral query needed by the pinned consumer.
- [x] Fill the caller-owned object and invoke its callback exactly once only after successful,
  current-account, authorized retrieval; missing data remains explicit non-success.
- [x] Prove SQLite/PostgreSQL parity, tenant isolation, stale-session rejection, and no fake metadata.
- [x] Run focused and full gates and obtain independent review before updating evidence.

The exact pins are Studio commit `ba049f6a2e08c3b6033660bb84da80c08722974b`, `StatusPanel.cpp`
blob/excerpt `86d2306f9c9462b241943325788a9056c6e3be8b`/`a9156f1c3f9e2fd40ae7215aeb4d26b172021f4e`,
`ProjectTask.hpp` blob/layout/callback excerpts
`6c9196c5e1278370f37d84fa98a2a1a7cfbabd14`/`afc8d530b4723eec0971824609ac6d3dae3bb58d`/
`15226032d4729ce90e5940d3cc41f70b60bd304d`, and `NetworkAgent.cpp` blob/excerpt
`f4a19cdffdb6242d1e27cc92778c9238c1f67e3e`/`7790222a37d042f2704cfb0fd78f01021aec52fe`.
The baseline Rust path returned explicit 501 unavailable; through the exact compiled ABI the observed
gap was return `+1`, zero callbacks, and an unchanged caller object.

The implemented Hub response has exactly `job_id`, `design_id`, `profile_id`, `instance_id`,
`task_id`, `model_id`, `model_name`, and `profile_name`. An ordinary submission uses its stable
positive Studio id for job/task, `0/0/0` for design/profile/instance, empty `model_id`, and real
nonempty project/preset names. `instance_id=0` is the explicit no-rating sentinel, never a repurposed
submission id. Any MakerWorld marker or unusable metadata returns
`409 studio_model_task_metadata_unavailable` and produces no callback.

A valid ABI call returns asynchronous admission `0`; HTTP completion is not reported synchronously.
Success updates the same caller-owned pointer and invokes its callback once. HTTP 409/404, malformed
2xx, and stale account/configuration leave the object untouched and invoke no callback. Cancellation
or destroy observed before the response/callback gate has the same outcome. Destroy waits for a
callback that already won the gate, and the final callback/account fence guarantees no callback after
destroy returns. Cancellation interrupts pending initial/retry GET, no-auth POST through its
response body, pending/direct revocation DELETE, and same-key follower waits. Once a successful
no-auth response is available, persistence and rotation/revocation bookkeeping drain to a consistent
state. Server delivery before that response is the ordinary unknowable HTTP-create outcome; the same
in-process create is not automatically retried, and cross-process locking/fsync has no hard real-time bound.

RED run `56f7e205-52a9-4878-abda-d3902d4f294d` exposed the half-open no-auth response destroy gap;
GREEN/destroy/compiled runs `aa6f7193-e699-45b8-9c92-8574fcf11d37`,
`ec498a26-c83a-4156-8319-f4b0f0f1851d`, and `9f40d58b-416f-4c0d-b1ba-0fdb5b0d549a` passed 1/1,
2/2, and 4/4. Local workspace run `d8622da6-4458-407d-8ae6-48ee8d0ac27b` passed 1,800/1,800
with one skip. The authorized Linux SSH host passed workspace run
`67858341-820f-42d7-9a8d-a408b03e6d3d` at 1,801/1,801 with one skip, PostgreSQL 16 run
`f3fef6c4-dcb9-46f1-9812-43040510eca4` at 7/7, and GCC compiled-task run
`6fe5e158-4b98-425f-b6ef-578624937801` at 17/17. Fmt and strict workspace Clippy passed locally
and on Linux. Final independent review found no Critical, Important, or Minor issue and returned
literal `VERDICT: APPROVE`. No Action, real Studio, authentication, printer, or firmware action was
used.

The three-file release-smoke implementation is complete. Final native candidates are built only by
the documented same-OS native commands; the existing tag workflow's GNU Windows/two-file path is
legacy Phase 24 behavior, is incompatible with this target contract, and is not a current
`02.08.01.55` delivery path. It was neither modified nor run for this Goal. The intended native-smoke
scope is `linux-amd64` and `windows-amd64`; macOS, Windows real-Studio execution, and hardware
print/control remain untested.

- Current final14 source is frozen at `HEAD 2ba0d1f2755501ea9e7d4babcf176db40638f643`.
  `pandar-bambu-final14-019f7b10.tar.gz` is 2,782,539 bytes with 1,548 regular members and SHA-256
  `c422d80d89052732db6b8ae87b68fd1e4145c64f588d8382deafef3345d86681`; canonical-tree,
  member-list, and freeze-evidence SHA-256 values are
  `43a4a577fb90327dad9e59bcb89dc1e91352bad83f27786a32cae34cb62136e5`,
  `5b32472c9372a992c23315d9b33691a0f269248b65db312590ed00556e21aac0`, and
  `70d545770086c6acde271d3181508adf4f0d91fc8213771363ec78b2792f5ec3`. Determinism and all
  unsafe/duplicate/case/reparse/content-diff checks passed.
- Final14 pre-freeze frontend evidence passed Web 38 files/327 tests, standalone auth 3 files/9 tests,
  both typechecks, zero-warning Web lint, both production builds, and callback smoke. Immutable-source
  Ubuntu validation passed fmt, strict workspace Clippy, module-size 2/2, release-smoke
  21/21, and workspace Nextest run `d2231751-1284-46b0-aee6-2e041ca1a203` at 1,781/1,781 with one
  separately reported skip in 812.413 seconds. The 109-network-plus-21-FT contract passed all five ABI
  modes, and 21 File Transfer entrypoints x 256 ASan/LSan cycles completed without sanitizer errors.
  Rust `-D warnings` passed; the retained Clippy log still contains C++ build and dependency
  future-incompatibility warnings.
- Final14 Linux archive `pandar-final14-linux-amd64-019f7b10.tar.gz` is 24,854,111 bytes with SHA-256
  `4e91f2457197532102544b02d4edac5354dc2982ec55fa707a057cbcba518b68`; its 202,300-byte evidence
  bundle has SHA-256 `db6a464ce6b9b4b5e4689e1f0f21962dd097349056e78beb57a8779e1352cb02`.
- Final14 official-AppImage attempt 1 passed with fixed AppImage SHA-256
  `e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995`. Studio retained the same
  PID/start-ticks identity, both libraries mapped 4/4, loader/certificate error counts were zero, and
  one development no-auth session was observed. The 10,603-byte, 23-member redacted evidence bundle
  has SHA-256 `7eac6abbc7364928147d60dd1c583d084c02debf1552734bc82a4dec59c941be` and records
  `authenticated_session_claim=false`; it does not fill the Better Auth WebView/ticket/session rows.
- Final14 remains an active candidate rather than a completed compatibility claim. Its archive
  predates Task 11. A newly frozen candidate and real Studio model-task evidence, authenticated Better
  Auth Studio UI, real Windows Studio, macOS, hardware, and live-firmware gates remain open.
- Historical final12 source archive `pandar-bambu-final12-019f7b10.tar.gz` is 2,740,698 bytes with 1,543 regular
  members, archive SHA-256 `17371828ef7a26cace73cfbed321d094bf38323670e8fa6ccf69d6cbfd4b7eee`,
  canonical tree SHA-256 `5aa0038dbc3f0962cc172646876263b0db04e1e6df5fbe571553af1967f242a6`,
  and member-list SHA-256 `87a6ad1dfaa404731ed30d7e265303cca64fc4278a478f9c12192c09373eb880`.
- Historical Windows final12 archive `pandar-final12-windows-amd64-019f7b10.tar.gz` is 21,285,799 bytes with
  SHA-256 `b4f6913eef7c1d09da9377fbce36b0ab759add25caac2baa0604c07a595440cb`.
  CLI, network-plugin, and BambuSource SHA-256 values are respectively
  `1e57a7cfc2b46717129e7ced227b358eedbaaa74064f2ae2ac5cd44eac576b32`,
  `43be9e73350cacb66ee2dfa991f1a7291175c4d18db2ec917a10a1489f9244d9`, and
  `20805176609ebe891ed45bc7171a34ad0d741351b5dbe8c3c4d9f9b4a5a2a49a`. Build run
  `4fa89d78-503f-4c51-a4e3-fc788a4f7f03`, ABI run
  `6b71c048-8377-4a61-a750-20c5531df864`, and packaged release-smoke run
  `d808cce0-6e5f-45e7-b4aa-f7b39642d67a` passed. The audited target-prefix set contains exactly 109
  network plus 21 File Transfer exports; all five caller modes passed, and the companion exposes one
  Pandar sentinel with zero `Bambu_*` exports. Consolidated Windows-native evidence SHA-256 is
  `11c38eb3c198cd07b2f96abbfbf70792b078170389e8869b230badbb98a404d2`.
- Historical final13 source archive `pandar-bambu-final13-019f7b10.tar.gz` is 2,751,227 bytes with 1,543 regular
  members, archive SHA-256 `71080abb1e7392b0440a179b5bca9fd80638de74a614105b8dc11a0f70959c34`,
  canonical tree SHA-256 `db0b7c3385c29ff0cdee1930a66f554a6845b58907373ef543563b829c245761`,
  member-list SHA-256 `87a6ad1dfaa404731ed30d7e265303cca64fc4278a478f9c12192c09373eb880`,
  and freeze-evidence SHA-256 `4d132e16f91365795f54c97f608483c34b55726c5f614f5bb8ffaac2ede1fb7f`.
  Determinism passed and every unsafe/duplicate/case/reparse/diff count was zero. Pre-freeze plugin run
  `da32fbc4-f37e-4198-af5e-c35f73512dcb` passed 368/368 with one separately reported skip.
- Historical final13 Windows complete run `90cb6a69-08a5-4421-a661-58e696c374a3` passed 1,778/1,778 with one
  separately reported skip in 1,050.084 seconds; firmware took 28.858 seconds. Fmt, strict Clippy,
  module-size 2/2, both tools 21/21, and frontend 37 files/324 tests plus typecheck/lint/build passed.
  `npm ci` recorded six audit vulnerabilities (three moderate, three high), retained as dependency-
  audit evidence. Clean evidence SHA-256 is
  `c1ac8807a427ae4b7003681e9ad343d668dab1d6aa7c143d14bc699fe58b7b89`.
- Historical final13 PostgreSQL harness `0c292295-f9ab-459b-89c2-ea74f2c9ff56` ran
  `24b49c19-cd07-42b5-a5a3-6d220345bd7e` and `1f4b8458-6397-4c0b-8ab3-23d37779c68a`; each passed
  55/55 with 831 filtered and zero runtime skips. Per-run log SHA-256 values are
  `b123f495e09de3c57c2c175000a37cc1fa7395dd0a9c52f1c2f72426c2f4dc08` and
  `b3e233f50fe1be9df43867e34307fd6193f09a2dc00940318bdfb8827f0a8d54`; normalized evidence SHA-256 is
  `7e04ae355f7bca3fb409bbc700b5c8f160194c0d2f9ec82df823c859566a2db7`.
- Historical final13 Windows archive `pandar-final13-windows-amd64-019f7b10.tar.gz` is 21,285,752 bytes with
  SHA-256 `6c50e77a0b4008ce46d86de51411117061c5118e18849ca1fb94f4a3f319db64`. ABI and release-smoke
  each passed 21/21, all five modes and the 109+21 contract passed, `dumpbin` reported 271 total plugin
  exports, and companion inspection found one Pandar sentinel and zero `Bambu_*` exports. Consolidated
  evidence SHA-256 is `3dab4bffa359e4c46eec77cbfb278ce3a1497f806a1d80343a1735b5a68f025b`.
  Build, ABI, and packaged-smoke runs were `0430ad0e-7f96-41c5-b9aa-1c6fd690fd16`,
  `2f27f859-b795-4420-b04a-30410ae7bcbc`, and `65ffc0b0-e17e-45da-bd3a-3375f5d88de1`; CLI,
  plugin, and companion SHA-256 values were
  `a73fbe47a56fd557f14912e0e774007e0a4774ce83f250ce2a9cc41e52da8d57`,
  `7861e454eb9dd6122eabc6252102de462228eec526f47149e00a037d3dc48eba`, and
  `eaf98016c7d38cb6121a525a0f7a5bb5f0c59df333722798c5f76cee279fdfe6`.
- Historical final13 Linux attempt 2 run `6ec3a215-9430-4ad2-adc7-f692ca156333` passed 1,779/1,779 with one
  separately reported skip in 792.687 seconds; firmware passed in 27.315 seconds. Fmt, strict Clippy,
  module-size 2/2, ABI-tool 22/22, release-smoke-tool 21/21, all five ABI modes, exact three-file
  package/runtime audit, and 21 File Transfer entrypoints x 256 ASan/LSan cycles passed. Archive
  `pandar-final13-linux-amd64-019f7b10.tar.gz` is 24,854,768 bytes with SHA-256
  `4166e6012e6c1bf7cdf056ba3bfb28f0fbc9d216c31e5ed2e8620adb8b5fcccc`; CLI, plugin, and companion
  SHA-256 values are `7c44138d559ee62d02d4ac7fe0c23c7091e99a7782aac8a163a0c3565458d77f`,
  `f9baf8346901fdc2ba20aeee786029e47af495bad3ee2e754f440db89010be24`, and
  `88d34358be39ed3d239aeb317df8f34a92d4652877e86a9849c66e32347c1df2`. Evidence-bundle SHA-256 is
  `aa7478fe0f74debcc5f3d1f5ec53a2222d726beafe5224935aa3382c24f6097a`.
- Historical final13 Linux attempt 1 run `c8a134c4-e775-4f37-b6ed-74ccb1b79123` is non-promotable harness
  history: product gates were green, but its wrapper expected 21 total exports from an FT-only
  invocation while the checker reported the full 130-name library contract, so overall exit was 1.
  Final11/final12 Linux/AppImage hashes remain historical and do not fill the final13 field.
- Historical final13 exact-AppImage attempt 8 passed with official Ubuntu 22.04 Bambu Studio `02.08.01.55`
  AppImage SHA-256 `e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995`, runner seed SHA-256
  `72b7d020ef537c7bd510910086d9dcafd3ad0e38e24614216630e27767a46be0`, fresh `AppRun` SHA-256
  `eaf5a1c6ff4f0d49d6e0c0bacf106309daa2c822ca1ebe8739067699e6cdaef4`, and the passed final13
  Linux package. Studio PID `137`/ticks `192688662` remained unchanged through two offline failures
  and one success/commit after Hub PID `674`/ticks `192689166` became ready. Both libraries mapped
  4/4; active/total token count was `1/1`; create/revoke/discard counts were `1/0/0`; and loader/
  certificate-error counts were zero. The 7,211-byte, 23-member redacted evidence bundle SHA-256 is
  `a4453c8dce3829cc1a84a372a772b516812fe1564b310e61db9e9009a11cf9d2`; manifest, member-list,
  and hashes-file SHA-256 values are `7ef2a8547ba767f5d0be174b491fa40c2946a0add71adb4043a9abe8d54c1a8a`,
  `d79e3f0b6b3672241324a11a2b7f7d8d727c464303f11cbe8745c4f8e60e496f`, and
  `ee623a39f5db110b9c26076bdb9a9b440404170402cc3fe840e402cdce2ee1a9`. External plus 21 internal
  hashes passed; raw state, database files, and login content were not retrieved. Attempts 1-7 remain
  locale/data-directory/first-run harness history. This proves exact module load and development no-
  auth same-process recovery only; authenticated UI/session, printers/jobs/print/logout, unsupported
  UI, hardware, and firmware remain untested.
- A pre-final Linux tree with manifest SHA-256
  `668f541a8e535018495d8a8969fa6a6d5b70daef49ed848c4c03ab19c40e4f9a` and source-archive SHA-256
  `e8c4d17505e9102b7f9fa3fbce8e653dddc7277b33f02671f603818fc1580b3b` passed the exact firmware
  probe 21/21. This is behavioral stress evidence only, not a final13 freeze or complete gate.
- Final13 implementation review returned `APPROVE` with no Blocking, Important, or Minor finding; the
  production delta from final12 touched only four Rust connection files, changed no C++ ABI, and kept
  `connection.rs` at 388 lines. The historical persistence review returned `VERDICT: APPROVE` with two
  Minor limitations: the completed ledger is unbounded, and an ordinary rollback error does not claim
  crash durability. The final evidence-document review completed after correcting its sole Minor
  terminology finding.

Mandatory final verification:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
cargo test -p pandar-core --test module_size
# Run the frontend lint/test/typecheck/build gates only when frontend behavior changes.
git diff --check
git status --short
```

Frontend commands are required when a Studio-facing capability also changes Web behavior; otherwise
record them as not applicable with the exact unchanged boundary. PostgreSQL verification is mandatory
for database-dependent slices.

Historical final12 Windows verification from the immutable archive passed `cargo fmt --all -- --check`, strict
workspace Clippy with zero warnings, module-size 2/2, ABI-tool tests 21/21, release-smoke tests 21/21,
and workspace Nextest 1,776/1,776 in final run
`5e6f3720-4c1b-4a55-ac34-2250c0cefba7`. The first complete attempt had one firmware-probe failure;
that probe passed in isolation and then passed again in the required complete rerun, so that historical
Windows gate is `passed_after_full_rerun`. Clean-gate evidence SHA-256 is
`a6fc922b5069c78dcbe077f6c4238794777f6b17b62574ee75638c46256fb342`. The frontend lockfile was
verified by `npm ci` of 897 packages; 37 test files and 324 tests passed, followed by typecheck,
zero-warning lint, and production build. Real PostgreSQL verification passed the 55/55 filtered cases
named above. The source archive contains 1,543 regular members and has member-list SHA-256
`87a6ad1dfaa404731ed30d7e265303cca64fc4278a478f9c12192c09373eb880`; an independently generated
second archive matched byte-for-byte. That candidate became non-promotable when later Linux stress
  exposed the background-refresh race.

Historical final13 verification passed `cargo fmt --all -- --check`, strict workspace Clippy with zero
warnings, module-size 2/2, ABI/release tools 21/21, frontend 37 files/324 tests plus typecheck/lint/
build, workspace Nextest 1,778/1,778 with one separately reported skip, two PostgreSQL 55/55 runs,
the Windows MSVC native package gate, and corrected Ubuntu-native/ASan attempt 2 at 1,779/1,779.
Linux attempt 1 remains a non-promotable harness failure; exact-AppImage attempt 8 passed the narrow
load/development no-auth recovery gate. No GitHub Action was added or run.

Current final14 verification passed its immutable source freeze, pre-freeze frontend gates, Ubuntu-
native fmt/strict Clippy/module-size/workspace Nextest, three-file release package and smoke, all five
ABI modes, C++ File Transfer ASan/LSan, and official-AppImage exact-load/development no-auth attempt 1.
This completes Task 10. Task 11's working-tree implementation and focused automated evidence are
complete, its final local/Linux automated gates passed, and independent review returned
`VERDICT: APPROVE`. The AppImage evidence records
`authenticated_session_claim=false`; Task 8's real desktop rows, a new frozen candidate with real
Studio model-task evidence, and the platform/hardware boundaries above keep the Goal active.

#### Post-Task 11 Evidence Addendum: Selected Target and Final16

This 2026-07-23 addendum preserves the approved Task 11 body above while recording the later
selected-target correction and final16 evidence. It supersedes all final14 current-status language
above.

- [x] Define the effective cloud target as selected or explicitly subscribed, with heartbeat planning
  over the deduplicated union.
- [x] Preserve a target when either ownership source remains. Only absence from both sources retires
  cloud state and Cloud delivery tickets; cloud retirement never cancels or advances a Local
  generation or Local ticket.
- [x] Preserve final15/run6 as non-promotable historical evidence. That run selected the synthetic
  printer, but Studio's single-device path did not add an explicit subscription, so the pre-correction
  plugin did not deliver the fixture transition or request the model task.
- [x] Freeze final16 from source archive SHA-256
  `24b45dd30c3509c02b609548409f05fa72490512525621dbc0574a05aa62a039` against exact Studio
  commit `ba049f6a2e08c3b6033660bb84da80c08722974b`, version `02.08.01.55`.
- [x] Pass immutable Linux verification: workspace Nextest 1,808/1,808 with one configured skip, fmt,
  strict workspace Clippy, module-size, ABI tools 22/22, release-smoke tools 25/25, packaged tasks
  18/18, exactly 109 network plus 21 File Transfer exports, all 21 File Transfer entrypoints x 256
  ASan/LSan cycles, and PostgreSQL 16.14 at 7/7 with zero runtime skips. The release archive SHA-256
  is `023dcad198674c8ad1c20eb9bc34df9ef9685f49dfeca6e6b5ea58188f3a24a3`; the Linux evidence
  archive SHA-256 is `fe35290675aac4e6ce323a8ebc75bde1c34d373b1df7506f7f8a65b69ffea950`.
- [x] Pass the bounded official-AppImage proof using AppImage SHA-256
  `e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995` and packaged plugin
  SHA-256 `3bcce9085205d6af67dc9671cf58cd6f9fb694d5a587b43d160dc8b6a9b0712f`. The fail-closed
  loopback mock observed exactly one model-task HTTP 200 and four lifecycle events exactly once and
  in order: request started, response accepted, callback started, callback returned. The redacted
  evidence manifest SHA-256 is
  `c6ba9b6282581119d3baec720e26990ad63efc20eb394b0c71dced89081d5fd9`. The deterministic
  245,225-byte redacted official-AppImage evidence archive SHA-256 is
  `f07c369ad9e0354ef40142294d9385e9c454fd534a04badce4be000f49c06eca`; an independent second
  generation matched byte-for-byte.
- [x] Record the claim boundary: synthetic persisted authenticated-shaped session and fail-closed
  loopback mock only; no downstream encrypted-log claim and no real authentication, Hub, Agent,
  database, hardware, print, control, cancel, or firmware action. GitHub Actions and Windows Studio
  were not used.

Final16 is current and its bounded Linux evidence chain is complete. Independent final
evidence-document and code/evidence reviews found no remaining issue and returned `APPROVE`; Codex
Goal `019f7b10-9262-74e1-aa9c-ba18a29beb2a` is complete.

## Final Completion Gate

Before marking the Codex Goal complete:

- re-read every completion criterion in the design and link it to test or real evidence;
- confirm the plugin advertises only version families with real evidence;
- confirm Direct LAN, FT, MakerWorld/cloud settings, package hosting, and other non-goals remain
  explicit and honest;
- confirm no target command or ABI surface returns undocumented success;
- confirm all claimed platforms have manifest rows and artifact hashes;
- for an uncommitted delivery, record the current `HEAD` plus the SHA-256 of the exact dirty source
  snapshot used by every native build; name a commit only after that commit actually exists.

Final gate result: passed. The current `HEAD`, exact final16 dirty-source snapshot, platform rows,
artifact hashes, unsupported surfaces, and bounded real-Studio claim were rechecked. Independent
evidence-document and code/evidence reviewers both returned `APPROVE`, and the code/evidence reviewer
independently reran the four selected-target regressions successfully.
