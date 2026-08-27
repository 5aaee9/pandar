use async_trait::async_trait;

pub const BAMBU_FILE_TRANSFER_PORT: u16 = 990;
pub const BAMBU_FILE_TRANSFER_USERNAME: &str = "bblp";
pub const BAMBU_FILE_TRANSFER_CHUNK_SIZE: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileTransferOperation {
    List,
    Download,
    Upload {
        size_bytes: u64,
    },
    PrintUpload {
        size_bytes: u64,
        try_emmc_print: bool,
    },
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrintUploadPolicy {
    pub try_emmc_print: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTransferRequest {
    pub operation: FileTransferOperation,
    pub path: String,
}

impl FileTransferRequest {
    fn new(operation: FileTransferOperation, path: impl Into<String>) -> Self {
        Self {
            operation,
            path: path.into(),
        }
    }

    pub fn list(path: impl Into<String>) -> Self {
        Self::new(FileTransferOperation::List, path)
    }

    pub fn download(path: impl Into<String>) -> Self {
        Self::new(FileTransferOperation::Download, path)
    }

    pub fn upload(path: impl Into<String>, size_bytes: u64) -> Self {
        Self::new(FileTransferOperation::Upload { size_bytes }, path)
    }

    pub fn print_upload(
        path: impl Into<String>,
        size_bytes: u64,
        policy: PrintUploadPolicy,
    ) -> Self {
        Self::new(
            FileTransferOperation::PrintUpload {
                size_bytes,
                try_emmc_print: policy.try_emmc_print,
            },
            path,
        )
    }

    pub fn delete(path: impl Into<String>) -> Self {
        Self::new(FileTransferOperation::Delete, path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileUploadResult {
    pub path: String,
    pub url: String,
}

impl FileUploadResult {
    pub fn ftp(path: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            url: format!("ftp://{}", path.trim_start_matches('/')),
            path,
        }
    }

    pub fn brtc_emmc(path: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            url: format!("brtc://emmc/{}", path.trim_start_matches('/')),
            path,
        }
    }
}

#[async_trait]
pub trait MachineFileTransfer: Send + Sync {
    async fn list(&self, path: &str) -> anyhow::Result<Vec<String>>;
    async fn download(&self, path: &str) -> anyhow::Result<Vec<u8>>;
    async fn upload(&self, path: &str, bytes: &[u8]) -> anyhow::Result<FileUploadResult>;
    async fn upload_print(
        &self,
        path: &str,
        bytes: &[u8],
        policy: PrintUploadPolicy,
    ) -> anyhow::Result<FileUploadResult>;
    async fn delete(&self, path: &str) -> anyhow::Result<()>;
}

#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub struct FakeMachineFileTransfer {
    state: std::sync::Arc<std::sync::Mutex<FakeMachineFileTransferState>>,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct FakeMachineFileTransferState {
    recorded: Vec<FileTransferRequest>,
    fail: bool,
}

#[cfg(test)]
impl FakeMachineFileTransfer {
    pub fn with_failure() -> Self {
        let state = FakeMachineFileTransferState {
            fail: true,
            ..Default::default()
        };
        Self {
            state: std::sync::Arc::new(std::sync::Mutex::new(state)),
        }
    }

    pub(crate) fn recorded_requests(&self) -> Vec<FileTransferRequest> {
        self.state.lock().unwrap().recorded.clone()
    }

    fn record(&self, request: FileTransferRequest) -> anyhow::Result<()> {
        let mut state = self.state.lock().unwrap();
        state.recorded.push(request);
        if state.fail {
            Err(anyhow::anyhow!("fake protected data transfer failure"))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
#[async_trait]
impl MachineFileTransfer for FakeMachineFileTransfer {
    async fn list(&self, path: &str) -> anyhow::Result<Vec<String>> {
        self.record(FileTransferRequest::list(path))?;
        Ok(vec!["ok".to_string()])
    }

    async fn download(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        self.record(FileTransferRequest::download(path))?;
        Ok(Vec::new())
    }

    async fn upload(&self, path: &str, bytes: &[u8]) -> anyhow::Result<FileUploadResult> {
        self.record(FileTransferRequest::upload(path, bytes.len() as u64))?;
        Ok(FileUploadResult::ftp(path))
    }

    async fn upload_print(
        &self,
        path: &str,
        bytes: &[u8],
        policy: PrintUploadPolicy,
    ) -> anyhow::Result<FileUploadResult> {
        self.record(FileTransferRequest::print_upload(
            path,
            bytes.len() as u64,
            policy,
        ))?;
        Ok(FileUploadResult::ftp(path))
    }

    async fn delete(&self, path: &str) -> anyhow::Result<()> {
        self.record(FileTransferRequest::delete(path))
    }
}

#[cfg(test)]
mod tests;
