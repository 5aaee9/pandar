# Release Installation

## Release Archive Selection

Select the archive that matches the operator host OS and CPU architecture:

| Host | Target label | Archive |
| --- | --- | --- |
| Linux x86_64/amd64 | `linux-amd64` | `pandar-release-<tag-or-sanitized-ref>-linux-amd64.tar.gz` |
| Linux arm64/aarch64 | `linux-arm64` | `pandar-release-<tag-or-sanitized-ref>-linux-arm64.tar.gz` |
| Windows x86_64/amd64 | `windows-amd64` | `pandar-release-<tag-or-sanitized-ref>-windows-amd64.tar.gz` |
| Windows arm64/aarch64 | `windows-arm64` | `pandar-release-<tag-or-sanitized-ref>-windows-arm64.tar.gz` |
| macOS Intel | `macos-amd64` | `pandar-release-<tag-or-sanitized-ref>-macos-amd64.tar.gz` |
| macOS Apple Silicon | `macos-arm64` | `pandar-release-<tag-or-sanitized-ref>-macos-arm64.tar.gz` |

Each archive contains the `pandar` CLI binary, or `pandar.exe` on Windows, and the matching `pandar-network-plugin` dynamic library for the target platform.

## Checksum Verification

Download the archive and its `.sha256` sidecar from the same release. Verify the sidecar before unpacking:

```bash
sha256sum -c pandar-release-<tag-or-sanitized-ref>-<target-label>.tar.gz.sha256
```

On macOS, use:

```bash
shasum -a 256 -c pandar-release-<tag-or-sanitized-ref>-<target-label>.tar.gz.sha256
```

The sidecar must name only the archive file, not a local path. Do not install an archive whose checksum fails or whose sidecar does not match the downloaded filename.

## CLI Startup Smoke

Unpack the archive and run the CLI help command before installing it into a shared path:

```bash
tar -xzf pandar-release-<tag-or-sanitized-ref>-<target-label>.tar.gz
./pandar --help
```

On Windows, run:

```powershell
tar -xzf pandar-release-<tag-or-sanitized-ref>-<target-label>.tar.gz
.\pandar.exe --help
```

If startup fails, keep the archive, checksum, target label, OS version, and terminal output for the release evidence record.

## Hub, Web, And Agent Deployment

The release archive provides the operator CLI and Bambu Studio plugin library. Deploy the running services with the existing container or NixOS paths:

- `pandar-hub`: Rust API server, default HTTP/WebSocket bind `0.0.0.0:8080`, default gRPC bind `0.0.0.0:50051`.
- `pandar-web`: Next.js frontend, default bind `0.0.0.0:3000`.
- `pandar-agent`: local-network agent that connects outward to Hub gRPC and talks to Bambu machines.

The hub needs `PANDAR_DATABASE_URL`. The frontend needs `APP_API_URL` and `APP_BASE_URL`. The agent needs `PANDAR_HUB_GRPC_URL`, tenant and agent IDs, an agent credential, and any `PANDAR_PRINTERS` entries for local machines.

For agent artifact downloads, set `PANDAR_HUB_API_URL` when `PANDAR_HUB_GRPC_URL` is not an HTTP(S) URL. Agents authenticate artifact downloads with `PANDAR_AGENT_CREDENTIAL`; do not distribute object-store credentials to agents or browsers.

## Docker Compose Shapes

Use the SQLite compose shape for single-process or local deployments:

```bash
APP_API_TOKEN=<tenant token> APP_TENANT_ID=<tenant uuid> docker compose -f docker-compose.sqlite.yml up --build
```

Use the PostgreSQL compose shape when the database must be external to the Hub container:

```bash
POSTGRES_PASSWORD=<db password> APP_API_TOKEN=<tenant token> APP_TENANT_ID=<tenant uuid> docker compose -f docker-compose.postgres.yml up --build
```

Use the PostgreSQL plus NATS profile to run the broker-backed deployment shape with S3-compatible artifact storage:

```bash
POSTGRES_PASSWORD=<db password> APP_API_TOKEN=<tenant token> APP_TENANT_ID=<tenant uuid> PANDAR_CONTROL_PLANE=nats PANDAR_ARTIFACT_STORAGE=s3 PANDAR_ARTIFACT_S3_BUCKET=<bucket> PANDAR_ARTIFACT_S3_REGION=<region> PANDAR_ARTIFACT_S3_ENDPOINT=<endpoint> PANDAR_ARTIFACT_S3_ACCESS_KEY_ID=<access key> PANDAR_ARTIFACT_S3_SECRET_ACCESS_KEY=<secret> docker compose -f docker-compose.postgres.yml --profile nats up --build
```

The compose file starts one `pandar-api` service with fixed host ports. For multiple Hub replicas, put replicas behind your own HTTP/gRPC routing layer and avoid publishing the same host ports from every container.

SQLite is for lightweight single-process deployments and rejects the NATS control plane. The SQLite compose shape keeps the filesystem artifact backend and `PANDAR_SPOOL_DIR`. PostgreSQL plus NATS should use S3-compatible artifact storage; a shared filesystem is accepted only with the explicit `PANDAR_ARTIFACT_FILESYSTEM_SHARED=true` readiness override when every Hub replica truly mounts the same artifact directory.

Back up SQLite deployments by capturing both the SQLite database file and the filesystem artifact directory. Back up PostgreSQL/object-storage deployments by capturing the PostgreSQL database and the configured object-storage bucket.

## NixOS services.pandar

NixOS deployments use the flake module exposed as `nixosModules.default` and `nixosModules.pandar`. Configure Hub, Web, and Agent through `services.pandar`.

Generated option documentation is in `docs/deployment/nixos/options.md`. Use it as the source for exact option names, package overrides, environment files, bind addresses, and agent credential wiring.

## Bambu Studio Plugin Replacement

Replace the Bambu Studio network plugin library with the archive's platform plugin file:

| OS | Plugin file |
| --- | --- |
| Linux | `libpandar_network_plugin.so` |
| Windows | `pandar_network_plugin.dll` |
| macOS | `libpandar_network_plugin.dylib` |

Keep the original Studio plugin file for rollback. Typical locations vary by Studio installation:

- Linux AppImage or extracted builds: replace the bundled network plugin library next to the extracted Studio libraries.
- Windows: replace the Bambu Studio network plugin DLL in the Studio installation's plugin or library directory.
- macOS: replace the network plugin dylib inside the Bambu Studio `.app` bundle's Frameworks or plugin library area.

Record real Studio load and sign-in evidence with `docs/compatibility/bambu-studio-plugin-smoke.md`. Do not treat release-smoke export checks as real Bambu Studio compatibility evidence.

## Unsupported Or Untested Targets

Target status is tracked in `docs/compatibility/release-artifacts.md`. For checksum, layout, CLI startup, plugin exports, and real host install columns, treat any target with `failed`, `blocked`, `unsupported`, or `untested` evidence as not proven for operator installation.

CI release-smoke evidence and real host installation evidence are separate. A target can pass archive layout, checksum, CLI startup, and packaged plugin export checks in CI while still having `untested` real host installation status.

| Target label | Current operator status | Reason | Next action |
| --- | --- | --- | --- |
| `linux-amd64` | `blocked` | Workflow-run artifact evidence exists from run `28102001464`, but no tagged GitHub Release or real host install evidence exists yet. | Select a tagged GitHub Release archive or a suitable workflow artifact from run `28102001464`, install the archive on a Linux x86_64 host, verify checksum, run `pandar --help`, inspect plugin replacement behavior, and record the row in `docs/compatibility/release-artifacts.md`. |
| `linux-arm64` | `blocked` | Workflow-run artifact evidence exists from run `28102001464`, but no tagged GitHub Release or real host install evidence exists yet; plugin export reliability is still watched because the Linux `cdylib` export path is fragile on arm64. | Select a tagged GitHub Release archive or a suitable workflow artifact from run `28102001464`, run release-smoke for the target, install on Linux arm64 hardware, and record export plus install evidence before treating it as usable. |
| `windows-amd64` | `blocked` | Workflow-run artifact evidence exists from run `28102001464`, but no tagged GitHub Release or real host install evidence exists yet; artifacts are unsigned and may trigger platform warnings. | Select a tagged GitHub Release archive or a suitable workflow artifact from run `28102001464`, install on Windows x86_64, verify checksum, run `pandar.exe --help`, replace the Studio plugin DLL for a controlled smoke, and record evidence. |
| `windows-arm64` | `blocked` | Built and packaged in run `28102001464`, but release-smoke failed before upload because the runner lacked an LLVM PE inspector for ARM64 PE; after the LLVM inspector fix, follow-up run `28103772270` was billing-blocked before any build step started. Artifacts are unsigned and may trigger platform warnings. | Produce a tagged GitHub Release archive or re-run `release.yml` after billing is restored to produce an uploaded Windows arm64 workflow artifact, install on Windows arm64, verify checksum, run `pandar.exe --help`, replace the Studio plugin DLL for a controlled smoke, and record evidence. |
| `macos-amd64` | `blocked` | Workflow-run artifact evidence exists from run `28102001464`, but no tagged GitHub Release or real host install evidence exists yet; artifacts are unsigned and may trigger Gatekeeper warnings. | Select a tagged GitHub Release archive or a suitable workflow artifact from run `28102001464`, install on Intel macOS, verify checksum, run `pandar --help`, replace the Studio plugin dylib for a controlled smoke, and record evidence. |
| `macos-arm64` | `blocked` | Workflow-run artifact evidence exists from run `28102001464`, but no tagged GitHub Release or real host install evidence exists yet; artifacts are unsigned and may trigger Gatekeeper warnings. | Select a tagged GitHub Release archive or a suitable workflow artifact from run `28102001464`, install on Apple Silicon macOS, verify checksum, run `pandar --help`, replace the Studio plugin dylib for a controlled smoke, and record evidence. |

## Operations Runbook

SQLite single-node checks:

- Check `/readyz` before exposing the deployment. `database=1`, `artifact_storage=1`, and `grpc=1` are required for normal service.
- Check `/metrics` for `pandar_readyz`, command/job counts, WebSocket ticket counters, control-plane counters, and print-report counters.
- Back up both the SQLite database and filesystem artifact directory together. A database backup without matching artifact files cannot restore pending print artifacts.

PostgreSQL + NATS + object-storage checks:

- Verify PostgreSQL readiness and migration completion before adding additional Hub replicas.
- Verify `PANDAR_CONTROL_PLANE=nats`, `PANDAR_NATS_URL`, and object-storage variables on every Hub replica.
- Check `/metrics` for `pandar_control_plane_messages_total`, `pandar_agent_sessions`, `pandar_commands_total`, `pandar_jobs_total`, `pandar_print_reports_total`, and `pandar_readyz`.
- Run the local Phase 26 dry-run harness and `--live-preflight` during release validation, then record any disposable live PostgreSQL/NATS/object-storage soak in `docs/compatibility/phase-26-soak-evidence.md`.

Recovery checks:

- Hub restart: verify agents reconnect or receive the next wake, queued/sent commands remain in the database, and WebSocket subscribers can reconnect with new tickets.
- NATS interruption: verify durable command/job state remains committed, restart the broker or Hub subscriber, then issue another wake-producing action if needed.
- Storage outage: verify `/readyz` reports `artifact_storage=0`; upload/download failures should use stable artifact error labels, and cleanup should leave rows for retry when delete fails.
- Printer/report issues: inspect print report counters, `machine_events`, command/job state, and full-chain agent logs before retrying operator actions.

## Signing Status

Phase 24 signing decision: `unsigned-accepted`.

Artifacts remain unsigned for the next release. Operators must verify `.sha256` checksums before installation and may see platform warnings from Windows SmartScreen, macOS Gatekeeper, or other local policy tools. Code signing, notarization, and signed archive distribution are deferred to a later phase.
