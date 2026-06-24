# Phase 26 Soak Evidence

## Local Dry-Run Evidence

| Date | Commit | Command | Scenarios | Result | Notes |
| --- | --- | --- | --- | --- | --- |
| 2026-06-24 | working tree before commit | `cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --iterations 1 --concurrency 2` | artifact, fanout, restart, storage, terminal | passed | Local SQLite/process dry-run only; verifies command wake convergence, WebSocket fanout, plugin submissions, storage failures, restart simulation, and terminal report idempotence without Docker/live services. |
| 2026-06-24 | working tree before commit | `cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --scenario artifact` | artifact | passed | Includes Hub A to Hub B command wake convergence and artifact download through Hub-mediated path. |
| 2026-06-24 | post-Phase 28 smoke-sync commit | `cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --iterations 1 --concurrency 2` | artifact, fanout, restart, storage, terminal | passed | Re-ran after Phase 28 metadata persistence. The smoke tool now constructs jobs with explicit `artifact_metadata_json: None`; all local dry-run scenarios passed. |
| 2026-06-24 | after SQLite immediate print-job write transaction fix | `cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --iterations 2 --concurrency 2` | artifact, fanout, restart, storage, terminal | passed | Reproduced and fixed a local SQLite `database is locked` failure in concurrent plugin-client pressure. Print-job audit transactions now start as SQLite immediate write transactions, and the smoke tool reports scenario context plus Hub logs through `RUST_LOG`. The storage scenario still emits expected injected put/open failure logs. |

## Live Soak Preflight Evidence

| Date | Commit | Command | Result | Notes |
| --- | --- | --- | --- | --- |
| 2026-06-24 | working tree before commit | `cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --live-preflight` | blocked | Preflight command exists and failed locally because disposable `PANDAR_SOAK_*` PostgreSQL, NATS, and object-storage variables are not configured. No live soak was attempted. |

## Live PostgreSQL + NATS + Object Storage Evidence

| Date | Commit | PostgreSQL | PostgreSQL latency/conflict notes | NATS | NATS reconnect notes | Object storage | Command | Result | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-06-24 | not committed | local PostgreSQL binaries available, but no disposable live soak URL configured | not run | no `nats-server` binary and no live NATS URL configured; podman has cached NATS images only | not run | no MinIO/S3-compatible endpoint, credentials, or live object-storage bucket configured | not run | blocked | `--live-preflight` now verifies the disposable live-soak environment contract, but this workspace lacks configured disposable PostgreSQL, NATS, and object-storage dependencies. No live soak was attempted; do not target production data. |
