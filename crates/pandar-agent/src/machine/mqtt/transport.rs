use std::{fmt, sync::Arc, time::Duration};

use crate::machine::BambuPrinterEndpoint;
use anyhow::{Context, anyhow, bail};
use async_trait::async_trait;
use rumqttc::{AsyncClient, MqttOptions, QoS, TlsConfiguration, Transport};
use rustls::{ClientConfig, pki_types::ServerName};
use serde_json::Value;
use tokio::{net::TcpStream, sync::OnceCell};
use tokio_rustls::TlsConnector;
use uuid::Uuid;

use super::{
    BAMBU_MQTT_MAX_PACKET_SIZE, BAMBU_MQTT_PORT, BAMBU_MQTT_RETAIN, BAMBU_MQTT_USERNAME,
    BambuMqttTopics, BambuMqttTransport, PublishedMqttCommand,
};

mod pump;
mod tls;

use pump::{MqttEventLoopPump, OverflowPolicy};
pub(crate) use tls::{BambuLanCertificateVerifier, bambu_mqtt_serial_from_certificate};

#[derive(Debug)]
struct MqttReportIdleTimeout(Duration);

impl fmt::Display for MqttReportIdleTimeout {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "timed out waiting for MQTT report after {:?}",
            self.0
        )
    }
}

impl std::error::Error for MqttReportIdleTimeout {}

pub(crate) fn mqtt_report_idle_timeout(timeout: Duration) -> anyhow::Error {
    anyhow::Error::new(MqttReportIdleTimeout(timeout))
}

pub(crate) fn is_mqtt_report_idle_timeout(error: &anyhow::Error) -> bool {
    error.downcast_ref::<MqttReportIdleTimeout>().is_some()
}

#[derive(Clone)]
pub struct RumqttcBambuMqttTransport {
    client: AsyncClient,
    pump: Arc<MqttEventLoopPump>,
    endpoint_serial: String,
    host: String,
    mqtt_serial: Arc<OnceCell<String>>,
}

impl RumqttcBambuMqttTransport {
    pub fn connect(endpoint: &BambuPrinterEndpoint) -> Self {
        Self::connect_with_client_role(endpoint, "command", OverflowPolicy::DropOldest)
    }

    pub fn connect_for_reports(endpoint: &BambuPrinterEndpoint) -> Self {
        Self::connect_with_client_role(endpoint, "reports", OverflowPolicy::FailConsumer)
    }

    fn connect_with_client_role(
        endpoint: &BambuPrinterEndpoint,
        role: &str,
        overflow_policy: OverflowPolicy,
    ) -> Self {
        let suffix = mqtt_session_client_suffix(role);
        let options = bambu_lan_mqtt_options(endpoint, Some(&suffix));

        Self::connect_with_options(
            options,
            endpoint.serial.clone(),
            endpoint.host.clone(),
            Arc::new(OnceCell::new()),
            overflow_policy,
        )
    }

    fn connect_with_options(
        options: MqttOptions,
        endpoint_serial: String,
        host: String,
        mqtt_serial: Arc<OnceCell<String>>,
        overflow_policy: OverflowPolicy,
    ) -> Self {
        let (client, event_loop) = AsyncClient::builder(options).capacity(10).build();
        Self {
            client,
            pump: Arc::new(MqttEventLoopPump::spawn(
                event_loop,
                endpoint_serial.clone(),
                overflow_policy,
            )),
            endpoint_serial,
            host,
            mqtt_serial,
        }
    }

    async fn mqtt_topic(&self, topic: &str) -> anyhow::Result<String> {
        let mqtt_serial = self
            .mqtt_serial
            .get_or_try_init(|| resolve_bambu_mqtt_serial(&self.host, &self.endpoint_serial))
            .await?;
        Ok(mqtt_topic_for_serial(
            &self.endpoint_serial,
            mqtt_serial,
            topic,
        ))
    }
}

fn mqtt_session_client_suffix(role: &str) -> String {
    format!("{role}-{}", Uuid::new_v4())
}

pub fn bambu_lan_mqtt_options(
    endpoint: &BambuPrinterEndpoint,
    suffix: Option<&str>,
) -> MqttOptions {
    let client_id = match suffix {
        Some(suffix) => format!("pandar-agent-{}-{suffix}", endpoint.serial),
        None => format!("pandar-agent-{}", endpoint.serial),
    };
    let mut options = MqttOptions::new(client_id, (endpoint.host.as_str(), BAMBU_MQTT_PORT));
    options.set_credentials(BAMBU_MQTT_USERNAME, endpoint.access_code.clone());
    options.set_transport(Transport::tls_with_config(bambu_lan_tls_config(
        &endpoint.serial,
    )));
    options.set_keep_alive(30);
    options.set_max_packet_size(BAMBU_MQTT_MAX_PACKET_SIZE, BAMBU_MQTT_MAX_PACKET_SIZE);

    options
}

pub fn bambu_lan_tls_config(expected_serial: &str) -> TlsConfiguration {
    TlsConfiguration::Rustls(bambu_lan_client_config(expected_serial))
}

pub(crate) fn bambu_lan_client_config(expected_serial: &str) -> Arc<ClientConfig> {
    let mut config =
        ClientConfig::builder_with_provider(rustls::crypto::aws_lc_rs::default_provider().into())
            .with_safe_default_protocol_versions()
            .expect("aws-lc-rs provider supports rustls safe default protocol versions")
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(BambuLanCertificateVerifier::new(
                expected_serial,
            )))
            .with_no_client_auth();
    config.alpn_protocols = Vec::new();
    Arc::new(config)
}

pub(crate) fn mqtt_topic_for_serial(
    endpoint_serial: &str,
    mqtt_serial: &str,
    topic: &str,
) -> String {
    let prefix = format!("device/{endpoint_serial}/");
    match topic.strip_prefix(&prefix) {
        Some(suffix) => format!("device/{mqtt_serial}/{suffix}"),
        None => topic.to_owned(),
    }
}

pub(crate) async fn resolve_bambu_mqtt_serial(
    host: &str,
    expected_serial: &str,
) -> anyhow::Result<String> {
    let server_name = ServerName::try_from(host.to_owned())
        .map_err(|_| anyhow!("invalid Bambu MQTT TLS server name {host}"))?;
    let tls_stream = tokio::time::timeout(Duration::from_secs(10), async {
        let stream = TcpStream::connect((host, BAMBU_MQTT_PORT))
            .await
            .with_context(|| format!("connect to Bambu MQTT TLS at {host}:{BAMBU_MQTT_PORT}"))?;
        TlsConnector::from(bambu_lan_client_config(expected_serial))
            .connect(server_name, stream)
            .await
            .with_context(|| format!("complete Bambu MQTT TLS handshake with {host}"))
    })
    .await
    .with_context(|| format!("Bambu MQTT TLS handshake timed out for {host}"))??;
    let certificate = tls_stream
        .get_ref()
        .1
        .peer_certificates()
        .and_then(|certificates| certificates.first())
        .ok_or_else(|| anyhow!("Bambu MQTT TLS peer at {host} did not provide a certificate"))?;

    bambu_mqtt_serial_from_certificate(certificate)
        .with_context(|| format!("resolve Bambu MQTT topic identity for {host}"))
}

pub(super) async fn resolved_request_topic(
    endpoint: &BambuPrinterEndpoint,
) -> anyhow::Result<String> {
    let mqtt_serial = resolve_bambu_mqtt_serial(&endpoint.host, &endpoint.serial).await?;
    Ok(mqtt_topic_for_serial(
        &endpoint.serial,
        &mqtt_serial,
        &BambuMqttTopics::for_serial(&endpoint.serial).request,
    ))
}

#[async_trait]
impl BambuMqttTransport for RumqttcBambuMqttTransport {
    async fn subscribe(&self, topic: &str) -> anyhow::Result<()> {
        let topic = self.mqtt_topic(topic).await?;
        self.client
            .subscribe(&topic, QoS::AtLeastOnce)
            .await
            .with_context(|| format!("rumqttc subscribe {topic}"))
    }

    async fn publish(&self, mut command: PublishedMqttCommand) -> anyhow::Result<()> {
        command.topic = self.mqtt_topic(&command.topic).await?;
        let qos = qos_from_u8(command.qos)?;
        let payload =
            serde_json::to_vec(&command.payload).context("encode MQTT command payload")?;
        self.client
            .publish(command.topic.clone(), qos, BAMBU_MQTT_RETAIN, payload)
            .await
            .with_context(|| format!("rumqttc publish {}", command.topic))
    }

    async fn next_report(&self, report_timeout: Duration) -> anyhow::Result<Value> {
        let result = tokio::time::timeout(report_timeout, self.pump.next_report()).await;

        match result {
            Ok(Ok(report)) => Ok(report),
            Ok(Err(err)) => {
                warn_mqtt_report_receive_failed(&err);
                Err(err)
            }
            Err(_) => {
                let err = mqtt_report_idle_timeout(report_timeout);
                warn_mqtt_report_receive_failed(&err);
                Err(err)
            }
        }
    }
}

pub(crate) fn warn_mqtt_report_receive_failed(err: &anyhow::Error) {
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

#[cfg(test)]
#[path = "transport_test.rs"]
mod transport_test;
