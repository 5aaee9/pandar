use clap::{Parser, Subcommand};
use pandar_hub::{
    artifacts::ArtifactStorageConfig,
    cleanup::{CleanupMode, CleanupOptions, cleanup_database},
    db::{Database, DatabaseConfig},
};

#[derive(Debug, Parser)]
#[command(name = "pandar", about = "Pandar operator CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Run pandar-hub")]
    Hub,
    #[command(about = "Run pandar-agent")]
    Agent(Box<pandar_agent::AgentConfig>),
    #[command(about = "Print CLI version")]
    Version,
    #[command(about = "Run retention cleanup")]
    Cleanup {
        #[arg(long, conflicts_with = "execute")]
        dry_run: bool,
        #[arg(long)]
        execute: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    match Cli::parse().command {
        Command::Hub => pandar_hub::run_from_env().await?,
        Command::Agent(config) => {
            tracing::info!("{}", pandar_agent::startup_summary(&config));
            pandar_agent::run(*config).await?;
        }
        Command::Version => println!("{}", env!("CARGO_PKG_VERSION")),
        Command::Cleanup { execute, .. } => {
            let database_url = std::env::var("PANDAR_DATABASE_URL")
                .unwrap_or_else(|_| "sqlite://pandar.db".to_owned());
            let config = DatabaseConfig::from_url(database_url)?;
            let database = Database::connect(&config).await?;
            database.migrate().await?;
            let mode = if execute {
                CleanupMode::Execute
            } else {
                CleanupMode::DryRun
            };
            let artifact_storage = if mode == CleanupMode::Execute {
                Some(ArtifactStorageConfig::from_env()?.build().await?)
            } else {
                None
            };
            let summary = cleanup_database(
                &database,
                artifact_storage.as_deref(),
                CleanupOptions::from_env()?,
                mode,
            )
            .await?;
            println!("{}", serde_json::to_string(&summary_json(&summary, mode))?);
        }
    }

    Ok(())
}

fn summary_json(
    summary: &pandar_hub::cleanup::CleanupSummary,
    mode: CleanupMode,
) -> serde_json::Value {
    serde_json::json!({
        "mode": match mode {
            CleanupMode::DryRun => "dry_run",
            CleanupMode::Execute => "execute",
        },
        "jobs": summary.jobs,
        "artifacts": summary.artifacts,
        "artifact_bytes": summary.artifact_bytes,
        "commands": summary.commands,
        "machine_events": summary.machine_events,
        "audit_events": summary.audit_events,
        "plugin_login_tickets": summary.plugin_login_tickets,
        "tenant_tokens": summary.tenant_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_agent_subcommand_with_agent_options() {
        let agent_id = "00000000-0000-4000-8000-000000000001";
        let tenant_id = "00000000-0000-4000-8000-000000000002";

        let cli = Cli::parse_from([
            "pandar",
            "agent",
            "--hub-grpc-url",
            "http://hub.internal:50051",
            "--agent-name",
            "garage",
            "--agent-id",
            agent_id,
            "--tenant-id",
            tenant_id,
            "--agent-credential",
            "pandar_ac_test",
        ]);

        let Command::Agent(config) = cli.command else {
            panic!("expected agent subcommand");
        };
        assert_eq!(config.hub_grpc_url, "http://hub.internal:50051");
        assert_eq!(config.agent_name, "garage");
        assert_eq!(config.agent_id, agent_id);
        assert_eq!(config.tenant_id, tenant_id);
        assert_eq!(config.agent_credential, "pandar_ac_test");
    }

    #[test]
    fn parses_hub_subcommand() {
        let cli = Cli::parse_from(["pandar", "hub"]);

        assert!(matches!(cli.command, Command::Hub));
    }
}
