use anyhow::{Context, ensure};

#[derive(Debug, Clone)]
pub struct HarnessConfig {
    pub mode: HarnessMode,
    pub iterations: usize,
    pub concurrency: usize,
    pub scenario: ScenarioFilter,
    pub run_id: String,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            mode: HarnessMode::DryRun,
            iterations: 1,
            concurrency: 2,
            scenario: ScenarioFilter::All,
            run_id: default_run_id(),
        }
    }
}

impl HarnessConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        ensure!(self.iterations > 0, "iterations must be greater than zero");
        ensure!(
            self.concurrency > 0,
            "concurrency must be greater than zero"
        );
        ensure!(
            self.concurrency <= 16,
            "concurrency must be 16 or less for scaled smoke"
        );
        ensure!(
            !(self.mode == HarnessMode::Live && self.scenario == ScenarioFilter::Storage),
            "storage failure scenario is local dry-run only"
        );
        ensure!(
            !(self.mode == HarnessMode::DryRun && self.scenario == ScenarioFilter::NatsReconnect),
            "nats reconnect scenario is live only"
        );
        ensure!(
            !(self.mode == HarnessMode::DryRun
                && self.scenario == ScenarioFilter::PostgresReconnect),
            "postgres reconnect scenario is live only"
        );
        Ok(())
    }

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

    pub fn fixture_suffix(&self, scenario: &str, iteration: usize, index: usize) -> String {
        match self.mode {
            HarnessMode::DryRun => format!("{scenario}-{iteration}-{index}"),
            HarnessMode::Live => format!("live-{}-{scenario}-{iteration}-{index}", self.run_id),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessMode {
    DryRun,
    Live,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioFilter {
    All,
    Artifact,
    Fanout,
    Restart,
    Storage,
    Terminal,
    NatsReconnect,
    PostgresReconnect,
}

impl ScenarioFilter {
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "all" => Ok(Self::All),
            "artifact" => Ok(Self::Artifact),
            "fanout" => Ok(Self::Fanout),
            "restart" => Ok(Self::Restart),
            "storage" => Ok(Self::Storage),
            "terminal" => Ok(Self::Terminal),
            "nats-reconnect" => Ok(Self::NatsReconnect),
            "postgres-reconnect" => Ok(Self::PostgresReconnect),
            other => anyhow::bail!("unknown scenario {other}"),
        }
    }
}

pub async fn run(config: HarnessConfig) -> anyhow::Result<()> {
    for iteration in 1..=config.iterations {
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
                ScenarioFilter::NatsReconnect => {
                    crate::scenarios::nats_reconnect(iteration, &config)
                        .await
                        .with_context(|| {
                            format!("scenario=nats-reconnect iteration={iteration}")
                        })?;
                    println!("PASS scenario=nats-reconnect iteration={iteration}");
                }
                ScenarioFilter::PostgresReconnect => {
                    crate::scenarios::postgres_reconnect(iteration, &config)
                        .await
                        .with_context(|| {
                            format!("scenario=postgres-reconnect iteration={iteration}")
                        })?;
                    println!("PASS scenario=postgres-reconnect iteration={iteration}");
                }
            }
        }
    }
    let mode = match config.mode {
        HarnessMode::DryRun => "dry-run",
        HarnessMode::Live => "live",
    };
    println!(
        "PASS scaled artifact smoke: {mode} scenarios passed iterations={} concurrency={}",
        config.iterations, config.concurrency
    );
    Ok(())
}

fn default_run_id() -> String {
    format!("{}-{}", std::process::id(), unix_millis())
}

fn unix_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
