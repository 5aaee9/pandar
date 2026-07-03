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

#[cfg(test)]
pub(crate) mod test_tracing {
    use std::{
        io::{self, Write},
        sync::{Arc, Mutex, Once},
    };

    use tracing_subscriber::fmt::MakeWriter;

    static INIT: Once = Once::new();
    static ACTIVE_CAPTURE: Mutex<Option<Arc<Mutex<Vec<u8>>>>> = Mutex::new(None);

    #[derive(Clone, Default)]
    pub(crate) struct CapturedLogs {
        buffer: Arc<Mutex<Vec<u8>>>,
    }

    impl CapturedLogs {
        pub(crate) fn contents(&self) -> String {
            let buffer = self
                .buffer
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            String::from_utf8(buffer).unwrap()
        }
    }

    pub(crate) fn capture_logs<T>(run: impl FnOnce() -> T) -> (CapturedLogs, T) {
        let _capture_guard = crate::TRACING_CAPTURE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        init_subscriber();
        let logs = CapturedLogs::default();
        let _active = ActiveCapture::new(logs.buffer.clone());
        let result = run();
        (logs, result)
    }

    fn init_subscriber() {
        INIT.call_once(|| {
            let subscriber = tracing_subscriber::fmt()
                .with_writer(CaptureWriter)
                .with_max_level(tracing::Level::TRACE)
                .with_ansi(false)
                .without_time()
                .finish();
            let _ = tracing::subscriber::set_global_default(subscriber);
            tracing_core::callsite::rebuild_interest_cache();
        });
    }

    struct ActiveCapture;

    impl ActiveCapture {
        fn new(buffer: Arc<Mutex<Vec<u8>>>) -> Self {
            *ACTIVE_CAPTURE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(buffer);
            tracing_core::callsite::rebuild_interest_cache();
            Self
        }
    }

    impl Drop for ActiveCapture {
        fn drop(&mut self) {
            *ACTIVE_CAPTURE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
            tracing_core::callsite::rebuild_interest_cache();
        }
    }

    #[derive(Clone)]
    struct CaptureWriter;

    impl<'writer> MakeWriter<'writer> for CaptureWriter {
        type Writer = CaptureLogWriter;

        fn make_writer(&'writer self) -> Self::Writer {
            CaptureLogWriter
        }
    }

    struct CaptureLogWriter;

    impl Write for CaptureLogWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let Some(buffer) = ACTIVE_CAPTURE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
            else {
                return Ok(buf.len());
            };
            buffer
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}

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
mod tests;
