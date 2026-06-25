# Phase 25 Scaled Artifact Storage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace local-only print artifact handling with a backend-neutral storage boundary, streaming upload transport, Hub-mediated agent artifact fetch, and an S3-compatible backend so PostgreSQL + NATS Hub replicas do not require a shared spool directory.

**Architecture:** Keep job metadata in the existing SQLite/PostgreSQL repositories and move artifact bytes behind a new `ArtifactStorage` boundary owned by `AppState`. Browser uploads stream through a Next.js route handler to Hub multipart routes, plugin uploads stream multipart directly, and agents fetch artifacts back through Hub HTTP using their existing agent credential. Filesystem storage stays the SQLite/single-node default; S3-compatible storage is selected explicitly.

**Tech Stack:** Rust 2024, axum multipart, tokio file I/O, reqwest multipart, AWS SDK for Rust (`aws-config`, `aws-sdk-s3`) for S3-compatible object storage, existing SeaORM/sqlx repositories, Next.js route handlers, existing NATS control plane tests/fakes.

---

## Reviewed Inputs

- Spec: `docs/superpowers/specs/2026-06-24-phase-25-scaled-artifact-storage-design.md`
- Current Hub filesystem storage: `crates/pandar-hub/src/jobs.rs`
- Current browser base64 action: `frontend/app/actions.ts`, `frontend/app/dispatch-form.tsx`
- Current plugin base64 submission: `crates/pandar-network-plugin/src/lib.rs`
- Agent print artifact reader: `crates/pandar-agent/src/commands.rs`
- Print command proto: `proto/pandar/agent/v1/agent.proto`
- Command payload conversion: `crates/pandar-hub/src/grpc/commands.rs`, `crates/pandar-hub/src/repositories/jobs/create.rs`
- Cleanup CLI: `crates/pandar-app/src/main.rs`, `crates/pandar-hub/src/cleanup.rs`
- Readiness/metrics: `crates/pandar-hub/src/readiness.rs`, `crates/pandar-hub/src/metrics_export.rs`
- Deployment docs: `docs/development.md`, `docs/architecture.md`, `docs/release-installation.md`, `docker-compose.postgres.yml`, `docker-compose.sqlite.yml`

## Execution Rules

- Implement Phase 25 only.
- Do not keep JSON/base64 print submission as a compatibility fallback after browser and plugin callers are updated.
- Do not expose storage keys, local paths, S3 bucket/key values, object credentials, signed URLs, bearer tokens, plugin tickets, or agent credentials to browsers, plugins, public API responses, audit metadata, or unredacted logs.
- Do not parse slicer artifacts or add checksum/dedup/preview behavior.
- Keep repository behavior backend-neutral. Any schema change must have matching SQLite and PostgreSQL migrations plus parity tests.
- Keep PostgreSQL tests optional behind `PANDAR_TEST_POSTGRES_URL`; SQLite/default tests must run locally.
- Use the AWS SDK for Rust S3 client unless the plan is amended by a reviewed dependency decision. Configure endpoint, region, static credentials, and path-style behavior from Phase 25 env vars.
- SDD requires one final Lore-format commit and push only after implementation review, docs update, fresh verification, and final approval.

## File Structure

Create:

- `crates/pandar-hub/src/artifacts/mod.rs` - `ArtifactStorage` trait, config parsing, filesystem backend, upload staging helpers, redaction helper.
- `crates/pandar-hub/src/artifacts/s3.rs` - S3-compatible backend, internal S3 client trait, SDK adapter, and fake-client tests.
- `crates/pandar-hub/src/routes/artifacts.rs` - agent artifact download route.
- `frontend/app/api/tenants/[tenantId]/printers/[printerId]/jobs/route.ts` - browser upload proxy route handler.
- `tools/scaled-artifact-smoke/Cargo.toml` and `tools/scaled-artifact-smoke/src/main.rs` - scripted in-process two-Hub integration harness using a shared durable database and shared fake object storage by default, with optional live PostgreSQL/NATS/S3 service URLs.

Modify:

- Workspace and crate manifests: enable `axum` multipart, add S3 SDK crates, add `reqwest` multipart feature.
- `crates/pandar-hub/src/lib.rs` - replace `JobStorageConfig` field with `ArtifactStorageConfig`/shared storage handle and expose storage accessors.
- `crates/pandar-hub/src/jobs.rs` - remove or reduce to re-export compatibility during the move; final code should not keep local-only storage logic as the primary API.
- `crates/pandar-hub/src/routes.rs` - register multipart job routes and agent artifact route.
- `crates/pandar-hub/src/routes/jobs.rs` and `crates/pandar-hub/src/routes/plugin.rs` - replace JSON/base64 handlers with multipart handlers and shared creation helper.
- `crates/pandar-hub/src/repositories/jobs/create.rs` and command payload structs - add `artifact_download_path`.
- `proto/pandar/agent/v1/agent.proto`, generated protocol modules via build - add `artifact_download_path` to `PrintProjectFile`.
- `crates/pandar-hub/src/grpc/commands.rs` - populate proto field and enforce object-backed command reference.
- `crates/pandar-agent/src/lib.rs` and `crates/pandar-agent/src/commands.rs` - add `PANDAR_HUB_API_URL` and Hub HTTP artifact reader.
- `crates/pandar-network-plugin/src/lib.rs` and tests - stream multipart and update stable error allowlist.
- `frontend/app/actions.ts`, `frontend/app/dispatch-form.tsx`, `frontend/next.config.ts` - remove base64 conversion/server action body-size dependency and use upload proxy.
- `crates/pandar-app/src/main.rs`, `crates/pandar-hub/src/cleanup.rs` - delete artifacts through selected storage backend.
- `crates/pandar-hub/src/readiness.rs`, `crates/pandar-hub/src/metrics_export.rs`, `crates/pandar-hub/src/metrics.rs` - rename/check artifact storage health and storage failure counters.
- Docs and compose files listed above plus `docs/roadmap.md`.

## Task 1: Storage Boundary And Filesystem Backend

**Files:**

- Create: `crates/pandar-hub/src/artifacts/mod.rs`
- Modify: `crates/pandar-hub/src/lib.rs`
- Modify: `crates/pandar-hub/src/jobs.rs`

- [ ] **Step 1: Add failing storage tests**

Add tests in `crates/pandar-hub/src/artifacts/mod.rs` for:

- `filesystem_storage_rejects_empty_and_oversized_upload`
- `filesystem_storage_sanitizes_filename_and_returns_opaque_key`
- `filesystem_storage_open_and_delete_round_trip`
- `filesystem_storage_rejects_unsafe_key_on_read_and_delete`
- `artifact_storage_config_defaults_to_filesystem`

Run:

```bash
cargo test -p pandar-hub artifacts::tests -- --nocapture
```

Expected before implementation: compile failure because `artifacts` does not exist.

- [ ] **Step 2: Implement the trait and filesystem backend**

Create `ArtifactStorage` with async methods:

```rust
#[async_trait::async_trait]
pub trait ArtifactStorage: Send + Sync {
    async fn put_artifact(&self, input: StoreArtifactInput<'_>) -> anyhow::Result<StoredArtifact>;
    async fn open_artifact(&self, storage_key: &str) -> anyhow::Result<ArtifactBody>;
    async fn delete_artifact(&self, storage_key: &str) -> anyhow::Result<()>;
    async fn check_ready(&self) -> anyhow::Result<()>;
    fn max_artifact_bytes(&self) -> usize;
    fn backend(&self) -> ArtifactStorageBackend;
}
```

Use a staged upload file as the input body, not `Vec<u8>`. Keep filename sanitization from `jobs.rs`. Keep `PANDAR_SPOOL_DIR`, `PANDAR_MAX_ARTIFACT_BYTES`, and default `pandar-spool` behavior.

- [ ] **Step 3: Wire `AppState` to the new storage handle**

Replace `job_storage: JobStorageConfig` with an `Arc<dyn ArtifactStorage>` plus a config wrapper if needed. Keep an accessor named `artifact_storage()`. Update temporary test state construction to use filesystem storage.

Temporarily keep `job_storage()` only if too many tests still refer to it in this task, but remove it before the final task unless it is just a small alias to `artifact_storage()` with no local-spool semantics.

- [ ] **Step 4: Run focused storage tests**

Run:

```bash
cargo test -p pandar-hub artifacts::tests -- --nocapture
```

Expected: storage tests pass.

## Task 2: Multipart Upload Routes And Shared Job Creation Helper

**Files:**

- Modify: `Cargo.toml`
- Modify: `crates/pandar-hub/Cargo.toml`
- Modify: `crates/pandar-hub/src/routes.rs`
- Modify: `crates/pandar-hub/src/routes/jobs.rs`
- Modify: `crates/pandar-hub/src/routes/plugin.rs`
- Modify: `crates/pandar-hub/src/routes/tests/jobs/create.rs`
- Modify: `crates/pandar-hub/src/routes/tests/plugin.rs`
- Modify: `frontend/app/dispatch-form.tsx`

- [ ] **Step 1: Enable multipart dependencies**

Update workspace `axum` to include `"multipart"` and keep `"ws"`. Do not add a new multipart parser crate.

Replace the current router-wide base64 body limit with route-local handling: keep a small default body limit for JSON/control routes, and apply the large artifact limit only to multipart print routes where streaming enforcement counts uploaded file bytes. Do not leave `max_artifact_bytes * 2 + 4096` as a global router limit.

Run:

```bash
cargo check -p pandar-hub
```

Expected before route edits: may compile or fail only where signatures still expect JSON/base64.

- [ ] **Step 2: Add failing Hub route tests**

Add route tests covering multipart browser and plugin print creation:

- successful create writes storage and queues command
- missing file part returns `artifact_invalid_upload`
- empty file returns `artifact_empty`
- oversized stream returns `artifact_too_large`
- invalid printer id returns `invalid_printer_id`
- missing printer returns `printer_not_found`
- bad plate returns `artifact_invalid_plate`
- repository failure deletes stored artifact through the storage boundary

Run:

```bash
cargo test -p pandar-hub routes::tests::jobs::create -- --nocapture
cargo test -p pandar-hub routes::tests::plugin -- --nocapture
```

Expected before implementation: tests fail or do not compile because handlers still accept JSON/base64.

- [ ] **Step 3: Implement staged multipart parsing**

Add one helper used by browser and plugin routes:

- Parse text fields for current logical request values.
- Stream the file field into a tempfile under the selected storage backend's staging directory or system temp.
- Count bytes as chunks arrive; reject above `PANDAR_MAX_ARTIFACT_BYTES` with `artifact_too_large`.
- Reject zero bytes with `artifact_empty`.
- Reject malformed multipart/missing file as `artifact_invalid_upload`.
- Call `ArtifactStorage::put_artifact` with the staged file.

Do not call `Field::bytes()` for artifact bodies.

- [ ] **Step 4: Replace JSON/base64 handlers**

Change:

- `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs`
- `POST /api/v1/plugin/prints`

to multipart handlers. Delete `artifact_base64` from request DTOs and retire `validate_artifact_submission`. Keep stable validation/error labels.

- [ ] **Step 5: Run route tests**

Run:

```bash
cargo test -p pandar-hub routes::tests::jobs::create -- --nocapture
cargo test -p pandar-hub routes::tests::plugin -- --nocapture
```

Expected: multipart route tests pass.

## Task 3: Frontend Upload Proxy And Browser Form

**Files:**

- Create: `frontend/app/api/tenants/[tenantId]/printers/[printerId]/jobs/route.ts`
- Modify: `frontend/app/actions.ts`
- Modify: `frontend/app/dispatch-form.tsx`
- Modify: `frontend/next.config.ts`

- [ ] **Step 1: Add the Next route handler**

Create a route handler that accepts browser `multipart/form-data`, forwards the body to `${APP_API_URL}/api/v1/tenants/{tenantId}/printers/{printerId}/jobs`, and applies existing `apiHeaders()` server-side auth. Return the Hub status/body unchanged enough for the client to redirect on stable error labels.

Boundary rule: do not set a bare `Content-Type: multipart/form-data` header. Either forward the incoming `content-type` header including its browser boundary together with the original request body, or reconstruct a `FormData` object server-side and let `fetch` set the boundary. Merge only auth headers from `apiHeaders()`; do not overwrite the multipart boundary.

- [ ] **Step 2: Remove browser base64 conversion**

Change `DispatchForm` to keep a `File | null` and submit the actual file input. Remove hidden `artifact_base64`, `filename`, and `content_type` fields unless filename/content type are still needed as text metadata. Replace visible copy that mentions base64 with upload-neutral text.

Update backend error chips to remove `artifact_invalid_base64` and include `artifact_invalid_upload`.

- [ ] **Step 3: Remove server-action upload path**

Delete or narrow `createPrintJob` so browser print creation uses the route handler instead of the server action. Remove the 360 MB server action `bodySizeLimit` in `frontend/next.config.ts` if no other action needs it.

- [ ] **Step 4: Verify frontend**

Run:

```bash
npm --prefix frontend run build
```

Expected: Next.js build succeeds.

## Task 4: Plugin Multipart Submission

**Files:**

- Modify: `Cargo.toml`
- Modify: `crates/pandar-network-plugin/Cargo.toml`
- Modify: `crates/pandar-network-plugin/src/lib.rs`
- Modify: `crates/pandar-network-plugin/tests/http_boundary.rs`
- Modify: `crates/pandar-network-plugin/tests/studio_abi_probe.rs`

- [ ] **Step 1: Enable reqwest multipart**

Add the `multipart` feature to workspace `reqwest`. Keep the existing async runtime pattern in `crates/pandar-network-plugin/src/lib.rs`; do not switch to `reqwest::blocking`.

- [ ] **Step 2: Add failing plugin boundary tests**

Update tests to assert:

- plugin sends `multipart/form-data` to `/api/v1/plugin/prints`
- request body contains field names for printer id, plate id, flags, mappings, and file part
- `artifact_invalid_upload` passes through as stable
- `artifact_invalid_base64` is no longer in the stable allowlist
- missing and empty local artifact still fail before network without leaking paths

Run:

```bash
cargo test -p pandar-network-plugin --test http_boundary -- --nocapture
```

Expected before implementation: tests fail because plugin still posts JSON/base64.

- [ ] **Step 3: Implement multipart plugin submission**

Keep local file validation for missing/empty artifacts. Build a `reqwest::multipart::Form` with text fields and a file part inside the existing async request boundary that is executed by `runtime().block_on`. Use the display filename from Studio parameters, not the local path, for the multipart filename.

- [ ] **Step 4: Update Studio ABI probe**

Change the mock hub assertion from JSON `artifact_base64` to multipart shape. Keep the probe focused on ABI flow, not multipart parser completeness.

- [ ] **Step 5: Run plugin tests**

Run:

```bash
cargo test -p pandar-network-plugin
```

Expected: plugin tests pass.

## Task 5: Command Protocol And Dispatch Metadata

**Files:**

- Modify: `proto/pandar/agent/v1/agent.proto`
- Modify: `crates/pandar-hub/src/repositories/jobs/create.rs`
- Modify: `crates/pandar-hub/src/repositories/jobs.rs`
- Modify: `crates/pandar-hub/src/grpc/commands.rs`
- Modify: `crates/pandar-hub/src/grpc/tests/print_jobs.rs`
- Modify: `crates/pandar-agent/src/commands/tests/print.rs`

- [ ] **Step 1: Extend proto**

Add:

```proto
string artifact_download_path = 14;
```

to `PrintProjectFile`.

- [ ] **Step 2: Extend persisted command payload**

Add `artifact_download_path` to `PrintProjectFilePayload`. For every new create/retry/reprint/duplicate/plugin command, set:

```text
/api/v1/agents/{agent_id}/artifacts/{artifact_id}
```

Keep `storage_path` persisted for metadata and transitional filesystem reader tests.

- [ ] **Step 3: Enforce object-backed command safety**

Do not try to read storage backend state inside `hub_command_from_record(command: CommandRecord)`, because that function is currently pure. Add a small conversion options struct and thread it from the outbound pump where `AppState` is available:

```rust
pub struct CommandConversionOptions {
    pub require_artifact_download_path: bool,
}

pub fn hub_command_from_record_with_options(
    command: CommandRecord,
    options: CommandConversionOptions,
) -> Result<HubCommand, Status>
```

Keep `hub_command_from_record(command)` as a test/helper wrapper that calls the new function with `require_artifact_download_path: false`. In the production outbound dispatch path, pass `require_artifact_download_path: state.artifact_storage().backend().requires_hub_fetch()`. If the option is true and a print command payload has an empty `artifact_download_path`, return `Status::internal("missing artifact download path")` before the command is sent.

For new commands, tests should prove `artifact_download_path` is present regardless of backend.

- [ ] **Step 4: Run protocol tests**

Run:

```bash
cargo test -p pandar-hub grpc::tests::print_jobs -- --nocapture
cargo test -p pandar-hub repositories::tests::jobs -- --nocapture
cargo test -p pandar-agent commands::tests::print -- --nocapture
```

Expected: print commands include the Hub artifact path and agent precedence tests pass.

## Task 6: Agent Artifact Download Route And Reader

**Files:**

- Create: `crates/pandar-hub/src/routes/artifacts.rs`
- Create: `crates/pandar-hub/src/routes/tests/artifacts.rs`
- Modify: `crates/pandar-hub/src/routes.rs`
- Modify: `crates/pandar-hub/src/routes/tests/mod.rs`
- Modify: `crates/pandar-hub/src/repositories/agents.rs`
- Modify: `crates/pandar-hub/src/repositories/jobs.rs` or a focused child module
- Modify: `crates/pandar-hub/src/routes/tests/jobs/create.rs`
- Modify: `crates/pandar-agent/Cargo.toml`
- Modify: `crates/pandar-agent/src/lib.rs`
- Modify: `crates/pandar-agent/src/commands.rs`
- Modify: `crates/pandar-agent/src/commands/tests/print.rs`

- [ ] **Step 1: Add failing Hub artifact route tests**

Create `crates/pandar-hub/src/routes/tests/artifacts.rs`, register it in `routes/tests/mod.rs`, and add tests:

- valid agent credential downloads an artifact for a job assigned to that agent
- invalid credential returns `401`
- valid credential for another agent returns `403`
- missing artifact returns `404`
- route streams bytes from `ArtifactStorage::open_artifact`

Use the existing hashed agent credential logic and repository fixtures.

- [ ] **Step 2: Add ownership lookup**

Add a repository method such as:

```rust
pub async fn artifact_for_agent(
    &self,
    agent_id: AgentId,
    artifact_id: &str,
) -> RepositoryResult<Option<JobArtifact>>
```

It must join jobs/artifacts and require `jobs.agent_id = agent_id`. Add SQLite default tests and optional PostgreSQL parity when `PANDAR_TEST_POSTGRES_URL` is set.

- [ ] **Step 3: Implement agent HTTP auth route**

Parse `Authorization: Bearer <agent credential>`, hash the credential with the same `hash_secret` helper used by reverse gRPC in `crates/pandar-hub/src/grpc.rs`, load the agent credential record through `AgentRepository::get_credential_record`, and enforce revoked/mismatched credentials. Stream storage bytes in the response with `Content-Type` from metadata.

- [ ] **Step 4: Add agent reqwest dependency and `PANDAR_HUB_API_URL`**

Add `reqwest.workspace = true` to `crates/pandar-agent/Cargo.toml`; the workspace `reqwest` dependency already uses async rustls. Extend `AgentConfig` with:

```rust
#[arg(long, env = "PANDAR_HUB_API_URL")]
pub hub_api_url: Option<String>,
```

Derive an HTTP URL from `hub_grpc_url` only when it is an ordinary `http://` or `https://` URL. If derivation fails and `artifact_download_path` is present, fail with clear context that `PANDAR_HUB_API_URL` is required.

- [ ] **Step 5: Implement Hub artifact reader**

When a print command has `artifact_download_path`, use `reqwest` to GET `hub_api_url + artifact_download_path` with the agent credential bearer. Read the response body into bytes for the existing machine gateway call. Keep local filesystem reader only for commands without the new path.

- [ ] **Step 6: Run focused tests**

Run:

```bash
cargo test -p pandar-hub routes::tests::artifacts -- --nocapture
cargo test -p pandar-hub repositories::tests::jobs -- --nocapture
cargo test -p pandar-agent commands::tests::print -- --nocapture
```

Expected: route, repository, and agent print tests pass.

## Task 7: S3-Compatible Backend

**Files:**

- Modify: `Cargo.toml`
- Modify: `crates/pandar-hub/Cargo.toml`
- Create: `crates/pandar-hub/src/artifacts/s3.rs`
- Modify: `crates/pandar-hub/src/artifacts/mod.rs`

- [ ] **Step 1: Add dependencies**

Use supported Cargo commands to discover current compatible versions, then normalize the result into the workspace dependency table:

```bash
cargo add -p pandar-hub aws-config --features rt-tokio,rustls --no-default-features
cargo add -p pandar-hub aws-credential-types
cargo add -p pandar-hub aws-sdk-s3 --features rt-tokio,rustls --no-default-features
cargo add -p pandar-hub aws-smithy-types
```

After Cargo resolves versions, move those dependency declarations from `crates/pandar-hub/Cargo.toml` into root `[workspace.dependencies]` and leave `.workspace = true` entries in `crates/pandar-hub/Cargo.toml`. Do not use unsupported `cargo add --workspace`, and do not leave package-local versions when the workspace can own them.

Dependency rationale: choose AWS SDK for Rust because it is the official maintained S3 client, supports custom endpoints, static credentials, rustls, and service builder APIs for `put_object`, `get_object`, `delete_object`, and bucket readiness. Reject smaller unofficial S3 crates unless a reviewed dependency decision updates this plan, because Phase 25 needs predictable maintenance and S3-compatible behavior.

- [ ] **Step 2: Add failing S3 config tests**

Tests:

- missing bucket fails with `PANDAR_ARTIFACT_S3_BUCKET`
- missing credentials fail with the relevant env var name
- custom endpoint and force-path-style parse into config
- object keys are tenant/artifact scoped and do not include browser filenames as path authority

- [ ] **Step 3: Implement S3 backend with an internal client trait**

Define an internal seam in `artifacts/s3.rs`:

```rust
#[async_trait::async_trait]
trait S3ObjectClient: Send + Sync {
    async fn put_object(&self, bucket: &str, key: &str, body: ArtifactBody) -> anyhow::Result<()>;
    async fn get_object(&self, bucket: &str, key: &str) -> anyhow::Result<ArtifactBody>;
    async fn delete_object(&self, bucket: &str, key: &str) -> anyhow::Result<()>;
    async fn check_bucket(&self, bucket: &str) -> anyhow::Result<()>;
}
```

Production implements this trait with `aws_sdk_s3::Client`; tests use a fake in-memory client that records bucket/key/body calls and can inject not-found/delete/readiness errors.

Implement:

- `put_object` from staged file/body
- `get_object` to `ArtifactBody`
- `delete_object` treating not-found as success
- readiness via bucket reachability, with no permanent probe object

Use static credentials from Phase 25 env vars. Do not expose SDK errors directly without redaction/context.

- [ ] **Step 4: Run S3 tests**

Run:

```bash
cargo test -p pandar-hub artifacts::s3 -- --nocapture
```

Expected: S3 config/backend tests pass without a live S3 service by using the fake `S3ObjectClient`. If an SDK mock feature is added later, keep it dev-only.

## Task 8: Cleanup, Readiness, Metrics, And Scaled Guard

**Files:**

- Modify: `crates/pandar-app/src/main.rs`
- Modify: `crates/pandar-hub/src/cleanup.rs`
- Modify: `crates/pandar-hub/src/readiness.rs`
- Modify: `crates/pandar-hub/src/metrics.rs`
- Modify: `crates/pandar-hub/src/metrics_export.rs`
- Modify: `crates/pandar-hub/src/routes/tests/readiness_metrics.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/cleanup.rs`

- [ ] **Step 1: Add failing readiness and cleanup tests**

Tests:

- `/readyz` exposes `artifact_storage`, not `spool`
- Prometheus exposes `pandar_readyz{check="artifact_storage"}`
- PostgreSQL + NATS + filesystem without `PANDAR_ARTIFACT_FILESYSTEM_SHARED=true` is not ready
- cleanup execute calls storage delete before deleting artifact rows
- storage delete failure leaves artifact rows for retry

- [ ] **Step 2: Update cleanup CLI**

Change `pandar cleanup --execute` to create the same `ArtifactStorage` backend as Hub from env and call `delete_artifact` for each selected key before deleting rows. Keep dry-run non-mutating.

- [ ] **Step 3: Update readiness/metrics**

Rename the check to `artifact_storage`, call `ArtifactStorage::check_ready`, and add storage operation counters or labels for upload/write/read/delete/fetch failures if metrics state already has a natural place. Keep logs redacted with full cause chains.

- [ ] **Step 4: Run focused tests**

Run:

```bash
cargo test -p pandar-hub routes::tests::readiness_metrics -- --nocapture
cargo test -p pandar-hub repositories::tests::cleanup -- --nocapture
```

Expected: readiness and cleanup tests pass.

## Task 9: Scaled Artifact Smoke Harness

**Files:**

- Create: `tools/scaled-artifact-smoke/Cargo.toml`
- Create: `tools/scaled-artifact-smoke/src/main.rs`
- Modify: root `Cargo.toml` workspace `exclude` if needed
- Modify: `docs/development.md`

- [ ] **Step 1: Add smoke harness skeleton**

Create a small tool excluded from the workspace like `tools/release-smoke`. It should support:

```bash
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run
```

Dry-run must run a real in-process integration path, not a pure simulation. Use two `AppState` instances or two axum routers sharing one durable database file by default; when `PANDAR_TEST_POSTGRES_URL` and `PANDAR_TEST_NATS_URL` are set, the harness may run the same scenario against live PostgreSQL/NATS. The object storage backend may be a shared in-memory fake implementing `ArtifactStorage`, but both Hub states must use the same fake instance.

- [ ] **Step 2: Exercise the cross-Hub contract**

The dry-run mode must prove:

- Hub A's actual multipart route creates a job and stores bytes through the shared object-storage fake.
- Hub B uses the real command repository/outbound conversion path against the shared database and sees the queued command.
- The emitted `PrintProjectFile` contains `artifact_download_path`.
- A real axum router for Hub B serves the agent artifact download route.
- The agent-side Hub artifact reader fetches artifact bytes through Hub HTTP using `Authorization: Bearer <agent credential>`.
- No shared `PANDAR_SPOOL_DIR` path is used.

Live PostgreSQL/NATS/S3 mode may be env-gated, but the default smoke must still exercise real repository, route, command conversion, and agent fetch code with only object storage faked.

- [ ] **Step 3: Verify harness**

Run:

```bash
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run
```

Expected: exits 0 and prints a concise PASS summary.

## Task 10: Deployment Docs And Compose

**Files:**

- Modify: `docs/development.md`
- Modify: `docs/architecture.md`
- Modify: `docs/release-installation.md`
- Modify: `docker-compose.sqlite.yml`
- Modify: `docker-compose.postgres.yml`
- Modify: `docs/deployment/nixos/options.md` only if Nix module options are changed
- Modify: Nix module files if artifact storage env options are added

- [ ] **Step 1: Update storage docs**

Document:

- filesystem default for SQLite/single-node
- S3-compatible object storage env vars
- PostgreSQL + NATS requires object storage or explicit shared filesystem readiness override
- agent `PANDAR_HUB_API_URL`
- backup/restore split for SQLite/filesystem versus PostgreSQL/object storage
- cleanup CLI storage behavior

- [ ] **Step 2: Update compose**

Remove the implication that PostgreSQL + NATS only needs a shared `/spool`. Add commented or env-driven object storage settings for PostgreSQL deployments. Keep SQLite compose simple with filesystem spool.

- [ ] **Step 3: Verify docs references**

Run:

```bash
rg -n "artifact_base64|artifact_invalid_base64|PANDAR_SPOOL_DIR|spool|artifact_storage|PANDAR_ARTIFACT_STORAGE|PANDAR_HUB_API_URL" docs docker-compose*.yml frontend crates
```

Expected: `artifact_base64` and `artifact_invalid_base64` appear only in historical `docs/superpowers/specs` or `docs/superpowers/plans` records, not in active docs, frontend, plugin, or hub code. `PANDAR_SPOOL_DIR` and spool references describe only the filesystem backend, not universal artifact storage.

## Task 11: Roadmap And Final Verification

**Files:**

- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update roadmap**

Record Phase 25 completed work and explicitly note any untested live service evidence, such as live AWS/MinIO bucket tests or live multi-node NATS cluster tests if only fake-backed smoke ran.

- [ ] **Step 2: Run formatting**

Run:

```bash
cargo fmt
cargo fmt --check
```

Expected: both exit 0.

- [ ] **Step 3: Run Rust verification**

Run:

```bash
cargo clippy --workspace
cargo test -p pandar-network-plugin
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: all commands exit 0. If optional PostgreSQL is configured, also run:

```bash
cargo test -p pandar-hub repositories::tests::postgres -- --nocapture
```

- [ ] **Step 4: Run frontend verification**

Run:

```bash
npm --prefix frontend run build
```

Expected: exits 0.

- [ ] **Step 5: Run diff hygiene**

Run:

```bash
git diff --check
git status --short
```

Expected: no whitespace errors; status contains only Phase 25 intentional changes.

## Implementation Review Gates

After implementation and before docs/final commit:

- Run SDD spec-compliance review with an independent Codex reviewer. Required final line: `VERDICT: APPROVE`.
- Run SDD code-quality review with an independent Codex reviewer. Required final line: `VERDICT: APPROVE`.
- Run opencode final implementation review. Required final line: `VERDICT: APPROVE`.
- Fix and re-review until every required reviewer approves.

## Commit And Push

Only after all implementation reviews and fresh verification pass:

```bash
git add \
  Cargo.toml Cargo.lock \
  crates/pandar-hub crates/pandar-agent crates/pandar-app crates/pandar-network-plugin \
  frontend \
  proto \
  tools/scaled-artifact-smoke \
  docs docker-compose.sqlite.yml docker-compose.postgres.yml
git commit -m "$(cat <<'MSG'
Remove shared spool as the scaled artifact bottleneck

Constraint: PostgreSQL + NATS Hub replicas need Hub-mediated artifact storage and retrieval without exposing object credentials to agents or browsers.
Rejected: Keeping base64 JSON upload fallback | It preserves the proxy/body-limit failure mode Phase 25 removes.
Confidence: high
Scope-risk: broad
Directive: Keep artifact bytes behind ArtifactStorage; do not let browser, plugin, or agent callers supply storage keys.
Tested: cargo fmt --check; cargo clippy --workspace; cargo test -p pandar-network-plugin; cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run; cargo nextest run --manifest-path Cargo.toml --workspace; npm --prefix frontend run build; git diff --check
Not-tested: Live third-party S3 bucket and live multi-node NATS cluster unless explicitly recorded in docs/roadmap.md.
MSG
)"
git push origin main
```
