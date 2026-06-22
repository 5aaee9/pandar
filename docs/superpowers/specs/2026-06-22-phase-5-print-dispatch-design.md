# Phase 5 Print Dispatch Design

## Goal

Phase 5 adds the first durable print-dispatch path: a client can create a tenant-scoped print job for an existing printer, hub stores the job and artifact metadata, hub queues a print command through the existing command ledger, and `pandar-agent` uploads the artifact to the configured printer file-transfer boundary before publishing a Bambu `project_file` MQTT command.

This phase intentionally builds the smallest verifiable print path. It does not add authentication, cloud object storage, slicing, live Bambu compatibility validation, AMS mapping UI, resumable uploads, or frontend WebSocket job updates.

## Scope

- Persist print jobs and job artifacts in `pandar-hub` for both SQLite and PostgreSQL.
- Accept a simple HTTP print request for an existing tenant/printer.
- Store uploaded artifact bytes on hub local disk under a configurable spool directory.
- Queue a `print_project_file` command through the existing command ledger.
- Deliver `PrintProjectFile` over the existing reverse gRPC command stream.
- Execute the command in `pandar-agent` using configured local printer credentials.
- Upload bytes through the existing `MachineFileTransfer` boundary and publish MQTT `project_file` through the existing MQTT transport.
- Reconcile command result and job status from the existing command acknowledgement/result path.
- Add a small frontend job dispatch form and job history panel backed by HTTP only.
- Update documentation and roadmap.

Out of scope:

- User authentication and authorization beyond the existing tenant path boundary.
- Browser multipart upload, large-file streaming, resumable uploads, or remote object storage.
- Slicing `.3mf` or `.stl` inputs; clients submit an already printable 3MF/G-code-like artifact.
- Real FTPS runtime implementation or live-printer tests.
- Detailed printer progress tracking. Phase 5 stores dispatch lifecycle state; progress from MQTT reports remains later work.
- AMS tray mapping UI. Phase 5 stores `use_ams`, `flow_cali`, and `timelapse` booleans only.

## Data Model

`pandar-core` gains print job domain records:

- `JobId`: UUID newtype.
- `JobStatus`: `queued`, `sent`, `acknowledged`, `succeeded`, `failed`.
- `Job`: tenant-scoped print request with `id`, `tenant_id`, `printer_id`, `agent_id`, `artifact_id`, `command_id`, `status`, `error`, `created_at`, and `updated_at`.
- `JobArtifact`: tenant-scoped artifact metadata with `id`, `tenant_id`, `filename`, `content_type`, `size_bytes`, `storage_path`, and `created_at`.

Hub migrations add `job_artifacts` and `jobs` tables for SQLite and PostgreSQL. `jobs.command_id` references `commands(id)` and lets command result events update the matching job. All IDs are persisted as UUID strings. `storage_path` is a hub-local relative path under the configured spool root, not a user-controlled absolute path.

In Phase 5, `succeeded` means the agent completed dispatch: it read the artifact, uploaded it through the machine file-transfer boundary, and successfully published the MQTT `project_file` command. It does not mean the printer physically completed the print. Physical print progress/completion will require later MQTT report reconciliation.

## Hub Storage

`pandar-hub` gets a local spool root:

- Environment variable: `PANDAR_SPOOL_DIR`.
- Default: `pandar-spool`.
- Files are written under `{spool_root}/{tenant_id}/{artifact_id}/{safe_filename}`.
- Filename sanitization keeps ASCII alphanumeric, `.`, `_`, and `-`; other characters become `_`; empty sanitized names become `artifact.bin`.
- The hub creates parent directories before writing bytes.
- File write errors keep full context chains.

Phase 5 request bodies are JSON with artifact bytes encoded as base64. This is deliberately simple and testable. It is not the final large-file upload design.

Artifact size limits:

- Environment variable: `PANDAR_MAX_ARTIFACT_BYTES`.
- Default: `10485760` bytes (10 MiB).
- The hub rejects decoded artifact bodies larger than the limit with `413 Payload Too Large` and error code `artifact_too_large`.
- Empty decoded artifact bodies return `400`.
- Invalid `PANDAR_MAX_ARTIFACT_BYTES` fails hub startup with full context instead of silently falling back.

## Hub Repository

Add `JobRepository` with backend-neutral behavior:

- `create_print_job(tenant_id, printer_id, artifact_input, print_options)` validates tenant/printer ownership, creates `job_artifacts`, queues a `print_project_file` command for the printer's current agent, creates a `jobs` row linked to that command, and returns the job with artifact and command metadata.
- `list_for_tenant(tenant_id)` returns jobs newest first and returns `MissingTenant` when the tenant is missing.
- `get_for_tenant(tenant_id, job_id)` returns `Ok(None)` for unknown jobs.
- `mark_for_command(command_id, status, error)` updates the job linked to a command, if any. Duplicate terminal updates are idempotent.

Command repository adds `enqueue_print_project_file(tenant_id, agent_id, printer_id, payload)`. It must verify that the agent belongs to the tenant and the printer belongs to both tenant and agent. Wrong tenant/agent/printer ownership returns existing repository ownership errors or `MissingPrinter`.

`RepositoryError` gains `MissingPrinter`, `MissingJob`, and `InvalidPersistedJobStatus`. Route/gRPC mappings follow existing stable patterns: missing resources are `404`, ownership mismatch is `403`, invalid persisted data logs and returns internal errors.

Create ordering and cleanup:

1. The route decodes and size-checks the artifact before opening a database transaction.
2. The route writes artifact bytes to the spool path before creating database rows.
3. `JobRepository::create_print_job` runs artifact metadata insert, print command insert, and job insert in a single database transaction.
4. If the transaction fails, the route attempts to remove the newly written spool file and returns the original database error with context. Cleanup failure is logged with `{err:#}` but does not replace the original error.
5. A queued print command must never be committed without a linked job row in the same transaction.
6. A spool file without database rows is acceptable only after a process crash between file write and transaction commit; this phase does not add garbage collection.

## Command Payload And gRPC

Command kind: `print_project_file`.

Command payload JSON:

```json
{
  "job_id": "uuid",
  "artifact_id": "uuid",
  "filename": "part.3mf",
  "serial_number": "01S00EXAMPLE",
  "storage_path": "tenant/artifact/part.3mf",
  "size_bytes": 12345,
  "plate_id": 1,
  "use_ams": false,
  "flow_cali": true,
  "timelapse": false
}
```

`proto/pandar/agent/v1/agent.proto` adds:

```proto
message PrintProjectFile {
  string job_id = 1;
  string artifact_id = 2;
  string printer_id = 3;
  string serial_number = 4;
  string filename = 5;
  string storage_path = 6;
  uint64 size_bytes = 7;
  uint32 plate_id = 8;
  bool use_ams = 9;
  bool flow_cali = 10;
  bool timelapse = 11;
}
```

`HubCommand.command` adds `PrintProjectFile print_project_file = 11;`.

The hub outbound command conversion reads `CommandRecord.kind`. `refresh_printers` keeps the current behavior. `print_project_file` deserializes payload JSON and sends `PrintProjectFile`. Malformed persisted payload is treated as an internal repository/protocol error with full context logging.

When the hub receives command acknowledgement and result events, it updates the command ledger as today and also updates a linked job:

- accepted ack: job `acknowledged`.
- rejected ack: job `failed` with ack error.
- success result: job `succeeded`.
- failure result: job `failed` with result error.

The job is created as `queued`. When command is marked sent by the outbound pump, the linked job becomes `sent`.

## HTTP API

Phase 5 adds:

- `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs`
- `GET /api/v1/tenants/{tenant_id}/jobs`
- `GET /api/v1/tenants/{tenant_id}/jobs/{job_id}`

Create request:

```json
{
  "filename": "part.3mf",
  "content_type": "application/octet-stream",
  "artifact_base64": "...",
  "plate_id": 1,
  "use_ams": false,
  "flow_cali": true,
  "timelapse": false
}
```

Rules:

- Invalid tenant, printer, or job IDs return `400`.
- Missing tenant, printer, or job returns `404`.
- Empty filename or artifact body returns `400`.
- Invalid base64 returns `400`.
- Decoded artifact larger than `PANDAR_MAX_ARTIFACT_BYTES` returns `413` with `artifact_too_large`.
- `plate_id` is required and must be at least `1`.
- `content_type` defaults to `application/octet-stream` when empty.
- Successful create returns `201` with job, artifact, and command response fields.

Job response:

```json
{
  "id": "uuid",
  "tenant_id": "uuid",
  "printer_id": "uuid",
  "agent_id": "uuid",
  "artifact_id": "uuid",
  "command_id": "uuid",
  "status": "queued",
  "error": null,
  "created_at": "2026-06-22T00:00:00Z",
  "updated_at": "2026-06-22T00:00:00Z",
  "artifact": {
    "id": "uuid",
    "filename": "part.3mf",
    "content_type": "application/octet-stream",
    "size_bytes": 12345
  },
  "command": {
    "id": "uuid",
    "kind": "print_project_file",
    "status": "queued"
  }
}
```

## Agent Execution

`BambuMachineGateway` gains `print_project_file(command)`.

Agent command handling for `PrintProjectFile`:

1. Emit accepted command ack after validating the requested `serial_number` exists in local `PANDAR_PRINTERS`.
2. Read artifact bytes from the command `storage_path` only through the agent-local artifact reader abstraction. In Phase 5 tests this is fake/in-memory; local runtime reads from a configured path if available.
3. Upload bytes to the printer file transfer path `filename` through `run_with_transfer_mode` and `MachineFileTransfer`.
4. Publish MQTT `project_file` with `filename`, `plate_id`, `task_id = job_id`, `subtask_id = artifact_id`, `use_ams`, `flow_cali`, and `timelapse`.
5. Emit success command result after MQTT publish succeeds, otherwise emit failure with full context.

Because hub-local spool paths are not automatically mounted into user machines, Phase 5 runtime is primarily useful when hub and agent share a filesystem or tests inject an artifact reader. The HTTP and command contract are still durable and prepare the later remote artifact-download path.

No Phase 5 tests open real Bambu MQTT or file-transfer sockets.

Agent artifact reader contract:

- Environment variable: `PANDAR_ARTIFACT_ROOT`.
- Default: current working directory.
- The runtime artifact reader joins `PANDAR_ARTIFACT_ROOT` with the command `storage_path`.
- `storage_path` must be relative, must not contain parent directory components, and must not contain platform path prefixes. Unsafe paths fail before file IO.
- Missing files produce a failed command result whose error preserves the full context chain, including the requested storage path.
- Tests use an in-memory artifact reader and assert that unsafe and missing paths fail without MQTT publish or file-transfer upload.

Printer identity contract:

- Hub commands include both hub `printer_id` and Bambu `serial_number`.
- Hub `printer_id` remains for command/job audit and tenant scoping.
- The agent selects the local `BambuPrinterEndpoint` by exact `serial_number == endpoint.serial`.
- `PANDAR_PRINTERS` is not extended with hub printer IDs in Phase 5.
- Unknown serial numbers produce a rejected command ack with an error containing the serial number and do not read artifacts, upload files, or publish MQTT.

## Frontend

The frontend remains HTTP-only and server-rendered. It adds to the Phase 4 dashboard:

- Job list for the selected tenant.
- A compact print dispatch form for the selected printer.
- The form accepts filename, base64 artifact text, plate id, and the three boolean options.
- It submits to the hub job endpoint with a server action or standard HTML form path that does not require client-side state.
- It shows job status, printer id, artifact filename, created time, and error when present.

This is an operator/debug UI, not a polished upload UX. A real browser file picker and large upload path remain future work.

## Tests And Verification

Required tests:

- Core job ID/status/domain validation.
- SQLite migrations include `job_artifacts` and `jobs` tables.
- Job repository create/list/get/status behavior.
- Command repository print command validates printer ownership and payload.
- Optional PostgreSQL job repository behavior under `PANDAR_TEST_POSTGRES_URL`.
- gRPC outbound stream sends `PrintProjectFile` for queued print commands.
- gRPC ack/result updates linked job status and stale replaced streams cannot update jobs.
- Agent command handling uploads through fake file transfer, publishes fake MQTT `project_file`, and emits ack/result in order.
- Agent failure paths preserve context chains.
- HTTP job create/list/detail route behavior and validation, including invalid base64, empty artifact, and oversized artifact.
- Frontend TypeScript build.

Required verification before commit:

- `cargo fmt`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
- `npm run build` in `frontend/`
- `git diff --check`
- No protobuf generated Rust files outside `target`.

## Safety

- Phase 5 uses local fake transports in tests and does not open real Bambu sockets.
- Local spool writes are tenant/artifact scoped and do not use caller-controlled absolute paths.
- Existing stale-session protection remains mandatory for command acknowledgement/result updates.
- Database behavior must stay equivalent between SQLite and PostgreSQL.
- Docs and roadmap must be updated after implementation review.
