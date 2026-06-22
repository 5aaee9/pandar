use std::{sync::Arc, time::Duration};

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use rustls::{ClientConfig, version};
use suppaftp::{
    Status,
    tokio::{AsyncRustlsConnector, AsyncRustlsFtpStream},
    types::FileType,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    time::timeout,
};

use crate::machine::{
    BambuPrinterEndpoint,
    compatibility::ftps_tls_1_2_cap,
    file_transfer::{
        BAMBU_FILE_TRANSFER_CHUNK_SIZE, BAMBU_FILE_TRANSFER_PORT, BAMBU_FILE_TRANSFER_USERNAME,
        MachineFileTransfer, TransferProtectionMode,
    },
    mqtt::BambuLanCertificateVerifier,
};

#[allow(dead_code)]
const DEFAULT_FTPS_TIMEOUT_SECONDS: u64 = 30;

#[derive(Debug, Clone)]
pub struct FtpsMachineFileTransfer {
    endpoint: BambuPrinterEndpoint,
}

impl FtpsMachineFileTransfer {
    pub fn new(endpoint: BambuPrinterEndpoint) -> Self {
        Self { endpoint }
    }

    pub fn endpoint(&self) -> &BambuPrinterEndpoint {
        &self.endpoint
    }

    async fn with_session<T, Fut>(
        &self,
        mode: TransferProtectionMode,
        operation: impl FnOnce(AsyncRustlsFtpStream) -> Fut,
    ) -> anyhow::Result<T>
    where
        Fut: Future<Output = anyhow::Result<T>>,
    {
        let host = self.endpoint.host.clone();
        let access_code = self.endpoint.access_code.clone();
        let profile = FtpsProfile::for_model(self.endpoint.model.as_deref());
        let timeout_host = host.clone();

        timeout(
            Duration::from_secs(DEFAULT_FTPS_TIMEOUT_SECONDS),
            async move {
                let connector = bambu_lan_ftps_connector(profile);
                let mut stream = AsyncRustlsFtpStream::connect_secure_implicit(
                    (host.as_str(), BAMBU_FILE_TRANSFER_PORT),
                    connector,
                    host.as_str(),
                )
                .await
                .with_context(|| format!("connect implicit FTPS to {host}:990"))?;

                stream
                    .login(BAMBU_FILE_TRANSFER_USERNAME, access_code.as_str())
                    .await
                    .with_context(|| format!("login to Bambu FTPS at {host} as bblp"))?;

                apply_transfer_mode(&mut stream, mode)
                    .await
                    .with_context(|| format!("set Bambu FTPS transfer mode {mode:?} for {host}"))?;

                operation(stream).await
            },
        )
        .await
        .with_context(|| format!("Bambu FTPS operation timed out for {timeout_host}"))?
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FtpsProfile {
    pub(crate) cap_tls_1_2: bool,
}

#[allow(dead_code)]
impl FtpsProfile {
    pub(crate) fn for_model(model: Option<&str>) -> Self {
        Self {
            cap_tls_1_2: ftps_tls_1_2_cap(model),
        }
    }
}

pub(crate) fn bambu_lan_ftps_tls_config(profile: FtpsProfile) -> Arc<ClientConfig> {
    let provider = rustls::crypto::aws_lc_rs::default_provider().into();
    let builder = ClientConfig::builder_with_provider(provider);
    let builder = if profile.cap_tls_1_2 {
        builder
            .with_protocol_versions(&[&version::TLS12])
            .expect("aws-lc-rs provider supports rustls TLS 1.2")
    } else {
        builder
            .with_safe_default_protocol_versions()
            .expect("aws-lc-rs provider supports rustls safe default protocol versions")
    };
    let mut config = builder
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(BambuLanCertificateVerifier))
        .with_no_client_auth();
    config.alpn_protocols = Vec::new();
    Arc::new(config)
}

#[allow(dead_code)]
fn bambu_lan_ftps_connector(profile: FtpsProfile) -> AsyncRustlsConnector {
    tokio_rustls::TlsConnector::from(bambu_lan_ftps_tls_config(profile)).into()
}

async fn apply_transfer_mode(
    stream: &mut AsyncRustlsFtpStream,
    mode: TransferProtectionMode,
) -> anyhow::Result<()> {
    stream
        .custom_command("PBSZ 0", &[Status::CommandOk])
        .await
        .context("send PBSZ 0")?;

    match mode {
        TransferProtectionMode::ProtectedData => {
            stream
                .custom_command("PROT P", &[Status::CommandOk])
                .await
                .context("send PROT P")?;
        }
        TransferProtectionMode::ClearData => {
            stream
                .custom_command("PROT C", &[Status::CommandOk])
                .await
                .context("send PROT C")?;
        }
    }

    stream
        .transfer_type(FileType::Binary)
        .await
        .context("set binary transfer type")?;

    Ok(())
}

async fn upload_in_bambu_chunks(
    stream: &mut AsyncRustlsFtpStream,
    path: &str,
    bytes: &[u8],
) -> anyhow::Result<()> {
    let mut data = stream
        .put_with_stream(path)
        .await
        .with_context(|| format!("start Bambu FTPS upload for {path}"))?;

    for chunk in bytes.chunks(BAMBU_FILE_TRANSFER_CHUNK_SIZE) {
        data.write_all(chunk)
            .await
            .map_err(suppaftp::FtpError::ConnectionError)
            .with_context(|| format!("write Bambu FTPS upload chunk for {path}"))?;
    }

    stream
        .finalize_put_stream(data)
        .await
        .with_context(|| format!("finalize Bambu FTPS upload for {path}"))?;

    Ok(())
}

#[allow(dead_code)]
pub(crate) fn bambu_lan_ftps_tls_config_for_default_profile() -> Arc<ClientConfig> {
    bambu_lan_ftps_tls_config(FtpsProfile::for_model(None))
}

#[allow(dead_code)]
#[derive(Debug)]
enum UploadVerification {
    Verified,
}

#[allow(dead_code)]
fn verify_uploaded_size(
    expected: usize,
    actual: Option<usize>,
    path: &str,
) -> anyhow::Result<UploadVerification> {
    match actual {
        Some(actual) if actual == expected => Ok(UploadVerification::Verified),
        Some(actual) => Err(anyhow!(
            "uploaded size mismatch for {path}: expected {expected} bytes, server reported {actual} bytes"
        )),
        None => Err(anyhow!(
            "uploaded size mismatch for {path}: server did not return SIZE"
        )),
    }
}

#[allow(dead_code)]
async fn suppaftp_api_compile_proof(
    host: String,
    access_code: String,
    connector: AsyncRustlsConnector,
) -> suppaftp::FtpResult<()> {
    let mut ftp = AsyncRustlsFtpStream::connect_secure_implicit(
        (host.as_str(), BAMBU_FILE_TRANSFER_PORT),
        connector,
        host.as_str(),
    )
    .await?;
    ftp.login(BAMBU_FILE_TRANSFER_USERNAME, access_code.as_str())
        .await?;
    ftp.custom_command("PBSZ 0", &[Status::CommandOk]).await?;
    ftp.custom_command("PROT P", &[Status::CommandOk]).await?;
    ftp.custom_command("PROT C", &[Status::CommandOk]).await?;
    ftp.transfer_type(FileType::Binary).await?;

    let mut data = ftp.put_with_stream("pandar-api-proof.3mf").await?;
    for chunk in b"proof".chunks(BAMBU_FILE_TRANSFER_CHUNK_SIZE) {
        data.write_all(chunk)
            .await
            .map_err(suppaftp::FtpError::ConnectionError)?;
    }
    ftp.finalize_put_stream(data).await?;
    ftp.size("pandar-api-proof.3mf").await?;
    ftp.rm("pandar-api-proof.3mf").await?;
    Ok(())
}

#[async_trait]
impl MachineFileTransfer for FtpsMachineFileTransfer {
    async fn list(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<Vec<String>> {
        let path = path.to_string();
        self.with_session(mode, |mut stream| async move {
            stream
                .nlst(Some(&path))
                .await
                .with_context(|| format!("list Bambu FTPS directory {path}"))
        })
        .await
    }

    async fn download(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<Vec<u8>> {
        let path = path.to_string();
        self.with_session(mode, |mut stream| async move {
            stream
                .retr(&path, |mut data| {
                    Box::pin(async move {
                        let mut bytes = Vec::new();
                        data.read_to_end(&mut bytes)
                            .await
                            .map_err(suppaftp::FtpError::ConnectionError)?;
                        Ok((bytes, data))
                    })
                })
                .await
                .with_context(|| format!("download Bambu FTPS file {path}"))
        })
        .await
    }

    async fn upload(
        &self,
        path: &str,
        bytes: &[u8],
        mode: TransferProtectionMode,
    ) -> anyhow::Result<()> {
        let path = path.to_string();
        let bytes = bytes.to_vec();
        let expected = bytes.len();

        self.with_session(mode, |mut stream| async move {
            upload_in_bambu_chunks(&mut stream, &path, &bytes).await?;
            let actual = stream
                .size(&path)
                .await
                .with_context(|| format!("verify Bambu FTPS file size for {path}"))?;
            verify_uploaded_size(expected, Some(actual), &path)?;
            Ok(())
        })
        .await
    }

    async fn delete(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<()> {
        let path = path.to_string();
        self.with_session(mode, |mut stream| async move {
            stream
                .rm(&path)
                .await
                .with_context(|| format!("delete Bambu FTPS file {path}"))
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_caps_tls_for_known_aliases_only() {
        assert!(!FtpsProfile::for_model(None).cap_tls_1_2);
        assert!(!FtpsProfile::for_model(Some("P1S")).cap_tls_1_2);
        assert!(FtpsProfile::for_model(Some("P2S")).cap_tls_1_2);
        assert!(FtpsProfile::for_model(Some("N7")).cap_tls_1_2);
        assert!(FtpsProfile::for_model(Some("X2D")).cap_tls_1_2);
        assert!(FtpsProfile::for_model(Some("N6")).cap_tls_1_2);
    }

    #[test]
    fn default_profile_builds_tls_config() {
        let config = bambu_lan_ftps_tls_config_for_default_profile();

        assert!(config.alpn_protocols.is_empty());
    }

    #[test]
    fn p2s_profile_builds_tls_config() {
        let config = bambu_lan_ftps_tls_config(FtpsProfile::for_model(Some("P2S")));

        assert!(config.alpn_protocols.is_empty());
    }

    #[test]
    fn upload_size_verification_accepts_exact_match() {
        assert!(matches!(
            verify_uploaded_size(42, Some(42), "Metadata/job.3mf").unwrap(),
            UploadVerification::Verified
        ));
    }

    #[test]
    fn upload_size_verification_rejects_mismatch() {
        let err = verify_uploaded_size(42, Some(41), "Metadata/job.3mf").unwrap_err();
        let message = err.to_string();

        assert!(message.contains("Metadata/job.3mf"));
        assert!(message.contains("expected 42 bytes"));
        assert!(message.contains("server reported 41 bytes"));
    }

    #[test]
    fn upload_size_verification_rejects_missing_size() {
        let err = verify_uploaded_size(42, None, "Metadata/job.3mf").unwrap_err();
        let message = err.to_string();

        assert!(message.contains("Metadata/job.3mf"));
        assert!(message.contains("server did not return SIZE"));
    }
}
