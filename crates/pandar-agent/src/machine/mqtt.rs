use std::{sync::Arc, time::Duration};

#[cfg(test)]
use std::collections::VecDeque;

use anyhow::{Context, anyhow, bail};
use async_trait::async_trait;
use rumqttc::{
    AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS, TlsConfiguration, Transport,
};
use rustls::{
    ClientConfig, DigitallySignedStruct, Error as TlsError, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::machine::{BambuPrinterEndpoint, MachineSnapshot};

pub const BAMBU_MQTT_PORT: u16 = 8883;
pub const BAMBU_MQTT_USERNAME: &str = "bblp";
pub const BAMBU_MQTT_QOS: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BambuMqttTopics {
    pub report: String,
    pub request: String,
}

impl BambuMqttTopics {
    pub fn for_serial(serial: &str) -> Self {
        Self {
            report: format!("device/{serial}/report"),
            request: format!("device/{serial}/request"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrintSpeed(u8);

impl PrintSpeed {
    pub fn new(mode: u8) -> anyhow::Result<Self> {
        if !(1..=4).contains(&mode) {
            bail!("invalid Bambu print speed mode {mode}; expected 1..=4");
        }

        Ok(Self(mode))
    }

    pub fn as_u8(self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFileCommand {
    pub filename: String,
    pub plate_id: u32,
    pub task_id: String,
    pub subtask_id: String,
    pub use_ams: bool,
    pub flow_cali: bool,
    pub timelapse: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BambuMqttCommand {
    RequestPushAll,
    PausePrint,
    ResumePrint,
    StopPrint,
    SetPrintSpeed(PrintSpeed),
    RawJson(Value),
    ProjectFile(ProjectFileCommand),
}

impl BambuMqttCommand {
    pub fn payload(&self) -> Value {
        match self {
            Self::RequestPushAll => json!({"pushing": {"command": "pushall"}}),
            Self::PausePrint => json!({"print": {"command": "pause", "sequence_id": "0"}}),
            Self::ResumePrint => json!({"print": {"command": "resume", "sequence_id": "0"}}),
            Self::StopPrint => json!({"print": {"command": "stop", "sequence_id": "0"}}),
            Self::SetPrintSpeed(speed) => {
                json!({"print": {"command": "print_speed", "param": speed.as_u8().to_string(), "sequence_id": "0"}})
            }
            Self::RawJson(payload) => payload.clone(),
            Self::ProjectFile(command) => json!({
                "print": {
                    "command": "project_file",
                    "sequence_id": "20000",
                    "param": format!("Metadata/plate_{}.gcode", command.plate_id),
                    "url": format!("ftp://{}", command.filename),
                    "file": command.filename,
                    "task_id": command.task_id,
                    "subtask_id": command.subtask_id,
                    "use_ams": command.use_ams,
                    "flow_cali": command.flow_cali,
                    "timelapse": command.timelapse,
                }
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PublishedMqttCommand {
    pub topic: String,
    pub payload: Value,
    pub qos: u8,
}

#[async_trait]
pub trait BambuMqttTransport: Send + Sync {
    async fn subscribe(&self, topic: &str) -> anyhow::Result<()>;
    async fn publish(&self, command: PublishedMqttCommand) -> anyhow::Result<()>;
    async fn next_report(&self, timeout: Duration) -> anyhow::Result<Value>;
}

pub async fn refresh_printer<T>(
    transport: &T,
    endpoint: &BambuPrinterEndpoint,
    report_timeout: Duration,
) -> anyhow::Result<MachineSnapshot>
where
    T: BambuMqttTransport + ?Sized,
{
    async move {
        let topics = BambuMqttTopics::for_serial(&endpoint.serial);
        transport
            .subscribe(&topics.report)
            .await
            .with_context(|| format!("subscribe to report topic {}", topics.report))?;
        transport
            .publish(PublishedMqttCommand {
                topic: topics.request.clone(),
                payload: BambuMqttCommand::RequestPushAll.payload(),
                qos: BAMBU_MQTT_QOS,
            })
            .await
            .with_context(|| format!("publish pushall to request topic {}", topics.request))?;
        let report = transport
            .next_report(report_timeout)
            .await
            .context("wait for MQTT report")?;
        Ok::<MachineSnapshot, anyhow::Error>(snapshot_from_report(endpoint, &report))
    }
    .await
    .with_context(|| format!("refresh printer {}", endpoint.serial))
}

pub struct RumqttcBambuMqttTransport {
    client: AsyncClient,
    event_loop: Mutex<EventLoop>,
}

impl RumqttcBambuMqttTransport {
    pub fn connect(endpoint: &BambuPrinterEndpoint) -> Self {
        let mut options = MqttOptions::new(
            format!("pandar-agent-{}", endpoint.serial),
            endpoint.host.as_str(),
            BAMBU_MQTT_PORT,
        );
        options.set_credentials(BAMBU_MQTT_USERNAME, endpoint.access_code.as_str());
        options.set_transport(Transport::tls_with_config(bambu_lan_tls_config()));
        options.set_keep_alive(Duration::from_secs(30));

        let (client, event_loop) = AsyncClient::new(options, 10);
        Self {
            client,
            event_loop: Mutex::new(event_loop),
        }
    }
}

pub fn bambu_lan_tls_config() -> TlsConfiguration {
    let mut config =
        ClientConfig::builder_with_provider(rustls::crypto::aws_lc_rs::default_provider().into())
            .with_safe_default_protocol_versions()
            .expect("aws-lc-rs provider supports rustls safe default protocol versions")
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(BambuLanCertificateVerifier))
            .with_no_client_auth();
    config.alpn_protocols = Vec::new();
    TlsConfiguration::Rustls(Arc::new(config))
}

#[derive(Debug)]
pub(crate) struct BambuLanCertificateVerifier;

impl ServerCertVerifier for BambuLanCertificateVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        let provider = rustls::crypto::aws_lc_rs::default_provider();
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        let provider = rustls::crypto::aws_lc_rs::default_provider();
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::aws_lc_rs::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[async_trait]
impl BambuMqttTransport for RumqttcBambuMqttTransport {
    async fn subscribe(&self, topic: &str) -> anyhow::Result<()> {
        self.client
            .subscribe(topic, QoS::AtLeastOnce)
            .await
            .with_context(|| format!("rumqttc subscribe {topic}"))
    }

    async fn publish(&self, command: PublishedMqttCommand) -> anyhow::Result<()> {
        let qos = qos_from_u8(command.qos)?;
        let payload =
            serde_json::to_vec(&command.payload).context("encode MQTT command payload")?;
        self.client
            .publish(command.topic.clone(), qos, false, payload)
            .await
            .with_context(|| format!("rumqttc publish {}", command.topic))
    }

    async fn next_report(&self, report_timeout: Duration) -> anyhow::Result<Value> {
        tokio::time::timeout(report_timeout, async {
            let mut event_loop = self.event_loop.lock().await;
            loop {
                match event_loop.poll().await.context("poll rumqttc event loop")? {
                    Event::Incoming(Packet::Publish(publish)) => {
                        return serde_json::from_slice(publish.payload.as_ref())
                            .context("decode MQTT report payload as JSON");
                    }
                    _ => continue,
                }
            }
        })
        .await
        .map_err(|_| anyhow!("timed out waiting for MQTT report after {report_timeout:?}"))?
    }
}

fn qos_from_u8(qos: u8) -> anyhow::Result<QoS> {
    match qos {
        0 => Ok(QoS::AtMostOnce),
        1 => Ok(QoS::AtLeastOnce),
        2 => Ok(QoS::ExactlyOnce),
        _ => bail!("invalid MQTT QoS {qos}; expected 0, 1, or 2"),
    }
}

pub fn snapshot_from_report(endpoint: &BambuPrinterEndpoint, report: &Value) -> MachineSnapshot {
    let state = ["/print/gcode_state", "/print/state", "/state"]
        .into_iter()
        .find_map(|path| report.pointer(path).and_then(Value::as_str))
        .unwrap_or("unknown");

    MachineSnapshot {
        serial: endpoint.serial.clone(),
        name: endpoint
            .name
            .clone()
            .unwrap_or_else(|| endpoint.serial.clone()),
        model: endpoint.model.clone(),
        state: state.to_string(),
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub struct FakeMqttTransport {
    state: Arc<Mutex<FakeMqttTransportState>>,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct FakeMqttTransportState {
    subscriptions: Vec<String>,
    published_commands: Vec<PublishedMqttCommand>,
    reports: VecDeque<Value>,
    timeout: bool,
}

#[cfg(test)]
impl FakeMqttTransport {
    pub fn with_reports(reports: impl IntoIterator<Item = Value>) -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeMqttTransportState {
                reports: reports.into_iter().collect(),
                ..Default::default()
            })),
        }
    }

    pub fn with_timeout() -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeMqttTransportState {
                timeout: true,
                ..Default::default()
            })),
        }
    }

    pub async fn subscriptions(&self) -> Vec<String> {
        self.state.lock().await.subscriptions.clone()
    }

    pub async fn published_commands(&self) -> Vec<PublishedMqttCommand> {
        self.state.lock().await.published_commands.clone()
    }
}

#[cfg(test)]
#[async_trait]
impl BambuMqttTransport for FakeMqttTransport {
    async fn subscribe(&self, topic: &str) -> anyhow::Result<()> {
        self.state
            .lock()
            .await
            .subscriptions
            .push(topic.to_string());
        Ok(())
    }

    async fn publish(&self, command: PublishedMqttCommand) -> anyhow::Result<()> {
        self.state.lock().await.published_commands.push(command);
        Ok(())
    }

    async fn next_report(&self, _timeout: Duration) -> anyhow::Result<Value> {
        let mut state = self.state.lock().await;
        if state.timeout {
            bail!("timed out waiting for MQTT report");
        }
        state
            .reports
            .pop_front()
            .ok_or_else(|| anyhow!("timed out waiting for MQTT report"))
    }
}

#[cfg(test)]
mod tests;
