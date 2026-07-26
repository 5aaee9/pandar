use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use anyhow::Context;
use clap::Parser;
use pandar_core::created_at_now;
use tokio::{sync::mpsc, time::sleep};
use tokio_stream::wrappers::ReceiverStream;
use tonic::Request;

mod backoff;
mod camera_control;
mod command_stream;
pub mod commands;
pub mod machine;
pub mod protocol;
mod session_supervisor;
mod startup;
mod transport_security;

pub use backoff::ReconnectBackoff;
use backoff::{DEFAULT_REPORT_TIMEOUT, HEARTBEAT_INTERVAL, RunOutcome};
#[cfg(test)]
use command_stream::handle_command_stream_with_gateway;
use command_stream::run_command_stream_until_cancelled;
use machine::{FirmwareMachineGateway, runtime::RuntimeBambuMachineGateway};
use protocol::agent::v1::{
    AgentCapability, AgentEvent, AgentHeartbeat, AgentHello,
    agent_control_client::AgentControlClient, agent_event,
};
use session_supervisor::SessionSupervisor;
#[cfg(test)]
use session_supervisor::reap_session_task;
use startup::startup_printers;
use transport_security::validate_hub_transport_urls;

#[cfg(test)]
pub(crate) static TRACING_CAPTURE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
static ACTIVE_HEARTBEAT_TASKS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn active_heartbeat_tasks_for_test() -> usize {
    ACTIVE_HEARTBEAT_TASKS.load(Ordering::SeqCst)
}

#[cfg(test)]
struct HeartbeatTaskGuard;

#[cfg(test)]
impl HeartbeatTaskGuard {
    fn new() -> Self {
        ACTIVE_HEARTBEAT_TASKS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

#[cfg(test)]
impl Drop for HeartbeatTaskGuard {
    fn drop(&mut self) {
        ACTIVE_HEARTBEAT_TASKS.fetch_sub(1, Ordering::SeqCst);
    }
}

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
            capabilities: vec![
                AgentCapability::HandlePrintError as i32,
                AgentCapability::HandlePrintErrorSequenceZeroPubackOnly as i32,
                AgentCapability::RequiredDeviceFeatures as i32,
                AgentCapability::GcodeLine as i32,
                AgentCapability::FirmwareControl as i32,
            ],
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
    validate_hub_transport_urls(&config)?;
    let printers = startup_printers(&config).await?;
    let gateway = Arc::new(RuntimeBambuMachineGateway::new(
        config.clone(),
        printers,
        DEFAULT_REPORT_TIMEOUT,
    ));
    let mut backoff = ReconnectBackoff::new();
    loop {
        match run_once(config.clone(), Arc::clone(&gateway)).await {
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

async fn run_once(
    config: AgentConfig,
    gateway: Arc<RuntimeBambuMachineGateway>,
) -> anyhow::Result<RunOutcome> {
    static NEXT_SESSION_EPOCH: AtomicU64 = AtomicU64::new(1);
    let session_epoch = NEXT_SESSION_EPOCH.fetch_add(1, Ordering::Relaxed);
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

    let (cancel_session, cancelled) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let mut cancelled = Box::pin(async move {
            let _ = cancelled.await;
        });
        let heartbeat_sender = sender.clone();
        let heartbeat_config = config.clone();
        #[cfg(test)]
        let heartbeat_gateway = Arc::clone(&gateway);
        let heartbeat = tokio::spawn(async move {
            #[cfg(test)]
            let _guard = HeartbeatTaskGuard::new();
            #[cfg(test)]
            heartbeat_gateway
                .panic_heartbeat_for_test_if_installed()
                .await;
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
        let prepared = {
            let prepare = gateway.prepare_session(&sender);
            tokio::pin!(prepare);
            tokio::select! {
                result = &mut prepare => Some(result.context("prepare runtime printer session")),
                _ = &mut cancelled => None,
            }
        };
        let mut outcome = match prepared {
            Some(Ok(())) => {
                run_command_stream_until_cancelled(
                    &config,
                    Arc::clone(&gateway),
                    &sender,
                    response.into_inner(),
                    session_epoch,
                    cancelled,
                )
                .await
            }
            Some(Err(error)) => Err(error),
            None => Ok(RunOutcome::ConnectedThenEnded),
        };
        heartbeat.abort();
        match heartbeat.await {
            Ok(()) => {}
            Err(error) if error.is_cancelled() => {}
            Err(error) => {
                let error = anyhow::Error::new(error).context("join Agent heartbeat task");
                if outcome.is_ok() {
                    outcome = Err(error);
                } else {
                    tracing::warn!(
                        error = %format!("{error:#}"),
                        "additional reverse-session heartbeat teardown failure"
                    );
                }
            }
        }
        if let Err(error) = gateway.teardown_session_report_forwarders().await {
            let error = error.context("teardown runtime printer report forwarders");
            if outcome.is_ok() {
                outcome = Err(error);
            } else {
                tracing::warn!(
                    error = %format!("{error:#}"),
                    "additional reverse-session report teardown failure"
                );
            }
        }
        if let Err(error) = gateway.cancel_firmware_session(session_epoch).await {
            let error = error.context("teardown reverse-session firmware MQTT tasks");
            if outcome.is_ok() {
                outcome = Err(error);
            } else {
                tracing::warn!(
                    error = %format!("{error:#}"),
                    "additional reverse-session firmware teardown failure"
                );
            }
        }
        gateway.clear_session_sender(&sender).await;
        outcome
    });
    SessionSupervisor::new(cancel_session, task).join().await
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
