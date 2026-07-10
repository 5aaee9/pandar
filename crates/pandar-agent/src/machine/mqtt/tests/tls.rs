use std::{sync::Arc, time::Duration};

use rumqttc::TlsConfiguration;
use rustls::{
    ClientConfig, ServerConfig, SupportedProtocolVersion,
    pki_types::{CertificateDer, PrivateKeyDer, ServerName, pem::PemObject},
    server::{ClientHello, ResolvesServerCert},
    sign::CertifiedKey,
};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector};

use super::bambu_lan_tls_config;

#[tokio::test]
async fn lan_tls_accepts_bambu_x509_v1_certificates() {
    connect_to_v1_server(
        include_bytes!("tls/bambu-v1-key.pem"),
        TestTlsVersion::Tls13,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn lan_tls_rejects_invalid_handshake_signatures() {
    let error = connect_to_v1_server(include_bytes!("tls/wrong-key.pem"), TestTlsVersion::Tls13)
        .await
        .unwrap_err();

    assert!(error.contains("BadSignature"), "unexpected error: {error}");
}

#[tokio::test]
async fn lan_tls12_accepts_bambu_x509_v1_certificates() {
    connect_to_v1_server(
        include_bytes!("tls/bambu-v1-key.pem"),
        TestTlsVersion::Tls12,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn lan_tls12_rejects_invalid_handshake_signatures() {
    let error = connect_to_v1_server(include_bytes!("tls/wrong-key.pem"), TestTlsVersion::Tls12)
        .await
        .unwrap_err();

    assert!(error.contains("BadSignature"), "unexpected error: {error}");
}

async fn connect_to_v1_server(
    private_key_pem: &[u8],
    tls_version: TestTlsVersion,
) -> Result<(), String> {
    let certificate =
        CertificateDer::from_pem_slice(include_bytes!("tls/bambu-v1-cert.pem")).unwrap();
    let private_key = PrivateKeyDer::from_pem_slice(private_key_pem).unwrap();
    let provider = rustls::crypto::aws_lc_rs::default_provider();
    let signing_key = provider.key_provider.load_private_key(private_key).unwrap();
    let certified_key = CertifiedKey::new(vec![certificate], signing_key);
    let server_config = ServerConfig::builder_with_provider(provider.into())
        .with_protocol_versions(&[tls_version.protocol_version()])
        .unwrap()
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(TestCertificateResolver(Arc::new(certified_key))));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        TlsAcceptor::from(Arc::new(server_config))
            .accept(stream)
            .await
    });

    let client_config = tls_version.client_config();
    let stream = TcpStream::connect(address).await.unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        TlsConnector::from(client_config)
            .connect(ServerName::try_from("localhost").unwrap(), stream),
    )
    .await
    .unwrap();

    match result {
        Ok(_) => server
            .await
            .unwrap()
            .map(|_| ())
            .map_err(|err| err.to_string()),
        Err(err) => {
            let _ = server.await;
            Err(err.to_string())
        }
    }
}

#[derive(Clone, Copy)]
enum TestTlsVersion {
    Tls12,
    Tls13,
}

impl TestTlsVersion {
    fn protocol_version(self) -> &'static SupportedProtocolVersion {
        match self {
            Self::Tls12 => &rustls::version::TLS12,
            Self::Tls13 => &rustls::version::TLS13,
        }
    }

    fn client_config(self) -> Arc<ClientConfig> {
        match self {
            Self::Tls12 => {
                crate::machine::ftps::bambu_lan_ftps_tls_config(crate::machine::ftps::FtpsProfile {
                    cap_tls_1_2: true,
                })
            }
            Self::Tls13 => {
                let TlsConfiguration::Rustls(config) = bambu_lan_tls_config() else {
                    panic!("Bambu LAN MQTT must use rustls");
                };
                config
            }
        }
    }
}

#[derive(Debug)]
struct TestCertificateResolver(Arc<CertifiedKey>);

impl ResolvesServerCert for TestCertificateResolver {
    fn resolve(&self, _client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(Arc::clone(&self.0))
    }
}
