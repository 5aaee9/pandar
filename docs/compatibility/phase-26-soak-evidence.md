# Phase 26 Soak Evidence

## Local Dry-Run Evidence

| Date | Commit | Command | Scenarios | Result | Notes |
| --- | --- | --- | --- | --- | --- |
| 2026-06-24 | working tree before commit | `cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --iterations 1 --concurrency 2` | artifact, fanout, restart, storage, terminal | passed | Local SQLite/process dry-run only; verifies command wake convergence, WebSocket fanout, plugin submissions, storage failures, restart simulation, and terminal report idempotence without Docker/live services. |
| 2026-06-24 | working tree before commit | `cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --scenario artifact` | artifact | passed | Includes Hub A to Hub B command wake convergence and artifact download through Hub-mediated path. |

## Live PostgreSQL + NATS + Object Storage Evidence

| Date | Commit | PostgreSQL | PostgreSQL latency/conflict notes | NATS | NATS reconnect notes | Object storage | Command | Result | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| not run | not committed | not provided | not run | not provided | not run | not provided | not run | untested | Requires disposable live dependencies and must not target production data. |
