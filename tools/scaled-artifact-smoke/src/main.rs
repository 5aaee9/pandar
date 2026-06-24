mod fixture;
mod harness;
mod http;
mod scenarios;
mod storage;

use std::{env, process::ExitCode};

use anyhow::bail;
use harness::{HarnessConfig, ScenarioFilter};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("scaled artifact smoke failed: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let config = parse_args(env::args().skip(1))?;
    harness::run(config).await
}

fn parse_args(args: impl IntoIterator<Item = String>) -> anyhow::Result<HarnessConfig> {
    let mut config = HarnessConfig::default();
    let mut args = args.into_iter();
    let Some(mode) = args.next() else {
        usage()?;
        unreachable!();
    };
    if mode != "--dry-run" {
        usage()?;
    }
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--iterations" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--iterations requires a value"))?;
                config.iterations = value
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--iterations must be a positive integer"))?;
            }
            "--concurrency" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--concurrency requires a value"))?;
                config.concurrency = value
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--concurrency must be a positive integer"))?;
            }
            "--scenario" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--scenario requires a value"))?;
                config.scenario = ScenarioFilter::parse(&value)?;
            }
            _ => bail!("unknown argument {flag}"),
        }
    }
    config.validate()?;
    Ok(config)
}

fn usage() -> anyhow::Result<()> {
    bail!(
        "usage: pandar-scaled-artifact-smoke --dry-run [--iterations N] [--concurrency N] [--scenario all|artifact|fanout|restart|storage|terminal]"
    )
}
