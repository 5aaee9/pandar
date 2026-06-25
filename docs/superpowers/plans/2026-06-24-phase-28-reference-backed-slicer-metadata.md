# Phase 28 Reference-Backed Slicer Metadata Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Parse safe advisory Bambu 3MF metadata, let operators preview it before dispatch, persist it with job artifacts, and show it in Hub/plugin/frontend responses without changing authoritative print settings.

**Architecture:** Keep metadata parsing inside Hub at the upload boundary. Store optional parser-generated JSON on `job_artifacts`. Treat metadata as an artifact property reused by retry/reprint/duplicate flows, not as command or printer state. Preview shares only file staging and validation with job creation.

**Tech Stack:** Rust workspace with axum multipart, SeaORM/sqlx migrations for SQLite and PostgreSQL, bounded `zip` + `quick-xml` parsing in `pandar-hub`, Next.js proxy route and React dispatch form.

---

## File Map

- Modify `Cargo.toml`, `crates/pandar-hub/Cargo.toml`: add `zip` and `quick-xml` for Hub-only parsing.
- Create `crates/pandar-hub/src/artifacts/metadata.rs`: typed metadata model, ZIP/XML/JSON parser, filename display-stem helper, parser tests and fixture builders.
- Modify `crates/pandar-hub/src/artifacts.rs` or module tree: export metadata parser inside Hub.
- Modify `crates/pandar-hub/src/routes/jobs/multipart.rs`: refactor shared file staging, add metadata parsing on job create, expose file-only preview helper.
- Modify `crates/pandar-hub/src/routes/jobs.rs`, `crates/pandar-hub/src/routes.rs`: add metadata preview endpoint and include artifact metadata in job responses.
- Modify `crates/pandar-hub/src/routes/plugin.rs`: include optional artifact metadata in plugin print/list responses.
- Add migrations:
  - `crates/pandar-hub/migrations/sqlite/20260624010000_phase_28_slicer_metadata.sql`
  - `crates/pandar-hub/migrations/postgres/20260624010000_phase_28_slicer_metadata.sql`
- Modify `crates/pandar-hub/src/entities/job_artifacts.rs`: add nullable `metadata_json`.
- Modify `crates/pandar-core/src/job.rs`: add `metadata_json` to `JobArtifact` and `JobArtifactParts`.
- Modify `crates/pandar-hub/src/repositories/jobs.rs`, `jobs/create.rs`, `jobs/rows.rs`, `jobs/hydration.rs`, `jobs/artifacts.rs`, `jobs/recovery.rs`: carry metadata through artifact insert/build/hydration/access and keep recovery reuse semantics.
- Modify repository and route tests under `crates/pandar-hub/src/repositories/tests/jobs/`, `routes/tests/jobs/`, `routes/tests/plugin.rs`, and `routes/tests/plugin_multipart.rs`.
- Add frontend proxy route `frontend/app/api/tenants/[tenantId]/artifact-metadata-preview/route.ts`.
- Modify `frontend/app/dispatch-form.tsx`, `frontend/app/dashboard-types.ts`, `frontend/app/dashboard-runtime-sections.tsx`, and `frontend/app/recovery-actions.tsx` for preview and persisted summaries.
- Update docs: `docs/compatibility/phase-28-slicer-metadata.md`, `docs/roadmap.md`, and `docs/development.md` or `docs/architecture.md`.

---

### Task 1: Parser And Fixtures

**Files:**

- Modify `Cargo.toml`
- Modify `crates/pandar-hub/Cargo.toml`
- Create `crates/pandar-hub/src/artifacts/metadata.rs`
- Modify module export for `crates/pandar-hub/src/artifacts`

- [ ] **Step 1: Add focused failing parser tests**

In the new parser module, write tests first for:

- `.gcode` returns `None`.
- non-ZIP `.3mf` returns `None`.
- fixture 3MF with `Metadata/slice_info.config` extracts plate IDs, plate count, estimated seconds, weight grams, object names, and filament hints.
- fixture 3MF with `Metadata/model_settings.config` maps `plater_id` to plate name.
- fixture 3MF with `Metadata/plate_N.json` extracts fallback object names.
- `default_plate_id` follows precedence: `plate_*.gcode`, then `slice_info.config`, then `plate_*.json`, then `plate_*.png`.
- conflicting lower-precedence plate data does not overwrite higher-precedence fields for the same plate.
- oversized metadata members produce partial metadata plus a stable warning.
- unknown and path-traversal-like ZIP entries are ignored.
- display name is derived from filename suffix stripping only.

- [ ] **Step 2: Run focused failing parser tests**

Run:

```bash
cargo test -p pandar-hub artifacts::metadata
```

Expected: FAIL because parser module/dependencies are not implemented yet.

- [ ] **Step 3: Implement parser minimally**

Add workspace dependencies:

- `quick-xml`: use the current stable crates.io release compatible with the workspace Rust toolchain; `cargo info quick-xml` showed 0.40.1 on 2026-06-24.
- `zip`: choose the latest stable release compatible with the workspace Rust toolchain and avoid pre-release versions unless no stable compatible release exists. Enable only the decompression features needed for normal 3MF ZIP archives, such as deflate.

Record the chosen versions in the implementation notes or final summary.

Implementation rules:

- Candidate 3MF if filename ends with `.3mf` / `.gcode.3mf` or content type is `model/3mf`.
- `parse_artifact_metadata(filename, content_type, path) -> anyhow::Result<Option<ArtifactMetadata>>`.
- `Ok(None)` for unsupported files, malformed ZIP, missing known metadata, or malformed allowed metadata.
- Inspect only allowlisted `Metadata/` members from the spec.
- Enforce entry/member/total/plate/object/filament caps.
- Use `quick-xml` event parsing; use `serde_json` only for plate JSON object-name fallback.
- Do not expose raw parser errors in returned metadata warnings.

- [ ] **Step 4: Re-run parser tests**

Run:

```bash
cargo test -p pandar-hub artifacts::metadata
```

Expected: PASS.

### Task 2: Persistence And Repository Hydration

**Files:**

- Add SQLite/PostgreSQL migrations
- Modify `crates/pandar-hub/src/entities/job_artifacts.rs`
- Modify `crates/pandar-core/src/job.rs`
- Modify `crates/pandar-hub/src/repositories/jobs.rs`
- Modify `crates/pandar-hub/src/repositories/jobs/create.rs`
- Modify `crates/pandar-hub/src/repositories/jobs/rows.rs`
- Modify repository tests

- [ ] **Step 1: Add failing repository tests**

Add tests proving:

- `create_print_job` stores metadata JSON and `list_for_tenant` / `get_for_tenant` return it.
- missing metadata remains `None`.
- reprint and duplicate return the same artifact metadata as the source artifact.
- invalid persisted `metadata_json` causes a repository/data error with context when hydrated.

Update PostgreSQL repository tests so the new column is covered in the backend-neutral path.
These tests must include a dedicated assertion that PostgreSQL migrations expose `job_artifacts.metadata_json` and that a created job round-trips metadata through PostgreSQL hydration.

- [ ] **Step 2: Run focused failing repository tests**

Run:

```bash
cargo test -p pandar-hub job_repository_metadata
```

Expected: FAIL until migrations/entity/core/repository fields are implemented.

- [ ] **Step 3: Implement DB and repository fields**

Add both migrations:

```sql
ALTER TABLE job_artifacts ADD COLUMN metadata_json TEXT;
```

Then:

- Add `metadata_json: Option<String>` to SeaORM entity.
- Add `metadata_json: Option<String>` to `JobArtifact` and `JobArtifactParts`.
- Add `artifact_metadata_json: Option<String>` to `CreatePrintJob`.
- Insert metadata on new artifact creation.
- Build and hydrate `JobArtifact` with metadata everywhere through `rows::artifact_from_model`.
- Keep retry/reprint/duplicate semantics artifact-based: do not reparse and do not create a new artifact metadata value for existing artifacts.
- Validate persisted metadata in `artifact_from_model` by parsing it as `serde_json::Value` when `Some`; return `RepositoryError::Database` with full context on invalid JSON.

- [ ] **Step 4: Re-run repository tests**

Run:

```bash
cargo test -p pandar-hub job_repository_metadata
PANDAR_TEST_POSTGRES_URL=<postgres-url> cargo test -p pandar-hub repositories::tests::postgres
```

Expected: PASS.
If `PANDAR_TEST_POSTGRES_URL` is unavailable, Phase 28 is not complete. Record the missing PostgreSQL verification as a blocker instead of treating skipped tests as a pass.

2026-06-24 update: this PostgreSQL verification was later run against a disposable local PostgreSQL 17.10 instance with `postgres_job_metadata_round_trips_and_reuses_artifact_when_configured` and `cargo test -p pandar-hub metadata`; both passed. See `docs/compatibility/phase-28-slicer-metadata.md`.

### Task 3: Hub Routes, Multipart Preview, And Plugin Responses

**Files:**

- Modify `crates/pandar-hub/src/routes.rs`
- Modify `crates/pandar-hub/src/routes/jobs.rs`
- Modify `crates/pandar-hub/src/routes/jobs/multipart.rs`
- Modify `crates/pandar-hub/src/routes/plugin.rs`
- Modify route/plugin tests

- [ ] **Step 1: Add failing route tests**

Add tests proving:

- preview endpoint authorizes operators and rejects viewers.
- preview endpoint succeeds with only `file` plus optional `filename` and `content_type`.
- preview endpoint does not require printer, plate, AMS, calibration, timelapse, or mapping fields.
- preview endpoint returns `metadata: null` for unsupported artifacts.
- preview endpoint returns parsed metadata for fixture 3MF.
- preview endpoint creates no `job_artifacts`, `jobs`, `commands`, or audit rows.
- preview cleanup removes staged files on success and validation failure.
- job create persists parsed metadata and still succeeds when parsing returns `None`.
- plugin print response includes `artifact_metadata`.
- plugin job list response includes `artifact_metadata`.

- [ ] **Step 2: Run focused failing route tests**

Run:

```bash
cargo test -p pandar-hub metadata_preview
cargo test -p pandar-hub plugin_artifact_metadata
```

Expected: FAIL until routes are implemented.

- [ ] **Step 3: Refactor multipart without changing existing errors**

In `routes/jobs/multipart.rs`:

- Keep current job-create behavior and stable error codes.
- Extract shared file staging/parsing that records `file`, optional `filename`, and optional `content_type`.
- Add a preview path that uses only shared file staging and does not call `prepare_print_job`.
- Parse metadata after the file is staged and before cleanup.
- For job creation, pass `artifact_metadata_json` into `CreatePrintJob`; parser failure or `None` must not block creation.
- Keep staged file cleanup on every success and error path.

- [ ] **Step 4: Implement response surfaces**

In Hub job responses:

- Add `metadata: Option<Value>` to `JobArtifactResponse`.
- Parse persisted metadata with full-context failure on invalid JSON.

In plugin responses:

- Add `artifact_metadata: Option<Value>` to `PluginPrintResponse` and `PluginJobResponse`.
- Do not expose artifact IDs or storage paths in plugin responses.

Add router entry:

```rust
.route(
    "/api/v1/tenants/{tenant_id}/artifact-metadata-preview",
    post(jobs::preview_artifact_metadata).layer(DefaultBodyLimit::disable()),
)
```

- [ ] **Step 5: Re-run route tests**

Run:

```bash
cargo test -p pandar-hub metadata_preview
cargo test -p pandar-hub plugin_artifact_metadata
cargo test -p pandar-hub job_create_
```

Expected: PASS.

### Task 4: Frontend Preview And Display

**Files:**

- Create `frontend/app/api/tenants/[tenantId]/artifact-metadata-preview/route.ts`
- Modify `frontend/app/dispatch-form.tsx`
- Modify `frontend/app/dashboard-types.ts`
- Modify `frontend/app/dashboard-runtime-sections.tsx`
- Modify `frontend/app/recovery-actions.tsx`

- [ ] **Step 1: Add frontend types and proxy route**

Mirror the existing job-upload proxy:

- Forward multipart body to Hub preview endpoint.
- Preserve content type.
- Use `apiHeaders()`.
- Return content-type response header only.

Add `ArtifactMetadata` / nested plate and filament types in `dashboard-types.ts`.

- [ ] **Step 2: Implement dispatch preview**

In `DispatchForm`:

- On file selection, POST a FormData with `file`, `filename`, and `content_type` to the preview proxy.
- Track preview state: idle/loading/ready/unavailable/error.
- Render compact advisory metadata summary.
- Keep dispatch enabled when preview is null or preview fails.
- Keep submitted `plate_id` explicit; do not auto-overwrite it.
- If adding a detected-plate helper, it must require an explicit user click.

- [ ] **Step 3: Display persisted metadata**

In job history and recovery cards:

- Show display name, plate count, selected/default plate estimate, and filament summary when present.
- Keep layout dense and operational.
- Do not add explanatory feature text or large marketing panels.

- [ ] **Step 4: Run frontend build**

Run:

```bash
npm --prefix frontend run build
```

Expected: PASS.

### Task 5: Documentation And Final Verification

**Files:**

- Create `docs/compatibility/phase-28-slicer-metadata.md`
- Modify `docs/roadmap.md`
- Modify `docs/development.md` or `docs/architecture.md`

- [ ] **Step 1: Update docs**

Document:

- reference files and extraction rules,
- parser limits,
- preview endpoint and response shape,
- non-blocking metadata failures,
- authoritative explicit dispatch settings,
- no-network verification evidence,
- real slicer/project fixture status.

Update roadmap to mark Phase 28 complete and define the next work item.

- [ ] **Step 2: Run full verification**

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path Cargo.toml --workspace
PANDAR_TEST_POSTGRES_URL=<postgres-url> cargo test -p pandar-hub repositories::tests::postgres
npm --prefix frontend run build
git diff --check
```

Expected: PASS.
PostgreSQL migration/repository verification is required for completion. If no PostgreSQL URL is available, stop and report Phase 28 as blocked rather than marking it complete.

2026-06-24 update: disposable PostgreSQL metadata verification has been recorded in `docs/compatibility/phase-28-slicer-metadata.md`.

- [ ] **Step 3: Final independent review**

Request final Codex review and opencode review. Use a neutral review contract that allows either verdict:

```text
VERDICT: APPROVE
BLOCKERS:
- None
REQUIRED_CHANGES:
- None
```

or:

```text
VERDICT: REQUEST_CHANGES
BLOCKERS:
- ...
REQUIRED_CHANGES:
- ...
```

Fix any `REQUEST_CHANGES` before committing.

- [ ] **Step 4: Commit and push**

Commit with Lore protocol and push `main`.
