use std::{sync::Arc, time::Duration};

use rcgen::{
    BasicConstraints, CertificateParams, CertifiedIssuer, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose,
};
use rumqttc::TlsConfiguration;
use rustls::{
    ClientConfig, RootCertStore, ServerConfig, SupportedProtocolVersion,
    client::danger::ServerCertVerifier,
    pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime, pem::PemObject},
    server::{ClientHello, ResolvesServerCert},
    sign::CertifiedKey,
};
use sha2::{Digest, Sha256};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector};

use super::{
    bambu_lan_tls_config,
    transport::{
        BambuLanCertificateVerifier, bambu_mqtt_serial_from_certificate, mqtt_topic_for_serial,
    },
};

#[test]
fn mqtt_topic_uses_certificate_common_name_without_changing_inventory_serial() {
    let certificate =
        CertificateDer::from_pem_slice(include_bytes!("tls/bambu-v1-cert.pem")).unwrap();
    let mqtt_serial = bambu_mqtt_serial_from_certificate(&certificate).unwrap();

    assert_eq!(mqtt_serial, "test-bambu-v1");
    assert_eq!(
        mqtt_topic_for_serial(
            "20P6BJ633100174",
            &mqtt_serial,
            "device/20P6BJ633100174/report",
        ),
        "device/test-bambu-v1/report"
    );
}

#[tokio::test]
async fn lan_tls_rejects_untrusted_bambu_x509_v1_certificates() {
    let error = connect_to_v1_server(
        include_bytes!("tls/bambu-v1-key.pem"),
        TestTlsVersion::Tls13,
        None,
    )
    .await
    .unwrap_err();

    assert!(
        error.contains("UnknownIssuer") || error.contains("UnsupportedCertVersion"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn lan_tls12_rejects_untrusted_bambu_x509_v1_certificates() {
    let error = connect_to_v1_server(
        include_bytes!("tls/bambu-v1-key.pem"),
        TestTlsVersion::Tls12,
        None,
    )
    .await
    .unwrap_err();

    assert!(
        error.contains("UnknownIssuer") || error.contains("UnsupportedCertVersion"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn lan_tls_rejects_leaf_only_certificate_with_wrong_pin() {
    let error = connect_to_v1_server(
        include_bytes!("tls/bambu-v1-key.pem"),
        TestTlsVersion::Tls13,
        Some([0_u8; 32]),
    )
    .await
    .unwrap_err();

    assert!(
        error.contains("ApplicationVerificationFailure"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn lan_tls_accepts_pinned_leaf_only_certificate_without_san() {
    let certificate =
        CertificateDer::from_pem_slice(include_bytes!("tls/bambu-v1-cert.pem")).unwrap();
    let fingerprint: [u8; 32] = Sha256::digest(certificate.as_ref()).into();

    for tls_version in [TestTlsVersion::Tls12, TestTlsVersion::Tls13] {
        connect_to_v1_server(
            include_bytes!("tls/bambu-v1-key.pem"),
            tls_version,
            Some(fingerprint),
        )
        .await
        .unwrap();
    }
}

#[test]
fn lan_tls_accepts_leaf_only_certificate_with_bundled_intermediate() {
    let root_key = KeyPair::generate().unwrap();
    let root = CertifiedIssuer::self_signed(
        test_certificate_params("test root", IsCa::Ca(BasicConstraints::Unconstrained)),
        root_key,
    )
    .unwrap();
    let intermediate_key = KeyPair::generate().unwrap();
    let intermediate = CertifiedIssuer::signed_by(
        test_certificate_params(
            "test intermediate",
            IsCa::Ca(BasicConstraints::Unconstrained),
        ),
        intermediate_key,
        &root,
    )
    .unwrap();
    let leaf_key = KeyPair::generate().unwrap();
    let leaf = test_certificate_params("test-bambu-chain", IsCa::NoCa)
        .signed_by(&leaf_key, &intermediate)
        .unwrap();
    let mut roots = RootCertStore::empty();
    roots.add(root.der().clone()).unwrap();
    let verifier = BambuLanCertificateVerifier::with_trust_material(
        "test-bambu-chain",
        roots,
        vec![intermediate.der().clone()],
    );

    verifier
        .verify_server_cert(
            leaf.der(),
            &[],
            &ServerName::try_from("localhost").unwrap(),
            &[],
            UnixTime::now(),
        )
        .unwrap();
}

fn test_certificate_params(common_name: &str, is_ca: IsCa) -> CertificateParams {
    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(DnType::CommonName, common_name);
    let mut params = CertificateParams::default();
    params.distinguished_name = distinguished_name;
    params.is_ca = is_ca;
    if matches!(is_ca, IsCa::Ca(_)) {
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
        ];
    } else {
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    }
    params
}

async fn connect_to_v1_server(
    private_key_pem: &[u8],
    tls_version: TestTlsVersion,
    trusted_leaf_sha256: Option<[u8; 32]>,
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

    let client_config = tls_version.client_config(trusted_leaf_sha256);
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

    fn client_config(self, trusted_leaf_sha256: Option<[u8; 32]>) -> Arc<ClientConfig> {
        let Some(trusted_leaf_sha256) = trusted_leaf_sha256 else {
            return match self {
                Self::Tls12 => crate::machine::ftps::bambu_lan_ftps_tls_config(
                    crate::machine::ftps::FtpsProfile { cap_tls_1_2: true },
                    "test-bambu-v1",
                ),
                Self::Tls13 => {
                    let TlsConfiguration::Rustls(config) = bambu_lan_tls_config("test-bambu-v1")
                    else {
                        panic!("Bambu LAN MQTT must use rustls");
                    };
                    config
                }
            };
        };
        Arc::new(
            ClientConfig::builder_with_provider(
                rustls::crypto::aws_lc_rs::default_provider().into(),
            )
            .with_protocol_versions(&[self.protocol_version()])
            .unwrap()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(
                BambuLanCertificateVerifier::with_trusted_leaf_sha256(
                    "test-bambu-v1",
                    trusted_leaf_sha256,
                ),
            ))
            .with_no_client_auth(),
        )
    }
}

#[derive(Debug)]
struct TestCertificateResolver(Arc<CertifiedKey>);

impl ResolvesServerCert for TestCertificateResolver {
    fn resolve(&self, _client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(Arc::clone(&self.0))
    }
}
