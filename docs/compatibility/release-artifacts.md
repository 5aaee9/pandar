# Release Artifact Compatibility

Phase 24 release-artifact evidence is separate from real Bambu Studio compatibility evidence. Native
release-smoke can prove an archive checksum, exact layout, CLI startup, network-plugin exports, and
BambuSource local-media export contract. It does not prove that Bambu Studio loaded the libraries,
played camera frames, or that a real printer accepted an action.

Current releases build separate artifacts for ABI series `02.06.00`, `02.06.01`, `02.07.00`,
`02.07.01`, `02.08.00`, and `02.08.01`. `studio-abi-profiles.json` pins the exact reference Studio
commit, source network-agent version, reported `<abi-series>.99` version, and export contract for each
series. `02.06.00` requires 103 network plus 21 File Transfer names, `02.06.01` through `02.08.00`
require 108 plus 21, and `02.08.01` requires 109 plus 21. Contract counts are not the dynamic
library's total export count; the historical final12 Windows PE has 271 exports after Pandar flat-FFI
and aws-lc exports are included.

## Status Values

| Status | Meaning |
| --- | --- |
| `passed` | Verified in the named environment with evidence captured. |
| `failed` | Attempted and failed; the failure boundary is recorded. |
| `blocked` | A named dependency or environment prevented the attempt. |
| `unsupported` | Intentionally unsupported by Pandar. |
| `untested` | No evidence has been recorded. |
| `in_progress` | Implementation exists, but one or more named final gates remain pending. |

## Required ABI-Series Candidate Layout

Each candidate archive has exactly three top-level files:

| Target | CLI | Network plugin | BambuSource companion |
| --- | --- | --- | --- |
| `linux-amd64` | `pandar` | `libpandar_network_plugin.so` | `libpandar_bambu_source.so` |
| `macos-amd64` | `pandar` | `libpandar_network_plugin.dylib` | `libpandar_bambu_source.dylib` |
| `macos-arm64` | `pandar` | `libpandar_network_plugin.dylib` | `libpandar_bambu_source.dylib` |
| `windows-amd64` | `pandar.exe` | `pandar_network_plugin.dll` | `pandar_bambu_source.dll` |

The network plugin must expose the complete pinned 124-, 129-, or 130-name Studio contract set for its
named ABI series; this check does not require the binary to have only that many total exports. The
companion must export `pandar_bambu_source_sentinel` plus exactly the 21 `Bambu_*` symbols used by the
pinned local-media ABI. Those exports implement only Pandar's authenticated loopback MJPEG path; they
do not imply cloud/TUTK/Agora, recording, discovery, or direct printer transport support. Studio
installation renames the two libraries to its exact platform filenames, including
`libbambu_networking.so` plus `libBambuSource.so` on Linux, and `bambu_networking.dll` plus
`BambuSource.dll` on Windows.

Current target-architecture release-smoke covers `linux-amd64`, `macos-amd64`, `macos-arm64`, and
`windows-amd64`. The release workflow builds each archive on its target OS. Both macOS matrix rows use
an Apple Silicon `macos-26` runner: arm64 executes natively, while amd64 is cross-compiled and executes
its CLI, ABI probe, and release-smoke under Rosetta 2. Package evidence does not replace a real Studio
run or hardware-print evidence; those surfaces remain `untested` until separately recorded.

Candidates for this target must be built by same-OS target toolchains. The tag workflow builds Linux
amd64, macOS amd64/arm64, and Windows amd64 desktop archives, includes the BambuSource companion, and
runs release-smoke for each target architecture and ABI series with the pinned Studio and Boost inputs
against the packaged plugin before publication. The macOS jobs suppress AppleDouble metadata so each
archive retains the exact three-file layout. The Windows job uses MSVC and also packages a
`pandar-studio-hook-<abi-series>-windows-amd64.zip` asset plus its checksum for each catalog entry.
That bundle has exactly `pandar_studio_hook.dll`, `pandar_network_plugin.dll`, and
`pandar_bambu_source.dll`; the Rust installer rejects any other layout. The initial `v0.1.0` workflow run `30653076144` built and
smoke-tested the native Windows artifacts, but rejected the Linux artifact because its Zig-built C++
shim was ABI-incompatible with the host `libstdc++` probe. Linux plugin builds now use the native GNU
toolchain. Replacement run `30654892795` passed both native build/smoke jobs and published the
release; a real Windows Studio run still must verify the download replacement.

## 2026-08-01 Stable 2.7.1 macOS arm64 Validation

An untagged Apple Silicon validation run on macOS `26.4` built the current `02.07.01` three-file layout
and ran the complete native release-smoke against reviewed Studio commit
`3f126b717ed1f10fee0f32f05ed9731808d0c8bb` and the pinned Boost archive. The archive SHA-256 was
`4f5d2255839ea75f8a6d78e6729d91b92a9becc935d1b62946d6cb8c02a87a41`; the plugin and BambuSource
SHA-256 values were `a43a10610421a34c5b2d4e49c4ededb4aa9d8a526474c69e98cf98327fac3930` and
`34d9780ad48db45de4065e238bba735c43d0bdbcf54a5a8dd79e7bb3a49e2687`. CLI execution, the exact
108-network-plus-21-File-Transfer export set, the sentinel-only companion policy, and all four native
`version,bind,print,ft` ABI modes passed on `aarch64-apple-darwin`.

Computer Use then launched the installed official `/Applications/BambuStudio.app`; its About dialog
reported `2.7.1.62`. With module-certificate validation disabled only in the isolated test
configuration, Studio PID `74129` reached its normal home UI and `lsof` plus `vmmap` showed both exact
Pandar dylibs mapped from `BambuStudio/plugins`. Their on-disk hashes remained the packaged hashes
above. No sign-in, ticket/token/profile, Hub/Agent, printer, print/control, firmware, or hardware path
was exercised. The original plugin files, `BambuStudio.conf`, installer backup state, and plugin trace
length were restored after the test; the original network/source hashes after restoration were
`88f1f98b548690ce472177e10f92b9aa580cefec6904893f0c5b32a162860126` and
`0b9e8f3508b9d4dbbc97ed9ccb312c2d8871464e940fc73567fb070e55a51f5a`.

## Historical 2026-08-01 Public Beta macOS arm64 Validation

An untagged Apple Silicon validation run on macOS `26.4` built the historical Public Beta three-file
layout and ran the complete native release-smoke against its pinned Studio commit and Boost archive. The local
archive SHA-256 was `1e488678543f882a7a6007746725d7319de6fdb19496ee567a6bbc03212603a9`;
the plugin and BambuSource SHA-256 values were
`84ab2155b6632f5d172ace0e2930ac93fe0976656451917344920af7c0206004` and
`3c45839eb1cbb656015bd79d5a66468d194b9511f90f61f1566ffe1da673bb63`. The archive contained
exactly `pandar`, `libpandar_network_plugin.dylib`, and `libpandar_bambu_source.dylib`; AppleDouble
members were disabled during packaging. CLI execution, all 109 network plus 21 File Transfer exports,
the sentinel-only companion policy, and the native `version,bind,print,ams,ft` ABI modes passed.

The official `02.08.01.55` macOS Public Beta DMG had SHA-256
`17eca4d63b909c728bf6d0cf8753397820f15b372e1ce69d6ab71be796a3af0d` and passed Apple's deep
code-signature verification. Computer Use launched that app from a read-only mount. The Studio process
reached its normal home UI and mapped the exact Pandar plugin and companion from
`BambuStudioBeta/plugins`; their hashes remained unchanged after startup. This is real target-version
module-load evidence for macOS arm64 only. It is not a tagged artifact, macOS amd64 evidence,
authenticated sign-in/session evidence, Hub/Agent/printer evidence, or a hardware-operation claim.

## v0.1.1 Tagged Release Evidence

Annotated tag `v0.1.1` resolves to commit `a442a120c198c17961c3663635e65ae63bf0ec98`.
Release-commit Checks run `31367299202`, tagged Checks run `31370250709`, Release run
`31370250664`, and Docker/Helm run `31370250632` all passed. The GitHub Release was published on
2026-08-10 with 60 assets: 24 desktop archives, six Windows Studio hook bundles, and one SHA-256
sidecar for each archive or bundle. All 30 downloaded sidecars passed `sha256sum -c`.

| ABI series | Linux amd64 | macOS amd64 | macOS arm64 | Windows amd64 | Windows hook |
| --- | --- | --- | --- | --- | --- |
| `02.06.00` | `b4c12035349af110ed857b56520b4b6c5f7e79b9e57728f110c0e10be6d0d0f1` | `e5c06d80dbe687df4f7d1d37b3d71856951679335765d7689f99b9c03e4a3047` | `3d970bcd0d73ef61c0c299a1a293d0b773d4a6269336599783a0896a1cbdd92b` | `d52c8771391841df2c01fbe907fd9aaa6838b410ff0af42d0e91ea40a373f957` | `10ec41b662ae65379824ee3de04b934df932817e7898bcc2c528f3a5b60f7bf7` |
| `02.06.01` | `4515ede4b2afe442cc51ed989fdbe759ed95f20943a19dac8537eba377470128` | `d51f5d125bbd2e863d7b0803a16ffe37b0178921169e624145e8310b63b7e847` | `7e4638ccb9217f0597e4e251cfb915427fcdf452d26c0946dfde256f2ca1782b` | `34d6dc4e9cefbe8dc380362436d7ecb778ed1dd029c3f4811d99a4995588dd31` | `7ab97339dc61d30a3a3f171e4ef63e4bb4eb0027254fb648ba37ddb666cbcef8` |
| `02.07.00` | `9e7de83b6aabdbe8b102fab65d06ebeb34d03959002b3571e3f8c6afadc3294b` | `bd60644e956d3ee1ea40d9a1ed0b41854ef10b06197352cd4885eb9bc9d8db91` | `1c8159726b70efea79c7cd3804256bf5032e556fa06a8847e820cec31cf8c247` | `8d6981833bee1c936ac850917aa7abafe7994d757b0ce560b753524a27d05071` | `b75ede3703904ac257291bf5bfaafd6f597e7c0037c397bca3ca5399fc952552` |
| `02.07.01` | `94005a45a830c2e689ea32e751131d031636400c6bd03a3420308d46d544ade4` | `459186b54d42932968333caa0a1d98690d6c31ac87cb3e5f087e91e69b1058ef` | `9a58378ac908db5542fcb8fa1d919f3d713e63ed1a5537d7c51025c3be3d7c15` | `22386f853ca16ad80d099e3c6739a14727aa74c5f60943d0d8e3a518a0f4fdae` | `9ffb69b032d8bed6ff6337b9c2c7b15e4636ce5d3bc6e308e33c90774f5fd27b` |
| `02.08.00` | `f9e03c79bf6d1abb9b07a83004a3718731fc951259b3556cdc4852f6ed1f26fa` | `0840981f580bb7ec9841f6bf7012579ecc5d673ce762bd91bcf514d362a44b78` | `395694f23a7f6f1d6d1e08893524c80e5b44672e913dff2fe4e0bab99e78d9b8` | `57add4cbfdd6eee390133fbbb0d82e09d2407a5b807cd726239a1260e571a438` | `d930de32151b173b43a29b751c41024a353dd31c627e07ba67664ef0262e74e8` |
| `02.08.01` | `a982e7662f98b4b5b5d77330a088428f23ca53f062529e1f3cc2e82b82689c6c` | `b9e7297438de1a946bb8167af3bb618c90ac08d88a531632b7c21ced7da322f2` | `19cd95113b6e539a403d3dc5a1d0deb44872d2d9388a17c893ec457a09e20949` | `6db89f704fb3463d9eb929f10b0250d54b54c2d8ff3d541be009e8979617ea58` | `15262805153c6e97bd5854677fffcfcbf2c8abebaf6d905317b00d7f488d2817` |

Every Release matrix job built on its target OS, ran the packaged CLI and ABI probe, verified the
exact three-file archive layout, and passed its ABI-series-specific Studio export contract before
publication. This remains package evidence, not real Studio or printer-hardware evidence.

The published Hub manifest digest is
`sha256:a54ecfd1a2c79bb208b39b5d6ff900d31385d0aa62d54215c98ca7b1b167f8c9`; the Web manifest
digest is `sha256:c10c65dcba1bd34934eeb412f1137bdc628145993ac01c3922436becc510d66d`.
Helm chart `0.1.1` has OCI digest
`sha256:7299c24ecd9274bdae5e395d5060be8d01e1e2e5b687129fc554824950b19926` and declares
`appVersion: 0.1.1`.

## v0.1.0 Tagged Release Evidence

Annotated tag `v0.1.0` resolves to commit `d50ef4223daf1fe5f45b6adc254ec91a9823bacc`.
Tagged Checks run `30654892831`, Release run `30654892795`, and Docker/Helm run `30654892588`
all passed. The GitHub Release was published on 2026-07-31 with exactly six desktop assets. After
publication, all three downloaded sidecars passed `sha256sum --check --strict`; both release archives
had exactly their declared three top-level files, and the Studio hook ZIP had exactly
`pandar_studio_hook.dll`, `pandar_network_plugin.dll`, and `pandar_bambu_source.dll`.

| Target | Archive SHA-256 | CLI startup | Exact three-file layout | 130-name Studio contract set | Companion sentinel/no `Bambu_*` | Native ABI probe | Exact Studio load |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `linux-amd64` | `ec6d60492afb101cde66c270b9550447185d281dbac51a3868d98be4f43fbd10` | `passed` | `passed` | `passed` | `passed` | `passed` | `untested` for this tagged artifact; historical final16 AppImage evidence remains separate |
| `windows-amd64` | `3e21f6c0a6c67ec47d9b826f45ebe40888acb8f8d8e3bcd5dad8496d082a15b8` | `passed` | `passed` | `passed` | `passed` | `passed` | `untested` |
| macOS | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` |

The Linux network plugin SHA-256 is
`374d9e6a3213e64b1f3245e1b5744186fde4677e4a5d12b11491f6faa0c01387`; its BambuSource
companion SHA-256 is `7199d3fa8347155ece25ec797f159b13b3171da2bf8ce710be0eb0cf99459b4c`.
The native Ubuntu 24.04 runner used Rust `1.97.1`, GNU C++ `13.3.0`, glibc `2.39`, and dynamic
`libstdc++.so.6`. The downloaded Linux archive was independently rerun through release-smoke after
publication and again passed CLI execution, 109 network plus 21 File Transfer declarations, all 130
contract exports, the sentinel-only companion policy, and the full packaged native ABI probe.

The Windows network plugin SHA-256 is
`3b800f6b7855efcef62490c6d159a279b97a859851940a793e455efe6e63e427`; its BambuSource
companion SHA-256 is `785ab8c6fc4729fe02bdb0c864e881ea2980882ff38d0182f3b5babe0bb16f9c`.
The native Windows runner used Rust `1.97.1`, target `x86_64-pc-windows-msvc`, and clang-cl
`20.1.8` with the MSVC x64 runtime. The Studio hook ZIP SHA-256 is
`fa80f37f72fbe705914139a75b3a04d3ad0c0cc63020d546f210dd6426f76089`; its hook DLL SHA-256
is `dd87a83e76b2beae104eaebda9bb449e7e28ae46ffc58d7c653c77a406d97459`.

The published Linux amd64 container manifests are Hub
`sha256:0276f3e056e90e4cc590f5b597ee28121a469ba5f7629c33105aaba405103dc0` and Web
`sha256:5f5a6cc74a04533db4bf790933bada9281b937133be5e684b2cc859444f76160`.
Helm chart `0.1.0` has OCI digest
`sha256:c36ca756aec016d501d32c81054986a1b5cc1ee34f0e3c8773d2f995a618108b` and declares
`appVersion: v0.1.0`.

## Historical Final16 Linux Candidate Evidence

Final16 is the historical verified Linux candidate for exact Public Beta `02.08.01.55`. Its immutable source input
`pandar-bambu-final16-019f7b10.tar.gz` is 2,793,904 bytes with SHA-256
`24b45dd30c3509c02b609548409f05fa72490512525621dbc0574a05aa62a039`; the canonical source-tree
SHA-256 is `c62c92167f466a915400953ec2d0e126bc34b3c6509a747ddee17dce8d52bf30`. The earlier
pre-fix freeze whose SHA-256 begins `6318d190` and ends `ab473` was rejected by the P1 review and
must never be used as a current candidate.

| Target | Source identity | Archive SHA-256 | CLI startup | Exact three-file layout | 130-name Studio contract set | Companion sentinel/no `Bambu_*` | Native ABI probe | Exact Studio load |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `linux-amd64` | final16: `2ba0d1f2755501ea9e7d4babcf176db40638f643 + 24b45dd30c3509c02b609548409f05fa72490512525621dbc0574a05aa62a039` | `023dcad198674c8ad1c20eb9bc34df9ef9685f49dfeca6e6b5ea58188f3a24a3` | `passed` | `passed` | `passed` | `passed` | `passed` | `passed` (official `02.08.01.55` AppImage; synthetic persisted session and loopback mock only) |
| `windows-amd64` | final16 source frozen; no final16 native package | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` |
| macOS | `none` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` |

Final16 Linux Nextest run `c9c96abe-5b80-4478-be33-9ceffef62a53` passed 1,808/1,808 executed
tests with one configured skip. Fmt, strict workspace Clippy, and module-size checks passed; the
standalone ABI tool passed 22/22, release-smoke passed 25/25, and packaged-task checks passed 18/18.
The packaged plugin exposed the pinned 109 network plus 21 File Transfer contract names, and all
21 File Transfer entrypoints passed 256 ownership cycles under ASan/LSan.

The disposable PostgreSQL 16.14 gate, Nextest run
`b73d7ce9-d3ab-424b-8d65-b4736e59f24b`, passed 7/7 with zero skips. Its dedicated container,
network, and volume were removed and their absence was verified after the run.

The exact three-file archive `pandar-final16-linux-amd64-019f7b10.tar.gz` is 24,891,706 bytes with
SHA-256 `023dcad198674c8ad1c20eb9bc34df9ef9685f49dfeca6e6b5ea58188f3a24a3`; its sidecar SHA-256
is `bde03e9633839432063d93768e10b0caf845755d216a653e20fa11d1461296f8`. Its only members are
`pandar`, `libpandar_network_plugin.so`, and `libpandar_bambu_source.so`. Their respective SHA-256
values are `b1762bfccdfc1f658147b19b23d7016707b5414d14f74be518e0b5663ddb1b22`,
`3bcce9085205d6af67dc9671cf58cd6f9fb694d5a587b43d160dc8b6a9b0712f`, and
`88d34358be39ed3d239aeb317df8f34a92d4652877e86a9849c66e32347c1df2`.
The native evidence archive has SHA-256
`fe35290675aac4e6ce323a8ebc75bde1c34d373b1df7506f7f8a65b69ffea950`; its sidecar SHA-256
is `00a560832428e045affad08617646f7e3d322e07c4849d20e5912be6d545595b`.

The controlled official Ubuntu AppImage run used Bambu Studio `02.08.01.55`, AppImage SHA-256
`e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995`. It observed exactly one
model-task request and one HTTP 200 response, followed by exactly one ordered sequence of
`request started`, `response accepted`, `callback started`, and `callback returned`. The evidence
manifest SHA-256 is `c6ba9b6282581119d3baec720e26990ad63efc20eb394b0c71dced89081d5fd9`.
The redacted bundle `pandar-final16-real-studio-evidence-019f7b10.tar.gz` is 245,225 bytes with
SHA-256 `f07c369ad9e0354ef40142294d9385e9c454fd534a04badce4be000f49c06eca`; an independent
second generation matched byte-for-byte. The archive contains only safe `evidence/` and `outer/`
artifacts and contains no runner or mock implementation and no synthetic token contents.
Its `.sha256` sidecar has SHA-256
`30c6e5d43b74f9770d19638b86cefddd96d4d861c16155c74d30b488adf7f1b6`, and
`sha256sum --check` passed.

This final16 AppImage result is deliberately narrow: it uses a synthetic persisted Studio session and
a loopback mock. It is not real authentication, Hub, Agent, database, printer, hardware, print, or
firmware evidence. No GitHub Action or Windows Studio process was used.

## Historical Final14 Linux Candidate Evidence

Final14 was the verified Linux candidate for the post-final13 Studio `fun` bit 48 capability repair
and Better Auth Studio return-intent repair before final16. It is frozen at source
`HEAD 2ba0d1f2755501ea9e7d4babcf176db40638f643`. Its immutable source archive is 2,782,539 bytes,
contains 1,548 regular members, and has SHA-256
`c422d80d89052732db6b8ae87b68fd1e4145c64f588d8382deafef3345d86681`. Canonical-tree,
member-list, and freeze-evidence SHA-256 values are
`43a4a577fb90327dad9e59bcb89dc1e91352bad83f27786a32cae34cb62136e5`,
`5b32472c9372a992c23315d9b33691a0f269248b65db312590ed00556e21aac0`, and
`70d545770086c6acde271d3181508adf4f0d91fc8213771363ec78b2792f5ec3`.

| Target | Source identity | Archive SHA-256 | CLI startup | Exact three-file layout | 130-name Studio contract set | Companion sentinel/no `Bambu_*` | Native ABI probe | Exact Studio load |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `linux-amd64` | final14: `2ba0d1f2755501ea9e7d4babcf176db40638f643 + c422d80d89052732db6b8ae87b68fd1e4145c64f588d8382deafef3345d86681` | `4e91f2457197532102544b02d4edac5354dc2982ec55fa707a057cbcba518b68` | `passed` | `passed` | `passed` | `passed` | `passed` | `passed` (official `02.08.01.55` AppImage; development no-auth only) |
| `windows-amd64` | final14 source frozen; no final14 native package | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` |
| macOS | `none` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` |

Final14 Linux Nextest run `d2231751-1284-46b0-aee6-2e041ca1a203` completed 1,781/1,781
executed tests with one separately reported skip in 812.413 seconds. Module-size checks passed 2/2.
The native ABI caller reported 109 network plus 21 File Transfer exports and passed all five
`version,bind,print,ams,ft` modes; packaged release-smoke passed 21/21.
The strict workspace Clippy command exited successfully with Rust `-D warnings`, but its retained log
contains C++ missing-field-initializer diagnostics and a `proc-macro-error2` future-incompatibility
warning. This gate is therefore not described as warning-free.

The exact three-file archive `pandar-final14-linux-amd64-019f7b10.tar.gz` is 24,854,111 bytes with
SHA-256 `4e91f2457197532102544b02d4edac5354dc2982ec55fa707a057cbcba518b68`.
Its sidecar has SHA-256 `c7e95f887fe415bcc592ab4475a4a6d5a070344d855c22e95ba7efe1323938a1`.
`pandar` is 64,238,920 bytes with SHA-256
`63b5f8d656839b5dc61d840060ad541eed05b365d2cd645005aabd246b38c364`;
`libpandar_network_plugin.so` is 11,826,136 bytes with SHA-256
`c95d06c41e2ecbcec4f28ef722f37d6f279715c7b2d95089f49a19e1247ff7fc`; and
`libpandar_bambu_source.so` is 403,152 bytes with SHA-256
`88d34358be39ed3d239aeb317df8f34a92d4652877e86a9849c66e32347c1df2`.

A separate 10,990,416-byte ASan plugin with SHA-256
`dd7aafe31d67909e201eb9d7af1c0f8f23afb8d913645b3b3e26deecb7ecdc7b` passed all 21 File
Transfer entrypoints for 256 ownership cycles each. Its sanitizer log SHA-256 is
`8b79c97e93c3eb26f3a650a9f5d84aefd820f682609cc5dc659d588cb92a7523`.
The 202,300-byte Linux evidence bundle has SHA-256
`db6a464ce6b9b4b5e4689e1f0f21962dd097349056e78beb57a8779e1352cb02`.

Final14 exact-AppImage attempt 1 loaded this package in the official Ubuntu 22.04 Bambu Studio
`02.08.01.55` AppImage with SHA-256
`e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995`. The fixed seed database
and extracted `AppRun` SHA-256 values are
`72b7d020ef537c7bd510910086d9dcafd3ad0e38e24614216630e27767a46be0` and
`eaf5a1c6ff4f0d49d6e0c0bacf106309daa2c822ca1ebe8739067699e6cdaef4`. Studio remained in the
same PID/start-tick identity, both package libraries had four process-map lines, loader/certificate
error counts were zero, and exactly one development no-auth session was observed.

Redacted archive `pandar-final14-appimage-redacted-evidence-019f7b10.tar.gz` is 10,603 bytes with
23 members and SHA-256 `7eac6abbc7364928147d60dd1c583d084c02debf1552734bc82a4dec59c941be`.
Its canonical member-list, manifest, evidence-files, and result-summary SHA-256 values are
`a39eef283f7ca81d1d6f0b3150de79f17fa7e052fdaf181b2aff88fec67146cc`,
`1326bf59616feaecf184e9015b4dfdc4ee9469f1495786ebf1b1f2e2c60ac295`,
`7f9736c045af21e51f29cc46ffdc82ac9affd593d0eb53963ac4d488aaa2bcf0`, and
`ea6c284576c2342f501d3803daafde584502fcf753513b301f2890b6aee1261a`.

This evidence promoted final14 as the then-current verified Linux native and exact-AppImage
development no-auth candidate. It did not prove the Better Auth sign-in page, localhost ticket,
authenticated token/profile, printer/task UI, logout UI, Windows or macOS Studio, hardware, live
firmware, or the pinned model-task overload.
The independent final14 evidence-document review returned `APPROVE` with no Blocking, Important, or
Minor finding after the recorded warning and candidate-identity wording was corrected.

## Historical Final13 Candidate Evidence

Final13 is frozen at source `HEAD 2ba0d1f2755501ea9e7d4babcf176db40638f643`. Its immutable build
input `pandar-bambu-final13-019f7b10.tar.gz` is 2,751,227 bytes, contains 1,543 regular members, and
has SHA-256 `71080abb1e7392b0440a179b5bca9fd80638de74a614105b8dc11a0f70959c34`.
Canonical tree, member-list, and freeze-evidence SHA-256 values are
`db0b7c3385c29ff0cdee1930a66f554a6845b58907373ef543563b829c245761`,
`87a6ad1dfaa404731ed30d7e265303cca64fc4278a478f9c12192c09373eb880`, and
`4d132e16f91365795f54c97f608483c34b55726c5f614f5bb8ffaac2ede1fb7f`. The independently
regenerated archive matched byte-for-byte. Unsafe member, duplicate, case-collision, source reparse-
point, extracted reparse-point, membership-diff, and content-diff counts were all zero. Pre-freeze
plugin run `da32fbc4-f37e-4198-af5e-c35f73512dcb` passed 368/368 with one separately reported skip.

| Target | Source identity | Archive SHA-256 | CLI startup | Exact three-file layout | 130-name Studio contract set | Companion sentinel/no `Bambu_*` | Native ABI probe | Exact Studio load |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `linux-amd64` | final13: `2ba0d1f2755501ea9e7d4babcf176db40638f643 + 71080abb1e7392b0440a179b5bca9fd80638de74a614105b8dc11a0f70959c34` | `4166e6012e6c1bf7cdf056ba3bfb28f0fbc9d216c31e5ed2e8620adb8b5fcccc` | `passed` | `passed` | `passed` | `passed` | `passed` | `passed` (official `02.08.01.55` AppImage; authenticated session remains untested) |
| `windows-amd64` | final13: `2ba0d1f2755501ea9e7d4babcf176db40638f643 + 71080abb1e7392b0440a179b5bca9fd80638de74a614105b8dc11a0f70959c34` | `6c50e77a0b4008ce46d86de51411117061c5118e18849ca1fb94f4a3f319db64` | `passed` | `passed` | `passed` (271 total PE exports) | `passed` | `passed` | `untested` |
| macOS | `none` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` |

The final13 Windows clean run `90cb6a69-08a5-4421-a661-58e696c374a3` passed 1,778/1,778 with one
separately reported skip in 1,050.084 seconds; the firmware probe passed in 28.858 seconds. Fmt,
strict workspace Clippy with zero warnings, module-size 2/2, ABI-tool 21/21, release-smoke-tool 21/21,
frontend 37 files/324 tests, typecheck, zero-warning lint, and production build all passed. `npm ci`
reported six audit vulnerabilities (three moderate and three high). That dependency-audit observation
is retained here and is not recategorized as a Bambu Studio parity failure. Clean evidence SHA-256 is
`c1ac8807a427ae4b7003681e9ad343d668dab1d6aa7c143d14bc699fe58b7b89`.

The final13 Windows archive `pandar-final13-windows-amd64-019f7b10.tar.gz` is 21,285,752 bytes with
the SHA-256 recorded in the table. It passed 21/21 pinned ABI checks and 21/21 packaged release-smoke
checks, all five `version,bind,print,ams,ft` modes, the 109-network plus 21-File-Transfer contract, and
the companion one-sentinel/zero-`Bambu_*` checks. `dumpbin` reported 271 total network-plugin exports.
Its 111-byte basename-only sidecar has SHA-256
`0d908e117a58bdd951f49b66ca40d84555f54b1d097dfdabbd3bac2acdf10922`. `pandar.exe` is
48,279,552 bytes with SHA-256 `a73fbe47a56fd557f14912e0e774007e0a4774ce83f250ce2a9cc41e52da8d57`;
`pandar_network_plugin.dll` is 8,681,984 bytes with SHA-256
`7861e454eb9dd6122eabc6252102de462228eec526f47149e00a037d3dc48eba`; and
`pandar_bambu_source.dll` is 106,496 bytes with SHA-256
`eaf98016c7d38cb6121a525a0f7a5bb5f0c59df333722798c5f76cee279fdfe6`.
Build, package validation, ABI, and packaged-smoke run ids are
`0430ad0e-7f96-41c5-b9aa-1c6fd690fd16`, `c8faa87d-3085-4d1c-81cf-b6ad9cdc0d8b`,
`2f27f859-b795-4420-b04a-30410ae7bcbc`, and `65ffc0b0-e17e-45da-bd3a-3375f5d88de1`.
The host was Windows 11 x64 build 26200 with Rust/Cargo 1.96.1, Visual Studio Build Tools 2022
17.14.37411.7, and MSVC 19.44.35228 Release `/MD`.
Consolidated native evidence SHA-256 is
`3dab4bffa359e4c46eec77cbfb278ce3a1497f806a1d80343a1735b5a68f025b`. Six earlier pre-product
manifest-harness calibration attempts are retained as infrastructure-only history; they did not run or
fail a Studio product contract. No Studio process, authentication, printer, firmware action, or GitHub
Action was used.

PostgreSQL 16.14 harness `0c292295-f9ab-459b-89c2-ea74f2c9ff56` ran
`24b49c19-cd07-42b5-a5a3-6d220345bd7e` and `1f4b8458-6397-4c0b-8ab3-23d37779c68a`; each passed
55/55 with 831 filtered and zero runtime skip markers. Their log SHA-256 values are
`b123f495e09de3c57c2c175000a37cc1fa7395dd0a9c52f1c2f72426c2f4dc08` and
`b3e233f50fe1be9df43867e34307fd6193f09a2dc00940318bdfb8827f0a8d54`; normalized evidence SHA-256 is
`7e04ae355f7bca3fb409bbc700b5c8f160194c0d2f9ec82df823c859566a2db7`. The frozen source remained
read-only and remote container/temp cleanup passed.

Final13 corrected Linux attempt 2 passed as a whole. Nextest run
`6ec3a215-9430-4ad2-adc7-f692ca156333` completed 1,779/1,779 executed tests with one separately
reported skip in 792.687 seconds; the exact firmware fixture passed in 27.315 seconds. Fmt, zero-
warning strict Clippy, module-size 2/2, ABI-tool 22/22, and release-smoke-tool 21/21 passed. The native
ABI caller reported 109 network plus 21 File Transfer exports and passed all five modes.

The exact three-file archive `pandar-final13-linux-amd64-019f7b10.tar.gz` is 24,854,768 bytes with
the SHA-256 recorded in the table. Its 109-byte sidecar has SHA-256
`2fa2a17bc39bd5ac31fe121f84d1747d17cf9bd8fb4dcc838eb21f4b997b6d26`. `pandar` is 64,238,928
bytes with SHA-256 `7c44138d559ee62d02d4ac7fe0c23c7091e99a7782aac8a163a0c3565458d77f`;
`libpandar_network_plugin.so` is 11,826,376 bytes with SHA-256
`f9baf8346901fdc2ba20aeee786029e47af495bad3ee2e754f440db89010be24`; and
`libpandar_bambu_source.so` is 403,152 bytes with SHA-256
`88d34358be39ed3d239aeb317df8f34a92d4652877e86a9849c66e32347c1df2`.

The Ubuntu 22.04.5 native environment used Rust/Cargo 1.97.1, Nextest 0.9.138, GCC/G++ 11.4.0, and
glibc 2.35. All three packaged files require at most `GLIBC_2.34`; the plugin's ceilings are
`GLIBCXX_3.4.29` and `CXXABI_1.3.9`. ABI, packaged-smoke, ELF-audit, and Nextest log SHA-256 values are
`cb5929a2698384fa52f8415cdca3908778a9fbd4724cda7bca31f3e85fdc29d2`,
`d75786fadfc3ec3fe0dbf0f8780d96b7c04846ad9462e2c607a0dc006d63566d`,
`82bf768ad39885698a1135a343ea502c8ca278cc0e4b7da8211e4cdcb32230f1`, and
`8a3d35b235ecb7f92fe0234f2f37c1d31fb4582c13b6dc288e5128b8410fb98f`.

A separate 10,990,416-byte ASan plugin with SHA-256
`26602e86750103802400f289a68b2430a28ac85188933a5bcb5a37c701b14e19` passed all 21 File
Transfer entrypoints for 256 ownership cycles each under ASan/LSan with zero sanitizer report. Its log
SHA-256 is `8b79c97e93c3eb26f3a650a9f5d84aefd820f682609cc5dc659d588cb92a7523`.
The 288,601-byte evidence bundle has SHA-256
`aa7478fe0f74debcc5f3d1f5ec53a2222d726beafe5224935aa3382c24f6097a`. Final post-audit found
1,543 source files, zero source symlinks, a clean pinned Studio checkout, and zero task containers.

Attempt 1 remains non-promotable harness history. Nextest run
`c8a134c4-e775-4f37-b6ed-74ccb1b79123` passed 1,779/1,779 with one skip, but the outer wrapper
incorrectly expected `plugin_exports=21` from an FT-only invocation while the checker correctly
reported the whole library's 130 contract exports; the overall attempt exited 1. The final evidence
bundle preserves this failure and the corrected expectation.
A pre-final Linux tree with manifest SHA-256
`668f541a8e535018495d8a8969fa6a6d5b70daef49ed848c4c03ab19c40e4f9a` and source-archive SHA-256
`e8c4d17505e9102b7f9fa3fbce8e653dddc7277b33f02671f603818fc1580b3b` passed the exact firmware
probe 21/21, but that remains behavioral stress evidence only.

### Final13 exact-AppImage artifact evidence

Attempt 8 loaded the passed final13 Linux package in the official Ubuntu 22.04 Bambu Studio
`02.08.01.55` AppImage. The AppImage SHA-256 is
`e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995`; official seed database and fresh
`AppRun` SHA-256 values are `72b7d020ef537c7bd510910086d9dcafd3ad0e38e24614216630e27767a46be0`
and `eaf5a1c6ff4f0d49d6e0c0bacf106309daa2c822ca1ebe8739067699e6cdaef4`. Each package library
had four process-map lines in unchanged Studio PID `137`/start ticks `192688662`. `ldd`, undefined-
symbol, `dlopen`, and certificate-error counts were zero.

The same process recorded two offline failures before Hub PID `674`/start ticks `192689166` became
ready, followed by exactly one success and one commit. Final active/total token count was `1/1`, and
create/revoke/discard counts were `1/0/0`. The mode-`0600`, 343-byte login file had SHA-256
`c67cbb2470085de83fb5f0cd79119c3cf70d97f56d424b657da1a00943b47e99`; its content was not
captured. Setup Wizard window count was zero with no interaction or UI injection. Final network state
was `none`, and cleanup left zero task containers/processes.

Redacted archive `pandar-final13-appimage-redacted-evidence-019f7b10.tar.gz` is 7,211 bytes with 23
members and SHA-256 `a4453c8dce3829cc1a84a372a772b516812fe1564b310e61db9e9009a11cf9d2`.
Manifest, member-list, and hashes-file SHA-256 values are
`7ef2a8547ba767f5d0be174b491fa40c2946a0add71adb4043a9abe8d54c1a8a`,
`d79e3f0b6b3672241324a11a2b7f7d8d727c464303f11cbe8745c4f8e60e496f`, and
`ee623a39f5db110b9c26076bdb9a9b440404170402cc3fe840e402cdce2ee1a9`. All external and 21
internal hashes passed, and evidence symlink count was zero. Raw state, database files, and login
content were not retrieved.

The task-local locale came from official Ubuntu `locales` `2.35-0ubuntu3.13`: deb SHA-256
`81c263acc29288d1684f845a5f2cb63bc5d8cc867ac3830acc46aa177ac7a7cc`, `en_US` source SHA-256
`38e3102344829f4ef998db66d064c0082b4bd1c8cf95e35ac3de12bb9f1d62f5`, UTF-8 charmap SHA-256
`a743fdbdb2d4b62a20fe1cf8565215ec12b03a8b71ff26b3f789bf97c3c737ff`, and 12-file `LOCPATH`
manifest SHA-256 `88421fcda8c7577fe7d1bc2769cdbf71a2317f388566247769cdd87cf8f0b1f5`.

Attempts 1-7 are retained as non-product harness calibration: 1-3 lacked usable `en_US` and targeted
the wrong data directory; 4 identified the language modal with read-only `xwininfo`; 5 showed
`C.utf8`/`en` was insufficient and exposed an incorrect `locale -a` verifier despite successful
`localedef`; 6 passed locale setup but found the Beta directory/first-run Wizard boundary; 7 used a
valid built-in preset but still wrote `BambuStudio` rather than `BambuStudioBeta`. Attempt 8 used the
runner's existing Beta `DATA_DIR`, task-local locale, and built-in `X1C0.4` preset and passed every
unchanged-process, load, credential, network, and cleanup assertion.

This row proves exact module load and development no-auth same-process recovery. It does not prove an
authenticated ticket/session, Studio printers/jobs/print/logout/unsupported UI, a real printer,
hardware action, or live firmware.

The final13 implementation review returned `APPROVE` with no Blocking, Important, or Minor finding.
Compared with final12, the product-code repair changed only four Rust connection files, made no C++
ABI change, and kept the largest production connection module, `connection.rs`, at 388 lines. The
final evidence-document review completed after correcting its sole Minor terminology finding.

### Historical final12 frozen evidence

Do not fill a final13 field from this subsection. Each completed final12 native archive, the
PostgreSQL run, and the Windows workspace gates used the same immutable frozen build-input archive.
Linux full validation later exposed a background-refresh/firmware-callback race, so final12 is
non-promotable even where an individual gate passed.

The frozen input is `pandar-bambu-final12-019f7b10.tar.gz`: 1,543 regular-file members, 2,740,698
bytes, SHA-256 `17371828ef7a26cace73cfbed321d094bf38323670e8fa6ccf69d6cbfd4b7eee`,
based on `HEAD 2ba0d1f2755501ea9e7d4babcf176db40638f643` plus the selected dirty files. Its canonical
source-tree/manifest SHA-256 is
`5aa0038dbc3f0962cc172646876263b0db04e1e6df5fbe571553af1967f242a6`, and its member-list
SHA-256 is `87a6ad1dfaa404731ed30d7e265303cca64fc4278a478f9c12192c09373eb880`.
The actual PowerShell selector checks the repository-relative path, preserves duplicates for explicit
rejection, and applies ordinal ordering:

```powershell
$selected = @(
  git -C $repo -c core.quotepath=false ls-files --cached --others --exclude-standard |
    Where-Object { Test-Path -LiteralPath (Join-Path $repo $_) -PathType Leaf } |
    Where-Object {
      $_ -notlike 'reference/*' -and
      $_ -notmatch '^crates/pandar-network-plugin/probe-' -and
      $_ -ne '.superpowers/sdd/progress.md'
    }
)
[Array]::Sort($selected, [StringComparer]::Ordinal)
$selected
```

The freeze rejected absolute/traversal/backslash names, ordinal duplicates, case-insensitive
collisions, and source reparse points; all four counts were zero. `tar -tvzf` proved that all 1,543
archive members were regular files and in exact selector order. Independent extraction found zero
reparse points and zero membership differences. Before-freeze, extracted, and after-freeze manifests
all had the source-tree hash above, and a separately assembled determinism archive had the identical
archive SHA-256. Freeze evidence SHA-256 is
`eb73daaa4f5eb099a47bb8f63bce745eb039234bd8ed19c879ed97e0dba8d47f`. The excluded reference
checkout, generated `probe-*` directories, and local SDD progress ledger are not build inputs; native
gates inject the separately pinned clean Studio checkout explicitly.

| Target | Source identity | Archive SHA-256 | CLI startup | Exact three-file layout | 130-name Studio contract set | Companion sentinel/no `Bambu_*` | Native ABI probe | Real Studio |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `linux-amd64` | historical final12: `2ba0d1f2755501ea9e7d4babcf176db40638f643 + 17371828ef7a26cace73cfbed321d094bf38323670e8fa6ccf69d6cbfd4b7eee` | `untested` (not produced) | `untested` | `untested` | `untested` | `untested` | `failed`: full wrapper rejected the otherwise successful C++ fixture on the exact stale-generation diagnostic | `untested` |
| `windows-amd64` | historical final12: `2ba0d1f2755501ea9e7d4babcf176db40638f643 + 17371828ef7a26cace73cfbed321d094bf38323670e8fa6ccf69d6cbfd4b7eee` | `b4f6913eef7c1d09da9377fbce36b0ab759add25caac2baa0604c07a595440cb` | `passed` | `passed` | `passed` (271 total PE exports) | `passed` | `passed` | `untested`; entire candidate is non-promotable |
| macOS | `none` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` |

The historical frozen final12 Windows clean gate passed fmt, strict Clippy, module-size 2/2, ABI-tool 21/21,
release-smoke-tool 21/21, frontend 37 files/324 tests, typecheck, zero-warning lint, and production
build. The first full workspace Nextest run `15b29e61-e9ae-4bcd-95c3-dfb61406f97d` completed
1,775/1,776 and failed only
`firmware_probe_wires_native_cloud_and_lan_behavior` with `firmware version refresh failed`. The exact
test passed in isolation in run `6fcc569c-7714-4f2d-b901-47f34120a268`; because isolation alone was
not a clean gate, complete workspace run `5e6f3720-4c1b-4a55-ac34-2250c0cefba7` was then executed and
passed 1,776/1,776, with one separately reported skipped test and the firmware and printer-presence
probes both passing. The source archive and tree hashes remained unchanged and the post-gate
membership diff was zero. Clean-gate evidence SHA-256 is
`a6fc922b5069c78dcbe077f6c4238794777f6b17b62574ee75638c46256fb342`.

Historical frozen final12 PostgreSQL 16.14 harness run `3e00d36c-7fb9-47d3-b71b-d9735ebe0eae`, Nextest run
`0b708279-6183-4477-9f78-31add8d7f423`, command `test(/postgres/) -j1 --no-fail-fast`, passed
55/55 with zero skip markers; 831 tests were outside the focused filter. Its evidence and test-log
SHA-256 values are `d7f002f5be8708844cce406895503ef7056b634bf04aad068722eb25ef15247e` and
`456ebcb37e91c7ac688a3537ecdb773d462d8037f37666da0071561ed226b87c`.

The no-auth artifact result is a development profile, not authenticated-session evidence. Hub issues
a credential only for exactly one tenant. Startup permits at most five attempts including the initial
attempt, and retries only a proven pre-delivery connection failure with a two-second initial delay,
30-second cap, and lifecycle/generation fences. Printer and task `401`/`410` reads share one no-auth
rotation and retry each logical request at most once; authenticated sessions never fall back to
no-auth.

Account persistence is serialized across Studio processes by `.pandar-plugin-account.lock`.
`Confirmed` means the namespace change and parent-directory durability were confirmed;
`ChangedUnconfirmed` means the change was published but durability is uncertain; an ordinary error
leaves the current canonical namespace unchanged without promising crash-durable rollback. Only
`Confirmed` login or revocation state is admitted. Requested logout stages pending intent first and,
if that cannot be confirmed, requires a confirmed direct intent before DELETE. Passive logout does
not revoke, but a concurrent request upgrades the same transition. Successful revocation records only
Hub URL plus token SHA-256 in the unbounded completed-revocation ledger, which blocks stale login
rewrites; direct completion removes duplicate pending state best-effort without undoing a successful
DELETE. Manual ledger cleanup requires every Studio process using the directory to be stopped and all
corresponding Hub sessions to be revoked, invalid, or expired.

For each completed native row, also record:

- Rust target and compiler version;
- C++ compiler/version, standard-library ABI, and runtime symbol baseline;
- network-plugin and BambuSource SHA-256 values independently from the archive hash;
- pinned Studio source path/commit and Boost archive SHA-256;
- release-smoke and pinned ABI-probe commands plus their run ids;
- redacted failures with their full lower-level cause chain.

### Historical final11 Linux amd64 evidence

Every value in this subsection is explicitly retained final11 regression evidence and is not a
current install candidate. Final13 corrected native/ASan attempt 2 and exact-AppImage attempt 8 passed
from the frozen input.

- Historical final11 frozen source: 2,719,551 bytes, SHA-256
  `345e7eb04d07a7f424ea3343fa9d5baa5b6ac7541a6e5001160ceed0b4c6c020`.
- Historical archive: `pandar-final11-ubuntu22-linux-amd64-019f7b10.tar.gz`, 24,805,193 bytes,
  SHA-256
  `7b7ac417e1c781fbb682552676822457cac6f57a1eb1dd288f2d851f1181a0c6`.
- `pandar`: 64,223,736 bytes, SHA-256
  `bcd7c6bb742ab56bb3c2031e7e74599a3fabf37b04569d03c67bd4370ddfcce4`.
- `libpandar_network_plugin.so`: 11,692,560 bytes, SHA-256
  `04665524141566129669198180c252b0e02bbe2f341f2103f46142c0777c4ab7`.
- `libpandar_bambu_source.so`: 403,152 bytes, SHA-256
  `88d34358be39ed3d239aeb317df8f34a92d4652877e86a9849c66e32347c1df2`.
- Native build environment: target `x86_64-unknown-linux-gnu`, Ubuntu 22.04.5, Rust/Cargo 1.97.1,
  GCC/G++ 11.4.0, and glibc 2.35. All three files have a highest
  requirement of `GLIBC_2.34`; the plugin additionally requires `GLIBCXX_3.4.29` and
  `CXXABI_1.3.9`. Every `NEEDED` library resolves on Jammy.
- The full ABI command exited 0 with `contract_scope=full`, 109 network plus 21 File Transfer symbols,
  the complete 130-name Studio contract set, and all `version,bind,print,ams,ft` modes passed. Packaged
  release-smoke also exited 0 with native CLI execution, native plugin execution, exact layout, and
  companion sentinel/no-`Bambu_*` inspection. The ABI output, release-smoke output, and ELF audit have
  SHA-256 `cb5929a2698384fa52f8415cdca3908778a9fbd4724cda7bca31f3e85fdc29d2`,
  `f1eead03bd2c238e6a54ed7f803ac093b5fdd372dd049d7af4589c3f77a11d7f`, and
  `a86093fc9f7c7b5f584dc304bb0b135f92c86d8b873e1c00af810a0ef218f7f6`.
  The aggregate Linux evidence SHA-256 is
  `16b26f92b8b5f623b4d5c3d0a83e10c1db76de4ccaadf5fce6573fffe63811b4`.
- A separate 10,898,552-byte ASan plugin with SHA-256
  `d6649b65c93cb118510e6ecb8382fd35472206b678d894e47816078032ed2d20` imports
  `libasan.so.6` and passed the 21-entrypoint × 256-cycle File Transfer ownership scope under
  ASan/LSan. Its log SHA-256 is
  `8b79c97e93c3eb26f3a650a9f5d84aefd820f682609cc5dc659d588cb92a7523`.
- The official Ubuntu 22.04 AppImage used for the historical final11 run has SHA-256
  `e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995`.
  That evidence recorded an isolated headless launch, no persistent host-setting change, and no Setup
  Wizard interaction; it does not define or predict the final13 runtime environment. Studio mapped
  each installed final11 library in four process-map lines and emitted the post-agent getter three
  times. In pinned source that getter is reachable only after the version and BambuSource gates,
  `NetworkAgent` construction, plugin agent creation, and agent startup. PID `2176` with start ticks
  `190073915` remained unchanged while two proven pre-delivery connection failures were followed by
  one HTTP 200 commit after Hub became ready.
  The redacted result recorded `retry_attempts=2`, `commits=1`, `discarded=0`, one active session, one
  create audit, zero revoke audits, and one mode-`0600` 343-byte login file. Undefined-symbol,
  `dlopen`, and certificate error counts were zero. The complete evidence, result, timeline, and
  component summaries have SHA-256
  `cc8a0ef1f16bfc3a109345f9ada4e15096ca5fcf6f6b50c82387cce53aee55dd`,
  `abb2abab9306387d2cee16f14e2d60fdde8071c92b44b4c79417e4f81b924bb5`,
  `1d01ee008458af445237778684b2b5b1f8a53c0ddd785477ddf83404beef9c71`, and
  `380d5281cc8012943348a4cb95d16a6eebb3e1704a2baccb8f4a34dc15072605`.
  This is same-process no-auth development evidence; authenticated WebView/ticket exchange, Studio
  printer/task UI, requested logout UI, hardware, and live firmware remain `untested`.
- Historical final5 startup-recovery regression only: the same Studio process did not bootstrap within
  30 seconds after Hub recovered, while restarting Studio against the ready Hub created one session.
  Its redacted evidence SHA-256 is
  `7f103873d222b8b51e1209c4836f2acc2579515cff9729dd89c4271032e801b0`; it is not current-candidate
  evidence.

### Historical Windows amd64 final12 native artifact

- Frozen source: 2,740,698 bytes, 1,543 regular members, SHA-256
  `17371828ef7a26cace73cfbed321d094bf38323670e8fa6ccf69d6cbfd4b7eee`; independently extracted
  and post-build source trees both matched
  `5aa0038dbc3f0962cc172646876263b0db04e1e6df5fbe571553af1967f242a6` with zero reparse points.
- Archive: `pandar-final12-windows-amd64-019f7b10.tar.gz`, 21,285,799 bytes, SHA-256
  `b4f6913eef7c1d09da9377fbce36b0ab759add25caac2baa0604c07a595440cb`. Its two-field,
  basename-only sidecar has SHA-256
  `8194c6a6bc4f8b2f0afb59bc49a239872d3724631734f0057449c51fa470a6ee`.
- `pandar.exe`: 48,279,552 bytes, SHA-256
  `1e57a7cfc2b46717129e7ced227b358eedbaaa74064f2ae2ac5cd44eac576b32`.
- `pandar_network_plugin.dll`: 8,681,984 bytes, SHA-256
  `43be9e73350cacb66ee2dfa991f1a7291175c4d18db2ec917a10a1489f9244d9`.
- `pandar_bambu_source.dll`: 106,496 bytes, SHA-256
  `20805176609ebe891ed45bc7171a34ad0d741351b5dbe8c3c4d9f9b4a5a2a49a`.
- Toolchain: Windows 11 x64, `rustc 1.96.1` host `x86_64-pc-windows-msvc`, Visual Studio Build
  Tools 2022 `17.14.37411.7`, and MSVC `cl.exe 19.44.35228` x64 with Release `/MD`. The plugin
  imports the expected UCRT plus `msvcp140.dll`, `vcruntime140.dll`, and `vcruntime140_1.dll`; the
  companion imports UCRT plus `vcruntime140.dll` and has no media dependency.
- Build run `4fa89d78-503f-4c51-a4e3-fc788a4f7f03`; full pinned ABI run
  `6b71c048-8377-4a61-a750-20c5531df864`; packaged release-smoke run
  `d808cce0-6e5f-45e7-b4aa-f7b39642d67a`. The packaged staged plugin passed all five native modes,
  the 109-network plus 21-File-Transfer Studio contract set, CLI `--help`/version `0.1.0`, exact
  three-file regular-only layout, and companion sentinel/no-`Bambu_*` checks against official clean
  Studio commit `ba049f6a2e08c3b6033660bb84da80c08722974b` and pinned Boost SHA-256
  `4d27e9efed0f6f152dc28db6430b9d3dfb40c0345da7342eaa5a987dde57bd95`.
  `dumpbin` reported 271 total network-plugin PE exports; 130 of them are the Studio contract set and
  the remainder include Pandar flat-FFI and aws-lc exports. The final12 Windows evidence SHA-256 is
  `11c38eb3c198cd07b2f96abbfbf70792b078170389e8869b230badbb98a404d2`.
- Real Windows Studio remains `untested`; no Studio GUI/process, authentication, or printer hardware
  was used.

## Historical Two-File And 129-Symbol Contract Evidence

Everything in this section predates the current BambuSource requirement and 130-name Studio contract
target. These archives contained only the CLI plus network plugin and checked a 129-name contract
set. They are retained as historical Phase 24 evidence, not as current `02.08.01.55` install
candidates.

| Run or artifact | Date | Targets | Historical result |
| --- | --- | --- | --- |
| `local-a79bcae` | 2026-06-24 | `linux-amd64` | Two-file local archive passed checksum, layout, CLI startup, and 129-name contract inspection. No release or real Studio claim. |
| workflow dispatch `28098334876` | 2026-06-24 | Linux, Windows, macOS | Linux artifacts uploaded; Windows plugin packaging and macOS CLI linking failed. Historical workflow evidence only. |
| workflow dispatch `28099917011` | 2026-06-24 | Linux, Windows, macOS | Linux two-file/129-name contract checks passed; Windows C++ runtime linking and macOS export checks failed. Historical workflow evidence only. |
| workflow dispatch `28102001464` | 2026-06-24 | Linux amd64/arm64, Windows amd64/arm64, macOS amd64/arm64 | Old two-file checks passed for Linux amd64/arm64, Windows amd64, and both macOS targets; Windows arm64 inspection failed. A clean Linux amd64 host also passed checksum/unpack/CLI/file inspection. None included BambuSource or proved real Studio. |
| workflow dispatch `28103772270` | 2026-06-24 | all old targets | Blocked before build steps; no artifact evidence was produced. |
| downloaded-artifact follow-up | 2026-06-25 | Linux arm64, Windows amd64, macOS amd64/arm64 | Cross-host static checks preserved the old archive/checksum/file-shape evidence. They did not prove target-host installation or Studio loading. |

No current verification uses GitHub Actions. The old workflow identifiers above are immutable
historical provenance only and are not instructions to re-run or modify a workflow.

## Release Availability Boundary

Release `v0.1.1` is available from GitHub Releases with the tagged evidence recorded above. Historical
workflow artifacts that lack the current three-file layout remain non-promotable. Future releases must
be validated again from their own downloaded archives on each native target.

## Real Studio And Hardware Boundaries

- Native release-smoke is not a real Studio row. Record real load/session evidence in
  `bambu-studio-plugin.md` by following `bambu-studio-plugin-smoke.md`.
- The current exact-Studio desktop smoke is a no-print smoke: load, version/agent creation, sign-in,
  ticket/token/profile, printers, Hub-backed jobs, outage/recovery, logout, and explicit unsupported
  results that require no machine action.
- Automated print-field, lifecycle, cancellation, task, and command-contract evidence is separate.
  It does not prove a hardware print, cancel, live firmware update, or movement command.
- Authenticated Linux session rows, Windows real Studio, macOS, and all hardware actions remain
  untested until separately authorized and recorded.

## Evidence Rules

- Use only the status vocabulary above in status columns.
- Verify the `.sha256` sidecar before unpacking and require it to name only the archive filename.
- Reject duplicate, nested, extra, or symlinked top-level entries.
- Inspect and execute the unpacked packaged artifact, not a separately built development library.
- Keep archive, network-plugin, and companion hashes distinct.
- Redact bearer tokens, plugin tickets, access codes, signed URLs, private host/IP values, and local
  paths from captured output.
- Keep failed, blocked, unsupported, and untested rows; absence of evidence is not success.
- Do not infer real Studio, target-host, or hardware compatibility from static cross-host inspection.
