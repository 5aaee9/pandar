use anyhow::Context;
use clap::Parser;
use pandar_agent::{AgentConfig, run, startup_summary};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Default to info so operational warnings (for example BRTC transport
    // fallback causes) reach the journal unless an operator narrows RUST_LOG.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let config = AgentConfig::parse();
    tracing::info!("{}", startup_summary(&config));

    run(config).await.context("pandar-agent failed")
}
