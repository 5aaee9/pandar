use clap::{Parser, Subcommand};
use pandar_hub::{
    cleanup::{CleanupMode, CleanupOptions, cleanup_database},
    db::{Database, DatabaseConfig},
    jobs::JobStorageConfig,
    redaction::redact_secrets,
};

#[derive(Debug, Parser)]
#[command(name = "pandar", about = "Pandar operator CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
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
            let summary = cleanup_database(&database, CleanupOptions::from_env()?, mode).await?;
            if mode == CleanupMode::Execute {
                let storage = JobStorageConfig::from_env()?;
                for storage_path in &summary.artifact_storage_paths {
                    storage.remove_artifact(storage_path).await.map_err(|err| {
                        anyhow::anyhow!(
                            "failed to remove cleanup artifact: {}",
                            redact_secrets(&format!("{err:#}"))
                        )
                    })?;
                }
                pandar_hub::cleanup::cleanup_artifact_rows(&database, &summary.artifact_ids)
                    .await?;
            }
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
