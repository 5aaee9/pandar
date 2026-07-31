# Bambu Studio Plugin Smoke Runbook

## Scope

This runbook records real Bambu Studio compatibility evidence for Phase 23. The current exact target
is commit `ba049f6a2e08c3b6033660bb84da80c08722974b`, Studio `02.08.01.55`, network agent
`02.08.01.52`, with the ABI caller built against Boost `1.84.0`. A successful local ABI or release-
smoke probe is not a real Studio compatibility claim.

## Prerequisites

- A running `pandar-hub` reachable from the desktop host.
- A running `pandar-web` with external auth configured.
- A tenant with at least one user who can create plugin login tickets.
- A verified plugin sign-in path: open the Pandar web UI, authenticate, select the tenant, and confirm `frontend/app/plugin-sign-in` can create a short-lived plugin login ticket before replacing the Studio plugin.
- A linked `pandar-agent`.
- Optional, separate hardware evidence only: a real printer connected through the agent. A hardware
  print/cancel/command is not required for this no-print desktop smoke.

The historical 2026-06-24 local-host search found no Studio binary. The Linux procedure uses the exact
official Ubuntu 22.04 AppImage in an isolated directory on the user-authorized SSH host. Final16 is the
current verified Linux compatibility baseline. Its native full matrix passed, and its controlled
official-AppImage harness proved the automatic selected-only model-task request/callback boundary.
Final14 remains historical module-load/development-no-auth evidence. Final15 is non-promotable
pre-correction evidence. Authenticated Better Auth sign-in/session and UI rows, real Hub/Agent/database
integration, real Windows Studio, macOS, hardware actions, print/control/cancel, and live firmware
remain untested; downstream behavior in Studio's encrypted logs is not claimed.

## Build Or Select Plugin Artifacts

Use one native three-file candidate archive built for the same OS and architecture as
the Bambu Studio installation. Pinned Studio `02.08.01.55` will not create its network agent unless it
can load a BambuSource library from the data-directory plugin folder, and the official archives do not
bundle one. Pandar's companion crosses only that load gate: it has a Pandar sentinel and no `Bambu_*`
camera/media exports, so Studio retains its fake-source fallback. It is not camera or media support.

The archive must contain exactly the CLI, network plugin, and BambuSource companion at top level. The
network plugin must expose 109 network plus 21 File Transfer exports (130 unique exports). Record all
three filenames and SHA-256 values, current `HEAD`, and the SHA-256 of the exact dirty source snapshot
used for the build; do not invent a commit for an uncommitted tree or record local filesystem paths.

Final16 is the current verified Linux baseline. Its immutable source archive has SHA-256
`24b45dd30c3509c02b609548409f05fa72490512525621dbc0574a05aa62a039`. The final16 Linux release
archive has SHA-256
`023dcad198674c8ad1c20eb9bc34df9ef9685f49dfeca6e6b5ea58188f3a24a3`; its network plugin and
BambuSource companion have SHA-256 values
`3bcce9085205d6af67dc9671cf58cd6f9fb694d5a587b43d160dc8b6a9b0712f` and
`88d34358be39ed3d239aeb317df8f34a92d4652877e86a9849c66e32347c1df2`.

From that exact source, workspace Nextest passed 1,808/1,808 with one configured skip; fmt, strict
Clippy, and module-size gates passed; ABI tools passed 22/22; release-smoke tools passed 25/25;
packaged tasks passed 18/18; all 130 exports were present; all 21 File Transfer entrypoints completed
256 ASan cycles; and PostgreSQL passed 7/7 with zero skipped. Final14 is retained as historical
evidence. Final15 passed a narrower native run but is non-promotable pre-correction evidence and must
not be selected as the current artifact.

Historical final13 uses immutable source archive `pandar-bambu-final13-019f7b10.tar.gz` at `HEAD`
`2ba0d1f2755501ea9e7d4babcf176db40638f643`. It is 2,751,227 bytes, contains 1,543 regular files,
and has SHA-256 `71080abb1e7392b0440a179b5bca9fd80638de74a614105b8dc11a0f70959c34`.
The canonical tree, member list, and freeze evidence have SHA-256
`db0b7c3385c29ff0cdee1930a66f554a6845b58907373ef543563b829c245761`,
`87a6ad1dfaa404731ed30d7e265303cca64fc4278a478f9c12192c09373eb880`, and
`4d132e16f91365795f54c97f608483c34b55726c5f614f5bb8ffaac2ede1fb7f`. Archive determinism,
unsafe-name, duplicate, case-collision, reparse-point, and post-extraction diff checks all passed; all
rejection/diff counts were zero. Pre-freeze plugin run `da32fbc4-f37e-4198-af5e-c35f73512dcb`
passed 368/368 with one separately reported skip.

Historical final13 completed evidence:

- Windows clean run `90cb6a69-08a5-4421-a661-58e696c374a3` passed 1,778/1,778 with one separately
  reported skip in 1,050.084 seconds; the firmware probe passed in 28.858 seconds. Fmt, zero-warning
  strict Clippy, module-size 2/2, ABI-tool 21/21, release-smoke-tool 21/21, and frontend 37 files/324
  tests plus typecheck, zero-warning lint, and production build passed. `npm ci` reported six audit
  vulnerabilities (three moderate and three high); this is retained as dependency-audit evidence and
  is not recategorized as a Studio-parity failure. Clean evidence SHA-256 is
  `c1ac8807a427ae4b7003681e9ad343d668dab1d6aa7c143d14bc699fe58b7b89`.
- PostgreSQL 16.14 harness `0c292295-f9ab-459b-89c2-ea74f2c9ff56` ran
  `24b49c19-cd07-42b5-a5a3-6d220345bd7e` and `1f4b8458-6397-4c0b-8ab3-23d37779c68a`; each passed
  55/55 with 831 filtered and zero runtime skip markers. Per-run log SHA-256 values are
  `b123f495e09de3c57c2c175000a37cc1fa7395dd0a9c52f1c2f72426c2f4dc08` and
  `b3e233f50fe1be9df43867e34307fd6193f09a2dc00940318bdfb8827f0a8d54`; normalized evidence SHA-256 is
  `7e04ae355f7bca3fb409bbc700b5c8f160194c0d2f9ec82df823c859566a2db7`. Source read-only and
  cleanup checks passed.
- Windows native archive `pandar-final13-windows-amd64-019f7b10.tar.gz` is 21,285,752 bytes with
  SHA-256 `6c50e77a0b4008ce46d86de51411117061c5118e18849ca1fb94f4a3f319db64`. ABI and packaged smoke
  each passed 21/21, all `version,bind,print,ams,ft` modes passed, the contract contained 109 network
  plus 21 File Transfer exports, `dumpbin` reported 271 total plugin exports, and companion inspection
  found one Pandar sentinel and zero `Bambu_*` exports. Consolidated evidence SHA-256 is
  `3dab4bffa359e4c46eec77cbfb278ce3a1497f806a1d80343a1735b5a68f025b`. Build, ABI, and smoke
  runs were `0430ad0e-7f96-41c5-b9aa-1c6fd690fd16`, `2f27f859-b795-4420-b04a-30410ae7bcbc`, and
  `65ffc0b0-e17e-45da-bd3a-3375f5d88de1`. CLI, plugin, and companion SHA-256 values were
  `a73fbe47a56fd557f14912e0e774007e0a4774ce83f250ce2a9cc41e52da8d57`,
  `7861e454eb9dd6122eabc6252102de462228eec526f47149e00a037d3dc48eba`, and
  `eaf98016c7d38cb6121a525a0f7a5bb5f0c59df333722798c5f76cee279fdfe6`. Six earlier pre-product
  manifest-harness calibration attempts remain infrastructure-only history and do not weaken or replace
  the completed artifact checks.
- Linux final13 attempt 2 passed as a whole. Nextest run `6ec3a215-9430-4ad2-adc7-f692ca156333`
  completed 1,779/1,779 with one separately reported skip in 792.687 seconds; the exact firmware
  fixture passed in 27.315 seconds. Fmt, zero-warning strict Clippy, module-size 2/2, ABI-tool 22/22,
  release-smoke-tool 21/21, all five ABI modes, and 21 File Transfer entrypoints x 256 ASan/LSan cycles
  passed. Archive `pandar-final13-linux-amd64-019f7b10.tar.gz` is 24,854,768 bytes with SHA-256
  `4166e6012e6c1bf7cdf056ba3bfb28f0fbc9d216c31e5ed2e8620adb8b5fcccc`; its CLI, plugin, and
  companion SHA-256 values are `7c44138d559ee62d02d4ac7fe0c23c7091e99a7782aac8a163a0c3565458d77f`,
  `f9baf8346901fdc2ba20aeee786029e47af495bad3ee2e754f440db89010be24`, and
  `88d34358be39ed3d239aeb317df8f34a92d4652877e86a9849c66e32347c1df2`. The evidence bundle
  SHA-256 is `aa7478fe0f74debcc5f3d1f5ec53a2222d726beafe5224935aa3382c24f6097a`.
- Linux attempt 1 run `c8a134c4-e775-4f37-b6ed-74ccb1b79123` remains non-promotable. Its product
  gates were green, but the outer wrapper expected `plugin_exports=21` from an FT-only invocation even
  though the checker intentionally reports the whole library's 130 contract exports; overall exit was
  1. The final bundle preserves this harness failure.
- Linux exact-AppImage attempt 8 passed with the same final13 archive. The official Ubuntu 22.04
  `02.08.01.55` AppImage has SHA-256
  `e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995`; official seed database and fresh
  `AppRun` SHA-256 values are `72b7d020ef537c7bd510910086d9dcafd3ad0e38e24614216630e27767a46be0`
  and `eaf5a1c6ff4f0d49d6e0c0bacf106309daa2c822ca1ebe8739067699e6cdaef4`. Studio PID `137` with
  start ticks `192688662` remained unchanged across two offline failures and one successful commit
  after Hub PID `674`/ticks `192689166` became ready. Each library mapped four times; `ldd`, undefined-
  symbol, `dlopen`, and certificate-error counts were zero.

The prior final12 frozen archive and the results below are historical evidence only:

- Windows native archive `pandar-final12-windows-amd64-019f7b10.tar.gz` is 21,285,799 bytes with
  SHA-256 `b4f6913eef7c1d09da9377fbce36b0ab759add25caac2baa0604c07a595440cb`. CLI, network-plugin, and
  BambuSource SHA-256 values are
  `1e57a7cfc2b46717129e7ced227b358eedbaaa74064f2ae2ac5cd44eac576b32`,
  `43be9e73350cacb66ee2dfa991f1a7291175c4d18db2ec917a10a1489f9244d9`, and
  `20805176609ebe891ed45bc7171a34ad0d741351b5dbe8c3c4d9f9b4a5a2a49a`. Build, ABI, and packaged
  release-smoke runs `4fa89d78-503f-4c51-a4e3-fc788a4f7f03`,
  `6b71c048-8377-4a61-a750-20c5531df864`, and `d808cce0-6e5f-45e7-b4aa-f7b39642d67a` passed;
  consolidated evidence SHA-256 is
  `11c38eb3c198cd07b2f96abbfbf70792b078170389e8869b230badbb98a404d2`. No real Windows Studio
  process was launched.
- The complete Windows clean gate passed in final Nextest run
  `5e6f3720-4c1b-4a55-ac34-2250c0cefba7` at 1,776/1,776. The first complete attempt's one firmware-
  probe failure passed in isolation and again in the required full rerun. Clean evidence SHA-256 is
  `a6fc922b5069c78dcbe077f6c4238794777f6b17b62574ee75638c46256fb342`.
- PostgreSQL 16.14 evidence run `3e00d36c-7fb9-47d3-b71b-d9735ebe0eae` and Nextest run
  `0b708279-6183-4477-9f78-31add8d7f423` passed 55/55 selected cases with 831 excluded by the filter
  and zero runtime skip markers. Evidence and log SHA-256 values are
  `d7f002f5be8708844cce406895503ef7056b634bf04aad068722eb25ef15247e` and
  `456ebcb37e91c7ac688a3537ecdb773d462d8037f37666da0071561ed226b87c`.
- Final12 Linux full validation did not pass: its compiled C++ fixture returned success, but the Rust
  wrapper rejected the run after the exact stale-generation diagnostic
  `pandar printer status refresh discarded: credentials changed during request`.

Historical final13 pre-freeze evidence is intentionally narrower:

- Background heartbeat refresh now preserves the last confirmed printer cache only while its request
  is in flight; foreground Studio print-info still invalidates immediately and fails closed. Directed
  tests `background_refresh_preserves_last_confirmed_cache_while_in_flight` and
  `foreground_refresh_invalidates_cache_while_in_flight` freeze that distinction.
- The firmware fixture arms a version-heartbeat sentinel and waits until the callback commits before
  beginning firmware command assertions. The wrapper permits empty stderr or only the exact stale-
  generation diagnostic above; every other diagnostic still fails the probe.
- Before the product fix, Windows stress iteration 2 failed with
  `firmware callback missed handoff deadline`, demonstrating the random callback loss. After the fix,
  six Windows stress iterations passed and iteration 7 reported
  `status callback logout deadlocked against firmware dispatcher` under the old three-second compound
  watchdog. An independent lock wait-for graph found no ABBA cycle: the firmware transition lock is
  released before waiting for the callback mutex, and callback execution does not hold the account-
  queue lock. The test now separates the start and logout failures, uses an eight-second internal
  watchdog, and gives the child process 45 seconds to report the precise failure first.
- A pre-final Linux source tree with manifest SHA-256
  `668f541a8e535018495d8a8969fa6a6d5b70daef49ed848c4c03ab19c40e4f9a` and source-archive SHA-256
  `e8c4d17505e9102b7f9fa3fbce8e653dddc7277b33f02671f603818fc1580b3b` passed the exact firmware
  probe 21/21. This is behavioral stress evidence only; it is not a final13 freeze, full workspace
  gate, native package gate, sanitizer result, or AppImage result.

No GitHub Action, authenticated Studio session, printer action, or live firmware action established any
of the evidence above. The exact-AppImage process establishes only the named load and development
no-auth recovery boundary.

Expected artifact names:

| OS      | CLI          | Network plugin                    | BambuSource companion          |
| ------- | ------------ | --------------------------------- | ------------------------------ |
| Linux   | `pandar`     | `libpandar_network_plugin.so`     | `libpandar_bambu_source.so`    |
| Windows | `pandar.exe` | `pandar_network_plugin.dll`       | `pandar_bambu_source.dll`      |

The current native release-smoke scope is only `linux-amd64` and `windows-amd64`. A future macOS
candidate must use `libpandar_network_plugin.dylib` plus `libpandar_bambu_source.dylib`, but no current
macOS artifact or real Studio evidence exists.

Repo-local build option for Linux:

```bash
cargo build -p pandar-network-plugin -p pandar-bambu-source --release
```

Repo-local build option for Windows from a Windows Rust environment:

```powershell
cargo build -p pandar-network-plugin -p pandar-bambu-source --release
```

For the current Phase 23 evidence, use the native packaged artifact and trace it to `HEAD` plus the
dirty snapshot SHA-256. A repo-local development library is not a substitute.

## Environment

```bash
export PANDAR_PLUGIN_HUB_URL="https://your-hub.example"
export PANDAR_PLUGIN_FRONTEND_URL="https://your-web.example"
```

## Preflight

Run the local preflight helper before replacing any Bambu Studio plugin file:

```bash
cargo run --manifest-path tools/studio-plugin-smoke/Cargo.toml -- \
  --preflight \
  --studio-path /path/to/BambuStudio \
  --plugin-artifact /path/to/libpandar_network_plugin.so \
  --hub-url "$PANDAR_PLUGIN_HUB_URL" \
  --frontend-url "$PANDAR_PLUGIN_FRONTEND_URL" \
  --os linux \
  --arch x86_64 \
  --studio-version "02.08.01.55" \
  --test-date YYYY-MM-DD \
  --pandar-commit "$(git rev-parse HEAD)"
```

A passing preflight only proves the prerequisite paths, network-plugin filename, URL shape, and evidence
metadata are ready for a manual run. It does not inspect the BambuSource companion; use native
`tools/release-smoke` evidence to require its sentinel and reject `Bambu_*` exports. Preflight does not
launch Bambu Studio or provide real Studio compatibility evidence; every Studio checklist item remains
`untested` until Studio is launched and exercised manually.

## Replace And Roll Back

1. Quit Studio and back up any existing network-plugin and BambuSource files in its data-directory
   `plugins` folder.
2. From an unpacked release directory, install both artifacts with
   `pandar install-network-plugin --data-dir <BambuStudio-data-dir>`. For non-release artifacts, pass
   `--plugin-file <network-library> --source-file <source-library>` explicitly.
3. Confirm the installed source name is exactly `BambuSource.dll`, `libBambuSource.so`, or
   `libBambuSource.dylib` for the current platform. Keep `BambuStudio.conf.pandar-bak` for rollback.
4. To roll back, quit Studio, restore the two backed-up plugin files (or remove Pandar's files when no
   originals existed), and restore `BambuStudio.conf.pandar-bak`.

## Smoke Checklist

| Step                                | Expected Result                                                           | Status     | Evidence |
| ----------------------------------- | ------------------------------------------------------------------------- | ---------- | -------- |
| Studio loads both libraries         | No missing-library, missing-symbol, or dynamic-loader error.               | `passed` | Final16 used the official AppImage and packaged plugin/companion in the controlled harness; the package had already passed its native loader and release-smoke gates. |
| BambuSource remains non-media       | Companion sentinel is present, `Bambu_*` exports are absent, and no camera/media capability is claimed. | `passed` | The final16 package passed one-sentinel/zero-`Bambu_*` inspection. No media or camera operation was invoked. |
| Login opens Pandar sign-in          | Studio WebView displays Pandar sign-in.                                   | `untested` |          |
| Localhost ticket callback completes | Studio receives plugin ticket through its local callback.                 | `untested` |          |
| Token exchange completes            | Studio exchanges the plugin ticket for a tenant-scoped plugin credential. | `untested` |          |
| Profile loads                       | Studio receives Bambu-shaped login state.                                 | `untested` |          |
| Printer list loads                  | Hub-backed printers display or an empty list is accepted.                 | `untested` |          |
| Job list loads                      | `get_user_tasks` returns the Hub-backed authorized page; a legitimately empty Hub page is accepted. | `untested` |          |
| Selected-only model-task callback   | Automatic selection of the sole fixture printer can request and return its model task without an explicit add-subscription call. | `passed` | Final16 produced exactly one model-task request/200 and the four request-started, response-accepted, callback-started, and callback-returned events once in order. This is a synthetic session/loopback-mock boundary, not a real backend or downstream encrypted-log claim. |
| Logout                              | Studio receives `studio_useroffline`.                                     | `untested` |          |
| Direct-printer and `ft_*` paths     | Unsupported behavior is stable and does not open machine sockets.         | `untested` |          |

### No-Auth Development Evidence

The no-auth bootstrap is a separate development-only path. It does not exercise the sign-in page,
localhost ticket callback, or ticket exchange, so it cannot promote any authenticated checklist row.
Hub issues a no-auth Studio credential only when exactly one tenant exists; zero tenants fail not
found, and multiple tenants fail with `ambiguous_no_auth_tenant` without creating a token or audit.
Never capture `pandar-plugin-login.json`; record only its mode, size, token length, and non-secret
profile fields.

Startup retry is deliberately narrow. Only a connection failure proven to occur before request
delivery is eligible. The sequence allows at most five attempts including the initial attempt, begins
at two seconds, doubles to a 30-second cap, and is fenced by logout/destroy plus Hub-configuration,
token, account, and generation changes. HTTP responses, ambiguous timeouts, and response loss are not
automatically retried.

Every persisted account mutation is serialized across processes by `.pandar-plugin-account.lock`.
Login, pending-revocation, and direct-intent state becomes active only after
`MutationDurability::Confirmed`. `ChangedUnconfirmed` means the namespace change was published without
confirmed directory durability and fails closed; an ordinary error describes the canonical namespace
visible at return rather than promising crash durability during an extreme rollback failure.

Requested logout first stages a confirmed pending self-revocation. If that fails, it must confirm a
direct-revocation intent before DELETE. Passive loss does not revoke, and a requested race may upgrade
passive finalization without duplicate callbacks. Retained login may be restored only before DELETE;
it is never restored after DELETE is attempted. Successful DELETE and idempotent `401`/`410` record a
completed tombstone with canonical Hub URL and token SHA-256 before cleanup, blocking stale login loads
and writes. Duplicate pending cleanup after direct success is best effort. The completed ledger is
currently unbounded and may be cleared only after all Studio processes using the directory are stopped
and all older Hub plugin sessions are invalid or expired. Printer and task list/detail/plate/subtask
calls share one no-auth rotation on `401`/`410`; concurrent callers share the winner, each request
retries at most once, and authenticated sessions never fall back to no-auth.

The historical final13 implementation review returned `APPROVE` with no Blocking, Important, or Minor finding.
The historical persistence review returned `VERDICT: APPROVE` with two Minor limitations: the
completed ledger is unbounded, and an ordinary rollback error describes the current namespace rather
than guaranteeing crash durability. The final evidence-document review completed after correcting
its sole Minor terminology finding.

### Final16 Official-AppImage Selected-Only Model-Task Evidence

The current final16 gate used the official Ubuntu 22.04 Bambu Studio `02.08.01.55` AppImage with
SHA-256 `e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995`, packaged network
plugin SHA-256 `3bcce9085205d6af67dc9671cf58cd6f9fb694d5a587b43d160dc8b6a9b0712f`, and BambuSource
companion SHA-256 `88d34358be39ed3d239aeb317df8f34a92d4652877e86a9849c66e32347c1df2`.
The runner SHA-256 is `7ab2c4cb8816ae4488e40fce71ec69684997739567d3a76f17d2c9e2a324873f`.

Studio automatically selected the sole fixture printer and did not issue an explicit add-subscription
call. The packaged plugin nevertheless treated the selected printer as a Cloud target: selected or
explicitly subscribed owns a Cloud target, heartbeat uses their deduplicated union, removing either
source retains the target while the other remains, and only loss of both sources retires Cloud state
and Cloud tickets. That retirement must not alter the virtual-local generation or any Local ticket.

The mock recorded exactly one model-task request and one HTTP 200. The plugin trace recorded exactly
four lifecycle events, once and in order: request started, response accepted, callback started, and
callback returned. Unexpected, legacy, and unsafe mutating request counts were all zero. Evidence manifest
SHA-256 is `c6ba9b6282581119d3baec720e26990ad63efc20eb394b0c71dced89081d5fd9`; result-summary
SHA-256 is `771d0a657e235eff40dffd1637175a4991bbbac7672b231133a20fddc11e3220`.

Deterministic redacted bundle `pandar-final16-real-studio-evidence-019f7b10.tar.gz` is 245,225 bytes
with 26 tar entries (23 files and three directories) and SHA-256
`f07c369ad9e0354ef40142294d9385e9c454fd534a04badce4be000f49c06eca`; an independent second
generation produced the same hash. Its sidecar SHA-256 is
`30c6e5d43b74f9770d19638b86cefddd96d4d861c16155c74d30b488adf7f1b6`. It contains only
manifest-covered safe evidence plus outer success/provenance and excludes the runner and mock
implementation plus synthetic token contents.

This is a deliberately narrow controlled boundary: a synthetic persisted authenticated-shaped
session and a loopback fail-closed mock in a read-only, network-isolated harness. It did not use real
authentication, Hub, Agent, database, printer, print/control/cancel, firmware, or any other hardware
action. Studio's logs are encrypted, so no downstream post-callback Studio behavior is claimed. No
GitHub Action was used.

### Historical Final14 Exact-AppImage Evidence

Attempt 1 passed the historical final14 Linux module-load gate with the official Ubuntu 22.04 Bambu
Studio `02.08.01.55` AppImage and package
`pandar-final14-linux-amd64-019f7b10.tar.gz`. The package is 24,854,111 bytes with SHA-256
`4e91f2457197532102544b02d4edac5354dc2982ec55fa707a057cbcba518b68`; its plugin SHA-256 is
`c95d06c41e2ecbcec4f28ef722f37d6f279715c7b2d95089f49a19e1247ff7fc`.

The official AppImage SHA-256 is
`e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995`; fixed seed database and
extracted `AppRun` SHA-256 values are `72b7d020ef537c7bd510910086d9dcafd3ad0e38e24614216630e27767a46be0`
and `eaf5a1c6ff4f0d49d6e0c0bacf106309daa2c822ca1ebe8739067699e6cdaef4`. Studio retained one
PID/start-tick identity, each package library produced four process-map lines, loader/certificate
error counts were zero, and exactly one development no-auth session was observed.

The redacted evidence archive `pandar-final14-appimage-redacted-evidence-019f7b10.tar.gz` is 10,603
bytes with 23 members and SHA-256
`7eac6abbc7364928147d60dd1c583d084c02debf1552734bc82a4dec59c941be`. Canonical member-list,
manifest, evidence-files, and result-summary SHA-256 values are
`a39eef283f7ca81d1d6f0b3150de79f17fa7e052fdaf181b2aff88fec67146cc`,
`1326bf59616feaecf184e9015b4dfdc4ee9469f1495786ebf1b1f2e2c60ac295`,
`7f9736c045af21e51f29cc46ffdc82ac9affd593d0eb53963ac4d488aaa2bcf0`, and
`ea6c284576c2342f501d3803daafde584502fcf753513b301f2890b6aee1261a`.

This historical result proves final14 exact-AppImage library load and development no-auth only. It does
not fill the Better Auth sign-in page, localhost ticket, authenticated token/profile, printers, jobs,
logout, unsupported-path UI, hardware, firmware, or model-task rows.

### Historical Final13 Exact-AppImage Evidence

Attempt 8 passed the historical final13 gate with the official AppImage and the already-passed
final13 Linux package. Studio PID `137`/start ticks `192688662` and Hub PID `674`/start ticks
`192689166` identify the observed processes. Before Hub was available the plugin recorded exactly two
proven pre-delivery failures; without restarting Studio it then recorded one success and one commit.
Final active/total token count was `1/1`; create/revoke/discard counts were `1/0/0`. The one login
file was mode `0600`, 343 bytes, and SHA-256
`c67cbb2470085de83fb5f0cd79119c3cf70d97f56d424b657da1a00943b47e99`. Its content was not
captured or retrieved.

The two installed libraries each produced four process-map lines. `ldd`, undefined-symbol, `dlopen`,
and certificate-error counts were all zero. Setup Wizard window count remained zero; the runner did
not interact with the wizard or inject UI input. Final observed network state was `none`, and cleanup
left zero task containers and zero task processes. These assertions prove exact-AppImage module load
and same-process development no-auth recovery only. Sign-in/WebView, localhost ticket, authenticated
token/profile, printers, tasks, print, logout, unsupported-path UI, real hardware, and firmware remain
`untested`.

The redacted evidence archive `pandar-final13-appimage-redacted-evidence-019f7b10.tar.gz` is 7,211
bytes with 23 members and SHA-256
`a4453c8dce3829cc1a84a372a772b516812fe1564b310e61db9e9009a11cf9d2`. Its manifest, canonical
member list, and hashes file have SHA-256
`7ef2a8547ba767f5d0be174b491fa40c2946a0add71adb4043a9abe8d54c1a8a`,
`d79e3f0b6b3672241324a11a2b7f7d8d727c464303f11cbe8745c4f8e60e496f`, and
`ee623a39f5db110b9c26076bdb9a9b440404170402cc3fe840e402cdce2ee1a9`. External hashes and all
21 internal hashes passed; the evidence contained zero symlinks. Raw runner state, database files,
and login content were deliberately not pulled back.

The task-local locale was built from the official Ubuntu `locales` `2.35-0ubuntu3.13` package, whose
deb SHA-256 is `81c263acc29288d1684f845a5f2cb63bc5d8cc867ac3830acc46aa177ac7a7cc`.
The `en_US` source and UTF-8 charmap SHA-256 values are
`38e3102344829f4ef998db66d064c0082b4bd1c8cf95e35ac3de12bb9f1d62f5` and
`a743fdbdb2d4b62a20fe1cf8565215ec12b03a8b71ff26b3f789bf97c3c737ff`; the 12-file `LOCPATH`
manifest SHA-256 is `88421fcda8c7577fe7d1bc2769cdbf71a2317f388566247769cdd87cf8f0b1f5`.

Attempts 1-7 remain environment-harness history, not product failures. Attempts 1-3 timed out before
plugin load because `en_US` was unavailable and the data directory was wrong. Attempt 4 used read-only
`xwininfo` to identify the language modal. Attempt 5 proved `C.utf8`/`en` insufficient; the locale-
build verifier incorrectly consulted `locale -a` even though `localedef` succeeded. Attempt 6 passed
locale setup but exposed the Beta data directory and first-run Setup Wizard. Attempt 7 supplied a valid
built-in preset but still wrote the non-Beta `BambuStudio` directory. Attempt 8 reused the runner's
existing `DATA_DIR=BambuStudioBeta`, the task-local locale, and the built-in `X1C0.4` preset; every
unchanged-process, loader, credential, network, and cleanup assertion passed.

### Historical Final11 AppImage Evidence

The 2026-07-22 final11 exact-AppImage run historically established this same-process regression result.
It does not fill any current final16 row:

| Check | Status | Evidence |
| ----- | ------ | -------- |
| Packaged plugin starts while Hub is unavailable | `passed` | Historical final11 only: both libraries mapped with four map lines each, the post-agent getter appeared three times, and two proven pre-delivery connection failures were recorded. |
| Same Studio process bootstraps after Hub becomes ready | `passed` | PID `2176` and start ticks `190073915` did not change. The failures were followed by one HTTP 200 commit; `retry_attempts=2`, `commits=1`, and `discarded=0`. |
| Credential creation remains singular | `passed` | One active session, one token-create audit, zero revoke audits, and one mode-`0600` 343-byte login file remained after recovery. |
| Loader and certificate boundary remains clean | `passed` | Undefined-symbol, `dlopen`, and certificate error counts were all zero. |
| Authenticated WebView, printer/task UI, and requested logout UI | `untested` | No authenticated account, ticket exchange, Studio printer/task UI, or logout interaction was exercised. Automated account/task/logout tests do not promote this row. |

The redacted final11 result, timeline, and component summaries are retained in the historical evidence
bundle only. Their hashes are intentionally not reused as current-candidate evidence.

Historical final5 regression only: on 2026-07-21 the same process did not bootstrap within 30 seconds
after Hub recovered, while restarting Studio against the ready Hub created one session. Its redacted
evidence SHA-256 is `7f103873d222b8b51e1209c4836f2acc2579515cff9729dd89c4271032e801b0`.
This is retained only as the startup-recovery regression, not current-candidate evidence.

No external account, Agent, printer, print/control action, Setup Wizard interaction, persistent host
setting, live firmware action, or GitHub Actions run was used.

This no-print checklist deliberately stops before print submission, cancellation, firmware mutation,
or printer controls. Automated field/lifecycle/cancellation/task tests belong to
`bambu-studio-print-contract.md`; they do not prove hardware behavior. If an authorized hardware run
is performed later, record it as a separate evidence row with printer state and scope, and never fold
it into the desktop load/session result.

## Evidence Capture And Redaction

- Capture Studio version, OS, architecture, all three archive filenames and SHA-256 values, current
  `HEAD`, exact dirty source snapshot SHA-256, and test date.
- Redact bearer tokens, plugin tickets, Bambu access codes, local artifact paths, and filesystem paths.
- Do not archive `pandar-plugin-login.json`, the runtime database, HTTP authorization headers, or raw
  ticket callback URLs. A token length or one-way evidence hash is sufficient when needed.
- Prefer short log excerpts over full logs.
- Preserve the exact distinction between a successful compiled fixture and a wrapper rejection. For
  the firmware probe, record whether stderr was empty, contained only the approved stale-generation
  line, or contained any other failing diagnostic.
- Label stress loops by exact source-tree identity and iteration count. A pre-final loop never fills a
  frozen-source, package, sanitizer, AppImage, or real-Studio field.

## Updating The Manifest

After the run, update `docs/compatibility/bambu-studio-plugin.md` with one row per Studio version, OS, and architecture. Keep failed or blocked attempts because they are compatibility evidence.
