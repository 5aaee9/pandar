# Phase 26 Production Soak, HA, And Failure Injection Design

## Objective

Prove that the scaled Hub model introduced in Phase 22 and the storage/upload pipeline introduced in Phase 25 recover under realistic concurrent use and common dependency failures before expanding the product surface area. Phase 26 adds repeatable soak/failure evidence, improves operator-facing subsystem signals, and documents deployment runbooks for both SQLite single-node and PostgreSQL plus NATS scaled deployments.

This phase is an operational hardening phase. It should not add new printer controls, slicer metadata parsing, direct object-store upload from browsers, or new user-facing workflows.

## Current State

- PostgreSQL plus NATS is the scaled control-plane shape. `ControlPlane` supports in-process and NATS backends, while PostgreSQL remains the durable source for commands, jobs, tickets, printers, and users.
- Browser WebSocket tickets are database-backed and can be consumed across Hub replicas.
- Artifact bytes are behind `ArtifactStorage`. Filesystem remains the default for SQLite/single-node deployments; S3-compatible storage is available for scaled deployments.
- `tools/scaled-artifact-smoke` currently proves a narrow cross-Hub artifact path using two `AppState` values, a shared database, a fake S3-like storage boundary, plugin multipart submission, command dequeue from another Hub state, and Hub-mediated agent artifact download.
- `/readyz` reports database, gRPC config, artifact storage, and external auth readiness. `/metrics` reports agent sessions, command counts, WebSocket tickets/subscriptions, job counts, print report counts, and readiness gauges.
- Deployment docs describe PostgreSQL plus NATS and object storage, but there is no repeatable Phase 26 soak harness, failure-injection command, or operator runbook that ties a failure to `/readyz`, `/metrics`, and logs.
- The current execution environment may not have Docker, PostgreSQL, NATS, MinIO, or cloud S3 credentials. Default verification must therefore work without external services, while live soak mode can require explicit environment variables.

## Scope

Phase 26 covers four incremental milestones:

1. Extend the local scaled smoke into a repeatable HA/failure harness that can run without external services.
2. Add targeted failure-injection coverage for control-plane lag/decode errors, storage read/write/delete failures, WebSocket ticket consumption across replicas, command wake convergence, and job progress convergence.
3. Refine metrics/log signals so operators can distinguish app, database, broker/control-plane, storage, agent/session, and printer/report failures.
4. Add deployment runbooks and soak evidence templates for SQLite single-node and PostgreSQL plus NATS plus object-storage deployments.

Live PostgreSQL plus NATS plus S3/MinIO soak is supported as an explicit harness mode or runbook path, but it is not required in the default test suite unless the needed services and credentials are provided.

## Soak Harness Contract

Create or extend a repository-local tool under `tools/` rather than relying on ad hoc shell commands. The tool should have at least:

- a default deterministic dry-run mode that uses local process fixtures and no external sockets except loopback HTTP listeners;
- a configurable iteration count, defaulting to a small value suitable for CI and local development;
- a concurrency setting for simulated plugin clients, WebSocket subscribers, and agent command drains;
- failure scenario selectors so individual scenarios can be run while developing;
- structured terminal output with `PASS`/`FAIL`, scenario names, iteration counts, and concise failure causes with full error context preserved internally.

The default dry-run harness must exercise:

- two Hub states sharing one database to model replicas;
- at least one Hub HTTP route for plugin multipart print creation;
- command dequeue from a different Hub state than the creator;
- Hub-mediated agent artifact download;
- WebSocket ticket issue on one state and consumption/subscription on another state;
- printer snapshot and job progress fanout to a subscriber owned by a different state;
- command wake convergence after the creating Hub publishes a wake;
- terminal print-report replay/idempotence sufficient to prove terminal state is not duplicated or regressed by repeated reports.

The harness should reuse existing repository APIs and route handlers where possible. It may use fakes for external NATS/S3 in default mode, but the fake must preserve the semantics that matter for Phase 26: shared durable database, cross-Hub fanout, Hub-owned artifact keys, agent-scoped artifact download, and explicit failure paths.

## Failure Injection Scenarios

Add deterministic tests or harness scenarios for these failures:

- Hub replica restart simulation:
  - drop one `AppState` or subscriber task;
  - keep the shared database and storage;
  - recreate another state;
  - prove queued commands, ticket behavior, and existing durable job/print state still converge.
- Control-plane subscriber lag or bad payload:
  - prove lag/decode failures are logged or counted without killing the subscriber loop;
  - prove subsequent valid control messages are still processed.
- Control-plane publish failure:
  - command/job creation that has already committed durable state should still return the existing success response where that is the current contract;
  - the failure must be visible through logs and/or metrics.
- Artifact storage failures:
  - write failure before metadata commit returns stable upload errors and creates no job/command rows;
  - read failure during agent download returns `artifact_unavailable` rather than `artifact_not_found` unless the backend classifies the error as not found;
  - delete failure during cleanup preserves artifact rows for retry.
- PostgreSQL latency or transaction conflict:
  - default mode may simulate with concurrent repository operations against SQLite or test fakes;
  - live mode should document how to run against disposable PostgreSQL and record latency/conflict evidence.
- WebSocket ticket cross-replica behavior:
  - issue on one state, consume on another;
  - wrong tenant, reused, and expired tickets remain rejected.
- Job progress convergence:
  - repeated terminal reports do not duplicate terminal machine events;
  - later stale reports do not regress terminal physical print state.

Failure injection should prefer small focused route/repository/runtime tests when a full harness scenario would be too broad. The harness should orchestrate end-to-end evidence; unit/route tests should pin individual edge cases.

## Observability Contract

Operators must be able to identify the failing subsystem from readiness, metrics, and logs:

- app/config:
  - invalid gRPC bind and invalid control-plane configuration remain readiness/startup-visible;
- database:
  - `/readyz` database check and metrics readiness gauge identify database failure;
  - metrics collection errors preserve table/query context;
- broker/control plane:
  - publish failures, subscriber lag, and decode failures are distinguishable from database and storage errors;
  - add counters or readiness-adjacent metrics if logs alone are insufficient for automated diagnosis;
- storage:
  - upload write, download read, and cleanup delete failures are distinguishable;
  - storage paths, object keys, bucket credentials, bearer tokens, and agent credentials remain redacted;
- agent/session:
  - online/offline session gauges and command status metrics remain usable after restart/reconnect simulations;
- printer/report:
  - print report success/error counters and machine event dedupe behavior remain visible.

If new metrics are added, names should follow the existing `pandar_*` Prometheus text output style and avoid raw tenant IDs. Tenant labels, if needed, must be hashed or omitted.

## Live Soak Mode

Default verification must not require Docker or live services. Live soak is an explicit opt-in path for operators or CI environments that provide disposable dependencies.

The live mode should require clear environment variables rather than guessing:

- `PANDAR_SOAK_DATABASE_URL` for disposable PostgreSQL;
- `PANDAR_SOAK_NATS_URL` for disposable NATS;
- S3-compatible artifact variables, either reusing the Phase 25 names or prefixed `PANDAR_SOAK_ARTIFACT_*`;
- optional iteration and concurrency settings.

Live soak should fail fast when required variables are missing. It must not run against production by accident; docs must state that the database and bucket must be disposable.

If a full live mode is too large for one milestone, Phase 26 may first add the documented command contract and dry-run/local harness, then leave the live evidence table empty until operators run it. The roadmap must distinguish local proof from live deployment evidence.

## Documentation Requirements

Update:

- `docs/development.md` with the local soak/failure harness command, default dry-run behavior, and optional live environment variables.
- `docs/release-installation.md` or a new operations document with:
  - SQLite single-node topology;
  - PostgreSQL plus NATS plus S3/object-storage topology;
  - what to check in `/readyz`, `/metrics`, and logs for database, broker, storage, agent, and printer/report failures;
  - recovery steps for Hub restart, NATS interruption, storage outage, and cleanup retry.
- `docs/roadmap.md` with completed Phase 26 work and any remaining live evidence gaps.
- A compatibility/evidence markdown file for soak runs if live evidence is recorded, with date, commit SHA, dependency versions, command, scenario set, result, and known gaps.

## Non-Goals

- No new live printer control endpoints.
- No slicer/project metadata parsing.
- No durable NATS replay or broker replacement.
- No production chaos tooling that kills real services automatically.
- No requirement that default tests start Docker, PostgreSQL, NATS, MinIO, or cloud S3.
- No direct browser, plugin, or agent connection to NATS or object storage.

## Milestones

1. Specify the soak/failure scenario matrix and extend the existing scaled smoke tool or add a Phase 26 tool with deterministic dry-run scenarios.
2. Add focused tests for control-plane failure handling, cross-replica ticket/event behavior, artifact-storage failures, and terminal print-report idempotence where coverage is missing.
3. Add or refine metrics/logs for broker/control-plane and storage failure classes, preserving redaction guarantees.
4. Add operational runbooks and evidence templates, then update the roadmap.
5. Add optional live soak command support only if it can be made explicit, disposable, and safe within the phase.

## Acceptance Criteria

- A default local command provides repeatable Phase 26 HA/failure evidence without Docker or external credentials.
- The evidence covers agent sessions, command dispatch/wake, WebSocket fanout, plugin calls, print-job creation, artifact upload/download, and at least one terminal job-progress convergence scenario.
- Focused tests cover storage write/read/delete failure behavior and WebSocket ticket cross-replica safety.
- Control-plane lag/decode/publish failures are observable and do not crash unrelated serving paths.
- `/readyz`, `/metrics`, and logs let operators distinguish app/config, database, broker/control-plane, storage, agent/session, and printer/report failures.
- Docs describe SQLite single-node and PostgreSQL plus NATS plus object-storage topologies, operational checks, and recovery runbooks.
- The roadmap clearly states what Phase 26 proves locally and what remains unverified until live PostgreSQL/NATS/object-storage soak evidence is recorded.

## Verification Plan

- Run the new or extended soak harness in default dry-run mode.
- Run targeted hub tests for readiness/metrics, control-plane behavior, artifact route failures, cleanup storage failure, ticket cross-replica behavior, and print-report terminal idempotence.
- Run `cargo fmt --check`.
- Run `cargo clippy --workspace --all-targets -- -D warnings`.
- Run `cargo nextest run --manifest-path Cargo.toml --workspace`.
- Run `npm --prefix frontend run build` if frontend or shared API contracts change.
- Run optional live soak only when disposable PostgreSQL, NATS, and object-storage variables are explicitly provided; otherwise document it as not run.
