# Release Installation

Release `v0.1.2` will be published at <https://github.com/ProjectPandar/pandar/releases/tag/v0.1.2>. The tag also publishes these service artifacts:

- `ghcr.io/projectpandar/pandar/hub:v0.1.2`
- `ghcr.io/projectpandar/pandar/web:v0.1.2`
- Helm chart `0.1.2` at `oci://ghcr.io/projectpandar/pandar/chart/pandar`

## Release Archive Selection

Select the archive that matches the operator host OS and CPU architecture:

| Host                 | Target label    | Archive                                                                            |
| -------------------- | --------------- | ---------------------------------------------------------------------------------- |
| Linux x86_64/amd64   | `linux-amd64`   | `pandar-release-<tag-or-sanitized-ref>-studio-<abi-series>-linux-amd64.tar.gz`     |
| macOS x86_64/amd64   | `macos-amd64`   | `pandar-release-<tag-or-sanitized-ref>-studio-<abi-series>-macos-amd64.tar.gz`     |
| macOS Apple Silicon  | `macos-arm64`   | `pandar-release-<tag-or-sanitized-ref>-studio-<abi-series>-macos-arm64.tar.gz`     |
| Windows x86_64/amd64 | `windows-amd64` | `pandar-release-<tag-or-sanitized-ref>-studio-<abi-series>-windows-amd64.tar.gz`   |

The immutable `v0.1.0` release predates macOS desktop archives. The current tag workflow additionally
publishes macOS amd64 and arm64 archives. Both use an Apple Silicon runner: arm64 executes natively,
while amd64 is cross-compiled and executes its CLI, ABI probe, and release-smoke under Rosetta 2.
Every published archive contains exactly three top-level files: the `pandar` CLI (or `pandar.exe`),
the matching network plugin, and the matching BambuSource companion. The current tag workflow builds
each archive on its target OS, validates the pinned Studio contract through the packaged plugin, and
verifies the checksum, exact layout, companion sentinel plus exact 21-entry local-media ABI, and CLI
startup before publication. The immutable `v0.1.0` archives retain their historical sentinel-only
companion and do not contain the later local-camera implementation.
The separate Windows Studio hook bundles are also built natively with MSVC.

Choose the archive whose ABI series matches the first three components of `app.version` in
`BambuStudio.conf`. Supported ABI series are `02.06.00`, `02.06.01`, `02.07.00`, `02.07.01`,
`02.08.00`, and `02.08.01`. The `02.06.00` series requires 103 network plus 21 File Transfer names,
`02.06.01` through `02.08.00` require 108 plus 21, and `02.08.01` requires 109 plus 21. Their C++
ABIs differ, so no archive is a fallback for another series. The installer verifies the installed
Studio version resolves to the archive's ABI series before changing the Studio plugin directory.

## Checksum Verification

Download the archive and its `.sha256` sidecar from the same release. Verify the sidecar before unpacking:

```bash
sha256sum -c pandar-release-<tag-or-sanitized-ref>-studio-<abi-series>-<target-label>.tar.gz.sha256
```

On macOS, use:

```bash
shasum -a 256 -c pandar-release-<tag-or-sanitized-ref>-studio-<abi-series>-<target-label>.tar.gz.sha256
```

The sidecar must name only the archive file, not a local path. Do not install an archive whose checksum fails or whose sidecar does not match the downloaded filename.

## CLI Startup Smoke

Unpack the archive and run the CLI help command before installing it into a shared path:

```bash
tar -xzf pandar-release-<tag-or-sanitized-ref>-studio-<abi-series>-<target-label>.tar.gz
./pandar --help
```

On Windows, run:

```powershell
tar -xzf pandar-release-<tag-or-sanitized-ref>-studio-<abi-series>-<target-label>.tar.gz
.\pandar.exe --help
```

If startup fails, keep the archive, checksum, target label, OS version, and terminal output for the release evidence record.

## Windows Bambu Studio Hook

For each supported Bambu Studio ABI series on Windows x86-64, the tagged release also publishes
`pandar-studio-hook-<abi-series>-windows-amd64.zip` and its `.sha256` sidecar. The bundle is built
natively with MSVC and contains the Studio hook, matching network plugin, and BambuSource companion.
The immutable `v0.1.0` bundle has the historical sentinel-only companion; newly built bundles contain
the constrained local-camera companion instead. The hook accepts only versions present in
`studio-abi-profiles.json` and keeps its verified package cache separate for every ABI series.

Close Studio, then install the hook with a Windows `pandar.exe`. Use an elevated terminal when the
Studio program directory is under `Program Files`:

```powershell
.\pandar.exe install-studio-hook `
  --studio-dir "C:\Program Files\Bambu Studio" `
  --data-dir "$env:APPDATA\BambuStudio"
```

The command resolves the latest GitHub Release, downloads both fixed-name assets, verifies the
SHA-256 sidecar, rejects unexpected ZIP members, installs the current Pandar plugin immediately, and
caches a Studio-shaped `%LOCALAPPDATA%\Pandar\studio-hook\networking_plugins.zip`. The hook remains a `swscale-8.dll` proxy and keeps
the original as `swscale8original.dll`. In supported Studio builds it intercepts only the final
Windows rename of `networking_plugins.zip`, replacing Bambu's downloaded archive with the verified
cached Pandar archive. If that cache is missing, the hook fails the plugin installation instead of
falling back to Bambu's archive. Other Studio downloads are unchanged.

Uninstall the hook while Studio is closed:

```powershell
.\pandar.exe uninstall-studio-hook `
  --studio-dir "C:\Program Files\Bambu Studio" `
  --data-dir "$env:APPDATA\BambuStudio"
```

Uninstall restores `swscale-8.dll` and removes the cached replacement package. It does not remove the
currently installed Pandar network plugin. Set `PANDAR_STUDIO_LOG_LOCAL_KEY=1` before starting Studio
when the existing local log-key patch is also required. The hook refuses plugin-download replacement
outside catalog-supported Studio ABI series; reinstall after a Studio application update because the application
installer may replace `swscale-8.dll`.

## Hub, Web, And Agent Deployment

The release archive provides the operator CLI and Bambu Studio plugin library. Deploy the running services with the existing container or NixOS paths:

- `pandar-hub`: Rust API server, default HTTP/WebSocket bind `127.0.0.1:8080`, default gRPC bind `127.0.0.1:50051`, and default observability bind `127.0.0.1:9090`. External publication must be configured explicitly; non-loopback gRPC also requires TLS.
- `pandar-web`: Next.js frontend, default bind `0.0.0.0:3000`.
- `pandar-agent`: local-network agent that connects outward to Hub gRPC and talks to Bambu machines.

Hub gRPC accepts at most 1,024 transports globally and, by default, 64 simultaneous unauthenticated/setup transports per source IP. Successful Agent or camera credential authentication transfers that transport into bounded established quotas of 8 connections per Agent and 128 per tenant. Agent messages are capped at 1 MiB; camera chunks have a stricter 64 KiB limit, and retained camera HTTP responses default to a limit of 8 per tenant. Set `PANDAR_HUB_CAMERA_MAX_STREAMS_PER_TENANT` to a positive integer to change that tenant limit. There is no process-wide camera HTTP response limit, so multi-tenant operators should enforce an aggregate connection limit at the reverse proxy. Set `PANDAR_HUB_GRPC_MAX_UNAUTHENTICATED_CONNECTIONS_PER_PEER` to a positive integer when a trusted reverse proxy or NAT requires a different startup burst, and enforce connection/rate limits at that proxy because Hub sees the proxy address rather than the original client.

The hub needs `PANDAR_DATABASE_URL` and `PANDAR_PRINTER_ACCESS_CODE_KEY`. The latter is the unpadded base64url encoding of exactly 32 random bytes (`openssl rand -base64 32 | tr '+/' '-_' | tr -d '='`) and encrypts persisted Bambu access codes with versioned AES-256-GCM envelopes. Use the same key on every Hub replica and retain it with database backups; a missing or incorrect key prevents startup. The frontend needs `APP_API_URL`, `APP_BASE_URL`, and provider metadata when external auth is used. Set `APP_PUBLIC_API_URL` to the Hub URL reachable from Studio when `APP_API_URL` is an internal service address; Web publishes it through `/.well-known/pandar` for plugin discovery. The agent needs `PANDAR_HUB_GRPC_URL`, tenant and agent IDs, an agent credential, and any `PANDAR_PRINTERS` entries for local machines. Bambu X.509 v1 leaves directly signed with RSA/SHA-256 by the bundled trusted Bambu CA set require no per-device configuration. If a printer sends a leaf-only TLS certificate whose issuer is absent from that set (observed on P2S), set `PANDAR_BAMBU_CERTIFICATE_SHA256_PINS` to a JSON object such as `{"22E8BJ610801473":"AA:BB:…"}` using a SHA-256 fingerprint verified through a separate trusted channel.

For Clerk, Logto, or Better Auth deployments, configure `pandar-hub` with `PANDAR_EXTERNAL_AUTH_PROVIDER`, issuer, JWKS URL, required audience, and allowed algorithms. Non-loopback JWKS URLs must use HTTPS, and JWKS redirects are rejected. Configure `pandar-web` with `APP_AUTH_PROVIDER` and the matching provider metadata, and leave `APP_API_TOKEN` and `APP_AUTH_BEARER_TOKEN` unset. Unknown Web provider values and external-auth/static-token combinations fail startup. Better Auth 1.6.25 uses `keyPairConfig.alg = "RS256"` for RSA JWT signing, matching Pandar's `PANDAR_EXTERNAL_AUTH_ALGORITHMS=RS256` verifier setting.

For local development or explicitly trusted single-user deployments, `PANDAR_HUB_NO_AUTH=true` disables hub HTTP/WebSocket bearer authentication and role checks and emits a startup warning. Do not enable it on an untrusted network. Agent reverse gRPC authentication remains credential-based.

For a self-hosted Better Auth deployment, run the optional `pandar-auth` service and point the other services at it:

```bash
PANDAR_AUTH_BASE_URL=https://auth.example.com
PANDAR_AUTH_TRUSTED_ORIGINS=https://pandar.example.com
PANDAR_AUTH_DASHBOARD_CALLBACK_URL=https://pandar.example.com/auth/betterauth/callback
PANDAR_AUTH_DASHBOARD_SIGN_OUT_URL=https://pandar.example.com/auth/betterauth/session
PANDAR_AUTH_DATABASE_FILE=/var/lib/pandar-auth/auth.db
PANDAR_AUTH_JWT_MAX_AGE_SECONDS=43200
PANDAR_AUTH_MAGIC_LINK_TTL_SECONDS=1800
PANDAR_AUTH_EMAIL_PROVIDER=resend
PANDAR_AUTH_EMAIL_FROM='Pandar <auth@example.com>'
PANDAR_AUTH_EMAIL_BRAND_NAME=Pandar
BETTER_AUTH_SECRET=<long random secret>
RESEND_API_KEY=<resend api key>

APP_AUTH_PROVIDER=betterauth
APP_AUTH_BETTER_AUTH_BASE_URL=https://auth.example.com
APP_AUTH_COOKIE_MAX_AGE_SECONDS=43200

PANDAR_EXTERNAL_AUTH_PROVIDER=betterauth
PANDAR_EXTERNAL_AUTH_ISSUER=https://auth.example.com
PANDAR_EXTERNAL_AUTH_JWKS_URL=https://auth.example.com/api/auth/jwks
PANDAR_EXTERNAL_AUTH_AUDIENCE=https://auth.example.com
PANDAR_EXTERNAL_AUTH_ALGORITHMS=RS256
PANDAR_AUTH_ALLOW_TENANT_SELF_CREATE=true
```

Production startup rejects non-HTTPS Auth base, trusted-origin, Dashboard callback, and sign-out URLs so bearer delivery and session transitions cannot fall back to cleartext.

For SMTP delivery, set `PANDAR_AUTH_EMAIL_PROVIDER=smtp` instead of `resend` and provide `PANDAR_AUTH_SMTP_HOST`, `PANDAR_AUTH_SMTP_PORT`, `PANDAR_AUTH_SMTP_USERNAME`, `PANDAR_AUTH_SMTP_PASSWORD`, and `PANDAR_AUTH_SMTP_TLS=starttls|tls|none`. Runtime startup fails when the selected email provider is incomplete; builds use dummy email settings only so the Next.js package can be compiled without production secrets.

`BETTER_AUTH_SECRET` is also used by Better Auth to encrypt stored JWKS private keys by default. Rotating it without re-encrypting or clearing the issuer `jwks` table makes existing signing keys undecryptable and breaks JWT issuance.

Keep `PANDAR_AUTH_JWT_MAX_AGE_SECONDS` and `APP_AUTH_COOKIE_MAX_AGE_SECONDS` aligned. If the dashboard cookie outlives the JWT, authenticated dashboard requests will fail until the user signs in again; if the cookie is shorter, users reauthenticate earlier than the issuer token requires.

When Better Auth is used from Bambu Studio, keep `PANDAR_AUTH_DASHBOARD_CALLBACK_URL` pointed at the
dashboard `/auth/betterauth/callback` route. Pandar carries the selected tenant and Studio localhost
callback through magic-link/passkey completion as a versioned base64url `return_to`; the dashboard
accepts only the same-origin `/plugin-sign-in` target and never places the bearer JWT in that target.

For agent artifact downloads, set `PANDAR_HUB_API_URL` when `PANDAR_HUB_GRPC_URL` is not an HTTP(S) URL. Agents authenticate artifact downloads with `PANDAR_AGENT_CREDENTIAL`; do not distribute object-store credentials to agents or browsers.

## Docker Compose Shapes

Use the SQLite compose shape for single-process or local deployments. All Compose shapes require `PANDAR_HUB_GRPC_TLS_CERT_FILE` and `PANDAR_HUB_GRPC_TLS_KEY_FILE` host paths because the remotely published Agent gRPC port is TLS-only. HTTP API and Web ports bind to host loopback for termination by a same-host HTTPS reverse proxy:

```bash
PANDAR_PRINTER_ACCESS_CODE_KEY=<base64url key> APP_API_TOKEN=<tenant token> APP_TENANT_ID=<tenant uuid> docker compose -f docker-compose.sqlite.yml up --build
```

Use the PostgreSQL compose shape when the database must be external to the Hub container:

```bash
PANDAR_PRINTER_ACCESS_CODE_KEY=<base64url key> POSTGRES_PASSWORD=<db password> APP_API_TOKEN=<tenant token> APP_TENANT_ID=<tenant uuid> docker compose -f docker-compose.postgres.yml up --build
```

Use external auth by setting both Hub verification variables and Web provider variables:

```bash
PANDAR_PRINTER_ACCESS_CODE_KEY=<base64url key> PANDAR_EXTERNAL_AUTH_PROVIDER=betterauth PANDAR_EXTERNAL_AUTH_ISSUER=https://auth.example.com PANDAR_EXTERNAL_AUTH_JWKS_URL=https://auth.example.com/api/auth/jwks PANDAR_EXTERNAL_AUTH_AUDIENCE=https://auth.example.com PANDAR_EXTERNAL_AUTH_ALGORITHMS=RS256 APP_BASE_URL=https://pandar.example.com APP_AUTH_PROVIDER=betterauth APP_AUTH_BETTER_AUTH_BASE_URL=https://auth.example.com docker compose -f docker-compose.sqlite.yml up --build
```

Use the PostgreSQL plus NATS profile to run the broker-backed deployment shape with S3-compatible artifact storage:

```bash
PANDAR_PRINTER_ACCESS_CODE_KEY=<base64url key> POSTGRES_PASSWORD=<db password> APP_API_TOKEN=<tenant token> APP_TENANT_ID=<tenant uuid> PANDAR_CONTROL_PLANE=nats PANDAR_ARTIFACT_STORAGE=s3 PANDAR_ARTIFACT_S3_BUCKET=<bucket> PANDAR_ARTIFACT_S3_REGION=<region> PANDAR_ARTIFACT_S3_ENDPOINT=<endpoint> PANDAR_ARTIFACT_S3_ACCESS_KEY_ID=<access key> PANDAR_ARTIFACT_S3_SECRET_ACCESS_KEY=<secret> docker compose -f docker-compose.postgres.yml --profile nats up --build
```

The compose file starts one `pandar-api` service with fixed host ports. For multiple Hub replicas, put replicas behind your own HTTP/gRPC routing layer and avoid publishing the same host ports from every container.

SQLite is for lightweight single-process deployments and rejects the NATS control plane. The SQLite compose shape keeps the filesystem artifact backend and `PANDAR_SPOOL_DIR`. PostgreSQL plus NATS should use S3-compatible artifact storage; a shared filesystem is accepted only with the explicit `PANDAR_ARTIFACT_FILESYSTEM_SHARED=true` readiness override when every Hub replica truly mounts the same artifact directory.

Back up SQLite deployments by capturing the SQLite database file, filesystem artifact directory, and `PANDAR_PRINTER_ACCESS_CODE_KEY`. Back up PostgreSQL/object-storage deployments by capturing the PostgreSQL database, configured object-storage bucket, and the same encryption key. Store the key separately from database media while preserving their recovery association.

## NixOS services.pandar

NixOS deployments use the flake module exposed as `nixosModules.default` and `nixosModules.pandar`. Configure Hub, Web, and Agent through `services.pandar`.

Use root-owned runtime `EnvironmentFile` paths outside `/nix/store` for every NixOS secret. `services.pandar.hub.environmentFile` must contain `PANDAR_DATABASE_URL` and `PANDAR_PRINTER_ACCESS_CODE_KEY`; `services.pandar.agent.environmentFile` must contain `PANDAR_AGENT_CREDENTIAL` and may contain secret-bearing `PANDAR_PRINTERS`; `services.pandar-auth.environmentFile` carries `BETTER_AUTH_SECRET` and the selected email-provider secret; and `services.pandar.web.environmentFile` carries a static single-user token when that mode is used. The module rejects these variables in `extraEnvironment`, and the former plain `hub.databaseUrl`, `agent.credential`, and `agent.printers` options no longer exist. Generated option documentation is in `docs/deployment/nixos/options.md`.

## Bambu Studio Plugin And BambuSource Replacement

Every supported Studio ABI series requires the network plugin and BambuSource companion before agent
creation. Use the platform files from the archive matching the installed ABI series:

| OS      | Network plugin                   | BambuSource companion          | Current validation                    |
| ------- | -------------------------------- | ------------------------------ | ------------------------------------- |
| Linux   | `libpandar_network_plugin.so`    | `libpandar_bambu_source.so`    | next tagged archive remains untested |
| Windows | `pandar_network_plugin.dll`      | `pandar_bambu_source.dll`      | next tagged archive remains untested |
| macOS   | `libpandar_network_plugin.dylib` | `libpandar_bambu_source.dylib` | local arm64 validated; untagged      |

Install both from an unpacked release archive with the CLI:

```text
pandar install-network-plugin --data-dir <BambuStudio-data-dir>
```

When the file flags are omitted, the command reads the platform-specific release files from the
current working directory (`libpandar_network_plugin.so` and `libpandar_bambu_source.so` on Linux,
or `pandar_network_plugin.dll` and `pandar_bambu_source.dll` on Windows). Use `--plugin-file` and
`--source-file` to override either path for development builds. The CLI is compiled for the same
ABI series as its bundled plugin, reads the exact `BambuStudio.conf` version, and fails before
copying when the installed Studio version resolves to a different or unsupported series.

The installer writes Studio's exact names, including `libbambu_networking.so` and
`libBambuSource.so` on Linux, `libbambu_networking.dylib` plus `libBambuSource.dylib` on macOS, and
`bambu_networking.dll` plus `BambuSource.dll` on Windows. The current companion exports
`pandar_bambu_source_sentinel` plus the exact 21 `Bambu_*` entrypoints required for Pandar's
constrained local-camera path. It accepts only a random one-use
`bambu:///local/127.0.0.1?...` relay URL and does not accept direct printer credentials or implement
Bambu cloud/TUTK/Agora media.

Studio live view is available only for A1, A1 Mini, P1S, and A2L when the printer is online through a
current Agent that advertises the camera capability. Every other model fails closed. The printer host
and access code stay in Agent configuration and must never be copied into Studio, its configuration,
or a support bundle. Real-device playback has not yet been validated; use the Web monitor until your
target model and platform have recorded compatibility evidence.

Keep both original Studio library files for rollback. Typical locations vary by Studio installation:

- Linux AppImage or extracted builds: install both exact library names in Studio's data-directory
  `plugins` folder.
- Windows: install both exact DLL names in Studio's data-directory `plugins` folder.
- macOS: install both exact dylib names in
  `~/Library/Application Support/BambuStudio/plugins`. Historical Public Beta builds use
  `~/Library/Application Support/BambuStudioBeta/plugins`; pass
  `--data-dir "$HOME/Library/Application Support/BambuStudioBeta"` explicitly.

### Plugin account-state recovery

The selected Studio config directory contains account state that must move as one serialized
namespace:

| File                                       | Purpose                                                             | Cleanup rule                                                                                                         |
| ------------------------------------------ | ------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `.pandar-plugin-account.lock`              | Cross-process account mutation lock.                                | Leave it in place. Never remove it while any Studio process using this directory is running.                         |
| `pandar-plugin-login.json`                 | Current persisted Studio login, including its bearer credential.    | Do not copy, restore, or delete it independently while Studio is running.                                            |
| `pandar-plugin-pending-revocations.json`   | Durable pending-first revocation queue.                             | Do not delete it merely to suppress a retry; it may be the only recovery path for a still-valid Hub session.         |
| `pandar-plugin-direct-revocation.json`     | Fallback intent that must be confirmed before an unstaged DELETE.   | Keep it until replay completes or the corresponding Hub session is independently invalidated.                        |
| `pandar-plugin-completed-revocations.json` | Hub URL plus token-hash tombstones that block stale login rewrites. | It is unbounded and has no automatic compaction; apply the full manual-reset prerequisites below before clearing it. |

For a manual account-state reset, first stop every Bambu Studio process that uses this config
directory. Then revoke, invalidate, or allow expiry of every corresponding Hub plugin session,
including tokens represented by login, pending, direct, or completed state. Only after both conditions
hold may an operator back up and remove the four JSON state files as one reset; the lock file can
remain. Do not restore an old login file afterward. A successful direct DELETE may leave a duplicate
pending entry only when best-effort cleanup failed; replay is idempotent, so retain the files and let
normal recovery reconcile them rather than deleting either file by hand.

Record real Studio load and sign-in evidence with `docs/compatibility/bambu-studio-plugin-smoke.md`.
Do not treat release-smoke export checks as real Bambu Studio compatibility evidence. The current
desktop checklist is a no-print smoke; automated print/cancel/command evidence and live hardware
evidence remain separate.

## Unsupported Or Untested Targets

Target status is tracked in `docs/compatibility/release-artifacts.md`. For checksum, layout, CLI
startup, plugin exports, and real host install columns, treat `in_progress`, `failed`, `blocked`,
`unsupported`, or `untested` as not proven for operator installation.

Native release-smoke and real Studio evidence are separate. Public Beta package and workflow evidence
is retained only as history; it is not a stable `02.07.01.62` candidate or an instruction to run an
Action.

### Historical Public Beta final16 evidence

The historical final16 immutable input `pandar-bambu-final16-019f7b10.tar.gz` is 2,793,904 bytes with
SHA-256 `24b45dd30c3509c02b609548409f05fa72490512525621dbc0574a05aa62a039`; its canonical
source-tree SHA-256 is `c62c92167f466a915400953ec2d0e126bc34b3c6509a747ddee17dce8d52bf30`.
The preceding pre-fix freeze whose SHA-256 begins `6318d190` and ends `ab473` was rejected by P1
review and is not an install candidate.

The historical Linux archive `pandar-final16-linux-amd64-019f7b10.tar.gz` is 24,891,706 bytes with
SHA-256 `023dcad198674c8ad1c20eb9bc34df9ef9685f49dfeca6e6b5ea58188f3a24a3` and sidecar SHA-256
`bde03e9633839432063d93768e10b0caf845755d216a653e20fa11d1461296f8`. Its exact top-level
members are `pandar`, `libpandar_network_plugin.so`, and `libpandar_bambu_source.so`; their SHA-256
values are `b1762bfccdfc1f658147b19b23d7016707b5414d14f74be518e0b5663ddb1b22`,
`3bcce9085205d6af67dc9671cf58cd6f9fb694d5a587b43d160dc8b6a9b0712f`, and
`88d34358be39ed3d239aeb317df8f34a92d4652877e86a9849c66e32347c1df2`.
Its native evidence archive and sidecar SHA-256 values are
`fe35290675aac4e6ce323a8ebc75bde1c34d373b1df7506f7f8a65b69ffea950` and
`00a560832428e045affad08617646f7e3d322e07c4849d20e5912be6d545595b`.

Final16 Linux validation passed workspace Nextest 1,808/1,808 with one configured skip in run
`c9c96abe-5b80-4478-be33-9ceffef62a53`, fmt, strict workspace Clippy, module-size checks, ABI tool
22/22, release-smoke 25/25, packaged tasks 18/18, the 109-network plus 21-File-Transfer export
contract, and 21 entrypoints x 256 ASan/LSan cycles. Disposable PostgreSQL 16.14 run
`b73d7ce9-d3ab-424b-8d65-b4736e59f24b` passed 7/7 with zero skips; its dedicated container,
network, and volume were removed and their absence was verified.

The official Bambu Studio `02.08.01.55` AppImage with SHA-256
`e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995` made exactly one
model-task request, received one HTTP 200 response, and produced one ordered four-event lifecycle
through callback return. Evidence-manifest SHA-256 is
`c6ba9b6282581119d3baec720e26990ad63efc20eb394b0c71dced89081d5fd9`.
The redacted bundle `pandar-final16-real-studio-evidence-019f7b10.tar.gz` is 245,225 bytes with
SHA-256 `f07c369ad9e0354ef40142294d9385e9c454fd534a04badce4be000f49c06eca`; an independent
second generation matched byte-for-byte. It contains only safe `evidence/` and `outer/` artifacts,
with no runner or mock implementation and no synthetic token contents.
Its `.sha256` sidecar has SHA-256
`30c6e5d43b74f9770d19638b86cefddd96d4d861c16155c74d30b488adf7f1b6`, and
`sha256sum --check` passed.
This controlled result used a synthetic persisted session and loopback mock; it is not real
authentication, Hub, Agent, database, printer, hardware, print, or firmware evidence. No GitHub
Action or Windows Studio process was used.

The historical final14 immutable input is `pandar-bambu-final14-019f7b10.tar.gz` at source `HEAD`
`2ba0d1f2755501ea9e7d4babcf176db40638f643`, 2,782,539 bytes and 1,548 regular members. Archive,
member-list, canonical-tree, and freeze-evidence SHA-256 values are
`c422d80d89052732db6b8ae87b68fd1e4145c64f588d8382deafef3345d86681`,
`5b32472c9372a992c23315d9b33691a0f269248b65db312590ed00556e21aac0`,
`43a4a577fb90327dad9e59bcb89dc1e91352bad83f27786a32cae34cb62136e5`, and
`70d545770086c6acde271d3181508adf4f0d91fc8213771363ec78b2792f5ec3`. Determinism passed and
all unsafe, duplicate, case-collision, reparse-point, membership-diff, and content-diff counts were zero.

The historical final14 Linux archive `pandar-final14-linux-amd64-019f7b10.tar.gz` is 24,854,111
bytes with SHA-256 `4e91f2457197532102544b02d4edac5354dc2982ec55fa707a057cbcba518b68`
and sidecar SHA-256 `c7e95f887fe415bcc592ab4475a4a6d5a070344d855c22e95ba7efe1323938a1`.
Ubuntu 22.04 Nextest run `d2231751-1284-46b0-aee6-2e041ca1a203` passed 1,781/1,781 with one
separately reported skip; fmt, strict Clippy, module-size 2/2, release-smoke-tool 21/21, all five ABI
modes, 109 network plus 21 File Transfer exports, the companion sentinel/no-`Bambu_*` rule, and
21 x 256 ASan/LSan cycles passed. The native evidence bundle is 202,300 bytes with SHA-256
`db6a464ce6b9b4b5e4689e1f0f21962dd097349056e78beb57a8779e1352cb02`.

Historical final14 official-AppImage attempt 1 used the fixed official Ubuntu 22.04 AppImage SHA-256
`e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995`. Studio PID
`137`/start ticks `193373032` remained unchanged across two offline failures and one successful
development no-auth commit; both libraries mapped 4/4, one plugin session was committed, and
loader/certificate error counts were zero. Redacted evidence
`pandar-final14-appimage-redacted-evidence-019f7b10.tar.gz` is 10,603 bytes with 23 members and
SHA-256 `7eac6abbc7364928147d60dd1c583d084c02debf1552734bc82a4dec59c941be`.
This is historical exact module-load and development no-auth recovery evidence, not authenticated
Studio evidence or a current install candidate.

The historical final13 build input is archive `pandar-bambu-final13-019f7b10.tar.gz`, source `HEAD`
`2ba0d1f2755501ea9e7d4babcf176db40638f643`, 2,751,227 bytes and 1,543 regular members. Its
archive SHA-256 is `71080abb1e7392b0440a179b5bca9fd80638de74a614105b8dc11a0f70959c34`, member-list SHA-256
is `87a6ad1dfaa404731ed30d7e265303cca64fc4278a478f9c12192c09373eb880`, canonical tree SHA-256
is `db0b7c3385c29ff0cdee1930a66f554a6845b58907373ef543563b829c245761`, and freeze-evidence
SHA-256 is `4d132e16f91365795f54c97f608483c34b55726c5f614f5bb8ffaac2ede1fb7f`. Determinism passed;
unsafe member, duplicate, case-collision, reparse-point, membership-diff, and content-diff counts were
zero.

The historical Windows native archive `pandar-final13-windows-amd64-019f7b10.tar.gz` is 21,285,752
bytes with SHA-256 `6c50e77a0b4008ce46d86de51411117061c5118e18849ca1fb94f4a3f319db64`.
It passed native CLI execution, exact three-file layout, all five ABI modes, the 109-network plus
21-File-Transfer contract, 21/21 ABI and release-smoke checks, and companion inspection. Consolidated
evidence SHA-256 is `3dab4bffa359e4c46eec77cbfb278ce3a1497f806a1d80343a1735b5a68f025b`.
The basename-only sidecar has SHA-256
`0d908e117a58bdd951f49b66ca40d84555f54b1d097dfdabbd3bac2acdf10922`. CLI, plugin, and companion
SHA-256 values are `a73fbe47a56fd557f14912e0e774007e0a4774ce83f250ce2a9cc41e52da8d57`,
`7861e454eb9dd6122eabc6252102de462228eec526f47149e00a037d3dc48eba`, and
`eaf98016c7d38cb6121a525a0f7a5bb5f0c59df333722798c5f76cee279fdfe6`.
This is a native artifact result, not a real Windows Studio session; authentication and hardware remain
untested. Do not publish the cross-platform alignment as complete while authenticated Linux, real
Windows Studio, macOS, hardware, and live-firmware evidence remain pending.

The historical Linux native archive `pandar-final13-linux-amd64-019f7b10.tar.gz` is 24,854,768 bytes
with SHA-256 `4166e6012e6c1bf7cdf056ba3bfb28f0fbc9d216c31e5ed2e8620adb8b5fcccc`.
It passed native CLI execution, exact three-file layout, all five ABI modes, the 109-network plus
21-File-Transfer contract, 22/22 ABI-tool tests, 21/21 release-smoke-tool tests, companion inspection,
and 21 File Transfer entrypoints x 256 ASan/LSan ownership cycles. CLI, plugin, and companion SHA-256
values are `7c44138d559ee62d02d4ac7fe0c23c7091e99a7782aac8a163a0c3565458d77f`,
`f9baf8346901fdc2ba20aeee786029e47af495bad3ee2e754f440db89010be24`, and
`88d34358be39ed3d239aeb317df8f34a92d4652877e86a9849c66e32347c1df2`. Evidence-bundle SHA-256
is `aa7478fe0f74debcc5f3d1f5ec53a2222d726beafe5224935aa3382c24f6097a`.

Final13 exact-AppImage attempt 8 then loaded this package in the official Ubuntu 22.04 Bambu Studio
`02.08.01.55` AppImage with SHA-256
`e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995`. Studio PID `137`/start
ticks `192688662` remained unchanged across two offline failures and one successful commit after Hub
became ready; both libraries mapped 4/4, and loader/certificate error counts were zero. Final
active/total token count was `1/1`, with create/revoke/discard counts `1/0/0`. Redacted evidence bundle
`pandar-final13-appimage-redacted-evidence-019f7b10.tar.gz` is 7,211 bytes with SHA-256
`a4453c8dce3829cc1a84a372a772b516812fe1564b310e61db9e9009a11cf9d2`. This proves exact module
load and development no-auth same-process recovery only; it is not an authenticated Studio session.

The historical final12 alignment input is archive `pandar-bambu-final12-019f7b10.tar.gz`, source HEAD
`2ba0d1f2755501ea9e7d4babcf176db40638f643`, 2,740,698 bytes and 1,543 regular members. Its
archive SHA-256 is `17371828ef7a26cace73cfbed321d094bf38323670e8fa6ccf69d6cbfd4b7eee`, canonical
member-list SHA-256 is `87a6ad1dfaa404731ed30d7e265303cca64fc4278a478f9c12192c09373eb880`, and
source-tree/manifest SHA-256 is
`5aa0038dbc3f0962cc172646876263b0db04e1e6df5fbe571553af1967f242a6`. Later Linux validation
exposed a background-refresh/firmware-callback race, so frozen final5, final11, and final12 results
remain only where explicitly labeled historical. Do not install any of those archives as a stable
`02.07.01.62` candidate.

Frozen final12 PostgreSQL 16.14 harness run `3e00d36c-7fb9-47d3-b71b-d9735ebe0eae` and Nextest
run `0b708279-6183-4477-9f78-31add8d7f423` passed 55/55 focused cases with zero skip markers; 831
tests were outside the filter. The PostgreSQL evidence SHA-256 is
`d7f002f5be8708844cce406895503ef7056b634bf04aad068722eb25ef15247e`.

Historical final13 PostgreSQL 16.14 harness `0c292295-f9ab-459b-89c2-ea74f2c9ff56` ran
`24b49c19-cd07-42b5-a5a3-6d220345bd7e` and `1f4b8458-6397-4c0b-8ab3-23d37779c68a`; each passed
55/55 focused cases with 831 filtered and zero runtime skip markers. Normalized evidence SHA-256 is
`7e04ae355f7bca3fb409bbc700b5c8f160194c0d2f9ec82df823c859566a2db7`; source read-only and
cleanup checks passed.

| Target label    | Current operator status | Current ABI-series evidence                                                                                              | Next action                                                                                   |
| --------------- | ----------------------- | ------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------- |
| `linux-amd64`   | `untested`              | No current six-series tagged package or real Studio run exists; final16 is historical Public Beta evidence only.        | Run the tag workflow and exact Studio checklist for each supported ABI series.                 |
| `windows-amd64` | `untested`              | The ABI-series hook/package contract is configured, but no current native package or real Studio run exists.            | Run the native MSVC release job and exact Studio checklist for each supported ABI series.      |
| `linux-arm64`   | `untested`              | No current three-file native candidate exists.                                                                          | Do not publish a Studio compatibility claim.                                                  |
| `windows-arm64` | `untested`              | No current three-file native candidate exists.                                                                          | Do not publish a Studio compatibility claim.                                                  |
| `macos-amd64`   | `untested`              | The Apple Silicon/Rosetta release job is configured but has not run; no matching real Studio evidence exists.           | Run the tag workflow and exact Studio checklist under Rosetta.                                |
| `macos-arm64`   | `in_progress`           | Local native package/ABI/release-smoke and exact-version module load passed; no tagged artifact or authenticated session exists. | Run the tag workflow and authenticated checklist before claiming full compatibility.          |

## Operations Runbook

SQLite single-node checks:

- Check `/readyz` on the private observability listener (default `127.0.0.1:9090`) before exposing the deployment. `database=1`, `artifact_storage=1`, and `grpc=1` are required for normal service.
- Check `/metrics` on the private observability listener for `pandar_readyz`, command/job counts, WebSocket ticket counters, control-plane counters, and print-report counters.
- Back up both the SQLite database and filesystem artifact directory together. A database backup without matching artifact files cannot restore pending print artifacts.

PostgreSQL + NATS + object-storage checks:

- Verify PostgreSQL readiness and migration completion before adding additional Hub replicas.
- Verify `PANDAR_CONTROL_PLANE=nats`, `PANDAR_NATS_URL`, and object-storage variables on every Hub replica.
- Check `/metrics` on the private observability listener for `pandar_control_plane_messages_total`, `pandar_agent_sessions`, `pandar_commands_total`, `pandar_jobs_total`, `pandar_print_reports_total`, and `pandar_readyz`.
- Run the local Phase 26 dry-run harness and `--live-preflight` during release validation, then record any disposable live PostgreSQL/NATS/object-storage soak in `docs/compatibility/phase-26-soak-evidence.md`.

Recovery checks:

- Hub restart: verify agents reconnect or receive the next wake, queued/sent commands remain in the database, and WebSocket subscribers can reconnect with new tickets.
- NATS interruption: verify durable command/job state remains committed, restart the broker or Hub subscriber, then issue another wake-producing action if needed.
- Storage outage: verify `/readyz` reports `artifact_storage=0`; upload/download failures should use stable artifact error labels, and cleanup should leave rows for retry when delete fails.
- Printer/report issues: inspect print report counters, `machine_events`, command/job state, and full-chain agent logs before retrying operator actions.

## Signing Status

Phase 24 signing decision: `unsigned-accepted`.

Artifacts remain unsigned for the next release. Operators must verify `.sha256` checksums before installation and may see platform warnings from Windows SmartScreen, macOS Gatekeeper, or other local policy tools. Code signing, notarization, and signed archive distribution are deferred to a later phase.
