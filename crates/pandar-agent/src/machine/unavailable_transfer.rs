use anyhow::bail;
use async_trait::async_trait;

use super::file_transfer::{FileUploadResult, MachineFileTransfer, PrintUploadPolicy};

#[derive(Debug, Clone, Copy, Default)]
pub struct UnavailableMachineFileTransfer;

#[async_trait]
impl MachineFileTransfer for UnavailableMachineFileTransfer {
    async fn list(&self, _path: &str) -> anyhow::Result<Vec<String>> {
        bail!("Bambu FTPS runtime is not implemented in this phase")
    }

    async fn download(&self, _path: &str) -> anyhow::Result<Vec<u8>> {
        bail!("Bambu FTPS runtime is not implemented in this phase")
    }

    async fn upload(&self, _path: &str, _bytes: &[u8]) -> anyhow::Result<FileUploadResult> {
        bail!("Bambu FTPS runtime is not implemented in this phase")
    }

    async fn upload_print(
        &self,
        _path: &str,
        _bytes: &[u8],
        _policy: PrintUploadPolicy,
    ) -> anyhow::Result<FileUploadResult> {
        bail!("Bambu print transfer runtime is not implemented in this phase")
    }

    async fn delete(&self, _path: &str) -> anyhow::Result<()> {
        bail!("Bambu FTPS runtime is not implemented in this phase")
    }
}
