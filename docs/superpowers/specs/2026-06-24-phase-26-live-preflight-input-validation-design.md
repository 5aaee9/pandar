# Phase 26 Live Preflight Input Validation Design

## Goal

Strengthen `tools/scaled-artifact-smoke --live-preflight` so Phase 26 operators get actionable, local-only validation of disposable live-soak inputs before attempting PostgreSQL + NATS + object-storage soak evidence.

## Scope

This milestone only validates environment variable presence, value shape, and obvious production-target risk. It does not start PostgreSQL, NATS, MinIO, Docker, or podman; it does not connect to any network service; and it does not claim live soak evidence.

Files in scope:

- `tools/scaled-artifact-smoke/src/live.rs`
- `tools/scaled-artifact-smoke/src/main.rs` only if usage text needs a wording update
- `docs/development.md`
- `docs/compatibility/phase-26-soak-evidence.md`
- `docs/roadmap.md`

## Current State

`--live-preflight` currently checks that these variables exist and that `PANDAR_SOAK_DATABASE_URL` does not contain `prod` or `production`:

- `PANDAR_SOAK_DATABASE_URL`
- `PANDAR_SOAK_NATS_URL`
- `PANDAR_SOAK_ARTIFACT_S3_BUCKET`
- `PANDAR_SOAK_ARTIFACT_S3_REGION`
- `PANDAR_SOAK_ARTIFACT_S3_ENDPOINT`
- `PANDAR_SOAK_ARTIFACT_S3_ACCESS_KEY_ID`
- `PANDAR_SOAK_ARTIFACT_S3_SECRET_ACCESS_KEY`

That catches missing inputs, but it does not catch common wrong-shape values such as an HTTP NATS URL, a PostgreSQL URL without a soak/disposable marker, an empty bucket-like placeholder, or a non-HTTP S3 endpoint.

## Required Behavior

- Keep one command: `cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --live-preflight`.
- Keep the command local-only and connection-free.
- Preserve reporting of all missing required variables in one error.
- Reject `PANDAR_SOAK_DATABASE_URL` unless:
  - it starts with `postgres://` or `postgresql://`;
  - its lowercased URL contains at least one disposable marker from the exact list `soak`, `disposable`, `ephemeral`, `test`;
  - its lowercased URL does not contain either production marker from the exact list `prod`, `production`.
- Reject `PANDAR_SOAK_NATS_URL` unless it starts with `nats://`.
- Reject `PANDAR_SOAK_ARTIFACT_S3_ENDPOINT` unless it starts with `http://` or `https://`.
- Reject blank or placeholder-looking bucket, region, access key, and secret values. A value is placeholder-looking when its trimmed lowercase form:
  - starts with `<` and ends with `>`;
  - starts with `value-for-`;
  - exactly equals one of `bucket`, `region`, `access-key`, `secret`, `changeme`.
- When inputs fail shape/safety validation, report all invalid variables in one error message instead of stopping at the first invalid variable.
- Missing-variable validation runs first. If any required variable is missing or blank, return only the collected missing-variable error and do not run shape validation.
- On success, print `PASS live soak preflight`.

## Valid Disposable Input Example

These values must pass preflight because they are complete, disposable-looking, and local-only strings:

- `PANDAR_SOAK_DATABASE_URL=postgres://pandar_soak@localhost/pandar_soak`
- `PANDAR_SOAK_NATS_URL=nats://127.0.0.1:4222`
- `PANDAR_SOAK_ARTIFACT_S3_BUCKET=pandar-soak-artifacts`
- `PANDAR_SOAK_ARTIFACT_S3_REGION=us-east-1`
- `PANDAR_SOAK_ARTIFACT_S3_ENDPOINT=http://127.0.0.1:9000`
- `PANDAR_SOAK_ARTIFACT_S3_ACCESS_KEY_ID=pandar-soak-access`
- `PANDAR_SOAK_ARTIFACT_S3_SECRET_ACCESS_KEY=pandar-soak-secret`

`nats://` is the only accepted NATS URL scheme for this milestone. `tls://`, `ws://`, and HTTP(S) NATS endpoints are intentionally rejected because the current Hub control-plane configuration documents `PANDAR_NATS_URL` as a NATS server URL.

## Error Shape

Keep `PreflightError::Missing(Vec<&'static str>)` for missing-variable failures. Replace the current single unsafe database variant with an invalid-variable variant that carries every invalid variable and a short reason:

```rust
Invalid(Vec<InvalidVariable>)
```

`InvalidVariable` must expose:

- `name: &'static str`
- `reason: &'static str`

Unit tests should assert the exact invalid variable names. Reason strings only need to be stable enough to make the CLI output actionable.

## Documentation Requirements

- `docs/development.md` must state that `--live-preflight` now checks local input shape and safety markers, but still does not connect to live services.
- `docs/compatibility/phase-26-soak-evidence.md` must record the new preflight behavior as local preflight evidence only, not live soak evidence.
- `docs/roadmap.md` Phase 26 must mention that preflight validates disposable input shape and safety markers, while live PostgreSQL + NATS + object-storage soak remains blocked until endpoints and credentials are supplied.

## Acceptance Criteria

- Unit tests cover:
  - all missing variables are reported together;
  - a complete disposable input set passes;
  - production PostgreSQL URLs fail;
  - a PostgreSQL URL containing both a disposable marker and a production marker fails;
  - PostgreSQL URLs without a disposable marker fail;
  - non-PostgreSQL database URLs fail;
  - non-`nats://` NATS URLs fail;
  - non-HTTP(S) S3 endpoints fail;
  - placeholder bucket/region/access-key/secret values fail and are reported together.
- `cargo test --manifest-path tools/scaled-artifact-smoke/Cargo.toml live::tests` passes.
- `cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --live-preflight` fails in this workspace with a missing-variable message when no `PANDAR_SOAK_*` variables are configured.
- A shell invocation that supplies valid disposable values makes `--live-preflight` print `PASS live soak preflight`.
- Fresh verification includes:
  - `cargo fmt --check`
  - `cargo clippy --manifest-path tools/scaled-artifact-smoke/Cargo.toml`
  - `cargo test --manifest-path tools/scaled-artifact-smoke/Cargo.toml live::tests`
  - the failing missing-variable preflight command
  - the passing valid-value preflight command
  - `cargo nextest run --manifest-path "Cargo.toml" --workspace`
  - `git diff --check`
