# Phase 26 Live Soak Runner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `--live` runner to `tools/scaled-artifact-smoke` that executes live-capable Phase 26 scenarios against disposable PostgreSQL, NATS, and S3-compatible object storage without claiming live evidence unless the live command is actually run.

**Architecture:** Keep scenario logic shared by introducing an execution mode and world factory. Dry-run worlds keep temporary SQLite, in-process control plane, and local shared object storage; live worlds reuse the existing preflight validation, connect to PostgreSQL, build a NATS control plane, and build S3 storage from soak-prefixed values only.

**Tech Stack:** Rust 2024, tokio, anyhow, axum smoke helpers, `pandar-hub` public database/control-plane/artifact-storage APIs, SeaORM-backed repositories.

---

## Reviewed Spec

- `docs/superpowers/specs/2026-06-24-phase-26-live-soak-runner-design.md`

Spec review gate:

- Codex critic: `VERDICT: APPROVE`
- opencode: `VERDICT: APPROVE`

## Files

- Modify: `tools/scaled-artifact-smoke/src/main.rs`
  - Parse `--live`, keep no-arg usage failure, reject live storage scenario before external connections.
  - Add parser unit tests.
- Modify: `tools/scaled-artifact-smoke/src/harness.rs`
  - Add `HarnessMode`, live-capable scenario selection, run summary wording, and world factory plumbing.
- Modify: `tools/scaled-artifact-smoke/src/fixture.rs`
  - Add dry/live world construction.
  - Build PostgreSQL database, NATS control plane, and S3 storage for live mode from explicit config values.
  - Add run-id suffix helper.
- Modify: `tools/scaled-artifact-smoke/src/live.rs`
  - Expose environment collection and validation helpers for both `--live-preflight` and `--live`.
  - Parse optional NATS subject and S3 path-style flag with soak-prefixed error names.
  - Map soak S3 values into `S3ArtifactStorageConfig::from_env_values` without reading production `PANDAR_ARTIFACT_S3_*`.
- Modify: `tools/scaled-artifact-smoke/src/scenarios.rs`
  - Replace direct `SmokeWorld::new()` calls in live-capable scenarios with `SmokeWorld::for_config(config)`.
  - Keep storage failure scenario local-only.
- Modify: `docs/development.md`
- Modify: `docs/compatibility/phase-26-soak-evidence.md`
- Modify: `docs/roadmap.md`

## Task 1: CLI Mode Parsing And Validation

**Files:**

- Modify: `tools/scaled-artifact-smoke/src/main.rs`
- Modify: `tools/scaled-artifact-smoke/src/harness.rs`

- [x] **Step 1: Add failing parser tests**

Add a `#[cfg(test)]` module to `tools/scaled-artifact-smoke/src/main.rs` with tests that assert:

```rust
#[test]
fn parse_rejects_no_mode() {
    assert!(parse_args(Vec::<String>::new()).is_err());
}

#[test]
fn parse_accepts_dry_run_mode() {
    let config = parse_args(["--dry-run"].into_iter().map(str::to_owned)).unwrap();
    assert_eq!(config.mode, HarnessMode::DryRun);
    assert_eq!(config.iterations, 1);
    assert_eq!(config.concurrency, 2);
    assert_eq!(config.scenario, ScenarioFilter::All);
}

#[test]
fn parse_accepts_live_mode() {
    let config = parse_args(
        ["--live", "--iterations", "2", "--concurrency", "3", "--scenario", "artifact"]
            .into_iter()
            .map(str::to_owned),
    )
    .unwrap();
    assert_eq!(config.mode, HarnessMode::Live);
    assert_eq!(config.iterations, 2);
    assert_eq!(config.concurrency, 3);
    assert_eq!(config.scenario, ScenarioFilter::Artifact);
}

#[test]
fn parse_rejects_live_storage_scenario() {
    let err = parse_args(["--live", "--scenario", "storage"].into_iter().map(str::to_owned))
        .unwrap_err();
    assert!(format!("{err:#}").contains("storage failure scenario is local dry-run only"));
}

#[test]
fn live_all_excludes_storage_scenario() {
    let config = parse_args(["--live", "--scenario", "all"].into_iter().map(str::to_owned))
        .unwrap();
    assert_eq!(
        config.included_scenarios(),
        vec![
            ScenarioFilter::Artifact,
            ScenarioFilter::Fanout,
            ScenarioFilter::Restart,
            ScenarioFilter::Terminal,
        ]
    );
}

#[test]
fn dry_run_all_includes_storage_scenario() {
    let config = parse_args(["--dry-run", "--scenario", "all"].into_iter().map(str::to_owned))
        .unwrap();
    assert_eq!(
        config.included_scenarios(),
        vec![
            ScenarioFilter::Artifact,
            ScenarioFilter::Fanout,
            ScenarioFilter::Restart,
            ScenarioFilter::Storage,
            ScenarioFilter::Terminal,
        ]
    );
}
```

Import `HarnessMode` with the existing `harness::{...}` import. These tests should fail before implementation because `HarnessMode` and `--live` do not exist.

- [ ] **Step 2: Run parser tests and confirm RED**

Run:

```bash
cargo test --manifest-path tools/scaled-artifact-smoke/Cargo.toml main::tests
```

Expected: compile failure mentioning missing `HarnessMode` or parsing failure for `--live`.

- [x] **Step 3: Implement CLI mode parsing**

In `tools/scaled-artifact-smoke/src/harness.rs`, add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessMode {
    DryRun,
    Live,
}
```

Add `pub mode: HarnessMode` to `HarnessConfig`, and set `mode: HarnessMode::DryRun` in `Default`.

Add:

```rust
impl HarnessConfig {
    pub fn included_scenarios(&self) -> Vec<ScenarioFilter> {
        match (self.mode, self.scenario) {
            (HarnessMode::Live, ScenarioFilter::All) => vec![
                ScenarioFilter::Artifact,
                ScenarioFilter::Fanout,
                ScenarioFilter::Restart,
                ScenarioFilter::Terminal,
            ],
            (_, ScenarioFilter::All) => vec![
                ScenarioFilter::Artifact,
                ScenarioFilter::Fanout,
                ScenarioFilter::Restart,
                ScenarioFilter::Storage,
                ScenarioFilter::Terminal,
            ],
            (_, scenario) => vec![scenario],
        }
    }
}
```

Update `HarnessConfig::validate`:

```rust
ensure!(self.iterations > 0, "iterations must be greater than zero");
ensure!(self.concurrency > 0, "concurrency must be greater than zero");
ensure!(
    self.concurrency <= 16,
    "concurrency must be 16 or less for scaled smoke"
);
ensure!(
    !(self.mode == HarnessMode::Live && self.scenario == ScenarioFilter::Storage),
    "storage failure scenario is local dry-run only"
);
```

The live mode intentionally inherits the same `16` cap for this milestone.

In `tools/scaled-artifact-smoke/src/main.rs`, update the import:

```rust
use harness::{HarnessConfig, HarnessMode, ScenarioFilter};
```

Change mode parsing:

```rust
match mode.as_str() {
    "--dry-run" => config.mode = HarnessMode::DryRun,
    "--live" => config.mode = HarnessMode::Live,
    _ => usage()?,
}
```

Update usage text:

```rust
"usage: pandar-scaled-artifact-smoke --dry-run [--iterations N] [--concurrency N] [--scenario all|artifact|fanout|restart|storage|terminal] | --live [--iterations N] [--concurrency N] [--scenario all|artifact|fanout|restart|terminal] | --live-preflight"
```

- [x] **Step 4: Run parser tests and confirm GREEN**

Run:

```bash
cargo test --manifest-path tools/scaled-artifact-smoke/Cargo.toml main::tests
```

Expected: parser tests pass.

## Task 2: Live Environment Config Mapping

**Files:**

- Modify: `tools/scaled-artifact-smoke/src/live.rs`

- [x] **Step 1: Add failing live config tests**

Add tests in `tools/scaled-artifact-smoke/src/live.rs`:

```rust
#[test]
fn live_config_defaults_optional_values() {
    let config = LiveConfig::from_values(complete_values()).unwrap();
    assert_eq!(config.database_url, "postgres://pandar_soak@localhost/pandar_soak");
    assert_eq!(config.nats_url, "nats://127.0.0.1:4222");
    assert_eq!(config.nats_subject, "pandar.soak.control");
    assert!(config.s3_force_path_style);
}

#[test]
fn live_config_accepts_optional_values() {
    let mut values = complete_values();
    values.insert(
        "PANDAR_SOAK_NATS_SUBJECT".to_owned(),
        "pandar.custom.soak".to_owned(),
    );
    values.insert(
        "PANDAR_SOAK_ARTIFACT_S3_FORCE_PATH_STYLE".to_owned(),
        "false".to_owned(),
    );

    let config = LiveConfig::from_values(values).unwrap();
    assert_eq!(config.nats_subject, "pandar.custom.soak");
    assert!(!config.s3_force_path_style);
}

#[test]
fn live_config_rejects_invalid_path_style_with_soak_name() {
    let mut values = complete_values();
    values.insert(
        "PANDAR_SOAK_ARTIFACT_S3_FORCE_PATH_STYLE".to_owned(),
        "maybe".to_owned(),
    );

    let err = LiveConfig::from_values(values).unwrap_err();
    assert!(format!("{err:#}").contains("PANDAR_SOAK_ARTIFACT_S3_FORCE_PATH_STYLE"));
}

#[test]
fn live_config_ignores_production_s3_environment_names() {
    let mut values = complete_values();
    values.insert(
        "PANDAR_ARTIFACT_S3_BUCKET".to_owned(),
        "production-bucket".to_owned(),
    );
    values.insert(
        "PANDAR_ARTIFACT_S3_REGION".to_owned(),
        "production-region".to_owned(),
    );
    values.insert(
        "PANDAR_ARTIFACT_S3_ENDPOINT".to_owned(),
        "https://production.example.invalid".to_owned(),
    );
    values.insert(
        "PANDAR_ARTIFACT_S3_ACCESS_KEY_ID".to_owned(),
        "production-access".to_owned(),
    );
    values.insert(
        "PANDAR_ARTIFACT_S3_SECRET_ACCESS_KEY".to_owned(),
        "production-secret".to_owned(),
    );

    let config = LiveConfig::from_values(values).unwrap();
    assert_eq!(config.s3_bucket, "pandar-soak-artifacts");
    assert_eq!(config.s3_region, "us-east-1");
    assert_eq!(config.s3_endpoint, "http://127.0.0.1:9000");
    assert_eq!(config.s3_access_key_id, "pandar-soak-access");
    assert_eq!(config.s3_secret_access_key, "pandar-soak-secret");
}
```

These tests should fail before implementation because `LiveConfig` does not exist.

- [ ] **Step 2: Run live config tests and confirm RED**

Run:

```bash
cargo test --manifest-path tools/scaled-artifact-smoke/Cargo.toml live::tests::live_config
```

Expected: compile failure mentioning missing `LiveConfig`.

- [x] **Step 3: Implement `LiveConfig`**

In `tools/scaled-artifact-smoke/src/live.rs`, add:

```rust
pub const DEFAULT_SOAK_NATS_SUBJECT: &str = "pandar.soak.control";

#[derive(Debug, Clone)]
pub struct LiveConfig {
    pub database_url: String,
    pub nats_url: String,
    pub nats_subject: String,
    pub s3_bucket: String,
    pub s3_region: String,
    pub s3_endpoint: String,
    pub s3_access_key_id: String,
    pub s3_secret_access_key: String,
    pub s3_force_path_style: bool,
}
```

Add:

```rust
pub fn collect_env() -> BTreeMap<String, String> {
    REQUIRED_ENV
        .iter()
        .chain([
            &"PANDAR_SOAK_NATS_SUBJECT",
            &"PANDAR_SOAK_ARTIFACT_S3_FORCE_PATH_STYLE",
        ])
        .filter_map(|name| env::var(name).ok().map(|value| ((*name).to_owned(), value)))
        .collect()
}
```

Update `run_preflight` to call `collect_env()`.

Implement:

```rust
impl LiveConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_values(collect_env())
    }

    pub fn from_values(values: BTreeMap<String, String>) -> anyhow::Result<Self> {
        validate(&values).map_err(|error| anyhow::anyhow!("{error}"))?;
        let s3_force_path_style = parse_optional_bool(
            &values,
            "PANDAR_SOAK_ARTIFACT_S3_FORCE_PATH_STYLE",
            true,
        )?;
        Ok(Self {
            database_url: required_value(&values, "PANDAR_SOAK_DATABASE_URL").to_owned(),
            nats_url: required_value(&values, "PANDAR_SOAK_NATS_URL").to_owned(),
            nats_subject: optional_value(&values, "PANDAR_SOAK_NATS_SUBJECT")
                .unwrap_or(DEFAULT_SOAK_NATS_SUBJECT)
                .to_owned(),
            s3_bucket: required_value(&values, "PANDAR_SOAK_ARTIFACT_S3_BUCKET").to_owned(),
            s3_region: required_value(&values, "PANDAR_SOAK_ARTIFACT_S3_REGION").to_owned(),
            s3_endpoint: required_value(&values, "PANDAR_SOAK_ARTIFACT_S3_ENDPOINT").to_owned(),
            s3_access_key_id: required_value(&values, "PANDAR_SOAK_ARTIFACT_S3_ACCESS_KEY_ID")
                .to_owned(),
            s3_secret_access_key: required_value(
                &values,
                "PANDAR_SOAK_ARTIFACT_S3_SECRET_ACCESS_KEY",
            )
            .to_owned(),
            s3_force_path_style,
        })
    }
}
```

Add helpers:

```rust
fn optional_value<'a>(values: &'a BTreeMap<String, String>, name: &str) -> Option<&'a str> {
    values
        .get(name)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}

fn parse_optional_bool(
    values: &BTreeMap<String, String>,
    name: &'static str,
    default: bool,
) -> anyhow::Result<bool> {
    match optional_value(values, name) {
        None => Ok(default),
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(_) => anyhow::bail!("{name} must be true or false"),
    }
}
```

- [x] **Step 4: Run live config tests and confirm GREEN**

Run:

```bash
cargo test --manifest-path tools/scaled-artifact-smoke/Cargo.toml live::tests::live_config
```

Expected: live config tests pass.

## Task 3: Dry/Live SmokeWorld Construction

**Files:**

- Modify: `tools/scaled-artifact-smoke/src/fixture.rs`
- Modify: `tools/scaled-artifact-smoke/src/scenarios.rs`
- Modify: `tools/scaled-artifact-smoke/src/harness.rs`

- [x] **Step 1: Add failing run-id suffix test**

In `tools/scaled-artifact-smoke/src/fixture.rs`, add a test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_suffix_includes_live_run_id() {
        let config = HarnessConfig {
            mode: HarnessMode::Live,
            iterations: 1,
            concurrency: 2,
            scenario: ScenarioFilter::Artifact,
            run_id: "pid123-now456".to_owned(),
        };

        assert_eq!(
            config.fixture_suffix("artifact", 7, 3),
            "live-pid123-now456-artifact-7-3"
        );
    }
}
```

Import `HarnessConfig`, `HarnessMode`, and `ScenarioFilter` from `crate::harness`.

This test should fail until `run_id` and `fixture_suffix` exist.

- [ ] **Step 2: Run fixture test and confirm RED**

Run:

```bash
cargo test --manifest-path tools/scaled-artifact-smoke/Cargo.toml fixture::tests::fixture_suffix_includes_live_run_id
```

Expected: compile failure mentioning missing fields or method.

- [x] **Step 3: Add run id and world factory methods**

In `tools/scaled-artifact-smoke/src/harness.rs`:

- Add `pub run_id: String` to `HarnessConfig`.
- In `Default`, set `run_id: default_run_id()`.
- Add:

```rust
fn default_run_id() -> String {
    format!("{}-{}", std::process::id(), chrono_like_timestamp())
}

fn chrono_like_timestamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
```

Do not add a chrono dependency.

- Add:

```rust
impl HarnessConfig {
    pub fn fixture_suffix(&self, scenario: &str, iteration: usize, index: usize) -> String {
        match self.mode {
            HarnessMode::DryRun => format!("{scenario}-{iteration}-{index}"),
            HarnessMode::Live => format!("live-{}-{scenario}-{iteration}-{index}", self.run_id),
        }
    }
}
```

In `tools/scaled-artifact-smoke/src/fixture.rs`, replace `SmokeWorld::new()` with:

```rust
pub async fn for_config(config: &HarnessConfig) -> anyhow::Result<Self> {
    match config.mode {
        HarnessMode::DryRun => Self::dry_run().await,
        HarnessMode::Live => Self::live().await,
    }
}

pub async fn dry_run() -> anyhow::Result<Self> {
    // existing SmokeWorld::new body
}

async fn live() -> anyhow::Result<Self> {
    let live = crate::live::LiveConfig::from_env()?;
    let database = Database::connect(&DatabaseConfig::from_url(live.database_url)?)
        .await
        .context("connect live soak PostgreSQL database")?;
    database
        .migrate()
        .await
        .context("migrate live soak PostgreSQL database")?;
    let control_plane = ControlPlane::from_config(
        pandar_hub::cluster::ControlPlaneConfig::Nats {
            url: live.nats_url,
            subject: live.nats_subject,
        },
    )
    .await
    .context("connect live soak NATS control plane")?;
    let storage_config = pandar_hub::artifacts::S3ArtifactStorageConfig::from_env_values(
        Some(live.s3_bucket),
        Some(live.s3_region),
        Some(live.s3_endpoint),
        Some(live.s3_access_key_id),
        Some(live.s3_secret_access_key),
        Some(if live.s3_force_path_style { "true" } else { "false" }),
        None::<String>,
    )
    .context("build live soak S3 storage config from PANDAR_SOAK_* values")?;
    let storage: Arc<dyn ArtifactStorage> = Arc::new(
        storage_config
            .build()
            .await
            .context("connect live soak S3-compatible artifact storage")?,
    );
    storage
        .check_ready()
        .await
        .context("check live soak S3-compatible artifact storage readiness")?;
    let hub_a = AppState::from_database_with_control_plane(
        database.clone(),
        storage.clone(),
        control_plane.clone(),
    );
    let hub_b = AppState::from_database_with_control_plane(
        database.clone(),
        storage.clone(),
        control_plane.clone(),
    );
    Ok(Self {
        temp: tempfile::tempdir().context("create live smoke temp dir")?,
        database,
        storage,
        control_plane,
        hub_a,
        hub_b,
    })
}
```

Keep a `pub async fn new()` compatibility wrapper only if existing tests still call it:

```rust
pub async fn new() -> anyhow::Result<Self> {
    Self::dry_run().await
}
```

- [x] **Step 4: Route scenarios through the world factory**

In `tools/scaled-artifact-smoke/src/scenarios.rs`, replace live-capable `SmokeWorld::new().await?` calls with:

```rust
let world = SmokeWorld::for_config(config).await?;
```

For `restart_convergence` and `terminal_report_idempotence`, rename `_config` to `config` and use it.

Replace seed suffixes:

```rust
seed_fixture(&world.hub_a, &config.fixture_suffix("artifact", iteration, 0)).await?;
```

For concurrent pressure, use:

```rust
config.fixture_suffix("pressure", iteration, index)
```

Keep `storage_failures` using `world_with_storage(...)` and local fake storage only; it is rejected for live mode before this function runs.

In `tools/scaled-artifact-smoke/src/harness.rs`, update `run` to iterate over `config.included_scenarios()` rather than relying on `ScenarioFilter::includes`. Use a `match` so live `all` never reaches the storage branch:

```rust
for scenario in config.included_scenarios() {
    match scenario {
        ScenarioFilter::All => unreachable!("included_scenarios expands all"),
        ScenarioFilter::Artifact => {
            crate::scenarios::artifact_dispatch_download(iteration, &config)
                .await
                .with_context(|| format!("scenario=artifact iteration={iteration}"))?;
            println!("PASS scenario=artifact iteration={iteration}");
        }
        ScenarioFilter::Fanout => {
            crate::scenarios::websocket_fanout(iteration, &config)
                .await
                .with_context(|| format!("scenario=fanout iteration={iteration}"))?;
            println!("PASS scenario=fanout iteration={iteration}");
        }
        ScenarioFilter::Restart => {
            crate::scenarios::restart_convergence(iteration, &config)
                .await
                .with_context(|| format!("scenario=restart iteration={iteration}"))?;
            println!("PASS scenario=restart iteration={iteration}");
        }
        ScenarioFilter::Storage => {
            crate::scenarios::storage_failures(iteration, &config)
                .await
                .with_context(|| format!("scenario=storage iteration={iteration}"))?;
            println!("PASS scenario=storage iteration={iteration}");
        }
        ScenarioFilter::Terminal => {
            crate::scenarios::terminal_report_idempotence(iteration, &config)
                .await
                .with_context(|| format!("scenario=terminal iteration={iteration}"))?;
            println!("PASS scenario=terminal iteration={iteration}");
        }
    }
}
```

- [x] **Step 5: Run fixture/scenario tests**

Run:

```bash
cargo test --manifest-path tools/scaled-artifact-smoke/Cargo.toml fixture::tests::fixture_suffix_includes_live_run_id
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --iterations 1 --concurrency 2
```

Expected: suffix test passes and dry-run smoke passes.

## Task 4: Live Runner Summary And Docs

**Files:**

- Modify: `tools/scaled-artifact-smoke/src/harness.rs`
- Modify: `docs/development.md`
- Modify: `docs/compatibility/phase-26-soak-evidence.md`
- Modify: `docs/roadmap.md`

- [x] **Step 1: Update harness summary**

In `tools/scaled-artifact-smoke/src/harness.rs`, change the final summary to:

```rust
let mode = match config.mode {
    HarnessMode::DryRun => "dry-run",
    HarnessMode::Live => "live",
};
println!(
    "PASS scaled artifact smoke: {mode} scenarios passed iterations={} concurrency={}",
    config.iterations, config.concurrency
);
```

The dry-run summary wording will become `PASS scaled artifact smoke: dry-run scenarios passed ...`, which keeps the meaning and makes live output symmetrical.

- [x] **Step 2: Update development docs**

In `docs/development.md`, update the Phase 26 command block to include:

```bash
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --live --iterations 1 --concurrency 2
```

Add text that:

- `--live` runs artifact, fanout, restart, and terminal scenarios against disposable PostgreSQL, NATS, and S3-compatible object storage.
- `--live --scenario storage` is rejected because storage failure injection is local-only.
- `PANDAR_SOAK_NATS_SUBJECT` defaults to `pandar.soak.control`.
- `PANDAR_SOAK_ARTIFACT_S3_FORCE_PATH_STYLE` defaults to `true` and accepts only `true` or `false`.
- A successful `--live-preflight` is not live soak evidence; a successful `--live` command with real dependencies is required before updating the live evidence row to passed.

- [x] **Step 3: Update evidence and roadmap docs**

In `docs/compatibility/phase-26-soak-evidence.md`, add a local/preflight evidence row for this runner implementation with `working tree before commit`, mentioning that local verification proves runner wiring and dry-run behavior, not live dependency behavior.

Keep the live PostgreSQL + NATS + Object Storage table row `blocked`.

In `docs/roadmap.md`, update Phase 26 to say:

- The smoke tool now has a `--live` runner entry point for disposable PostgreSQL/NATS/S3-compatible storage.
- Live evidence remains blocked until real disposable dependencies are configured and the command succeeds.

Do not mark Phase 26 live soak as passed.

## Task 5: Verification And Commit

**Files:**

- All files modified above.

- [x] **Step 1: Run targeted tests**

Run:

```bash
cargo test --manifest-path tools/scaled-artifact-smoke/Cargo.toml
```

Expected: all smoke tool tests pass.

- [x] **Step 2: Run formatting and lint**

Run:

```bash
cargo fmt --check
cargo clippy --manifest-path tools/scaled-artifact-smoke/Cargo.toml
```

Expected: both commands exit 0.

- [x] **Step 3: Run smoke commands**

Run:

```bash
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --dry-run --iterations 1 --concurrency 2
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --live --scenario storage
cargo run --manifest-path tools/scaled-artifact-smoke/Cargo.toml -- --live-preflight
```

Expected:

- Dry-run passes with `PASS scaled artifact smoke: dry-run scenarios passed iterations=1 concurrency=2`.
- `--live --scenario storage` exits non-zero with `storage failure scenario is local dry-run only`.
- `--live-preflight` exits non-zero in this workspace if disposable variables are missing, with all missing required variables listed. If disposable variables are configured, it may pass; record the actual result.

- [x] **Step 4: Run workspace verification**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" --workspace
git diff --check
```

Expected: nextest passes; diff check reports no whitespace errors.

- [ ] **Step 5: Review diff and run final implementation review gates**

Run:

```bash
git status --short --branch
git diff --stat
git diff -- tools/scaled-artifact-smoke/src/main.rs tools/scaled-artifact-smoke/src/harness.rs tools/scaled-artifact-smoke/src/fixture.rs tools/scaled-artifact-smoke/src/live.rs tools/scaled-artifact-smoke/src/scenarios.rs docs/development.md docs/compatibility/phase-26-soak-evidence.md docs/roadmap.md docs/superpowers/specs/2026-06-24-phase-26-live-soak-runner-design.md docs/superpowers/plans/2026-06-24-phase-26-live-soak-runner.md
```

Dispatch:

- Codex independent implementation reviewer.
- opencode implementation reviewer.

Both must return `VERDICT: APPROVE` in the SDD implementation verdict format before commit.

- [ ] **Step 6: Commit and hand off push to the main SDD coordinator**

Use the Lore commit protocol. Suggested intent:

```text
Add a Phase 26 live soak runner entry point

Constraint: Real Phase 26 live evidence still requires disposable PostgreSQL, NATS, and S3-compatible object-storage endpoints that are not configured in this workspace.
Rejected: Treating preflight or dry-run success as live soak evidence | neither exercises real PostgreSQL/NATS/object-storage behavior.
Confidence: high
Scope-risk: moderate
Directive: Do not mark Phase 26 live soak passed until `--live` succeeds with disposable dependencies and evidence is recorded.
Tested: <fresh verification commands and review gates>
Not-tested: successful live PostgreSQL+NATS+object-storage soak; NATS reconnect; PostgreSQL latency/conflict behavior; object-storage network behavior.
```

The implementation subagent must not push. After final SDD implementation review and fresh verification, the main coordinator performs the push to `main` as the workflow/user-requested final integration step.
