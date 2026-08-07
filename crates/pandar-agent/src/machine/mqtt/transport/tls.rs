use std::collections::HashMap;

use anyhow::{Context, anyhow, bail};
use rustls::{
    CertificateError, DigitallySignedStruct, Error as TlsError, PeerMisbehaved, RootCertStore,
    SignatureScheme,
    client::{
        danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        verify_server_cert_signed_by_trust_anchor,
    },
    pki_types::{CertificateDer, ServerName, SubjectPublicKeyInfoDer, UnixTime, pem::PemObject},
};
use sha2::{Digest, Sha256};
use x509_parser::{
    oid_registry::OID_PKCS1_SHA256WITHRSA,
    prelude::{FromDer, X509Certificate, X509Version},
};

#[derive(Debug)]
pub(crate) struct BambuLanCertificateVerifier {
    expected_serial: String,
    roots: RootCertStore,
    trusted_roots: Vec<CertificateDer<'static>>,
    bundled_intermediates: Vec<CertificateDer<'static>>,
    trusted_leaf_sha256: Result<Option<[u8; 32]>, String>,
}

impl BambuLanCertificateVerifier {
    pub(crate) fn new(expected_serial: &str) -> Self {
        let mut roots = RootCertStore::empty();
        let trusted_roots =
            CertificateDer::pem_slice_iter(include_bytes!("../../bambu-printer-ca.pem"))
                .map(|certificate| certificate.expect("bundled Bambu printer CA must be valid PEM"))
                .collect::<Vec<_>>();
        for certificate in &trusted_roots {
            roots
                .add(certificate.clone())
                .expect("bundled Bambu printer CA must be a valid trust anchor");
        }
        let bundled_intermediates =
            CertificateDer::pem_slice_iter(include_bytes!("../../bambu-printer-intermediates.pem"))
                .map(|certificate| {
                    certificate.expect("bundled Bambu printer intermediate must be valid PEM")
                })
                .collect();
        Self {
            expected_serial: expected_serial.to_owned(),
            roots,
            trusted_roots,
            bundled_intermediates,
            trusted_leaf_sha256: configured_bambu_leaf_pin(expected_serial),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_trust_material(
        expected_serial: &str,
        trusted_roots: Vec<CertificateDer<'static>>,
        bundled_intermediates: Vec<CertificateDer<'static>>,
    ) -> Self {
        let mut roots = RootCertStore::empty();
        for certificate in &trusted_roots {
            roots.add(certificate.clone()).unwrap();
        }
        Self {
            expected_serial: expected_serial.to_owned(),
            roots,
            trusted_roots,
            bundled_intermediates,
            trusted_leaf_sha256: Ok(None),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_trusted_leaf_sha256(
        expected_serial: &str,
        trusted_leaf_sha256: [u8; 32],
    ) -> Self {
        let mut verifier = Self::new(expected_serial);
        verifier.trusted_leaf_sha256 = Ok(Some(trusted_leaf_sha256));
        verifier
    }
}

fn configured_bambu_leaf_pin(expected_serial: &str) -> Result<Option<[u8; 32]>, String> {
    let value = match std::env::var("PANDAR_BAMBU_CERTIFICATE_SHA256_PINS") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(error) => {
            return Err(format!(
                "read PANDAR_BAMBU_CERTIFICATE_SHA256_PINS: {error}"
            ));
        }
    };
    let pins = serde_json::from_str::<HashMap<String, String>>(&value)
        .map_err(|error| format!("parse PANDAR_BAMBU_CERTIFICATE_SHA256_PINS: {error}"))?;
    pins.get(expected_serial)
        .map(|fingerprint| parse_sha256_fingerprint(fingerprint))
        .transpose()
}

fn parse_sha256_fingerprint(value: &str) -> Result<[u8; 32], String> {
    let value = value.replace(':', "");
    if value.len() != 64 {
        return Err("Bambu certificate SHA-256 fingerprint must contain 64 hex digits".to_owned());
    }
    let mut fingerprint = [0_u8; 32];
    for (index, byte) in fingerprint.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| "Bambu certificate SHA-256 fingerprint contains non-hex digits")?;
    }
    Ok(fingerprint)
}

impl ServerCertVerifier for BambuLanCertificateVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        server_intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        let certificate = parse_bambu_certificate(end_entity)?;
        let actual_serial = bambu_mqtt_serial_from_certificate(end_entity)
            .map_err(|_| TlsError::InvalidCertificate(CertificateError::BadEncoding))?;
        if actual_serial != self.expected_serial {
            return Err(TlsError::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure,
            ));
        }

        match &self.trusted_leaf_sha256 {
            Ok(Some(expected_sha256)) => {
                if Sha256::digest(end_entity.as_ref()).as_slice() != expected_sha256 {
                    return Err(TlsError::InvalidCertificate(
                        CertificateError::ApplicationVerificationFailure,
                    ));
                }
                let now = i64::try_from(now.as_secs())
                    .map_err(|_| TlsError::InvalidCertificate(CertificateError::BadEncoding))?;
                if now < certificate.validity().not_before.timestamp() {
                    return Err(TlsError::InvalidCertificate(CertificateError::NotValidYet));
                }
                if now > certificate.validity().not_after.timestamp() {
                    return Err(TlsError::InvalidCertificate(CertificateError::Expired));
                }
            }
            Ok(None) if certificate.version() == X509Version::V1 => {
                verify_bambu_v1_certificate(&certificate, &self.trusted_roots, now)?;
            }
            Ok(None) => {
                let provider = rustls::crypto::aws_lc_rs::default_provider();
                let certificate = rustls::server::ParsedCertificate::try_from(end_entity)?;
                let mut intermediates = server_intermediates.to_vec();
                for bundled in &self.bundled_intermediates {
                    if !intermediates
                        .iter()
                        .any(|certificate| certificate.as_ref() == bundled.as_ref())
                    {
                        intermediates.push(bundled.clone());
                    }
                }
                verify_server_cert_signed_by_trust_anchor(
                    &certificate,
                    &self.roots,
                    &intermediates,
                    now,
                    provider.signature_verification_algorithms.all,
                )?;
            }
            Err(error) => return Err(TlsError::General(error.clone())),
        }

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

fn verify_bambu_v1_certificate(
    certificate: &X509Certificate<'_>,
    trusted_roots: &[CertificateDer<'_>],
    now: UnixTime,
) -> Result<(), TlsError> {
    if certificate.signature_algorithm != certificate.tbs_certificate.signature {
        return Err(TlsError::InvalidCertificate(CertificateError::BadEncoding));
    }
    if certificate.signature_algorithm.algorithm != OID_PKCS1_SHA256WITHRSA {
        return Err(TlsError::InvalidCertificate(
            CertificateError::ApplicationVerificationFailure,
        ));
    }

    let mut matching_issuer = false;
    let mut valid_signature = false;
    for root in trusted_roots {
        let root = parse_bambu_certificate(root)?;
        if certificate.issuer() != root.subject() {
            continue;
        }
        matching_issuer = true;
        if certificate
            .verify_signature(Some(root.public_key()))
            .is_ok()
        {
            valid_signature = true;
            break;
        }
    }
    if !matching_issuer {
        return Err(TlsError::InvalidCertificate(
            CertificateError::UnknownIssuer,
        ));
    }
    if !valid_signature {
        return Err(TlsError::InvalidCertificate(CertificateError::BadSignature));
    }

    let now = i64::try_from(now.as_secs())
        .map_err(|_| TlsError::InvalidCertificate(CertificateError::BadEncoding))?;
    if now < certificate.validity().not_before.timestamp() {
        return Err(TlsError::InvalidCertificate(CertificateError::NotValidYet));
    }
    if now > certificate.validity().not_after.timestamp() {
        return Err(TlsError::InvalidCertificate(CertificateError::Expired));
    }

    Ok(())
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

pub(crate) fn bambu_mqtt_serial_from_certificate(
    certificate: &CertificateDer<'_>,
) -> anyhow::Result<String> {
    let certificate = parse_bambu_certificate(certificate).context("parse Bambu certificate")?;
    let common_name = certificate
        .subject()
        .iter_common_name()
        .next()
        .ok_or_else(|| anyhow!("Bambu certificate is missing a common name"))?
        .as_str()
        .context("decode Bambu certificate common name")?
        .trim();
    if common_name.is_empty() {
        bail!("Bambu certificate has a blank common name");
    }
    Ok(common_name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundles_reviewed_n6_v2_intermediate() {
        let expected_sha256 = [
            0x68, 0xbf, 0x31, 0x82, 0xbc, 0x32, 0xd5, 0xa5, 0x45, 0x4f, 0x86, 0x49, 0x28, 0xab,
            0xaa, 0x29, 0x19, 0x41, 0xf6, 0xd5, 0xac, 0x6a, 0x86, 0xa0, 0xe5, 0xad, 0x6e, 0xcf,
            0xe2, 0xd5, 0x47, 0x7b,
        ];
        let verifier = BambuLanCertificateVerifier::new("test-serial");

        assert!(
            verifier
                .bundled_intermediates
                .iter()
                .any(
                    |certificate| Sha256::digest(certificate.as_ref()).as_slice()
                        == expected_sha256
                )
        );
    }
}
