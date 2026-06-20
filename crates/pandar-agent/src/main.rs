use anyhow::Context;
use clap::Parser;
use pandar_agent::{AgentConfig, startup_summary};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config = AgentConfig::parse();
    tracing::info!("{}", startup_summary(&config));

    run(config).context("pandar-agent failed")
}

fn run(_config: AgentConfig) -> anyhow::Result<()> {
    Ok(())
}
