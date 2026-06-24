# Phase 26 Live Preflight Input Validation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `tools/scaled-artifact-smoke --live-preflight` reject wrong-shape or unsafe live-soak inputs without connecting to external services.

**Architecture:** Keep all validation inside `tools/scaled-artifact-smoke/src/live.rs`. The command continues to collect environment variables, validate them in-memory, and print a pass message only when the required disposable PostgreSQL, NATS, and S3-compatible storage inputs are present and safe-looking.

**Tech Stack:** Rust 2024, `anyhow`, the existing standalone `tools/scaled-artifact-smoke` crate, Markdown docs.

---

### Task 1: Add Multi-Error Preflight Validation

**Files:**

- Modify: `tools/scaled-artifact-smoke/src/live.rs`
- Modify: `docs/development.md`
- Modify: `docs/compatibility/phase-26-soak-evidence.md`
- Modify: `docs/roadmap.md`

- [x] **Step 1: Add failing validation tests**

  In `tools/scaled-artifact-smoke/src/live.rs`, extend the existing `live::tests` module with tests for:

  - all missing variables are reported together;
  - complete disposable values pass;
  - production database URLs fail;
  - database URLs without a disposable marker fail;
  - non-PostgreSQL database URLs fail;
  - non-`nats://` NATS URLs fail;
  - non-HTTP(S) S3 endpoints fail;
  - placeholder bucket, region, access key, and secret values are all reported together.
  - a database URL containing both a disposable marker and a production marker fails.

  First rewrite the existing `complete_values()` helper so every default value uses the spec's valid disposable example:

  - `PANDAR_SOAK_DATABASE_URL=postgres://pandar_soak@localhost/pandar_soak`
  - `PANDAR_SOAK_NATS_URL=nats://127.0.0.1:4222`
  - `PANDAR_SOAK_ARTIFACT_S3_BUCKET=pandar-soak-artifacts`
  - `PANDAR_SOAK_ARTIFACT_S3_REGION=us-east-1`
  - `PANDAR_SOAK_ARTIFACT_S3_ENDPOINT=http://127.0.0.1:9000`
  - `PANDAR_SOAK_ARTIFACT_S3_ACCESS_KEY_ID=pandar-soak-access`
  - `PANDAR_SOAK_ARTIFACT_S3_SECRET_ACCESS_KEY=pandar-soak-secret`

  Individual rejection tests should start from `complete_values()` and override only the field under test so the expected `InvalidVariable` list stays precise.

  Update the existing `validate_rejects_production_database_url` test to expect the new `PreflightError::Invalid(Vec<InvalidVariable>)` shape. Search for any remaining `UnsafeDatabaseUrl` references and remove them as part of the enum migration.

  Run:

  ```bash
  cargo test --manifest-path tools/scaled-artifact-smoke/Cargo.toml live::tests
  ```

  Expected before implementation: tests for the new invalid shapes fail.

- [x] **Step 2: Implement collected invalid-variable reporting**

  Update `PreflightError` in `tools/scaled-artifact-smoke/src/live.rs` so it can report:

  - missing variables, preserving current all-missing behavior;
  - invalid variables as a collection, with each entry naming the variable and the reason.

  Add:

  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct InvalidVariable {
      pub name: &'static str,
      pub reason: &'static str,
  }
  ```

  and use `PreflightError::Invalid(Vec<InvalidVariable>)`. Keep `PreflightError::Missing(Vec<&'static str>)` and preserve the current behavior where missing-variable validation runs first; if anything is missing or blank, return only the collected missing-variable error and skip shape validation.

  Keep the public behavior simple: `Display` should produce a single actionable error line.

- [x] **Step 3: Implement local-only shape and safety checks**

  In `validate`, after missing-variable checks pass, collect invalid variables:

  - `PANDAR_SOAK_DATABASE_URL`: starts with `postgres://` or `postgresql://`, lowercased URL contains one of `soak`, `disposable`, `ephemeral`, or `test`, and lowercased URL does not contain `prod` or `production`.
  - `PANDAR_SOAK_NATS_URL`: starts with `nats://`.
  - `PANDAR_SOAK_ARTIFACT_S3_ENDPOINT`: starts with `http://` or `https://`.
  - `PANDAR_SOAK_ARTIFACT_S3_BUCKET`, `PANDAR_SOAK_ARTIFACT_S3_REGION`, `PANDAR_SOAK_ARTIFACT_S3_ACCESS_KEY_ID`, and `PANDAR_SOAK_ARTIFACT_S3_SECRET_ACCESS_KEY`: not blank and not placeholder-looking. A placeholder-looking value is one whose trimmed lowercase value starts with `<` and ends with `>`, starts with `value-for-`, or exactly equals one of `bucket`, `region`, `access-key`, `secret`, `changeme`.

  Return all invalid-variable entries together.

- [x] **Step 4: Update Phase 26 docs**

  Update:

  - `docs/development.md`: explain that `--live-preflight` checks input shape and disposable markers only, without connecting.
  - `docs/compatibility/phase-26-soak-evidence.md`: add a row for the strengthened local preflight behavior and keep live soak as blocked.
  - `docs/roadmap.md`: mention stronger preflight checks while keeping live soak unverified.

- [x] **Step 5: Verify**

  Run:

  ```bash
  cargo fmt --check
  cargo clippy --manifest-path tools/scaled-artifact-smoke/Cargo.toml
  cargo test --manifest-path tools/scaled-artifact-smoke/Cargo.toml live::tests
  cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --live-preflight
  PANDAR_SOAK_DATABASE_URL=postgres://pandar_soak@localhost/pandar_soak \
    PANDAR_SOAK_NATS_URL=nats://127.0.0.1:4222 \
    PANDAR_SOAK_ARTIFACT_S3_BUCKET=pandar-soak-artifacts \
    PANDAR_SOAK_ARTIFACT_S3_REGION=us-east-1 \
    PANDAR_SOAK_ARTIFACT_S3_ENDPOINT=http://127.0.0.1:9000 \
    PANDAR_SOAK_ARTIFACT_S3_ACCESS_KEY_ID=pandar-soak-access \
    PANDAR_SOAK_ARTIFACT_S3_SECRET_ACCESS_KEY=pandar-soak-secret \
    cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --live-preflight
  cargo nextest run --manifest-path "Cargo.toml" --workspace
  git diff --check
  ```

  Expected results:

  - formatting, clippy, tests, valid-value preflight, workspace nextest, and diff check exit 0;
  - missing-variable preflight exits non-zero and lists the missing `PANDAR_SOAK_*` variables;
  - valid-value preflight prints `PASS live soak preflight`.
