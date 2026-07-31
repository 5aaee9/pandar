use clap::{Parser, Subcommand};
use pandar_hub::{
    artifacts::ArtifactStorageConfig,
    cleanup::{CleanupMode, CleanupOptions, cleanup_database},
    db::{Database, DatabaseConfig},
};
use pandar_network_plugin::installer::{InstallNetworkPluginOptions, install_network_plugin};
use pandar_studio_hook::decrypt::decrypt_bambu_studio_local_key_log;
use pandar_studio_hook::installer::{
    InstallStudioHookOptions, UninstallStudioHookOptions, install_studio_hook,
    uninstall_studio_hook,
};
use serde::Serialize;
use std::path::PathBuf;

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
    #[command(about = "Install the Pandar network plugin and BambuSource companion")]
    InstallNetworkPlugin {
        #[arg(long)]
        plugin_file: PathBuf,
        #[arg(long)]
        source_file: PathBuf,
        #[arg(long)]
        data_dir: Option<PathBuf>,
    },
    #[command(about = "Install the Pandar Bambu Studio hook from the latest GitHub Release")]
    InstallStudioHook {
        #[arg(long)]
        studio_dir: Option<PathBuf>,
        #[arg(long)]
        data_dir: Option<PathBuf>,
    },
    #[command(about = "Uninstall the Pandar Bambu Studio hook")]
    UninstallStudioHook {
        #[arg(long)]
        studio_dir: Option<PathBuf>,
        #[arg(long)]
        data_dir: Option<PathBuf>,
    },
    #[command(about = "Decrypt a Bambu Studio log written with the local-key hook")]
    DecryptBambuStudioLog {
        #[arg(long)]
        log_file: PathBuf,
        #[arg(long)]
        output_file: PathBuf,
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
        Command::InstallNetworkPlugin {
            plugin_file,
            source_file,
            data_dir,
        } => {
            let summary = install_network_plugin(InstallNetworkPluginOptions {
                plugin_file,
                source_file,
                data_dir,
            })?;
            println!(
                "{}",
                serde_json::to_string(&NetworkPluginJson {
                    plugin_path: summary.plugin_path,
                    source_path: summary.source_path,
                    config_path: summary.config_path,
                })?
            );
        }
        Command::InstallStudioHook {
            studio_dir,
            data_dir,
        } => {
            let summary = install_studio_hook(InstallStudioHookOptions {
                studio_dir,
                data_dir,
            })
            .await?;
            println!("{}", serde_json::to_string(&studio_hook_json(summary))?);
        }
        Command::UninstallStudioHook {
            studio_dir,
            data_dir,
        } => {
            let summary = uninstall_studio_hook(UninstallStudioHookOptions {
                studio_dir,
                data_dir,
            })?;
            println!("{}", serde_json::to_string(&studio_hook_json(summary))?);
        }
        Command::DecryptBambuStudioLog {
            log_file,
            output_file,
        } => {
            decrypt_bambu_studio_local_key_log(&log_file, &output_file)?;
            println!(
                "{}",
                serde_json::to_string(&DecryptBambuStudioLogJson {
                    log_file,
                    output_file,
                })?
            );
        }
    }

    Ok(())
}

#[derive(Serialize)]
struct NetworkPluginJson {
    plugin_path: PathBuf,
    source_path: PathBuf,
    config_path: PathBuf,
}

#[derive(Serialize)]
struct DecryptBambuStudioLogJson {
    log_file: PathBuf,
    output_file: PathBuf,
}

#[derive(Serialize)]
struct StudioHookJson {
    studio_dir: PathBuf,
    proxy_path: PathBuf,
    original_path: PathBuf,
    plugin_path: PathBuf,
    source_path: PathBuf,
    config_path: PathBuf,
    plugin_package_path: PathBuf,
}

fn studio_hook_json(summary: pandar_studio_hook::installer::StudioHookSummary) -> StudioHookJson {
    StudioHookJson {
        studio_dir: summary.studio_dir,
        proxy_path: summary.proxy_path,
        original_path: summary.original_path,
        plugin_path: summary.plugin_path,
        source_path: summary.source_path,
        config_path: summary.config_path,
        plugin_package_path: summary.plugin_package_path,
    }
}

#[derive(Serialize)]
struct CleanupSummaryJson<'a> {
    mode: &'a str,
    jobs: i64,
    artifacts: i64,
    artifact_bytes: i64,
    commands: i64,
    machine_events: i64,
    audit_events: i64,
    plugin_login_tickets: i64,
    tenant_tokens: i64,
}

fn summary_json(
    summary: &pandar_hub::cleanup::CleanupSummary,
    mode: CleanupMode,
) -> CleanupSummaryJson<'static> {
    CleanupSummaryJson {
        mode: match mode {
            CleanupMode::DryRun => "dry_run",
            CleanupMode::Execute => "execute",
        },
        jobs: summary.jobs,
        artifacts: summary.artifacts,
        artifact_bytes: summary.artifact_bytes,
        commands: summary.commands,
        machine_events: summary.machine_events,
        audit_events: summary.audit_events,
        plugin_login_tickets: summary.plugin_login_tickets,
        tenant_tokens: summary.tenant_tokens,
    }
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

    #[test]
    fn parses_install_network_plugin_subcommand() {
        let cli = Cli::parse_from([
            "pandar",
            "install-network-plugin",
            "--plugin-file",
            "target/release/pandar_network_plugin.dll",
            "--source-file",
            "target/release/pandar_bambu_source.dll",
            "--data-dir",
            "C:/Users/test/AppData/Roaming/BambuStudio",
        ]);

        let Command::InstallNetworkPlugin {
            plugin_file,
            source_file,
            data_dir,
        } = cli.command
        else {
            panic!("expected install-network-plugin subcommand");
        };
        assert_eq!(
            plugin_file,
            PathBuf::from("target/release/pandar_network_plugin.dll")
        );
        assert_eq!(
            source_file,
            PathBuf::from("target/release/pandar_bambu_source.dll")
        );
        assert_eq!(
            data_dir,
            Some(PathBuf::from("C:/Users/test/AppData/Roaming/BambuStudio"))
        );
    }

    #[test]
    fn parses_install_studio_hook_subcommand() {
        let cli = Cli::parse_from([
            "pandar",
            "install-studio-hook",
            "--studio-dir",
            "C:/Program Files/Bambu Studio",
            "--data-dir",
            "C:/Users/test/AppData/Roaming/BambuStudio",
        ]);

        let Command::InstallStudioHook {
            studio_dir,
            data_dir,
        } = cli.command
        else {
            panic!("expected install-studio-hook subcommand");
        };
        assert_eq!(
            studio_dir,
            Some(PathBuf::from("C:/Program Files/Bambu Studio"))
        );
        assert_eq!(
            data_dir,
            Some(PathBuf::from("C:/Users/test/AppData/Roaming/BambuStudio"))
        );
    }

    #[test]
    fn parses_uninstall_studio_hook_subcommand() {
        let cli = Cli::parse_from([
            "pandar",
            "uninstall-studio-hook",
            "--studio-dir",
            "C:/Program Files/Bambu Studio",
            "--data-dir",
            "C:/Users/test/AppData/Roaming/BambuStudio",
        ]);

        let Command::UninstallStudioHook {
            studio_dir,
            data_dir,
        } = cli.command
        else {
            panic!("expected uninstall-studio-hook subcommand");
        };
        assert_eq!(
            studio_dir,
            Some(PathBuf::from("C:/Program Files/Bambu Studio"))
        );
        assert_eq!(
            data_dir,
            Some(PathBuf::from("C:/Users/test/AppData/Roaming/BambuStudio"))
        );
    }

    #[test]
    fn rejects_removed_studio_dev_hook_commands() {
        assert!(Cli::try_parse_from(["pandar", "install-studio-dev-hook"]).is_err());
        assert!(Cli::try_parse_from(["pandar", "uninstall-studio-dev-hook"]).is_err());
    }

    #[test]
    fn parses_decrypt_bambu_studio_log_subcommand() {
        let cli = Cli::parse_from([
            "pandar",
            "decrypt-bambu-studio-log",
            "--log-file",
            "studio_enc_cn.log.0",
            "--output-file",
            "studio.log",
        ]);

        let Command::DecryptBambuStudioLog {
            log_file,
            output_file,
        } = cli.command
        else {
            panic!("expected decrypt-bambu-studio-log subcommand");
        };
        assert_eq!(log_file, PathBuf::from("studio_enc_cn.log.0"));
        assert_eq!(output_file, PathBuf::from("studio.log"));
    }
}
