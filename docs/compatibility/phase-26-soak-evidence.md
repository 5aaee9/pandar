# Phase 26 Soak Evidence

## Local Dry-Run Evidence

| Date | Commit | Command | Scenarios | Result | Notes |
| --- | --- | --- | --- | --- | --- |
| 2026-06-24 | working tree before commit | `cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --iterations 1 --concurrency 2` | artifact, fanout, restart, storage, terminal | passed | Local SQLite/process dry-run only; verifies command wake convergence, WebSocket fanout, plugin submissions, storage failures, restart simulation, and terminal report idempotence without Docker/live services. |
| 2026-06-24 | working tree before commit | `cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --scenario artifact` | artifact | passed | Includes Hub A to Hub B command wake convergence and artifact download through Hub-mediated path. |
| 2026-06-24 | post-Phase 28 smoke-sync commit | `cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --iterations 1 --concurrency 2` | artifact, fanout, restart, storage, terminal | passed | Re-ran after Phase 28 metadata persistence. The smoke tool now constructs jobs with explicit `artifact_metadata_json: None`; all local dry-run scenarios passed. |
| 2026-06-24 | after SQLite immediate print-job write transaction fix | `cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --iterations 2 --concurrency 2` | artifact, fanout, restart, storage, terminal | passed | Reproduced and fixed a local SQLite `database is locked` failure in concurrent plugin-client pressure. Print-job audit transactions now start as SQLite immediate write transactions, and the smoke tool reports scenario context plus Hub logs through `RUST_LOG`. The storage scenario still emits expected injected put/open failure logs. |
| 2026-06-24 | working tree before commit | `cargo test --manifest-path tools/scaled-artifact-smoke/Cargo.toml main::tests`; `cargo test --manifest-path tools/scaled-artifact-smoke/Cargo.toml live::tests`; `cargo test --manifest-path tools/scaled-artifact-smoke/Cargo.toml fixture::tests::fixture_suffix_includes_live_run_id`; `cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --iterations 1 --concurrency 2` | artifact, fanout, restart, storage, terminal | passed local runner wiring | Verifies `--live` parsing, live-capable scenario selection, soak-prefixed config mapping, live run-id suffixing, and unchanged local dry-run behavior. This is runner implementation evidence only; no live PostgreSQL/NATS/object-storage dependencies were contacted. |

## Live Soak Preflight Evidence

| Date | Commit | Command | Result | Notes |
| --- | --- | --- | --- | --- |
| 2026-06-24 | working tree before commit | `cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --live-preflight` | blocked | Preflight command exists and failed locally because disposable `PANDAR_SOAK_*` PostgreSQL, NATS, and object-storage variables are not configured. No live soak was attempted. |
| 2026-06-24 | working tree before commit | `cargo test --manifest-path tools/scaled-artifact-smoke/Cargo.toml live::tests`; `cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --live-preflight`; valid disposable `PANDAR_SOAK_*` environment with local loopback URLs | passed local preflight validation | Strengthened preflight now validates missing-variable aggregation, PostgreSQL scheme plus disposable/production markers, `nats://` URL shape, HTTP(S) S3 endpoint shape, and placeholder-like object-storage values. The command remains connection-free; this is preflight evidence only, not live PostgreSQL/NATS/object-storage soak evidence. |

## Live PostgreSQL + NATS + Object Storage Evidence

| Date | Commit | PostgreSQL | PostgreSQL latency/conflict notes | NATS | NATS reconnect notes | Object storage | Command | Result | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-06-24 | not committed | local PostgreSQL binaries available, but no disposable live soak URL configured | not run | no `nats-server` binary and no live NATS URL configured; podman has cached NATS images only | not run | no MinIO/S3-compatible endpoint, credentials, or live object-storage bucket configured | `--live` not run with real dependencies | blocked | The smoke tool has a live runner entry point, and local tests verify its config wiring. This workspace still lacks configured disposable PostgreSQL, NATS, and object-storage dependencies, so no live soak was attempted; do not target production data. |
