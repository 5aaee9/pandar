# Phase 26 Live Soak Runner Design

## Purpose

Add a real Phase 26 live soak runner to `tools/scaled-artifact-smoke` so the existing scaled Hub scenarios can be executed against disposable PostgreSQL, NATS, and S3-compatible object storage when those dependencies are available.

The current `--live-preflight` command proves only that required environment variables are present and look disposable. This milestone moves the tool one step closer to Phase 26 completion by adding the execution path that consumes those variables and runs live-capable scenarios. It must not claim live soak evidence unless the live command is actually run successfully against real disposable dependencies.

## Scope

Implement a new command:

```bash
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --live [--iterations N] [--concurrency N] [--scenario all|artifact|fanout|restart|terminal]
```

The command must:

- Run the same live-capable scenario logic as the local dry-run harness.
- Use `PANDAR_SOAK_DATABASE_URL` for a PostgreSQL database.
- Use `PANDAR_SOAK_NATS_URL` for a NATS control plane.
- Use the `PANDAR_SOAK_ARTIFACT_S3_*` variables for S3-compatible object storage.
- Reuse the existing `--live-preflight` validation before connecting to external services.
- Keep `--dry-run` as the explicit local evidence path with the existing fake/local dependencies.
- Keep the storage failure-injection scenario local-only because it intentionally uses fake storage failures.

## Out Of Scope

- Starting PostgreSQL, NATS, MinIO, or cloud services.
- Adding Docker Compose or testcontainers.
- Simulating NATS disconnect/reconnect with real brokers.
- Measuring latency distributions or writing performance thresholds.
- Automatically cleaning all live database rows or object-storage keys after the run.
- Claiming Phase 26 live soak passed without a successful `--live` run recorded in `docs/compatibility/phase-26-soak-evidence.md`.

## Environment Contract

Required live variables:

- `PANDAR_SOAK_DATABASE_URL`
- `PANDAR_SOAK_NATS_URL`
- `PANDAR_SOAK_ARTIFACT_S3_BUCKET`
- `PANDAR_SOAK_ARTIFACT_S3_REGION`
- `PANDAR_SOAK_ARTIFACT_S3_ENDPOINT`
- `PANDAR_SOAK_ARTIFACT_S3_ACCESS_KEY_ID`
- `PANDAR_SOAK_ARTIFACT_S3_SECRET_ACCESS_KEY`

Optional live variables:

- `PANDAR_SOAK_NATS_SUBJECT`
- `PANDAR_SOAK_ARTIFACT_S3_FORCE_PATH_STYLE`

`PANDAR_SOAK_NATS_SUBJECT` defaults to `pandar.soak.control`.

`PANDAR_SOAK_ARTIFACT_S3_FORCE_PATH_STYLE` accepts only `true` or `false` and defaults to `true`, because the documented live soak target is S3-compatible object storage such as MinIO.

`PANDAR_SOAK_DATABASE_URL` must remain PostgreSQL-only and must keep the current disposable marker and production-marker checks from `--live-preflight`.

## Architecture

The smoke harness gains an explicit execution mode:

- `DryRun`: creates a temporary SQLite database, in-process control plane, and local shared object storage.
- `Live`: connects to PostgreSQL, builds a NATS control plane, and builds S3-compatible artifact storage from the soak environment.

Scenario functions should continue to depend on `SmokeWorld` and `HarnessConfig`, not on process environment. `SmokeWorld` becomes responsible for constructing either local or live dependencies. This keeps scenario logic shared and prevents a second, divergent live implementation.

Live `hub_a` and `hub_b` must share one PostgreSQL database and one NATS-backed control plane, mirroring dry-run's shared database and shared control-plane semantics while replacing the local implementations with real dependencies.

Live runs must use unique fixture suffixes that include a per-process run id. The run id should be deterministic enough for tests by accepting an injected value in unit-level helpers, while production CLI construction may derive it from process id plus current timestamp. This prevents repeated live runs against the same disposable database from colliding with previous tenant, user, agent, printer, and token rows.

No-argument execution must continue to print usage and fail. Operators must choose either `--dry-run`, `--live-preflight`, or `--live` explicitly.

Live S3 setup must build `S3ArtifactStorageConfig` from soak-prefixed values passed through code, not by temporarily setting or reading production `PANDAR_ARTIFACT_S3_*` variables. This prevents a developer shell with production `PANDAR_ARTIFACT_S3_*` values from influencing the soak runner.

The PostgreSQL-only live command is a soak-harness constraint, not a new hub data-access boundary. Normal hub repository behavior must remain backend-neutral; dry-run still exercises the SQLite path locally.

## Scenario Behavior

`--dry-run --scenario all` keeps the current scenario set:

- `artifact`
- `fanout`
- `restart`
- `storage`
- `terminal`

`--live --scenario all` runs only live-capable scenarios:

- `artifact`
- `fanout`
- `restart`
- `terminal`

`--live --scenario storage` must fail before connecting with a clear error that storage failure injection is local-only. The live command must not inject failures into a real bucket.

Successful live-capable scenarios must print the same per-scenario `PASS scenario=... iteration=...` lines as dry-run, and the final summary must identify live mode:

```text
PASS scaled artifact smoke: live scenarios passed iterations=N concurrency=M
```

## Safety

The live command must call the same validation used by `--live-preflight` before opening PostgreSQL, NATS, or S3 connections.

The live command must not infer safety from variable presence alone. Missing variables must still be reported together, and invalid variables must still be reported together.

The live command may migrate the disposable PostgreSQL database and create tenant/job/artifact rows as part of the smoke run. That is acceptable only because the URL must pass disposable marker checks and production marker rejection.

The tool must preserve full error chains when live connection, migration, NATS subscription, S3 readiness, artifact upload, artifact download, or scenario checks fail.

## Documentation

Update:

- `docs/development.md`: document `--live` usage, optional variables, and the storage scenario exclusion.
- `docs/compatibility/phase-26-soak-evidence.md`: add a row for the new runner command as local implementation evidence if verified without live dependencies; keep the actual live soak row `blocked` until a real run succeeds.
- `docs/roadmap.md`: state that Phase 26 now has a live runner entry point, while live PostgreSQL/NATS/object-storage evidence remains blocked until disposable dependencies are provided and the command is run.

## Acceptance Criteria

- `--live-preflight` behavior and messages remain compatible with the current tests.
- CLI parsing accepts `--live` with `--iterations`, `--concurrency`, and live-capable `--scenario` values.
- CLI parsing still rejects no-argument execution and requires an explicit mode.
- CLI parsing rejects `--live --scenario storage` before connecting to external services.
- Live setup maps soak environment variables into PostgreSQL, NATS, and S3-compatible storage config without using production `PANDAR_*` variables.
- Live S3 setup uses a soak-specific mapping path into `S3ArtifactStorageConfig`, and tests prove production `PANDAR_ARTIFACT_S3_*` variables are neither read nor required.
- Live setup rejects invalid optional boolean `PANDAR_SOAK_ARTIFACT_S3_FORCE_PATH_STYLE`.
- Live fixture names include a run id so repeated runs against the same disposable database do not collide.
- Successful live runs print `PASS scaled artifact smoke: live scenarios passed iterations=N concurrency=M`.
- Unit tests cover dry-run parsing, live parsing, live storage-scenario rejection, optional path-style parsing, and run-id suffixing.
- Existing dry-run scenario tests still pass.
- Docs distinguish runner availability from actual live soak evidence.

## Verification

Required local verification:

```bash
cargo fmt --check
cargo clippy --manifest-path tools/scaled-artifact-smoke/Cargo.toml
cargo test --manifest-path tools/scaled-artifact-smoke/Cargo.toml
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --iterations 1 --concurrency 2
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --live-preflight
cargo nextest run --manifest-path "Cargo.toml" --workspace
git diff --check
```

The `--live-preflight` command is expected to fail in workspaces without disposable live variables. That failure is acceptable when it reports the missing variables and exits non-zero.

Do not require a successful `--live` run for this milestone unless disposable PostgreSQL, NATS, and S3-compatible endpoints are actually configured in the environment.
