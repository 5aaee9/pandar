use anyhow::Context;
use async_trait::async_trait;
use pandar_core::compatibility::brtc_emmc_upload_supported;

use crate::machine::{
    BambuPrinterEndpoint,
    brtc::BrtcMachineFileTransfer,
    file_transfer::{FileUploadResult, MachineFileTransfer},
    ftps::FtpsMachineFileTransfer,
};

use super::file_transfer::TransferProtectionMode;

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

    fn should_try_brtc_upload(&self, path: &str) -> bool {
        if !path.ends_with(".gcode.3mf") {
            return false;
        }
        brtc_emmc_upload_supported(self.endpoint.model.as_deref())
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
        if self.should_try_brtc_upload(path) {
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

    async fn delete(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<()> {
        self.ftps.delete(path, mode).await
    }
}
