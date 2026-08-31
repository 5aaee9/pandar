use anyhow::Context;
use async_trait::async_trait;

use crate::machine::{
    BambuPrinterEndpoint,
    brtc::BrtcMachineFileTransfer,
    file_transfer::{FileUploadResult, MachineFileTransfer, PrintUploadPolicy},
    ftps::FtpsMachineFileTransfer,
};

#[derive(Debug, Clone)]
pub struct BambuMachineFileTransfer {
    endpoint: BambuPrinterEndpoint,
    ftps: FtpsMachineFileTransfer,
    brtc: BrtcMachineFileTransfer,
}

impl BambuMachineFileTransfer {
    pub fn new(endpoint: BambuPrinterEndpoint) -> Self {
        Self {
            ftps: FtpsMachineFileTransfer::new(endpoint.clone()),
            brtc: BrtcMachineFileTransfer::new(endpoint.clone()),
            endpoint,
        }
    }

    /// Printable model uploads go to the machine's eMMC over BRTC first. There
    /// is deliberately no model allowlist: the transport probe is self-selecting
    /// because machines without the tunnel fail fast, and any BRTC failure
    /// degrades to protected FTPS while keeping the BRTC cause visible.
    async fn upload_with_brtc_preference(
        &self,
        path: &str,
        bytes: &[u8],
    ) -> anyhow::Result<FileUploadResult> {
        if path.ends_with(".gcode.3mf") {
            match self.brtc.upload_emmc(path, bytes).await {
                Ok(_) => return Ok(FileUploadResult::brtc_emmc(path)),
                Err(brtc_error) => {
                    tracing::warn!(
                        host = %self.endpoint.host,
                        "BRTC eMMC upload failed, falling back to FTPS: {brtc_error:#}"
                    );
                    return self.ftps.upload(path, bytes).await.with_context(|| {
                        format!("BRTC upload failed before FTPS fallback: {brtc_error:#}")
                    });
                }
            }
        }
        self.ftps.upload(path, bytes).await
    }
}

#[async_trait]
impl MachineFileTransfer for BambuMachineFileTransfer {
    async fn list(&self, path: &str) -> anyhow::Result<Vec<String>> {
        self.ftps.list(path).await
    }

    async fn download(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        self.ftps.download(path).await
    }

    async fn upload(&self, path: &str, bytes: &[u8]) -> anyhow::Result<FileUploadResult> {
        self.upload_with_brtc_preference(path, bytes).await
    }

    async fn upload_print(
        &self,
        path: &str,
        bytes: &[u8],
        _policy: PrintUploadPolicy,
    ) -> anyhow::Result<FileUploadResult> {
        self.upload_with_brtc_preference(path, bytes).await
    }

    async fn delete(&self, path: &str) -> anyhow::Result<()> {
        self.ftps.delete(path).await
    }
}

#[cfg(test)]
mod tests;
