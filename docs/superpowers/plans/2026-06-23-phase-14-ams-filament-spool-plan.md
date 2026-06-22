# Phase 14 AMS Filament Spool Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Promote AMS and external-spool state into tenant-scoped hub/frontend data while preserving Bambu print mapping semantics.

**Architecture:** Keep firmware parsing in `pandar-agent`, persistence/merge/usage derivation in `pandar-hub`, and shared response-safe domain shapes in `pandar-core` only where cross-crate use is needed. The hub stores normalized JSON text with backend-neutral SeaORM repositories and exposes parsed HTTP shapes; the agent sends material patches as JSON strings to preserve absent/null semantics.

**Tech Stack:** Rust, tokio, tonic/prost, axum, SeaORM 2, sqlx migrations, serde_json, Next.js/React/Tailwind.

---

## File Structure

- Modify `proto/pandar/agent/v1/agent.proto`: add material and mapping string fields only.
- Create `crates/pandar-agent/src/machine/materials.rs`: raw MQTT material patch normalizer plus tests.
- Modify `crates/pandar-agent/src/machine/mqtt.rs` and `crates/pandar-agent/src/machine/mod.rs`: attach material patches to reports and emit mapping keys in `project_file`.
- Modify `crates/pandar-agent/src/commands.rs`: carry mapping fields from proto into agent machine request.
- Modify `crates/pandar-hub/migrations/{sqlite,postgres}/20260623010000_phase_14_materials.sql`: jobs mapping columns, material snapshots, filament usage.
- Create `crates/pandar-hub/src/entities/printer_material_snapshots.rs` and `job_filament_usages.rs`; update `entities/mod.rs`.
- Create `crates/pandar-hub/src/repositories/materials.rs`; extend `repositories/jobs` for mapping persistence and terminal usage derivation.
- Create `crates/pandar-hub/src/repositories/tests/materials.rs`; update `repositories/tests/mod.rs`.
- Modify `crates/pandar-hub/src/grpc/print_reports.rs` and `repositories/jobs/print_reports.rs`: pass material patch JSON through print report reconciliation.
- Modify `crates/pandar-hub/src/grpc/commands.rs`, `repositories/commands.rs`, and job create payload code: dispatch mapping JSON to agents.
- Modify `crates/pandar-hub/src/routes/{printers,jobs}.rs`: expose `materials` and `material`.
- Modify `frontend/app/dashboard-types.ts` and `frontend/app/page.tsx`: render material summaries and job material rows.
- Update `docs/architecture.md` before final implementation review; update `docs/roadmap.md` after implementation review and full verification so Phase 14 completion is not marked early.

## Task 1: Protocol And Agent Material Normalization

Status: complete

**Files:**
- Modify: `proto/pandar/agent/v1/agent.proto`
- Create: `crates/pandar-agent/src/machine/materials.rs`
- Modify: `crates/pandar-agent/src/machine/mqtt.rs`
- Modify: `crates/pandar-agent/src/machine/mod.rs`
- Test: `crates/pandar-agent/src/machine/materials.rs`
- Test: `crates/pandar-agent/src/machine/mqtt/tests.rs`

- [ ] Add proto fields: `PrintJobReport.printer_materials_json = 18`, `PrintProjectFile.ams_mapping_json = 12`, and `PrintProjectFile.ams_mapping2_json = 13`.
- [ ] Write failing agent tests for full AMS snapshot, partial update, absent/null clears, `tray_exist_bits` integer/hex cleanup, `power_on_flag=false` zero and non-zero bitmasks, `replace_trays` / `replace_external_spools` emission rules, single `vt_tray` object and one-entry `vt_tray` array as merge patches without external replacement, multi-entry `vt_tray` array as external replacement, `vir_slot` precedence, single `vir_slot id=255 -> 254`, active tray `0..=15`/`128..=135`/`254`/`255`, filament id conversion, color normalization, credential-key filtering, no credential-shaped values in agent logs, and `PrintJobReport.printer_materials_json`.
- [ ] Implement `materials.rs` as a stateless normalizer returning `Option<serde_json::Value>`. Use `serde_json::Value` for patch construction so absent/null are explicit.
- [ ] Wire `print_report_from_report` / `print_job_report_event` to include serialized material patch JSON or an empty string.
- [ ] Write failing MQTT payload tests for no mapping, `ams_mapping` only, `ams_mapping2` only, both mappings, flat external `254`/`255 -> -1` rewrite, and unchanged `use_ams`.
- [ ] Extend `ProjectFileCommand` with optional mapping JSON strings and build Bambu payload keys `ams_mapping` and `ams_mapping_2` only when supplied.
- [ ] Run targeted verification:

```bash
cargo test -p pandar-agent machine::materials
cargo test -p pandar-agent machine::mqtt
```

## Task 2: Hub Schema, Entities, And Material Repository

Status: complete

**Files:**
- Create: `crates/pandar-hub/migrations/sqlite/20260623010000_phase_14_materials.sql`
- Create: `crates/pandar-hub/migrations/postgres/20260623010000_phase_14_materials.sql`
- Create: `crates/pandar-hub/src/entities/printer_material_snapshots.rs`
- Create: `crates/pandar-hub/src/entities/job_filament_usages.rs`
- Modify: `crates/pandar-hub/src/entities/jobs.rs`
- Modify: `crates/pandar-hub/src/entities/mod.rs`
- Create: `crates/pandar-hub/src/repositories/materials.rs`
- Modify: `crates/pandar-hub/src/repositories/mod.rs`
- Test: `crates/pandar-hub/src/repositories/tests/phase1.rs`
- Test: `crates/pandar-hub/src/repositories/tests/materials.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/mod.rs`

- [x] Add migrations with `TEXT` JSON columns in both backends, `jobs.ams_mapping_json`, `jobs.ams_mapping2_json`, `printer_material_snapshots`, and `job_filament_usages`. Include required nullability, FKs, unique constraints, indexes, and equivalent allowed-value checks or document repository validation in tests.
- [x] Add SeaORM entities and register them.
- [x] Implement a backend-neutral material repository with latest snapshot lookup, tenant list, and upsert-from-patch.
- [x] Implement merge helpers with `serde_json::Value`: absent preserves, null clears, concrete overwrites, collection merge by `unit_id`/`tray_id`/external `(254,tray_id)`, replace flags, older `observed_at` ignore, equal timestamp accept.
- [x] Add migration parity tests plus `repositories::tests::materials` SQLite tests for tenant scoping, invalid material JSON ignored for material state, partial replay, out-of-order replay, credential-key filtering, and no credential-shaped values persisted or logged when malformed material input is rejected.
- [x] Add optional PostgreSQL material snapshot/repository behavior coverage behind the existing `PANDAR_TEST_POSTGRES_URL` harness, while keeping static SQLite/PostgreSQL migration parity coverage when no PostgreSQL URL is configured.
- [x] Run targeted verification:

```bash
cargo test -p pandar-hub repositories::tests::phase1
cargo test -p pandar-hub repositories::tests::materials
```

## Task 3: Job Mapping Persistence And Usage Derivation

Status: complete

**Files:**
- Modify: `crates/pandar-core/src/job.rs`
- Modify: `crates/pandar-hub/src/repositories/jobs.rs`
- Modify: `crates/pandar-hub/src/repositories/jobs/create.rs`
- Modify: `crates/pandar-hub/src/repositories/jobs/rows.rs`
- Modify: `crates/pandar-hub/src/repositories/jobs/print_reports.rs`
- Modify: `crates/pandar-hub/src/repositories/commands.rs`
- Modify: `crates/pandar-hub/src/grpc/commands.rs`
- Test: `crates/pandar-hub/src/repositories/tests/jobs.rs`
- Test: `crates/pandar-hub/src/repositories/tests/materials.rs`
- Test: `crates/pandar-hub/src/grpc/tests/print_jobs.rs`

- [x] Extend core/repository job models with optional `ams_mapping_json`, `ams_mapping2_json`, and usage rows without changing dispatch status semantics.
- [x] Persist validated mapping JSON during job creation and include it in `PrintProjectFilePayload`.
- [x] Convert queued command payloads into proto mapping strings; empty means absent.
- [x] Treat corrupt persisted `ams_mapping_json` or `ams_mapping2_json` during queued command serialization as a command serialization/protocol error that preserves the lower parse context.
- [x] Add `printer_materials_json` to print report reconciliation input and call the material repository from `apply_print_report_tx` so progress reconciliation, material snapshot upsert, and terminal usage derivation run from the same observed report.
- [x] Treat empty or invalid `printer_materials_json` as ignored for material state while print progress still reconciles; log parse failures with the full cause chain and without raw credential-shaped payload values.
- [x] Implement usage derivation on terminal physical statuses only: mapping2 precedence, normal AMS, AMS-HT `128..=135`, external canonical `(254,tray_id)`, flat `255` unmapped, duplicates per slot, first terminal identity fixed, idempotent replay.
- [x] Add SQLite tests for mapping-only, mapping2-only, both, neither, null-vs-empty persistence, invalid persisted mapping command serialization error with context, external identity matching, duplicate slots, stale material after terminal derivation, terminal report material snapshot upsert, idempotent usage rows, and optional PostgreSQL behavior behind existing harness.
- [x] Run targeted verification:

```bash
cargo test -p pandar-hub repositories::tests::jobs
cargo test -p pandar-hub repositories::tests::materials
cargo test -p pandar-hub grpc::tests::print_jobs
```

## Task 4: gRPC Print Reports, HTTP Routes, And Frontend

Status: complete

**Files:**
- Modify: `crates/pandar-hub/src/grpc/print_reports.rs`
- Modify: `crates/pandar-hub/src/routes/printers.rs`
- Modify: `crates/pandar-hub/src/routes/jobs.rs`
- Modify: `crates/pandar-hub/src/routes/tests/printers.rs`
- Modify: `crates/pandar-hub/src/routes/tests/jobs.rs`
- Modify: `frontend/app/dashboard-types.ts`
- Modify: `frontend/app/page.tsx`

- [x] Extend gRPC print report parsing to pass `printer_materials_json` into repository reconciliation while keeping old agents compatible with empty strings.
- [x] Extend printer list/detail responses with `materials: null | { ams_units, external_spools, active_tray, observed_at }`.
- [x] Extend job create request validation for optional mapping arrays, including `400` on invalid shape, string values, and more than 32 entries.
- [x] Extend job list/detail responses with `material: { ams_mapping, ams_mapping2, filament_usage }`, rendering persisted `NULL` as JSON `null` and persisted `[]` as `[]`.
- [x] Treat corrupt persisted job mapping JSON during job response loading as a repository error that preserves the lower parse context instead of rendering partial material data.
- [x] Update frontend types and dashboard rendering for printer material summary and job material rows; keep dispatch form simple and mapping fields optional through API clients only.
- [x] Add route tests for material response shapes, mapping validation, corrupt persisted mapping JSON response errors with context, role scoping unchanged, no credential fields rendered, and no credential-shaped mapping values echoed in validation or log output.
- [x] Run targeted verification:

```bash
cargo test -p pandar-hub routes::tests::printers
cargo test -p pandar-hub routes::tests::jobs
cd frontend && npm run build
```

## Task 5: Final Verification And Docs

Status: in progress

**Files:**
- Modify before final implementation review: `docs/architecture.md`
- Modify after implementation review and full verification: `docs/roadmap.md`

- [ ] Run formatting and Rust checks:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

- [ ] Run frontend build:

```bash
cd frontend && npm run build
```

- [ ] Confirm generated protobuf output is not in git diff:

```bash
git status --short
git diff --name-only | rg '\\.(pb|tonic)\\.rs$' && exit 1 || true
```

- [ ] Before final implementation review, update `docs/architecture.md` with material-state boundary, mapping persistence, and Spoolman non-goal so the final review covers docs with code.
- [ ] After final implementation review approval and fresh full verification, update `docs/roadmap.md` to mark Phase 14 completed, inspect `git status`, commit with Lore protocol, and push `main`; if the push is rejected, credentials are missing, or remote policy blocks it, report the blocker with the local commit SHA and exact push error.

## Self-Review

- Spec coverage: protocol, agent normalization, hub merge/persistence, mapping usage, HTTP, frontend, tests, docs, and verification are covered.
- Placeholder scan: no unresolved placeholders are intentionally present.
- Type consistency: public names use `ams_mapping2`; Bambu MQTT uses `ams_mapping_2`; material patch merge preserves JSON absent/null semantics.
