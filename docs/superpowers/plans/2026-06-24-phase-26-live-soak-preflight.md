# Phase 26 Live Soak Preflight Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a deterministic Phase 26 live-soak preflight command without claiming or running live soak evidence.

**Architecture:** Keep `main.rs` as the CLI entrypoint and add preflight validation in a focused `live.rs` module. The command validates required environment variables and a simple production-name guard, while dry-run orchestration remains owned by the existing harness/scenario modules.

**Tech Stack:** Rust 2024, `anyhow`, existing `tools/scaled-artifact-smoke`, markdown docs.

---

## File Structure

- Modify: `tools/scaled-artifact-smoke/src/main.rs`
  - Add `mod live;`.
  - Route `--live-preflight` to `live::run_preflight`.
  - Keep `--dry-run` parsing unchanged.
- Create: `tools/scaled-artifact-smoke/src/live.rs`
  - Own required environment variable names, validation result, production database URL guard, output formatting, and unit tests.
- Modify: `docs/development.md`
  - Add the preflight command and clarify it is not live soak evidence.
- Modify: `docs/release-installation.md`
  - Add the preflight command to Phase 26 operational checks.
- Modify: `docs/compatibility/phase-26-soak-evidence.md`
  - Record current preflight result separately from live soak rows.
- Modify: `docs/roadmap.md`
  - Replace the stale "tool has no live mode" blocker with the narrower "preflight exists, disposable dependencies are missing" blocker.

## Task 1: Add Preflight Validation Tests And Implementation

**Files:**
- Modify: `tools/scaled-artifact-smoke/src/main.rs`
- Create: `tools/scaled-artifact-smoke/src/live.rs`

- [ ] **Step 1: Write failing unit tests**

Create `tools/scaled-artifact-smoke/src/live.rs` with tests first:

```rust
use std::{collections::BTreeMap, env};

pub const REQUIRED_ENV: &[&str] = &[
    "PANDAR_SOAK_DATABASE_URL",
    "PANDAR_SOAK_NATS_URL",
    "PANDAR_SOAK_ARTIFACT_S3_BUCKET",
    "PANDAR_SOAK_ARTIFACT_S3_REGION",
    "PANDAR_SOAK_ARTIFACT_S3_ENDPOINT",
    "PANDAR_SOAK_ARTIFACT_S3_ACCESS_KEY_ID",
    "PANDAR_SOAK_ARTIFACT_S3_SECRET_ACCESS_KEY",
];

pub fn validate(values: &BTreeMap<String, String>) -> Result<(), PreflightError> {
    let _ = values;
    todo!("implemented in Step 3")
}

#[derive(Debug, PartialEq, Eq)]
pub enum PreflightError {
    Missing(Vec<&'static str>),
    UnsafeDatabaseUrl,
}

pub fn run_preflight() -> anyhow::Result<()> {
    let values = REQUIRED_ENV
        .iter()
        .filter_map(|name| env::var(name).ok().map(|value| ((*name).to_owned(), value)))
        .collect::<BTreeMap<_, _>>();
    validate(&values).map_err(|error| anyhow::anyhow!("{error:?}"))?;
    println!("PASS live soak preflight");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_values() -> BTreeMap<String, String> {
        REQUIRED_ENV
            .iter()
            .map(|name| ((*name).to_owned(), format!("value-for-{name}")))
            .collect()
    }

    #[test]
    fn validate_reports_all_missing_variables() {
        assert_eq!(
            validate(&BTreeMap::new()),
            Err(PreflightError::Missing(REQUIRED_ENV.to_vec()))
        );
    }

    #[test]
    fn validate_accepts_complete_disposable_inputs() {
        let mut values = complete_values();
        values.insert(
            "PANDAR_SOAK_DATABASE_URL".to_owned(),
            "postgres://pandar_soak@localhost/pandar_soak".to_owned(),
        );

        assert_eq!(validate(&values), Ok(()));
    }

    #[test]
    fn validate_rejects_production_database_url() {
        let mut values = complete_values();
        values.insert(
            "PANDAR_SOAK_DATABASE_URL".to_owned(),
            "postgres://pandar@db.example.com/pandar-PRODuction".to_owned(),
        );

        assert_eq!(validate(&values), Err(PreflightError::UnsafeDatabaseUrl));
    }
}
```

Also add `mod live;` near the other module declarations in `tools/scaled-artifact-smoke/src/main.rs` so the tests compile into the smoke binary during the RED check. Do not add CLI routing yet.

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
cargo test --manifest-path tools/scaled-artifact-smoke/Cargo.toml live::tests
```

Expected: tests compile but fail because `validate` still calls `todo!`.

- [ ] **Step 3: Implement minimal validation**

Replace `live.rs` with:

```rust
use std::{collections::BTreeMap, env, fmt};

pub const REQUIRED_ENV: &[&str] = &[
    "PANDAR_SOAK_DATABASE_URL",
    "PANDAR_SOAK_NATS_URL",
    "PANDAR_SOAK_ARTIFACT_S3_BUCKET",
    "PANDAR_SOAK_ARTIFACT_S3_REGION",
    "PANDAR_SOAK_ARTIFACT_S3_ENDPOINT",
    "PANDAR_SOAK_ARTIFACT_S3_ACCESS_KEY_ID",
    "PANDAR_SOAK_ARTIFACT_S3_SECRET_ACCESS_KEY",
];

#[derive(Debug, PartialEq, Eq)]
pub enum PreflightError {
    Missing(Vec<&'static str>),
    UnsafeDatabaseUrl,
}

impl fmt::Display for PreflightError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(names) => write!(
                formatter,
                "missing live soak environment variables: {}",
                names.join(", ")
            ),
            Self::UnsafeDatabaseUrl => write!(
                formatter,
                "PANDAR_SOAK_DATABASE_URL must point to disposable soak data, not production"
            ),
        }
    }
}

pub fn run_preflight() -> anyhow::Result<()> {
    let values = REQUIRED_ENV
        .iter()
        .filter_map(|name| env::var(name).ok().map(|value| ((*name).to_owned(), value)))
        .collect::<BTreeMap<_, _>>();
    validate(&values).map_err(|error| anyhow::anyhow!("{error}"))?;
    println!("PASS live soak preflight");
    Ok(())
}

pub fn validate(values: &BTreeMap<String, String>) -> Result<(), PreflightError> {
    let missing = REQUIRED_ENV
        .iter()
        .copied()
        .filter(|name| values.get(*name).is_none_or(|value| value.trim().is_empty()))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(PreflightError::Missing(missing));
    }

    let database_url = values
        .get("PANDAR_SOAK_DATABASE_URL")
        .expect("missing database URL was checked above")
        .to_ascii_lowercase();
    if database_url.contains("production") || database_url.contains("prod") {
        return Err(PreflightError::UnsafeDatabaseUrl);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_values() -> BTreeMap<String, String> {
        REQUIRED_ENV
            .iter()
            .map(|name| ((*name).to_owned(), format!("value-for-{name}")))
            .collect()
    }

    #[test]
    fn validate_reports_all_missing_variables() {
        assert_eq!(
            validate(&BTreeMap::new()),
            Err(PreflightError::Missing(REQUIRED_ENV.to_vec()))
        );
    }

    #[test]
    fn validate_accepts_complete_disposable_inputs() {
        let mut values = complete_values();
        values.insert(
            "PANDAR_SOAK_DATABASE_URL".to_owned(),
            "postgres://pandar_soak@localhost/pandar_soak".to_owned(),
        );

        assert_eq!(validate(&values), Ok(()));
    }

    #[test]
    fn validate_rejects_production_database_url() {
        let mut values = complete_values();
        values.insert(
            "PANDAR_SOAK_DATABASE_URL".to_owned(),
            "postgres://pandar@db.example.com/pandar-PRODuction".to_owned(),
        );

        assert_eq!(validate(&values), Err(PreflightError::UnsafeDatabaseUrl));
    }
}
```

- [ ] **Step 4: Route the CLI mode**

Change `run()` so it handles `--live-preflight` before dry-run parsing:

```rust
async fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|mode| mode == "--live-preflight") {
        if args.len() != 1 {
            usage()?;
        }
        return live::run_preflight();
    }
    let config = parse_args(args)?;
    harness::run(config).await
}
```

Update `usage()` to include `--live-preflight`:

```rust
"usage: pandar-scaled-artifact-smoke --dry-run [--iterations N] [--concurrency N] [--scenario all|artifact|fanout|restart|storage|terminal] | --live-preflight"
```

- [ ] **Step 5: Run focused validation**

Run:

```bash
cargo test --manifest-path tools/scaled-artifact-smoke/Cargo.toml live::tests
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --live-preflight
```

Expected: tests pass. The preflight command fails in this workspace with missing `PANDAR_SOAK_*` variables.

## Task 2: Update Phase 26 Documentation And Evidence

**Files:**
- Modify: `docs/development.md`
- Modify: `docs/release-installation.md`
- Modify: `docs/compatibility/phase-26-soak-evidence.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Document the preflight command in development docs**

In `docs/development.md`, add the preflight command immediately after the existing three dry-run command examples and before the "Optional live soak evidence variables" paragraph:

```markdown
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --live-preflight
```

Then add one sentence:

```markdown
`--live-preflight` checks only the disposable live soak environment contract; it does not connect to PostgreSQL, NATS, or object storage and is not live soak evidence.
```

- [ ] **Step 2: Document the preflight in release operations**

In `docs/release-installation.md`, replace the Phase 26 release-validation bullet:

```markdown
- Run the local Phase 26 dry-run harness during release validation, then record any disposable live PostgreSQL/NATS/object-storage soak in `docs/compatibility/phase-26-soak-evidence.md`.
```

with:

```markdown
- Run the local Phase 26 dry-run harness and `--live-preflight` during release validation, then record any disposable live PostgreSQL/NATS/object-storage soak in `docs/compatibility/phase-26-soak-evidence.md`.
```

- [ ] **Step 3: Record preflight evidence**

In `docs/compatibility/phase-26-soak-evidence.md`, add a section between local dry-run and live evidence:

```markdown
## Live Soak Preflight Evidence

| Date | Commit | Command | Result | Notes |
| --- | --- | --- | --- | --- |
| 2026-06-24 | working tree before commit | `cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --live-preflight` | blocked | Preflight command exists and failed locally because disposable `PANDAR_SOAK_*` PostgreSQL, NATS, and object-storage variables are not configured. No live soak was attempted. |
```

Keep the existing live PostgreSQL/NATS/object-storage row as `blocked`.

- [ ] **Step 4: Update roadmap wording**

In `docs/roadmap.md`, update the Phase 26 blocker bullet so it no longer says the smoke tool has no live mode. Use:

```markdown
- Checked live soak prerequisites on 2026-06-24: local PostgreSQL binaries are available and `tools/scaled-artifact-smoke --live-preflight` now verifies the disposable live-soak environment contract, but no disposable NATS/object-storage endpoint or credentials are configured, so live PostgreSQL+NATS+object-storage soak remains blocked.
```

Update `Immediate Next` only if needed to keep wording consistent; do not remove the live evidence requirement.

## Task 3: Final Verification And Review

**Files:**
- All files changed in Tasks 1 and 2.

- [ ] **Step 1: Run targeted checks**

Run:

```bash
cargo fmt --check
cargo clippy --manifest-path tools/scaled-artifact-smoke/Cargo.toml
cargo test --manifest-path tools/scaled-artifact-smoke/Cargo.toml live::tests
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --scenario storage
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --live-preflight
cargo nextest run --manifest-path Cargo.toml --workspace
git diff --check
```

Expected:

- formatting passes;
- smoke-tool clippy passes;
- live unit tests pass;
- dry-run storage scenario passes;
- live preflight exits non-zero with a missing-variable message in this workspace;
- workspace nextest passes;
- diff check passes.

- [ ] **Step 1a: Rollback note**

If this milestone has to be reverted, remove `tools/scaled-artifact-smoke/src/live.rs`, remove `mod live;` and the `--live-preflight` route/usage branch from `tools/scaled-artifact-smoke/src/main.rs`, and revert the Phase 26 docs/evidence wording. After rollback, run:

```bash
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --scenario storage
```

Expected: the existing dry-run storage scenario still passes.

- [ ] **Step 2: SDD final implementation review**

Dispatch the required independent Codex reviewer and opencode review with the spec, plan, diff, and verification output. Both must return exact `VERDICT: APPROVE`.

- [ ] **Step 3: Commit and push**

Commit only after both reviewers approve and fresh verification has run. Use a Lore-format commit message.
