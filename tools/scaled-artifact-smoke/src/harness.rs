use anyhow::ensure;

#[derive(Debug, Clone)]
pub struct HarnessConfig {
    pub iterations: usize,
    pub concurrency: usize,
    pub scenario: ScenarioFilter,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            iterations: 1,
            concurrency: 2,
            scenario: ScenarioFilter::All,
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
            "concurrency must be 16 or less for local dry-run"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioFilter {
    All,
    Artifact,
    Fanout,
    Restart,
    Storage,
    Terminal,
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
            other => anyhow::bail!("unknown scenario {other}"),
        }
    }

    pub fn includes(self, scenario: Self) -> bool {
        self == Self::All || self == scenario
    }
}

pub async fn run(config: HarnessConfig) -> anyhow::Result<()> {
    for iteration in 1..=config.iterations {
        if config.scenario.includes(ScenarioFilter::Artifact) {
            crate::scenarios::artifact_dispatch_download(iteration, &config).await?;
            println!("PASS scenario=artifact iteration={iteration}");
        }
        if config.scenario.includes(ScenarioFilter::Fanout) {
            crate::scenarios::websocket_fanout(iteration, &config).await?;
            println!("PASS scenario=fanout iteration={iteration}");
        }
        if config.scenario.includes(ScenarioFilter::Restart) {
            crate::scenarios::restart_convergence(iteration, &config).await?;
            println!("PASS scenario=restart iteration={iteration}");
        }
        if config.scenario.includes(ScenarioFilter::Storage) {
            crate::scenarios::storage_failures(iteration, &config).await?;
            println!("PASS scenario=storage iteration={iteration}");
        }
        if config.scenario.includes(ScenarioFilter::Terminal) {
            crate::scenarios::terminal_report_idempotence(iteration, &config).await?;
            println!("PASS scenario=terminal iteration={iteration}");
        }
    }
    println!(
        "PASS scaled artifact smoke: dry-run scenarios passed iterations={} concurrency={}",
        config.iterations, config.concurrency
    );
    Ok(())
}
