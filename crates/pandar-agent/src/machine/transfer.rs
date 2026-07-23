use anyhow::Context;
use async_trait::async_trait;
use pandar_core::compatibility::brtc_emmc_upload_supported;

use crate::machine::{
    BambuPrinterEndpoint,
    brtc::BrtcMachineFileTransfer,
    file_transfer::{FileUploadResult, MachineFileTransfer, PrintUploadPolicy},
    ftps::FtpsMachineFileTransfer,
};

use super::file_transfer::TransferProtectionMode;

const GENERIC_UPLOAD_POLICY: PrintUploadPolicy = PrintUploadPolicy {
    try_emmc_print: true,
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

    fn should_try_brtc_upload(&self, path: &str, policy: PrintUploadPolicy) -> bool {
        if !policy.try_emmc_print || !path.ends_with(".gcode.3mf") {
            return false;
        }
        brtc_emmc_upload_supported(self.endpoint.model.as_deref())
    }

    async fn upload_with_policy(
        &self,
        path: &str,
        bytes: &[u8],
        mode: TransferProtectionMode,
        policy: PrintUploadPolicy,
    ) -> anyhow::Result<FileUploadResult> {
        if self.should_try_brtc_upload(path, policy) {
            match self.brtc.upload_emmc(path, bytes).await {
                Ok(_) => return Ok(FileUploadResult::brtc_emmc(path)),
                Err(brtc_error) => {
                    return self.ftps.upload(path, bytes, mode).await.with_context(|| {
                        format!("BRTC upload failed before FTPS fallback: {brtc_error:#}")
                    });
                }
            }
        }
        self.ftps.upload(path, bytes, mode).await
    }
}

#[async_trait]
impl MachineFileTransfer for BambuMachineFileTransfer {
    async fn list(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<Vec<String>> {
        self.ftps.list(path, mode).await
    }

    async fn download(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<Vec<u8>> {
        self.ftps.download(path, mode).await
    }

    async fn upload(
        &self,
        path: &str,
        bytes: &[u8],
        mode: TransferProtectionMode,
    ) -> anyhow::Result<FileUploadResult> {
        self.upload_with_policy(path, bytes, mode, GENERIC_UPLOAD_POLICY)
            .await
    }

    async fn upload_print(
        &self,
        path: &str,
        bytes: &[u8],
        mode: TransferProtectionMode,
        policy: PrintUploadPolicy,
    ) -> anyhow::Result<FileUploadResult> {
        self.upload_with_policy(path, bytes, mode, policy).await
    }

    async fn delete(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<()> {
        self.ftps.delete(path, mode).await
    }
}

#[cfg(test)]
mod tests;
