mod fixture;
mod harness;
mod http;
mod live;
mod scenarios;
mod storage;

use std::{env, process::ExitCode};

use anyhow::bail;
use harness::{HarnessConfig, HarnessMode, ScenarioFilter};

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

fn parse_args(args: impl IntoIterator<Item = String>) -> anyhow::Result<HarnessConfig> {
    let mut config = HarnessConfig::default();
    let mut args = args.into_iter();
    let Some(mode) = args.next() else {
        usage()?;
        unreachable!();
    };
    match mode.as_str() {
        "--dry-run" => config.mode = HarnessMode::DryRun,
        "--live" => config.mode = HarnessMode::Live,
        _ => usage()?,
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
        "usage: pandar-scaled-artifact-smoke --dry-run [--iterations N] [--concurrency N] [--scenario all|artifact|fanout|restart|storage|terminal] | --live [--iterations N] [--concurrency N] [--scenario all|artifact|fanout|restart|terminal|nats-reconnect|postgres-reconnect] | --live-preflight"
    )
}

#[cfg(test)]
mod main {
    use super::*;

    mod tests {
        use super::*;

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
                [
                    "--live",
                    "--iterations",
                    "2",
                    "--concurrency",
                    "3",
                    "--scenario",
                    "artifact",
                ]
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
            let err =
                parse_args(["--live", "--scenario", "storage"].into_iter().map(str::to_owned))
                    .unwrap_err();
            assert!(format!("{err:#}").contains("storage failure scenario is local dry-run only"));
        }

        #[test]
        fn live_all_excludes_manual_fault_scenarios() {
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
            let config =
                parse_args(["--dry-run", "--scenario", "all"].into_iter().map(str::to_owned))
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

        #[test]
        fn parse_accepts_live_nats_reconnect_scenario() {
            let config = parse_args(
                ["--live", "--scenario", "nats-reconnect"]
                    .into_iter()
                    .map(str::to_owned),
            )
            .unwrap();
            assert_eq!(config.scenario, ScenarioFilter::NatsReconnect);
        }

        #[test]
        fn parse_rejects_dry_run_nats_reconnect_scenario() {
            let err = parse_args(
                ["--dry-run", "--scenario", "nats-reconnect"]
                    .into_iter()
                    .map(str::to_owned),
            )
            .unwrap_err();
            assert!(format!("{err:#}").contains("nats reconnect scenario is live only"));
        }

        #[test]
        fn parse_accepts_live_postgres_reconnect_scenario() {
            let config = parse_args(
                ["--live", "--scenario", "postgres-reconnect"]
                    .into_iter()
                    .map(str::to_owned),
            )
            .unwrap();
            assert_eq!(config.scenario, ScenarioFilter::PostgresReconnect);
        }

        #[test]
        fn parse_rejects_dry_run_postgres_reconnect_scenario() {
            let err = parse_args(
                ["--dry-run", "--scenario", "postgres-reconnect"]
                    .into_iter()
                    .map(str::to_owned),
            )
            .unwrap_err();
            assert!(format!("{err:#}").contains("postgres reconnect scenario is live only"));
        }
    }
}
