use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
};

use anyhow::{Context, anyhow};
use async_trait::async_trait;

use crate::machine::BambuPrinterEndpoint;

pub const BAMBU_FILE_TRANSFER_PORT: u16 = 990;
pub const BAMBU_FILE_TRANSFER_USERNAME: &str = "bblp";
pub const BAMBU_FILE_TRANSFER_CHUNK_SIZE: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferProtectionMode {
    ProtectedData,
    ClearData,
}

impl TransferProtectionMode {
    fn failure_context(self) -> &'static str {
        match self {
            Self::ProtectedData => "protected data transfer failed",
            Self::ClearData => "clear data transfer failed",
        }
    }
}

pub fn is_a1_model(model: Option<&str>) -> bool {
    model.is_some_and(|model| {
        let model = model.to_ascii_lowercase();
        model == "a1" || model.contains("a1 mini")
    })
}

#[derive(Debug, Clone, Default)]
pub struct TransferModeCache {
    modes: Arc<Mutex<HashMap<String, TransferProtectionMode>>>,
}

impl TransferModeCache {
    pub fn get(&self, endpoint_key: &str) -> Option<TransferProtectionMode> {
        self.modes.lock().unwrap().get(endpoint_key).copied()
    }

    pub fn store_success(&self, endpoint_key: &str, mode: TransferProtectionMode) {
        self.modes
            .lock()
            .unwrap()
            .insert(endpoint_key.to_string(), mode);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileTransferOperation {
    List,
    Download,
    Upload { size_bytes: u64 },
    Delete,
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

    pub fn delete(path: impl Into<String>) -> Self {
        Self::new(FileTransferOperation::Delete, path)
    }
}

#[async_trait]
pub trait MachineFileTransfer: Send + Sync {
    async fn list(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<Vec<String>>;
    async fn download(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<Vec<u8>>;
    async fn upload(
        &self,
        path: &str,
        bytes: &[u8],
        mode: TransferProtectionMode,
    ) -> anyhow::Result<()>;
    async fn delete(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<()>;
}

pub fn transfer_attempt_order(
    endpoint: &BambuPrinterEndpoint,
    cache: &TransferModeCache,
    force_clear: bool,
) -> Vec<TransferProtectionMode> {
    if let Some(mode) = cache.get(&endpoint.host) {
        return vec![mode];
    }

    if force_clear {
        return vec![TransferProtectionMode::ClearData];
    }

    if is_a1_model(endpoint.model.as_deref()) {
        vec![
            TransferProtectionMode::ProtectedData,
            TransferProtectionMode::ClearData,
        ]
    } else {
        vec![TransferProtectionMode::ProtectedData]
    }
}

pub async fn run_with_transfer_mode<F, Fut, T>(
    endpoint: &BambuPrinterEndpoint,
    cache: &TransferModeCache,
    force_clear: bool,
    mut operation: F,
) -> anyhow::Result<T>
where
    F: FnMut(TransferProtectionMode) -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let modes = transfer_attempt_order(endpoint, cache, force_clear);
    let mut failures = Vec::new();

    for mode in modes {
        match operation(mode)
            .await
            .with_context(|| mode.failure_context())
        {
            Ok(result) => {
                cache.store_success(&endpoint.host, mode);
                return Ok(result);
            }
            Err(err) => failures.push(err),
        }
    }

    let message = failures
        .iter()
        .map(|err| format!("{err:#}"))
        .collect::<Vec<_>>()
        .join("; ");
    Err(anyhow!(
        "all transfer modes failed for {}: {message}",
        endpoint.host
    ))
}

#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub struct FakeMachineFileTransfer {
    state: Arc<Mutex<FakeMachineFileTransferState>>,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct FakeMachineFileTransferState {
    recorded: Vec<(TransferProtectionMode, FileTransferRequest)>,
    fail_protected: bool,
    fail_clear: bool,
}

#[cfg(test)]
impl FakeMachineFileTransfer {
    pub fn with_failures(fail_protected: bool, fail_clear: bool) -> Self {
        let state = FakeMachineFileTransferState {
            fail_protected,
            fail_clear,
            ..Default::default()
        };
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }

    fn recorded_requests(&self) -> Vec<(TransferProtectionMode, FileTransferRequest)> {
        self.state.lock().unwrap().recorded.clone()
    }

    fn recorded_modes(&self) -> Vec<TransferProtectionMode> {
        self.recorded_requests()
            .iter()
            .map(|(mode, _)| *mode)
            .collect()
    }

    fn record(
        &self,
        mode: TransferProtectionMode,
        request: FileTransferRequest,
    ) -> anyhow::Result<()> {
        let mut state = self.state.lock().unwrap();
        state.recorded.push((mode, request));
        match mode {
            TransferProtectionMode::ProtectedData if state.fail_protected => {
                Err(anyhow!("fake protected data failure"))
            }
            TransferProtectionMode::ClearData if state.fail_clear => {
                Err(anyhow!("fake clear data failure"))
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
#[async_trait]
impl MachineFileTransfer for FakeMachineFileTransfer {
    async fn list(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<Vec<String>> {
        self.record(mode, FileTransferRequest::list(path))?;
        Ok(vec!["ok".to_string()])
    }

    async fn download(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<Vec<u8>> {
        self.record(mode, FileTransferRequest::download(path))?;
        Ok(Vec::new())
    }

    async fn upload(
        &self,
        path: &str,
        bytes: &[u8],
        mode: TransferProtectionMode,
    ) -> anyhow::Result<()> {
        self.record(mode, FileTransferRequest::upload(path, bytes.len() as u64))
    }

    async fn delete(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<()> {
        self.record(mode, FileTransferRequest::delete(path))
    }
}

#[cfg(test)]
mod tests;
