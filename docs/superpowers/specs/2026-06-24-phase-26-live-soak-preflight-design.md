# Phase 26 Live Soak Preflight Design

## Objective

Add an explicit Phase 26 live-soak preflight command to `tools/scaled-artifact-smoke` so operators can verify whether the required disposable PostgreSQL, NATS, and S3-compatible object-storage inputs are present before attempting live soak evidence.

This milestone does not run a live soak, open external service connections, or mark live evidence as passed. It converts the current vague blocker, "the smoke tool has no live mode", into a deterministic preflight result with actionable missing-variable output.

## Scope

- Add a `--live-preflight` CLI mode to `tools/scaled-artifact-smoke`.
- Keep the existing `--dry-run` mode behavior unchanged.
- Validate only environment shape:
  - `PANDAR_SOAK_DATABASE_URL`
  - `PANDAR_SOAK_NATS_URL`
  - `PANDAR_SOAK_ARTIFACT_S3_BUCKET`
  - `PANDAR_SOAK_ARTIFACT_S3_REGION`
  - `PANDAR_SOAK_ARTIFACT_S3_ENDPOINT`
  - `PANDAR_SOAK_ARTIFACT_S3_ACCESS_KEY_ID`
  - `PANDAR_SOAK_ARTIFACT_S3_SECRET_ACCESS_KEY`
- Reject obviously unsafe database URLs that mention production by substring, case-insensitive: `prod`, `production`. This is intentionally conservative and may reject benign disposable names containing `prod`; operators should choose clearly disposable soak database names.
- Print a concise `PASS live soak preflight` line when all required variables are present and the database URL passes the safety check.
- Fail with a non-zero exit and list every missing required variable when inputs are incomplete.
- Fail with a non-zero exit when the database URL trips the production guard.
- Update Phase 26 docs and evidence to describe the preflight and keep live soak result `blocked` until real disposable dependencies are configured and a real soak command exists.

## Non-Goals

- No live PostgreSQL, NATS, or object-storage network calls.
- No Docker, podman, MinIO, NATS server, or cloud dependency startup.
- No new Hub runtime behavior.
- No release evidence changes.
- No claim that Phase 26 live soak has passed.

## Acceptance Criteria

- `cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --scenario storage` still passes.
- `cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --live-preflight` fails in the current workspace with a clear missing-variable message.
- Unit tests cover missing variables, all variables present, and case-insensitive production database URL rejection.
- Documentation names the preflight command and says it is not live soak evidence.
- `docs/compatibility/phase-26-soak-evidence.md` records the preflight check separately from live soak evidence.
