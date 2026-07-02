use anyhow::Context;
use clap::Parser;
use pandar_core::created_at_now;
use tokio::{
    sync::mpsc,
    time::{Duration, sleep},
};
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
use tonic::{Request, Status};

pub mod commands;
pub mod machine;
pub mod protocol;

use commands::{handle_command_with_gateway, parse_printer_config};
use machine::{BambuMachineGateway, BambuPrinterEndpoint, runtime::RuntimeBambuMachineGateway};
use protocol::agent::v1::{
    AgentEvent, AgentHeartbeat, AgentHello, HubCommand, agent_control_client::AgentControlClient,
    agent_event,
};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
const DEFAULT_REPORT_TIMEOUT: Duration = Duration::from_secs(10);

#[cfg(test)]
pub(crate) static TRACING_CAPTURE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
    #[arg(long, env = "PANDAR_HUB_API_URL")]
    pub hub_api_url: Option<String>,
    #[arg(long, env = "PANDAR_AGENT_NAME", default_value = "local-agent")]
    pub agent_name: String,
    #[arg(long, env = "PANDAR_AGENT_ID")]
    pub agent_id: String,
    #[arg(long, env = "PANDAR_TENANT_ID")]
    pub tenant_id: String,
    #[arg(long, env = "PANDAR_AGENT_CREDENTIAL")]
    pub agent_credential: String,
    #[arg(
        long,
        env = "PANDAR_AGENT_VERSION",
        default_value = env!("CARGO_PKG_VERSION")
    )]
    pub agent_version: String,
    #[arg(long, env = "PANDAR_PRINTERS", default_value = "[]")]
    pub printers: String,
    #[arg(long, env = "PANDAR_ARTIFACT_ROOT", default_value = ".")]
    pub artifact_root: std::path::PathBuf,
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
            credential: config.agent_credential.clone(),
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

pub async fn run(config: AgentConfig) -> anyhow::Result<()> {
    let printers = startup_printers(&config)?;
    let gateway = RuntimeBambuMachineGateway::new(config.clone(), printers, DEFAULT_REPORT_TIMEOUT);
    let mut backoff = ReconnectBackoff::new();
    loop {
        match run_once(config.clone(), &gateway).await {
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

fn startup_printers(config: &AgentConfig) -> anyhow::Result<Vec<BambuPrinterEndpoint>> {
    parse_printer_config(&config.printers)
}

async fn run_once(
    config: AgentConfig,
    gateway: &RuntimeBambuMachineGateway,
) -> anyhow::Result<RunOutcome> {
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

    gateway.start_initial_report_forwarders(&sender).await;

    handle_command_stream_with_gateway(&config, gateway, &sender, response.into_inner()).await
}

async fn handle_command_stream_with_gateway<G, S>(
    config: &AgentConfig,
    gateway: &G,
    sender: &mpsc::Sender<AgentEvent>,
    mut commands: S,
) -> anyhow::Result<RunOutcome>
where
    G: BambuMachineGateway,
    S: Stream<Item = Result<HubCommand, Status>> + Unpin,
{
    while let Some(command) = commands
        .next()
        .await
        .transpose()
        .context("read hub command from reverse stream")?
    {
        handle_command_with_gateway(config, gateway, sender, command).await?;
    }

    Ok(RunOutcome::ConnectedThenEnded)
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
    use serde_json::json;

    use super::*;
    use crate::machine::{
        BambuMachineGateway, discovery::DiscoveredPrinter, file_transfer::FakeMachineFileTransfer,
        mqtt::FakeMqttTransport, runtime::test_support::TestRuntimeBambuMachineGateway,
    };
    use crate::protocol::agent::v1::{HubCommand, LinkPrinter, hub_command};

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
            "--agent-credential",
            "pandar_ac_test",
            "--agent-version",
            "9.8.7",
            "--printers",
            r#"[{"host":"192.0.2.10","serial":"SERIAL","access_code":"12345678"}]"#,
        ]);

        assert_eq!(config.hub_grpc_url, "http://hub.internal:50051");
        assert_eq!(config.hub_api_url, None);
        assert_eq!(config.agent_name, "garage");
        assert_eq!(config.agent_id, agent_id);
        assert_eq!(config.tenant_id, tenant_id);
        assert_eq!(config.agent_credential, "pandar_ac_test");
        assert_eq!(config.agent_version, "9.8.7");
        assert_eq!(
            config.printers,
            r#"[{"host":"192.0.2.10","serial":"SERIAL","access_code":"12345678"}]"#
        );
        assert_eq!(config.artifact_root, std::path::PathBuf::from("."));
    }

    #[test]
    fn invalid_printer_config_fails_before_reconnect_loop() {
        let config = AgentConfig {
            printers: r#"[{"host":"192.0.2.10","serial":"","access_code":"12345678"}]"#.to_owned(),
            ..test_config()
        };

        let err = startup_printers(&config).unwrap_err();

        assert!(format!("{err:#}").contains("PANDAR_PRINTERS"));
        assert!(format!("{err:#}").contains("serial"));
    }

    #[test]
    fn startup_summary_names_hub_and_agent() {
        let config = AgentConfig {
            hub_grpc_url: "http://hub.internal:50051".to_owned(),
            hub_api_url: None,
            agent_name: "garage".to_owned(),
            agent_id: "agent-id".to_owned(),
            tenant_id: "tenant-id".to_owned(),
            agent_credential: "pandar_ac_test".to_owned(),
            agent_version: env!("CARGO_PKG_VERSION").to_owned(),
            printers: "[]".to_owned(),
            artifact_root: ".".into(),
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
                credential: "pandar_ac_test".to_owned(),
            }))
        );
    }

    #[tokio::test]
    async fn ended_command_stream_preserves_runtime_linked_printer_for_reconnect() {
        let gateway = TestRuntimeBambuMachineGateway::new(
            Vec::new(),
            FakeMachineFileTransfer::default(),
            Duration::from_secs(1),
        );
        gateway
            .push_command_transport(FakeMqttTransport::with_reports([
                get_version_report("X1 Carbon"),
                json!({"print": {"state": "READY"}}),
                get_version_report("X1 Carbon"),
                json!({"print": {"state": "IDLE"}}),
            ]))
            .await;
        gateway
            .set_discovered_printers(vec![DiscoveredPrinter {
                serial_number: Some("SERIAL123".to_owned()),
                host: "192.0.2.10".to_owned(),
                name: Some("office".to_owned()),
                model: Some("X1 Carbon".to_owned()),
                source: "ssdp",
            }])
            .await;
        let config = test_config();
        let (sender, mut events) = mpsc::channel(8);

        handle_command_stream_with_gateway(
            &config,
            &gateway,
            &sender,
            tokio_stream::iter([Ok(link_printer_command())]),
        )
        .await
        .unwrap();
        assert!(received_success_result(&mut events));

        let (reconnected_sender, _) = mpsc::channel(8);
        handle_command_stream_with_gateway(
            &config,
            &gateway,
            &reconnected_sender,
            tokio_stream::iter([]),
        )
        .await
        .unwrap();

        let snapshots = gateway.refresh_printers().await.unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].serial, "SERIAL123");
        assert_eq!(snapshots[0].state, "IDLE");
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
            hub_api_url: None,
            agent_name: "garage".to_owned(),
            agent_id: "agent-id".to_owned(),
            tenant_id: "tenant-id".to_owned(),
            agent_credential: "pandar_ac_test".to_owned(),
            agent_version: "9.8.7".to_owned(),
            printers: "[]".to_owned(),
            artifact_root: ".".into(),
        }
    }

    fn get_version_report(model: &str) -> serde_json::Value {
        json!({
            "info": {
                "command": "get_version",
                "module": [{"name": "ota", "product_name": model}]
            }
        })
    }

    fn link_printer_command() -> HubCommand {
        HubCommand {
            command_id: uuid::Uuid::new_v4().to_string(),
            command: Some(hub_command::Command::LinkPrinter(LinkPrinter {
                host: "192.0.2.10".to_owned(),
                access_code: "12345678".to_owned(),
                name: "office".to_owned(),
                printer_type: "BambuLab".to_owned(),
            })),
        }
    }

    fn received_success_result(events: &mut mpsc::Receiver<AgentEvent>) -> bool {
        let mut received_success = false;
        while let Ok(event) = events.try_recv() {
            if matches!(
                event.event,
                Some(agent_event::Event::CommandResult(result)) if result.success
            ) {
                received_success = true;
            }
        }
        received_success
    }
}
