# Hub Horizontal Scaling Control Plane Design

## Goal

Enable `pandar-hub` to run in two explicit deployment modes:

- SQLite + in-process control plane for lightweight single-process deployments.
- PostgreSQL + NATS control plane for large-scale horizontal Hub deployments while keeping the existing gRPC agent connection model.

The feature must preserve Pandar's current tenant and agent authorization model. Agents and clients continue to authenticate only with Hub. NATS is an internal Hub service dependency, not a public agent/client endpoint.

## Non-Goals

- Do not migrate agents from gRPC to MQTT.
- Do not introduce RabbitMQ, EMQX, Kafka, or PostgreSQL `LISTEN/NOTIFY` as the primary control plane.
- Do not make SQLite support multi-process or HPA deployments.
- Do not duplicate tenant token authorization inside NATS.
- Do not add durable event replay in this phase.

## Current Constraints

Hub currently keeps these coordination surfaces in process memory:

- `SessionRegistry` maps `AgentId` to the local gRPC stream wake/close senders.
- `PrinterEventHub` keeps WebSocket broadcast senders and browser tickets in memory.
- Command records are already persisted in the database and are the reliable source for agent command delivery.

In a multi-Hub deployment, the Hub instance that accepts an HTTP command may not be the instance that owns the target agent's gRPC stream, and the Hub instance that receives a printer update may not be the instance that owns a user's WebSocket subscription.

## Architecture

Introduce a small Hub control plane boundary used by Hub internals:

- `ControlPlane::publish(HubControlMessage)` broadcasts control messages.
- `ControlPlane::subscribe()` returns a stream/receiver for control messages.
- `InProcessControlPlane` uses `tokio::sync::broadcast` and is the default for SQLite and tests.
- `NatsControlPlane` uses `async-nats` and is enabled only when `PANDAR_CONTROL_PLANE=nats` is configured.

The control plane carries hints and real-time fanout events. PostgreSQL remains the reliable fact source for persistent business state.

Print artifacts are still stored through `PANDAR_SPOOL_DIR` in this phase. PostgreSQL + NATS scales Hub coordination and metadata, but horizontally scaled print-job creation requires that artifact directory to be shared across Hub replicas or a later object-storage backend.

### AppState Ownership

`AppState` owns the configured control plane handle. `AppState::connect*` parses control-plane configuration after database configuration is known, rejects invalid database/control-plane combinations, constructs the control plane, and passes it to `AppState::from_database_with_control_plane`.

The control-plane subscriber task is started from `run_from_env` after `AppState` construction, next to the existing stale-session expiry task. Tests that do not run the full server may call a test-only/shared-state constructor that reuses the same database and `InProcessControlPlane` to simulate two Hub replicas. The subscriber task:

- receives `HubControlMessage` values;
- wakes or closes only matching local sessions in `SessionRegistry`;
- forwards `PrinterEvent` values only to local WebSocket subscribers;
- logs malformed or failed control-plane receive errors with full error context where available;
- exits only when the underlying receiver closes or the Tokio task is aborted by process shutdown.

`AppState` does not need graceful explicit shutdown for this phase because Hub already relies on process-level Tokio task shutdown for HTTP, gRPC, and session expiry tasks.

### Deployment Matrix

| Database   | Control Plane | Supported Shape                            |
| ---------- | ------------- | ------------------------------------------ |
| SQLite     | in-process    | Single Hub process, lightweight deployment |
| PostgreSQL | in-process    | Single Hub process using PostgreSQL        |
| PostgreSQL | NATS          | Horizontally scaled Hub replicas           |
| SQLite     | NATS          | Unsupported configuration                  |

If `PANDAR_CONTROL_PLANE=nats` is configured with SQLite, Hub startup must fail with a clear error.

## Control Messages

Use one internal message enum with JSON serialization:

- `AgentWake { tenant_id, agent_id }`
  Wakes the Hub replica that owns the agent stream. The receiving replica drains queued commands from the database.
- `AgentClose { tenant_id, agent_id }`
  Closes the current agent stream after credential rotation or revocation.
- `PrinterEvent { tenant_id, event }`
  Fans out real-time printer/job events to WebSocket subscribers on all Hub replicas.

The `AgentWake` message does not include command payloads. Command payloads stay in the database and are read by the connected Hub instance through the existing command repository.

Printer/job updates are delivered to WebSocket subscribers only through the control-plane subscriber path. The publishing code does not also call the local WebSocket broadcast sender directly. This means a publishing replica that receives its own NATS or in-process message produces exactly one local WebSocket delivery, not a duplicate.

## NATS Subject Design

Use a single deployment prefix to keep permissions simple:

- `pandar.hub.control`

The first implementation may use one subject for all control messages. Subject sharding can be added later if measured traffic requires it. NATS credentials are service credentials for Hub only. Hub instances may publish and subscribe under the Pandar internal prefix; agents and browser clients never connect to NATS.

## Authentication and Authorization

Pandar's current authorization remains authoritative:

- Tenant tokens and user tokens are validated by Hub.
- Agent credentials are validated by Hub against persisted agent records.
- Tenant/resource ownership checks remain in repositories and route handlers.
- NATS does not accept Tenant Tokens and does not enforce tenant policy.

Control messages must include tenant and agent identifiers only for routing and local filtering. Receivers must not treat control-plane payloads as proof of authorization.

## WebSocket Tickets

Browser WebSocket tickets are currently in process memory. In horizontally scaled mode, a ticket issued by one Hub replica must be accepted by another. Store printer event tickets in the database behind a repository boundary:

- `issue_ticket(tenant_id)` inserts a hashed, single-use ticket with RFC3339 expiry.
- `consume_ticket(tenant_id, plaintext_ticket)` atomically marks one unexpired, unused ticket as used.
- Ticket behavior remains unchanged for callers: one-use, tenant-bound, short TTL.

This repository must support both SQLite and PostgreSQL. SQLite keeps single-process control plane behavior, but ticket storage can still be database-backed for consistent code paths.

## Runtime Behavior

### Command Dispatch

1. HTTP route enqueues a command in the database.
2. Hub publishes `AgentWake`.
3. Every Hub replica receives the message.
4. Only the replica with a matching local session wakes the stream.
5. The outbound pump drains queued commands from the database.

If a control message is lost or NATS is temporarily unavailable, the command remains queued in the database. The current phase does not add durable NATS replay; operational recovery is reconnecting the agent or issuing another wake-producing action.

Every command-producing path must publish `AgentWake` after the command row is committed, including:

- refresh/discover/diagnose printer routes;
- print job creation;
- print dispatch retry;
- reprint;
- duplicate-and-print.

At least one print command path must be covered by a cross-replica wake test so print-specific command creation cannot regress back to local-only wake behavior.

### Credential Rotation and Revocation

1. Route updates the persisted credential state.
2. Hub publishes `AgentClose`.
3. The replica with the current local session closes the stream.
4. The agent reconnects and is re-authenticated by Hub.

### Printer and Job Events

1. A Hub replica receives an agent printer snapshot or print report.
2. It persists any durable state through existing repositories.
3. It publishes `PrinterEvent`.
4. All Hub replicas forward the event to their local WebSocket subscribers for that tenant.

## Configuration

Add environment configuration:

- `PANDAR_CONTROL_PLANE`
  - unset or `in-process`: use in-process control plane.
  - `nats`: use NATS control plane.
- `PANDAR_NATS_URL`
  Required when `PANDAR_CONTROL_PLANE=nats`.
- `PANDAR_NATS_SUBJECT`
  Optional; default `pandar.hub.control`.

Configuration errors must preserve cause/context chains in logs and returned errors.

NATS behavior:

- Hub startup fails if `PANDAR_CONTROL_PLANE=nats` and `PANDAR_NATS_URL` is missing or blank.
- Hub startup fails if the initial NATS connection cannot be established.
- After startup, publish failures are logged with full error context but do not roll back already-committed database writes. User-facing routes that successfully committed the durable command/ticket/job state should still return their normal success response.
- Runtime NATS reconnect behavior is delegated to `async-nats`. If the client cannot publish during a reconnect window, the route follows the publish-failure rule above.
- Subscriber receive/deserialize failures are logged and skipped; they do not crash HTTP/gRPC serving.

## Testing Requirements

Add targeted tests that prove:

- SQLite/default state uses an in-process control plane.
- SQLite/default `AppState` startup succeeds without any broker or NATS configuration.
- Configuring NATS with SQLite is rejected.
- Control-plane subscriber tests wait for subscription readiness before publishing non-durable control messages.
- Two `AppState` values sharing the same in-process control plane can simulate two Hub replicas:
  - a command enqueued by one state wakes the agent stream owned by the other;
  - a print job command created by one state wakes the agent stream owned by the other;
  - credential revocation on one state closes the agent stream owned by the other;
  - a printer event published by one state reaches a WebSocket subscriber on the other.
- Wrong-tenant/wrong-agent control messages do not wake or close unrelated agent streams, and wrong-tenant printer events do not reach a subscriber for a different tenant.
- Printer events are not duplicated when the publisher also has a local WebSocket subscriber.
- Browser printer event tickets can be issued by one state and consumed by another state sharing the same database.
- Existing one-use, wrong-tenant, and expired-ticket behavior still holds against both SQLite and PostgreSQL repository tests where PostgreSQL is configured.

NATS integration should be unit-tested at the boundary with a fake or in-process control plane. Do not require a live NATS server in the default test suite.

## Documentation Requirements

Update:

- `docs/architecture.md` with the deployment matrix and control plane role.
- `docs/development.md` or deployment docs with `PANDAR_CONTROL_PLANE`, `PANDAR_NATS_URL`, and local NATS notes.
- `docker-compose.postgres.yml` to include an optional NATS service or document how to enable it.
- `docs/roadmap.md` with completed and remaining scaling work.

## Acceptance Criteria

- Hub starts in existing SQLite mode without a broker.
- Hub can be configured for PostgreSQL + NATS without changing agent/client authentication.
- NATS is internal-only; no agent or browser code connects to it.
- Cross-replica command wake, session close, WebSocket event fanout, and ticket consumption are covered by tests using shared in-process control plane/database fixtures.
- `cargo fmt`, `cargo clippy`, and workspace tests pass or documented blockers are reported with exact output.
