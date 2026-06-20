use anyhow::Context;
use clap::Parser;
use pandar_core::created_at_now;
use tokio::{
    sync::mpsc,
    time::{Duration, sleep},
};
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tonic::Request;

pub mod protocol;

use protocol::agent::v1::{
    AgentEvent, AgentHeartbeat, AgentHello, CommandAck, CommandResult, HubCommand,
    agent_control_client::AgentControlClient, agent_event, hub_command,
};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Parser, PartialEq, Eq)]
#[command(
    name = "pandar-agent",
    about = "Connects local Bambu printers to pandar-hub"
)]
pub struct AgentConfig {
    #[arg(
        long,
        env = "PANDAR_HUB_GRPC_URL",
        default_value = "http://127.0.0.1:50051"
    )]
    pub hub_grpc_url: String,
    #[arg(long, env = "PANDAR_AGENT_NAME", default_value = "local-agent")]
    pub agent_name: String,
    #[arg(long, env = "PANDAR_AGENT_ID")]
    pub agent_id: String,
    #[arg(long, env = "PANDAR_TENANT_ID")]
    pub tenant_id: String,
    #[arg(
        long,
        env = "PANDAR_AGENT_VERSION",
        default_value = env!("CARGO_PKG_VERSION")
    )]
    pub agent_version: String,
}

pub trait BambuMachineGateway {
    fn label(&self) -> &str;
}

pub struct ReferenceBackedGateway {
    label: String,
}

impl ReferenceBackedGateway {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
        }
    }
}

impl BambuMachineGateway for ReferenceBackedGateway {
    fn label(&self) -> &str {
        &self.label
    }
}

pub fn startup_summary(config: &AgentConfig) -> String {
    format!(
        "agent {} will connect to {}",
        config.agent_name, config.hub_grpc_url
    )
}

pub fn hello_event(config: &AgentConfig) -> AgentEvent {
    event(
        config,
        "hello",
        agent_event::Event::Hello(AgentHello {
            name: config.agent_name.clone(),
            version: config.agent_version.clone(),
        }),
    )
}

pub fn heartbeat_event(config: &AgentConfig) -> AgentEvent {
    event(
        config,
        "heartbeat",
        agent_event::Event::Heartbeat(AgentHeartbeat {
            observed_at: created_at_now(),
        }),
    )
}

pub fn ack_event(config: &AgentConfig, command_id: &str) -> AgentEvent {
    event(
        config,
        "ack",
        agent_event::Event::CommandAck(CommandAck {
            command_id: command_id.to_owned(),
            accepted: true,
            error: String::new(),
        }),
    )
}

pub fn success_event(config: &AgentConfig, command_id: &str) -> AgentEvent {
    event(
        config,
        "success",
        agent_event::Event::CommandResult(CommandResult {
            command_id: command_id.to_owned(),
            success: true,
            error: String::new(),
        }),
    )
}

pub async fn run(config: AgentConfig) -> anyhow::Result<()> {
    let mut backoff = ReconnectBackoff::new();
    loop {
        match run_once(config.clone()).await {
            Ok(RunOutcome::ConnectedThenEnded) => backoff.reset(),
            Err(err) => {
                tracing::error!(error = %format!("{err:#}"), "agent reverse connection failed");
            }
        }

        let delay = backoff.next_delay();
        tracing::info!(
            delay_seconds = delay.as_secs(),
            "reconnecting to pandar-hub"
        );
        sleep(delay).await;
    }
}

async fn run_once(config: AgentConfig) -> anyhow::Result<RunOutcome> {
    let mut client = AgentControlClient::connect(config.hub_grpc_url.clone())
        .await
        .with_context(|| format!("connect to hub gRPC at {}", config.hub_grpc_url))?;
    let (sender, receiver) = mpsc::channel(16);
    sender
        .send(hello_event(&config))
        .await
        .context("queue agent hello event")?;

    let response = client
        .reverse_connect(Request::new(ReceiverStream::new(receiver)))
        .await
        .context("open reverse agent control stream")?;

    let heartbeat_sender = sender.clone();
    let heartbeat_config = config.clone();
    tokio::spawn(async move {
        loop {
            sleep(HEARTBEAT_INTERVAL).await;
            if heartbeat_sender
                .send(heartbeat_event(&heartbeat_config))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    let mut commands = response.into_inner();
    while let Some(command) = commands
        .next()
        .await
        .transpose()
        .context("read hub command from reverse stream")?
    {
        handle_command(&config, &sender, command).await?;
    }

    Ok(RunOutcome::ConnectedThenEnded)
}

async fn handle_command(
    config: &AgentConfig,
    sender: &mpsc::Sender<AgentEvent>,
    command: HubCommand,
) -> anyhow::Result<()> {
    match command.command {
        Some(hub_command::Command::RefreshPrinters(_)) => {
            emit_refresh_printers_events(config, sender, &command.command_id).await
        }
        None => Ok(()),
    }
}

async fn emit_refresh_printers_events(
    config: &AgentConfig,
    sender: &mpsc::Sender<AgentEvent>,
    command_id: &str,
) -> anyhow::Result<()> {
    sender
        .send(ack_event(config, command_id))
        .await
        .context("queue refresh-printers command ack")?;
    sender
        .send(success_event(config, command_id))
        .await
        .context("queue refresh-printers command success")?;
    Ok(())
}

#[derive(Debug)]
pub struct ReconnectBackoff {
    next: Duration,
}

impl ReconnectBackoff {
    pub fn new() -> Self {
        Self {
            next: Duration::from_secs(1),
        }
    }

    pub fn next_delay(&mut self) -> Duration {
        let delay = self.next;
        self.next = (self.next * 2).min(Duration::from_secs(30));
        delay
    }

    pub fn reset(&mut self) {
        self.next = Duration::from_secs(1);
    }
}

impl Default for ReconnectBackoff {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunOutcome {
    ConnectedThenEnded,
}

fn event(config: &AgentConfig, event_id: &str, event: agent_event::Event) -> AgentEvent {
    AgentEvent {
        agent_id: config.agent_id.to_string(),
        tenant_id: config.tenant_id.to_string(),
        event_id: event_id.to_owned(),
        event: Some(event),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_agent_cli_config() {
        let agent_id = uuid::Uuid::new_v4().to_string();
        let tenant_id = uuid::Uuid::new_v4().to_string();
        let config = AgentConfig::parse_from([
            "pandar-agent",
            "--hub-grpc-url",
            "http://hub.internal:50051",
            "--agent-name",
            "garage",
            "--agent-id",
            &agent_id,
            "--tenant-id",
            &tenant_id,
            "--agent-version",
            "9.8.7",
        ]);

        assert_eq!(config.hub_grpc_url, "http://hub.internal:50051");
        assert_eq!(config.agent_name, "garage");
        assert_eq!(config.agent_id, agent_id);
        assert_eq!(config.tenant_id, tenant_id);
        assert_eq!(config.agent_version, "9.8.7");
    }

    #[test]
    fn startup_summary_names_hub_and_agent() {
        let config = AgentConfig {
            hub_grpc_url: "http://hub.internal:50051".to_owned(),
            agent_name: "garage".to_owned(),
            agent_id: "agent-id".to_owned(),
            tenant_id: "tenant-id".to_owned(),
            agent_version: env!("CARGO_PKG_VERSION").to_owned(),
        };

        assert_eq!(
            startup_summary(&config),
            "agent garage will connect to http://hub.internal:50051"
        );
    }

    #[test]
    fn hello_event_has_agent_identity_and_version() {
        let config = test_config();

        let event = hello_event(&config);

        assert_eq!(event.agent_id, config.agent_id.to_string());
        assert_eq!(event.tenant_id, config.tenant_id.to_string());
        assert_eq!(event.event_id, "hello");
        assert_eq!(
            event.event,
            Some(agent_event::Event::Hello(AgentHello {
                name: "garage".to_owned(),
                version: "9.8.7".to_owned(),
            }))
        );
    }

    #[tokio::test]
    async fn refresh_printers_emits_ack_and_success() {
        let config = test_config();
        let command_id = uuid::Uuid::new_v4().to_string();
        let (sender, mut receiver) = mpsc::channel(2);

        handle_command(
            &config,
            &sender,
            HubCommand {
                command_id: command_id.clone(),
                command: Some(hub_command::Command::RefreshPrinters(Default::default())),
            },
        )
        .await
        .unwrap();
        drop(sender);

        let ack = receiver.recv().await.unwrap();
        let success = receiver.recv().await.unwrap();
        assert!(receiver.recv().await.is_none());
        assert_eq!(ack, ack_event(&config, &command_id));
        assert_eq!(success, success_event(&config, &command_id));
    }

    #[test]
    fn backoff_doubles_and_caps() {
        let mut backoff = ReconnectBackoff::new();

        let delays: Vec<_> = (0..8).map(|_| backoff.next_delay()).collect();

        assert_eq!(
            delays,
            [
                Duration::from_secs(1),
                Duration::from_secs(2),
                Duration::from_secs(4),
                Duration::from_secs(8),
                Duration::from_secs(16),
                Duration::from_secs(30),
                Duration::from_secs(30),
                Duration::from_secs(30),
            ]
        );
    }

    #[test]
    fn backoff_reset_returns_to_one_second() {
        let mut backoff = ReconnectBackoff::new();

        assert_eq!(backoff.next_delay(), Duration::from_secs(1));
        assert_eq!(backoff.next_delay(), Duration::from_secs(2));
        backoff.reset();

        assert_eq!(backoff.next_delay(), Duration::from_secs(1));
    }

    #[test]
    fn heartbeat_interval_is_fifteen_seconds() {
        assert_eq!(HEARTBEAT_INTERVAL, Duration::from_secs(15));
    }

    fn test_config() -> AgentConfig {
        AgentConfig {
            hub_grpc_url: "http://hub.internal:50051".to_owned(),
            agent_name: "garage".to_owned(),
            agent_id: "agent-id".to_owned(),
            tenant_id: "tenant-id".to_owned(),
            agent_version: "9.8.7".to_owned(),
        }
    }
}
