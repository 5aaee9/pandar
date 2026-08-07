use rustls::time_provider::TimeProvider;

use super::*;

#[tokio::test]
async fn trusted_root_certificate_needs_no_pin() {
    for tls_version in [TestTlsVersion::Tls12, TestTlsVersion::Tls13] {
        connect_to_server(
            include_bytes!("bambu-v1-chain-leaf.pem"),
            include_bytes!("bambu-v1-chain-leaf-key.pem"),
            tls_version,
            trusted_v1_client_config("test-bambu-v1", None),
        )
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn forged_signature_is_rejected() {
    let error = connect_to_server(
        include_bytes!("bambu-v1-forged-leaf.pem"),
        include_bytes!("bambu-v1-forged-leaf-key.pem"),
        TestTlsVersion::Tls13,
        trusted_v1_client_config("test-bambu-v1", None),
    )
    .await
    .unwrap_err();

    assert!(error.contains("BadSignature"), "unexpected error: {error}");
}

#[tokio::test]
async fn sha1_certificate_signature_is_rejected() {
    let error = connect_to_server(
        include_bytes!("bambu-v1-sha1-leaf.pem"),
        include_bytes!("bambu-v1-sha1-leaf-key.pem"),
        TestTlsVersion::Tls13,
        trusted_v1_client_config("test-bambu-v1", None),
    )
    .await
    .unwrap_err();

    assert!(
        error.contains("ApplicationVerificationFailure"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn mismatched_signature_algorithms_are_rejected() {
    let error = connect_to_server_certificate(
        certificate_with_mismatched_outer_signature_algorithm(),
        include_bytes!("bambu-v1-chain-leaf-key.pem"),
        TestTlsVersion::Tls13,
        trusted_v1_client_config("test-bambu-v1", None),
    )
    .await
    .unwrap_err();

    assert!(error.contains("BadEncoding"), "unexpected error: {error}");
}

#[tokio::test]
async fn wrong_common_name_is_rejected() {
    let error = connect_to_server(
        include_bytes!("bambu-v1-chain-leaf.pem"),
        include_bytes!("bambu-v1-chain-leaf-key.pem"),
        TestTlsVersion::Tls13,
        trusted_v1_client_config("different-printer", None),
    )
    .await
    .unwrap_err();

    assert!(
        error.contains("ApplicationVerificationFailure"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn expired_certificate_is_rejected() {
    let error = connect_to_server(
        include_bytes!("bambu-v1-chain-leaf.pem"),
        include_bytes!("bambu-v1-chain-leaf-key.pem"),
        TestTlsVersion::Tls13,
        trusted_v1_client_config(
            "test-bambu-v1",
            Some(UnixTime::since_unix_epoch(Duration::from_secs(
                7_258_118_400,
            ))),
        ),
    )
    .await
    .unwrap_err();

    assert!(error.contains("Expired"), "unexpected error: {error}");
}

#[tokio::test]
async fn not_yet_valid_certificate_is_rejected() {
    let error = connect_to_server(
        include_bytes!("bambu-v1-chain-leaf.pem"),
        include_bytes!("bambu-v1-chain-leaf-key.pem"),
        TestTlsVersion::Tls13,
        trusted_v1_client_config(
            "test-bambu-v1",
            Some(UnixTime::since_unix_epoch(Duration::from_secs(
                1_577_836_800,
            ))),
        ),
    )
    .await
    .unwrap_err();

    assert!(error.contains("NotValidYet"), "unexpected error: {error}");
}

fn certificate_with_mismatched_outer_signature_algorithm() -> CertificateDer<'static> {
    const SHA256_WITH_RSA_OID_DER: &[u8] = &[
        0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0b,
    ];
    let certificate =
        CertificateDer::from_pem_slice(include_bytes!("bambu-v1-chain-leaf.pem")).unwrap();
    let mut der = certificate.as_ref().to_vec();
    let offsets = der
        .windows(SHA256_WITH_RSA_OID_DER.len())
        .enumerate()
        .filter_map(|(offset, value)| (value == SHA256_WITH_RSA_OID_DER).then_some(offset))
        .collect::<Vec<_>>();
    assert_eq!(offsets.len(), 2);
    der[offsets[1] + SHA256_WITH_RSA_OID_DER.len() - 1] = 0x0c;
    CertificateDer::from(der)
}

fn trusted_v1_client_config(expected_serial: &str, now: Option<UnixTime>) -> Arc<ClientConfig> {
    let root = CertificateDer::from_pem_slice(include_bytes!("bambu-v1-chain-root.pem")).unwrap();
    let mut config =
        ClientConfig::builder_with_provider(rustls::crypto::aws_lc_rs::default_provider().into())
            .with_safe_default_protocol_versions()
            .unwrap()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(
                BambuLanCertificateVerifier::with_trust_material(
                    expected_serial,
                    vec![root],
                    Vec::new(),
                ),
            ))
            .with_no_client_auth();
    if let Some(now) = now {
        config.time_provider = Arc::new(FixedTimeProvider(now));
    }
    Arc::new(config)
}

#[derive(Debug)]
struct FixedTimeProvider(UnixTime);

impl TimeProvider for FixedTimeProvider {
    fn current_time(&self) -> Option<UnixTime> {
        Some(self.0)
    }
}
