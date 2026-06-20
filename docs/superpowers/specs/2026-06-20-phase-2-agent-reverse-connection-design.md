# Phase 2 Agent Reverse Connection Design

## Goal

Implement the first durable reverse-control channel between `pandar-agent` and `pandar-hub`.

Phase 2 makes a locally deployed agent connect outward to the hub over gRPC, identify itself as a tenant-scoped agent, keep a heartbeat, receive hub commands on the reverse stream, and return command acknowledgements/results. It does not open Bambu printer MQTT, FTPS, SFTP, or discovery sockets.

## Current Baseline

- `pandar-hub` is an Axum API server with SQLx-backed SQLite/PostgreSQL repositories.
- `agents` already persist `tenant_id`, `name`, `status`, `version`, `last_seen_at`, and `created_at`.
- `commands` already persist tenant, agent, optional printer, kind, status, payload JSON, error, created_at, and updated_at.
- `proto/pandar/agent/v1/agent.proto` contains a minimal bidirectional agent-control stream.
- `pandar-agent` has a CLI config with `PANDAR_HUB_GRPC_URL` and `PANDAR_AGENT_NAME`, but it does not connect to the hub yet.

## Scope

In scope:

- Generate Rust gRPC types from `proto/pandar/agent/v1/agent.proto`.
- Expand the agent-control proto for hello, heartbeat, printer snapshot placeholder, command dispatch, command acknowledgement, and command result.
- Add a gRPC server to `pandar-hub` alongside the existing Axum HTTP server.
- Add an in-memory hub session registry for live agent streams.
- Persist agent connection metadata through the existing repository/database boundary.
- Add command repository methods needed to enqueue and update hub-to-agent commands.
- Add a minimal `pandar-agent` reverse connection client with reconnect/backoff and heartbeat.
- Add local integration tests for one hub service and one simulated/real agent stream.

Out of scope:

- Authentication, tenant user authorization, and production pairing.
- Bambu machine MQTT, FTPS/SFTP, discovery, print dispatch, or printer credential handling.
- WebSocket broadcasting and frontend UI for live agent status.
- Cross-process distributed session coordination.

## Dependencies

Use `tonic` for gRPC and `prost`/`tonic-prost-build` for protobuf generation. Current tonic 0.14-compatible generation uses:

- `tonic_prost_build::configure().compile_protos(&["proto/..."], &["proto/"])` from `build.rs`.
- `tonic::include_proto!` for generated modules.
- Bidirectional streaming where the client passes a `Stream<Item = AgentEvent>` and receives a response stream of `HubCommand`.

Generated code should live behind crate-local modules, not leak generated names through unrelated public APIs.

Generated protobuf Rust files must not be committed. `build.rs` must run `protoc` before compiling the crates and write generated files into Cargo's `OUT_DIR` under `target/`, which is already gitignored. Use a vendored `protoc` build dependency so plain Cargo builds do not require a system `protoc` installation.

## Proto Contract

Keep package:

```proto
package pandar.agent.v1;
```

Service:

```proto
service AgentControl {
  rpc ReverseConnect(stream AgentEvent) returns (stream HubCommand);
}
```

The RPC is named `ReverseConnect` rather than `Connect` to avoid colliding with tonic's generated client `connect(...)` constructor.

`AgentEvent` fields:

- `agent_id` string, required for Phase 2.
- `tenant_id` string, required for Phase 2.
- `event_id` string, client-generated UUID string for dedup/debug.
- oneof:
  - `AgentHello hello`
  - `AgentHeartbeat heartbeat`
  - `PrinterSnapshot printer_snapshot`
  - `CommandAck command_ack`
  - `CommandResult command_result`

`AgentHello` fields:

- `name` string.
- `version` string.

`AgentHeartbeat` fields:

- `observed_at` string ISO-8601 UTC timestamp from the agent.

`PrinterSnapshot` remains a placeholder:

- `serial` string.
- `name` string.
- `state` string.

`CommandAck` fields:

- `command_id` string.
- `accepted` bool.
- `error` string.

`CommandResult` fields:

- `command_id` string.
- `success` bool.
- `error` string.

`HubCommand` fields:

- `command_id` string.
- oneof:
  - `RefreshPrinters refresh_printers`

Do not add raw MQTT commands to the Phase 2 proto.

## Persistence Changes

Add repository support without changing Phase 1 table names:

### Agents

Required `AgentRepository` methods:

- `get(agent_id) -> Option<Agent>`
- `update_connection(agent_id, status, version, last_seen_at) -> Agent`
- `mark_offline(agent_id, last_seen_at) -> Agent`

Behavior:

- `AgentHello` for an unknown `agent_id` returns a gRPC `not_found` status. Phase 2 uses already-created agents from the HTTP API; self-registration is intentionally not implemented.
- `update_connection` validates the agent exists and preserves tenant ownership.
- `last_seen_at` is an ISO-8601 UTC text string.

### Commands

Add a minimal command domain/repository boundary in `pandar-core` and `pandar-hub`:

- `CommandId` string-backed UUID ID.
- `CommandStatus`: `queued`, `sent`, `acknowledged`, `succeeded`, `failed`.
- `CommandRecord` with id, tenant_id, agent_id, optional printer_id, kind, status, payload_json, error, created_at, updated_at.

Required `CommandRepository` methods:

- `enqueue_refresh_printers(tenant_id, agent_id) -> CommandRecord`
- `next_queued_for_agent(tenant_id, agent_id) -> Option<CommandRecord>`
- `mark_sent(command_id, tenant_id, agent_id) -> CommandRecord`
- `mark_acknowledged(command_id, tenant_id, agent_id) -> CommandRecord`
- `mark_succeeded(command_id, tenant_id, agent_id) -> CommandRecord`
- `mark_failed(command_id, tenant_id, agent_id, error) -> CommandRecord`

Behavior:

- `enqueue_refresh_printers` must verify that `agent_id` exists and belongs to `tenant_id` before inserting a command.
- Enqueue for a missing agent returns gRPC `not_found` through the service boundary.
- Enqueue for an agent that belongs to a different tenant returns gRPC `permission_denied` through the service boundary.
- Hub sends only queued commands for the connected agent.
- Queue selection must filter by both the established stream `tenant_id` and `agent_id`; it must never yield a command for a different tenant than the connected stream.
- A command moves `queued -> sent` immediately before it is yielded to the gRPC response stream.
- `CommandAck.accepted = true` moves `sent -> acknowledged`.
- `CommandAck.accepted = false` moves command to `failed` with the ack error.
- `CommandResult.success = true` moves `sent` or `acknowledged` to `succeeded`.
- `CommandResult.success = false` moves `sent` or `acknowledged` to `failed` with the result error.
- Unknown command IDs in ack/result return gRPC `not_found`.
- Ack/result updates must verify that the command belongs to the established stream `tenant_id` and `agent_id`.
- A command ID owned by a different tenant or agent returns gRPC `permission_denied`.
- Repository updates must enforce current-status preconditions:
  - `mark_sent` succeeds only from `queued`.
  - `mark_acknowledged` succeeds only from `sent`.
  - `mark_succeeded` succeeds only from `sent` or `acknowledged`.
  - `mark_failed` succeeds only from `sent` or `acknowledged`.
- Duplicate terminal events are idempotent:
  - `mark_succeeded` on an already `succeeded` command returns the existing record.
  - `mark_failed` on an already `failed` command returns the existing record without overwriting the original error.
- Stale or out-of-order events are rejected:
  - ack/result for `queued` commands returns gRPC `failed_precondition`.
  - ack for `acknowledged`, `succeeded`, or `failed` commands returns `failed_precondition`.
  - success result for `failed` commands returns `failed_precondition`.
  - failure result for `succeeded` commands returns `failed_precondition`.
- These ownership and precondition failures must be represented as typed repository errors so the gRPC layer can map them to stable statuses.

Migrations:

- SQLite and PostgreSQL must remain equivalent.
- Add any needed indexes, at minimum `idx_commands_agent_status` on `(agent_id, status)`.
- Do not use backend-native enum or timestamp types.

## Hub Runtime Design

Add a gRPC runtime beside the existing HTTP runtime:

- `PANDAR_HUB_GRPC_BIND`, default `0.0.0.0:50051`.
- `pandar-hub` starts both HTTP and gRPC listeners under the same Tokio runtime.
- If either listener exits with an error, the process returns that error with full context.

Session registry:

- In-memory registry keyed by `AgentId`.
- Stores tenant_id, agent_id, agent name, version, connected_at, last_heartbeat_at, and a command sender for the active stream.
- Only one active session per agent. A new accepted connection for the same agent replaces the old registry entry and causes the old command stream to close.
- Heartbeat timeout is configurable by code constant in Phase 2: 45 seconds.
- A background task marks timed-out agents offline in the database and removes them from the registry.

Connect flow:

1. Hub waits for the first inbound event.
2. First event must be `AgentHello`; otherwise return gRPC `failed_precondition`.
3. Hub parses `tenant_id` and `agent_id`; malformed IDs return `invalid_argument`.
4. Hub loads the agent from the repository; missing agent returns `not_found`.
5. Hub verifies persisted `tenant_id` matches the stream `tenant_id`; mismatch returns `permission_denied`.
6. Hub updates agent status to `online`, version from hello, and last_seen_at.
7. Hub registers the session and returns a stream of `HubCommand`.
8. Heartbeat events update registry heartbeat and persisted last_seen/status.
9. Ack/result events update command state.

Command dispatch:

- The registry exposes `dispatch_refresh_printers(tenant_id, agent_id)` for tests and future HTTP handlers.
- Dispatch verifies tenant/agent ownership through the repository, enqueues a command in the database, and sends a wake-up to the active session for that same tenant/agent.
- The active session drains queued commands for its agent and yields them as `HubCommand`.
- If no session is active, the command remains queued.

## Agent Runtime Design

Extend `pandar-agent`:

- CLI/env:
  - keep `PANDAR_HUB_GRPC_URL`.
  - keep `PANDAR_AGENT_NAME`.
  - add required `PANDAR_AGENT_ID`.
  - add required `PANDAR_TENANT_ID`.
  - add optional `PANDAR_AGENT_VERSION`, default crate version.
- `run(config)` connects to hub using generated tonic client.
- On connect, send `AgentHello` as the first event.
- Send heartbeat events every 15 seconds.
- On `RefreshPrinters`, immediately send `CommandAck { accepted: true }` and `CommandResult { success: true }` because real printer discovery is out of scope.
- Reconnect with simple backoff: start at 1 second, double to a max of 30 seconds, reset after a successful connection.

Do not read printer credentials or open local printer sockets in Phase 2.

## API Impact

No new public HTTP endpoint is required in Phase 2. Existing HTTP routes must continue to pass.

Tests may call repository/session APIs directly to enqueue commands. If an HTTP test helper is needed, keep it test-only.

## Error Handling

- Preserve lower-level errors with `anyhow::Context` at startup, database, listener, and stream boundaries.
- Log unexpected stream/repository errors with full cause chains using `{err:#}` or equivalent.
- Public gRPC statuses should use stable tonic codes:
  - malformed UUID: `invalid_argument`
  - first event not hello: `failed_precondition`
  - missing agent or command: `not_found`
  - tenant mismatch: `permission_denied`
  - command enqueue for agent outside requested tenant: `permission_denied`
  - command ownership mismatch: `permission_denied`
  - stale or out-of-order command event: `failed_precondition`
  - unexpected repository/runtime failure: `internal`

## Testing

Required tests:

- Proto/build:
  - generated agent-control client/server modules compile.
- Repository:
  - agent `get`, `update_connection`, and `mark_offline` work on SQLite.
  - command enqueue, next queued, sent, ack, success, and failure transitions work on SQLite.
  - command enqueue rejects missing agents and wrong tenant/agent ownership.
  - queued-command selection filters by both tenant and agent.
  - command updates reject wrong tenant/agent ownership.
  - command updates enforce state preconditions and idempotent duplicate terminal events.
  - PostgreSQL equivalents run when `PANDAR_TEST_POSTGRES_URL` is set and skip cleanly otherwise.
- Hub gRPC:
  - connect rejects a stream whose first event is not hello.
  - connect rejects malformed IDs.
  - connect rejects missing agents.
  - connect rejects tenant mismatch.
  - hello marks the persisted agent online with version and last_seen_at.
  - heartbeat updates last_seen_at.
  - dispatch to an online agent yields a `RefreshPrinters` command and marks it sent.
  - dispatch for an agent outside the requested tenant returns `permission_denied`.
  - ack/result update command status.
  - ack/result for another agent's command returns `permission_denied`.
  - stale or out-of-order ack/result returns `failed_precondition`.
  - timeout marks an agent offline; tests may use a shorter test-only timeout.
- Agent:
  - CLI parses hub URL, agent name, agent ID, tenant ID, and version.
  - agent handles `RefreshPrinters` with ack and success result without Bambu network access.
- Regression:
  - existing HTTP and repository tests still pass.

Required verification:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

## Documentation Impact

Update:

- `README.md` with hub gRPC bind and agent env vars.
- `docs/architecture.md` with Phase 2 reverse-session behavior.
- `docs/roadmap.md` to mark completed Phase 2 items and identify Phase 3 Bambu transport as next.

## Acceptance Criteria

- `pandar-hub` starts HTTP and gRPC listeners together.
- A local `pandar-agent` can connect outward to the hub using known tenant/agent IDs.
- Hub persists online/last-seen/version metadata on hello/heartbeat.
- Hub session registry can dispatch a refresh-printers command to an online agent.
- Agent acknowledges and completes refresh-printers without opening Bambu machine network sockets.
- Queued commands stay queued if the agent is offline.
- Timed-out sessions are removed and marked offline.
- SQLite tests pass by default, and PostgreSQL tests are present behind `PANDAR_TEST_POSTGRES_URL`.
- Existing Phase 1 HTTP behavior remains compatible.
