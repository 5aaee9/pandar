use std::{sync::Arc, time::Duration};

#[cfg(test)]
use std::collections::VecDeque;

use anyhow::{Context, anyhow, bail};
use async_trait::async_trait;
use pandar_core::created_at_now;
use rumqttc::{
    AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS, TlsConfiguration, Transport,
};
use rustls::{
    ClientConfig, DigitallySignedStruct, Error as TlsError, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc};

use crate::{
    AgentConfig,
    machine::{BambuPrinterEndpoint, MachineSnapshot, materials::normalize_material_patch},
    protocol::agent::v1::{AgentEvent, MachineDiagnostic, PrintJobReport, agent_event},
};

pub const BAMBU_MQTT_PORT: u16 = 8883;
pub const BAMBU_MQTT_USERNAME: &str = "bblp";
pub const BAMBU_MQTT_QOS: u8 = 1;
const BAMBU_MQTT_MAX_PACKET_SIZE: usize = 256 * 1024;

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
    pub ams_mapping_json: Option<String>,
    pub ams_mapping2_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrintReportProgress {
    pub serial: String,
    pub job_id: Option<String>,
    pub artifact_id: Option<String>,
    pub subtask_id: Option<String>,
    pub gcode_state: Option<String>,
    pub percent: Option<u8>,
    pub remaining_time_minutes: Option<u32>,
    pub current_layer: Option<u32>,
    pub total_layers: Option<u32>,
    pub gcode_file: Option<String>,
    pub subtask_name: Option<String>,
    pub diagnostics: Vec<MachineReportDiagnostic>,
    pub observed_at: String,
    pub printer_materials_json: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MachineReportDiagnostic {
    pub kind: String,
    pub severity: String,
    pub code: Option<String>,
    pub message: String,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BambuMqttCommand {
    GetVersion,
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
            Self::GetVersion => json!({"info": {"command": "get_version", "sequence_id": "90002"}}),
            Self::RequestPushAll => json!({"pushing": {"command": "pushall"}}),
            Self::PausePrint => json!({"print": {"command": "pause", "sequence_id": "0"}}),
            Self::ResumePrint => json!({"print": {"command": "resume", "sequence_id": "0"}}),
            Self::StopPrint => json!({"print": {"command": "stop", "sequence_id": "0"}}),
            Self::SetPrintSpeed(speed) => {
                json!({"print": {"command": "print_speed", "param": speed.as_u8().to_string(), "sequence_id": "0"}})
            }
            Self::RawJson(payload) => payload.clone(),
            Self::ProjectFile(command) => project_file_payload(command),
        }
    }
}

fn project_file_payload(command: &ProjectFileCommand) -> Value {
    let mut print = serde_json::Map::new();
    print.insert("command".to_owned(), json!("project_file"));
    print.insert("sequence_id".to_owned(), json!("20000"));
    print.insert(
        "param".to_owned(),
        json!(format!("Metadata/plate_{}.gcode", command.plate_id)),
    );
    print.insert(
        "url".to_owned(),
        json!(format!("ftp://{}", command.filename)),
    );
    print.insert("file".to_owned(), json!(command.filename));
    print.insert("task_id".to_owned(), json!(command.task_id));
    print.insert("subtask_id".to_owned(), json!(command.subtask_id));
    print.insert("use_ams".to_owned(), json!(command.use_ams));
    print.insert("flow_cali".to_owned(), json!(command.flow_cali));
    print.insert("timelapse".to_owned(), json!(command.timelapse));

    if let Some(mapping) = command
        .ams_mapping_json
        .as_deref()
        .and_then(project_file_ams_mapping)
    {
        print.insert("ams_mapping".to_owned(), mapping);
    }
    if let Some(mapping) = command
        .ams_mapping2_json
        .as_deref()
        .and_then(project_file_mapping_value)
    {
        print.insert("ams_mapping_2".to_owned(), mapping);
    }

    json!({ "print": print })
}

fn project_file_ams_mapping(raw: &str) -> Option<Value> {
    let Value::Array(values) = project_file_mapping_value(raw)? else {
        return None;
    };
    Some(Value::Array(
        values
            .into_iter()
            .map(|value| match value.as_i64() {
                Some(254 | 255) => json!(-1),
                _ => value,
            })
            .collect(),
    ))
}

fn project_file_mapping_value(raw: &str) -> Option<Value> {
    serde_json::from_str(raw).ok()
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
        let discovered_model = discover_printer_model(transport, &topics, report_timeout)
            .await
            .inspect_err(|err| {
                tracing::warn!(
                    serial = %endpoint.serial,
                    error = %format!("{err:#}"),
                    "printer model discovery failed"
                );
            })?;
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
        let mut snapshot = snapshot_from_report(endpoint, &report);
        snapshot.model = Some(discovered_model);
        Ok::<MachineSnapshot, anyhow::Error>(snapshot)
    }
    .await
    .with_context(|| format!("refresh printer {}", endpoint.serial))
}

async fn discover_printer_model<T>(
    transport: &T,
    topics: &BambuMqttTopics,
    report_timeout: Duration,
) -> anyhow::Result<String>
where
    T: BambuMqttTransport + ?Sized,
{
    transport
        .publish(PublishedMqttCommand {
            topic: topics.request.clone(),
            payload: BambuMqttCommand::GetVersion.payload(),
            qos: BAMBU_MQTT_QOS,
        })
        .await
        .with_context(|| format!("publish get_version to request topic {}", topics.request))?;

    tokio::time::timeout(report_timeout, async {
        loop {
            let report = transport
                .next_report(report_timeout)
                .await
                .context("wait for MQTT get_version report")?;
            if is_get_version_report(&report) {
                return model_from_get_version_report(&report);
            }
        }
    })
    .await
    .map_err(|_| {
        anyhow!("timed out waiting for MQTT get_version report after {report_timeout:?}")
    })?
}

fn is_get_version_report(report: &Value) -> bool {
    report.pointer("/info/command").and_then(Value::as_str) == Some("get_version")
}

fn model_from_get_version_report(report: &Value) -> anyhow::Result<String> {
    let modules = report
        .pointer("/info/module")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("get_version report missing info.module array"))?;

    modules
        .iter()
        .find(|module| module.get("name").and_then(Value::as_str) == Some("ota"))
        .and_then(|module| trimmed_string(module.get("product_name")))
        .ok_or_else(|| anyhow!("get_version report missing ota product_name"))
}

pub struct RumqttcBambuMqttTransport {
    client: AsyncClient,
    event_loop: Mutex<EventLoop>,
}

impl RumqttcBambuMqttTransport {
    pub fn connect(endpoint: &BambuPrinterEndpoint) -> Self {
        Self::connect_with_client_suffix(endpoint, None)
    }

    pub fn connect_for_reports(endpoint: &BambuPrinterEndpoint) -> Self {
        Self::connect_with_client_suffix(endpoint, Some("reports"))
    }

    fn connect_with_client_suffix(endpoint: &BambuPrinterEndpoint, suffix: Option<&str>) -> Self {
        let options = bambu_lan_mqtt_options(endpoint, suffix);

        let (client, event_loop) = AsyncClient::new(options, 10);
        Self {
            client,
            event_loop: Mutex::new(event_loop),
        }
    }
}

pub fn bambu_lan_mqtt_options(
    endpoint: &BambuPrinterEndpoint,
    suffix: Option<&str>,
) -> MqttOptions {
    let client_id = match suffix {
        Some(suffix) => format!("pandar-agent-{}-{suffix}", endpoint.serial),
        None => format!("pandar-agent-{}", endpoint.serial),
    };
    let mut options = MqttOptions::new(client_id, endpoint.host.as_str(), BAMBU_MQTT_PORT);
    options.set_credentials(BAMBU_MQTT_USERNAME, endpoint.access_code.as_str());
    options.set_transport(Transport::tls_with_config(bambu_lan_tls_config()));
    options.set_keep_alive(Duration::from_secs(30));
    options.set_max_packet_size(BAMBU_MQTT_MAX_PACKET_SIZE, BAMBU_MQTT_MAX_PACKET_SIZE);

    options
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
        let result = tokio::time::timeout(report_timeout, async {
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
        .await;

        match result {
            Ok(Ok(report)) => Ok(report),
            Ok(Err(err)) => {
                warn_mqtt_report_receive_failed(&err);
                Err(err)
            }
            Err(_) => {
                let err = anyhow!("timed out waiting for MQTT report after {report_timeout:?}");
                warn_mqtt_report_receive_failed(&err);
                Err(err)
            }
        }
    }
}

fn warn_mqtt_report_receive_failed(err: &anyhow::Error) {
    tracing::warn!(
        error = %format!("{err:#}"),
        "MQTT report receive failed"
    );
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
        model: None,
        state: state.to_string(),
    }
}

pub fn print_report_from_report(
    endpoint: &BambuPrinterEndpoint,
    report: &Value,
) -> PrintReportProgress {
    let print = report.get("print").unwrap_or(&Value::Null);
    let subtask_id = trimmed_string(print.get("subtask_id"));

    let mut diagnostics = Vec::new();
    if let Some(print_error) = print.get("print_error").and_then(diagnostic_message) {
        diagnostics.push(MachineReportDiagnostic {
            kind: "print_error".to_owned(),
            severity: "error".to_owned(),
            code: None,
            message: print_error,
            payload: print.get("print_error").cloned().unwrap_or(Value::Null),
        });
    }
    collect_hms_diagnostics(report, &mut diagnostics);

    let observed_at = created_at_now();
    let printer_materials_json = normalize_material_patch(report, &observed_at)
        .and_then(|patch| serde_json::to_string(&patch).ok())
        .unwrap_or_default();

    PrintReportProgress {
        serial: endpoint.serial.clone(),
        job_id: trimmed_string(print.get("task_id")),
        artifact_id: subtask_id.clone(),
        subtask_id,
        gcode_state: trimmed_string(print.get("gcode_state")),
        percent: bounded_u32(print.get("mc_percent"), 0, 100).map(|value| value as u8),
        remaining_time_minutes: bounded_u32(print.get("mc_remaining_time"), 0, 4320),
        current_layer: bounded_u32(print.get("layer_num"), 0, 100_000),
        total_layers: bounded_u32(print.get("total_layer_num"), 0, 100_000),
        gcode_file: trimmed_string(print.get("gcode_file")),
        subtask_name: trimmed_string(print.get("subtask_name")),
        diagnostics,
        observed_at,
        printer_materials_json,
    }
}

pub fn print_job_report_event(config: &AgentConfig, progress: PrintReportProgress) -> AgentEvent {
    AgentEvent {
        agent_id: config.agent_id.clone(),
        tenant_id: config.tenant_id.clone(),
        event_id: format!("print-report-{}", progress.serial),
        event: Some(agent_event::Event::PrintJobReport(PrintJobReport {
            serial: progress.serial,
            job_id: progress.job_id.unwrap_or_default(),
            artifact_id: progress.artifact_id.unwrap_or_default(),
            subtask_id: progress.subtask_id.unwrap_or_default(),
            gcode_file: progress.gcode_file.unwrap_or_default(),
            subtask_name: progress.subtask_name.unwrap_or_default(),
            gcode_state: progress.gcode_state.unwrap_or_default(),
            percent: progress.percent.unwrap_or_default().into(),
            has_percent: progress.percent.is_some(),
            remaining_time_minutes: progress.remaining_time_minutes.unwrap_or_default(),
            has_remaining_time_minutes: progress.remaining_time_minutes.is_some(),
            current_layer: progress.current_layer.unwrap_or_default(),
            has_current_layer: progress.current_layer.is_some(),
            total_layers: progress.total_layers.unwrap_or_default(),
            has_total_layers: progress.total_layers.is_some(),
            diagnostics: progress
                .diagnostics
                .into_iter()
                .map(|diagnostic| MachineDiagnostic {
                    kind: diagnostic.kind,
                    severity: diagnostic.severity,
                    code: diagnostic.code.unwrap_or_default(),
                    message: diagnostic.message,
                    payload_json: serde_json::to_string(&diagnostic.payload)
                        .unwrap_or_else(|_| "null".to_owned()),
                })
                .collect(),
            observed_at: progress.observed_at,
            printer_materials_json: progress.printer_materials_json,
        })),
    }
}

pub async fn forward_print_reports<T>(
    config: &AgentConfig,
    transport: &T,
    endpoint: &BambuPrinterEndpoint,
    report_timeout: Duration,
    sender: &mpsc::Sender<AgentEvent>,
) -> anyhow::Result<()>
where
    T: BambuMqttTransport + ?Sized,
{
    let topics = BambuMqttTopics::for_serial(&endpoint.serial);
    transport
        .subscribe(&topics.report)
        .await
        .with_context(|| format!("subscribe to report topic {}", topics.report))?;

    loop {
        if sender.is_closed() {
            break;
        }

        match transport.next_report(report_timeout).await {
            Ok(report) => {
                let progress = print_report_from_report(endpoint, &report);
                if sender
                    .send(print_job_report_event(config, progress))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Err(err) => {
                tracing::warn!(
                    serial = %endpoint.serial,
                    error = %format!("{err:#}"),
                    "printer report receive failed"
                );
            }
        }
    }

    Ok(())
}

fn trimmed_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn bounded_u32(value: Option<&Value>, min: u32, max: u32) -> Option<u32> {
    let value = match value? {
        Value::Number(number) => {
            if let Some(value) = number.as_u64() {
                u32::try_from(value).ok()?
            } else if let Some(value) = number.as_i64() {
                u32::try_from(value).ok()?
            } else {
                let value = number.as_f64()?;
                if !value.is_finite() || value.fract() != 0.0 || value < 0.0 {
                    return None;
                }
                u32::try_from(value as u64).ok()?
            }
        }
        Value::String(raw) => raw.trim().parse().ok()?,
        _ => return None,
    };

    (min..=max).contains(&value).then_some(value)
}

fn diagnostic_message(value: &Value) -> Option<String> {
    match value {
        Value::String(raw) => {
            let trimmed = raw.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        }
        Value::Object(_) => message_from_object(value),
        Value::Null => None,
        other => Some(other.to_string()).filter(|message| !message.is_empty()),
    }
}

fn collect_hms_diagnostics(report: &Value, diagnostics: &mut Vec<MachineReportDiagnostic>) {
    for container in [report, report.get("print").unwrap_or(&Value::Null)] {
        let Value::Object(fields) = container else {
            continue;
        };
        for (key, value) in fields {
            if key.to_ascii_lowercase().contains("hms") {
                collect_hms_value(value, diagnostics);
            }
        }
    }
}

fn collect_hms_value(value: &Value, diagnostics: &mut Vec<MachineReportDiagnostic>) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_hms_value(value, diagnostics);
            }
        }
        Value::Object(_) => {
            if let (Some(code), Some(message)) =
                (code_from_object(value), message_from_object(value))
            {
                diagnostics.push(MachineReportDiagnostic {
                    kind: "hms".to_owned(),
                    severity: "warning".to_owned(),
                    code: Some(code),
                    message,
                    payload: value.clone(),
                });
            }
        }
        _ => {}
    }
}

fn code_from_object(value: &Value) -> Option<String> {
    ["code", "hms_code", "error_code"]
        .into_iter()
        .find_map(|key| trimmed_string(value.get(key)))
}

fn message_from_object(value: &Value) -> Option<String> {
    ["message", "msg", "description", "info"]
        .into_iter()
        .find_map(|key| trimmed_string(value.get(key)))
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
    fail_publish_payload: Option<Value>,
    infinite_unrelated_reports: bool,
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

    pub fn with_publish_failure(payload: Value) -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeMqttTransportState {
                fail_publish_payload: Some(payload),
                ..Default::default()
            })),
        }
    }

    pub fn with_infinite_unrelated_reports() -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeMqttTransportState {
                infinite_unrelated_reports: true,
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
        let mut state = self.state.lock().await;
        if state.fail_publish_payload.as_ref() == Some(&command.payload) {
            bail!("fake publish failure");
        }
        state.published_commands.push(command);
        Ok(())
    }

    async fn next_report(&self, _timeout: Duration) -> anyhow::Result<Value> {
        {
            let mut state = self.state.lock().await;
            if state.timeout {
                bail!("timed out waiting for MQTT report");
            }
            if let Some(report) = state.reports.pop_front() {
                return Ok(report);
            }
            if !state.infinite_unrelated_reports {
                bail!("timed out waiting for MQTT report");
            }
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
        Ok(json!({"print": {"gcode_state": "RUNNING"}}))
    }
}

#[cfg(test)]
mod tests;
