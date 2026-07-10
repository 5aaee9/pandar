use std::{sync::Arc, time::Duration};

use anyhow::{Context, anyhow, bail};
use async_trait::async_trait;
use rumqttc::{
    AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS, TlsConfiguration, Transport,
};
use rustls::{
    CertificateError, ClientConfig, DigitallySignedStruct, Error as TlsError, PeerMisbehaved,
    SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, SubjectPublicKeyInfoDer, UnixTime},
};
use serde_json::Value;
use tokio::sync::Mutex;
use x509_parser::prelude::{FromDer, X509Certificate};

use crate::machine::BambuPrinterEndpoint;

use super::{
    BAMBU_MQTT_MAX_PACKET_SIZE, BAMBU_MQTT_PORT, BAMBU_MQTT_USERNAME, BambuMqttTransport,
    PublishedMqttCommand,
};

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
        let algorithms = provider
            .signature_verification_algorithms
            .mapping
            .iter()
            .find_map(|(scheme, algorithms)| (*scheme == dss.scheme).then_some(*algorithms))
            .ok_or(TlsError::PeerMisbehaved(
                PeerMisbehaved::SignedHandshakeWithUnadvertisedSigScheme,
            ))?;
        let certificate = parse_bambu_certificate(cert)?;
        let public_key = &certificate.public_key().subject_public_key.data;

        algorithms
            .iter()
            .find_map(|algorithm| {
                algorithm
                    .verify_signature(public_key, message, dss.signature())
                    .is_ok()
                    .then_some(HandshakeSignatureValid::assertion())
            })
            .ok_or(TlsError::InvalidCertificate(CertificateError::BadSignature))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        let provider = rustls::crypto::aws_lc_rs::default_provider();
        let certificate = parse_bambu_certificate(cert)?;
        let public_key = SubjectPublicKeyInfoDer::from(certificate.public_key().raw);
        rustls::crypto::verify_tls13_signature_with_raw_key(
            message,
            &public_key,
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

fn parse_bambu_certificate<'a>(
    certificate: &'a CertificateDer<'_>,
) -> Result<X509Certificate<'a>, TlsError> {
    let (remainder, certificate) = X509Certificate::from_der(certificate.as_ref())
        .map_err(|_| TlsError::InvalidCertificate(CertificateError::BadEncoding))?;
    if !remainder.is_empty() {
        return Err(TlsError::InvalidCertificate(CertificateError::BadEncoding));
    }
    Ok(certificate)
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
