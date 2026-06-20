use anyhow::Context;
use clap::Parser;
use pandar_agent::{AgentConfig, run, startup_summary};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config = AgentConfig::parse();
    tracing::info!("{}", startup_summary(&config));

    run(config).await.context("pandar-agent failed")
}
