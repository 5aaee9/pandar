use std::{sync::Arc, time::Duration};

use anyhow::{Context, anyhow, bail};
use async_trait::async_trait;
use pandar_core::PrintTransferPhase;
use rustls::{
    ClientConfig,
    client::{Resumption, Tls12Resumption, danger::ServerCertVerifier},
    version,
};
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
        FileUploadResult, MachineFileTransfer, PrintUploadPolicy,
    },
    mqtt::BambuLanCertificateVerifier,
};

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
                let connector = bambu_lan_ftps_connector(profile, &self.endpoint.serial);
                let mut stream = AsyncRustlsFtpStream::connect_secure_implicit(
                    (host.as_str(), BAMBU_FILE_TRANSFER_PORT),
                    connector,
                    host.as_str(),
                )
                .await
                .context(PrintTransferPhase::Connect)
                .with_context(|| format!("connect implicit FTPS to {host}:990"))?;

                stream
                    .login(BAMBU_FILE_TRANSFER_USERNAME, access_code.as_str())
                    .await
                    .context(PrintTransferPhase::Login)
                    .with_context(|| format!("login to Bambu FTPS at {host} as bblp"))?;

                protect_data_channel(&mut stream)
                    .await
                    .context(PrintTransferPhase::Protection)
                    .with_context(|| format!("protect Bambu FTPS data channel for {host}"))?;

                operation(stream).await
            },
        )
        .await
        .context(PrintTransferPhase::Timeout)
        .with_context(|| format!("Bambu FTPS operation timed out for {timeout_host}"))?
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FtpsProfile {
    pub(crate) cap_tls_1_2: bool,
}

impl FtpsProfile {
    pub(crate) fn for_model(model: Option<&str>) -> Self {
        Self {
            cap_tls_1_2: ftps_tls_1_2_cap(model),
        }
    }
}

pub(crate) fn bambu_lan_ftps_tls_config(
    profile: FtpsProfile,
    expected_serial: &str,
) -> Arc<ClientConfig> {
    ftps_tls_config(
        profile,
        Arc::new(BambuLanCertificateVerifier::new(expected_serial)),
    )
}

fn ftps_tls_config(
    profile: FtpsProfile,
    verifier: Arc<dyn ServerCertVerifier>,
) -> Arc<ClientConfig> {
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
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    if profile.cap_tls_1_2 {
        config.resumption =
            Resumption::in_memory_sessions(256).tls12_resumption(Tls12Resumption::SessionIdOnly);
    }
    config.alpn_protocols = Vec::new();
    Arc::new(config)
}

fn bambu_lan_ftps_connector(profile: FtpsProfile, expected_serial: &str) -> AsyncRustlsConnector {
    tokio_rustls::TlsConnector::from(bambu_lan_ftps_tls_config(profile, expected_serial)).into()
}

async fn protect_data_channel(stream: &mut AsyncRustlsFtpStream) -> anyhow::Result<()> {
    stream
        .custom_command("PBSZ 0", &[Status::CommandOk])
        .await
        .context("send PBSZ 0")?;

    stream
        .custom_command("PROT P", &[Status::CommandOk])
        .await
        .context("send PROT P")?;

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
        .context(PrintTransferPhase::DataConnection)
        .with_context(|| format!("start Bambu FTPS upload for {path}"))?;

    for chunk in bytes.chunks(BAMBU_FILE_TRANSFER_CHUNK_SIZE) {
        data.write_all(chunk)
            .await
            .map_err(suppaftp::FtpError::ConnectionError)
            .context(PrintTransferPhase::Write)
            .with_context(|| format!("write Bambu FTPS upload chunk for {path}"))?;
    }

    stream
        .finalize_put_stream(data)
        .await
        .context(PrintTransferPhase::Finalize)
        .with_context(|| format!("finalize Bambu FTPS upload for {path}"))?;

    Ok(())
}

#[cfg(test)]
pub(crate) fn bambu_lan_ftps_tls_config_for_default_profile() -> Arc<ClientConfig> {
    bambu_lan_ftps_tls_config(FtpsProfile::for_model(None), "test-printer")
}

#[derive(Debug)]
enum UploadVerification {
    Verified,
}

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

#[async_trait]
impl MachineFileTransfer for FtpsMachineFileTransfer {
    async fn list(&self, path: &str) -> anyhow::Result<Vec<String>> {
        let path = path.to_string();
        self.with_session(|mut stream| async move {
            stream
                .nlst(Some(&path))
                .await
                .with_context(|| format!("list Bambu FTPS directory {path}"))
        })
        .await
    }

    async fn download(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        let path = path.to_string();
        self.with_session(|mut stream| async move {
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

    async fn upload(&self, path: &str, bytes: &[u8]) -> anyhow::Result<FileUploadResult> {
        let path = path.to_string();
        let bytes = bytes.to_vec();
        let expected = bytes.len();

        self.with_session(|mut stream| async move {
            upload_in_bambu_chunks(&mut stream, &path, &bytes).await?;
            let actual = stream
                .size(&path)
                .await
                .context(PrintTransferPhase::Verify)
                .with_context(|| format!("verify Bambu FTPS file size for {path}"))?;
            verify_uploaded_size(expected, Some(actual), &path)
                .context(PrintTransferPhase::Verify)?;
            Ok(FileUploadResult::ftp(path))
        })
        .await
    }

    async fn upload_print(
        &self,
        path: &str,
        bytes: &[u8],
        policy: PrintUploadPolicy,
    ) -> anyhow::Result<FileUploadResult> {
        if policy.try_emmc_print {
            bail!("FTPS-only transfer cannot honor try_emmc_print");
        }
        self.upload(path, bytes).await
    }

    async fn delete(&self, path: &str) -> anyhow::Result<()> {
        let path = path.to_string();
        self.with_session(|mut stream| async move {
            stream
                .rm(&path)
                .await
                .with_context(|| format!("delete Bambu FTPS file {path}"))
        })
        .await
    }
}

#[cfg(test)]
mod tests;
