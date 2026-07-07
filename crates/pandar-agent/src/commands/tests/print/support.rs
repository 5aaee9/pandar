use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::Mutex;

use crate::{
    commands::ArtifactReader,
    machine::{
        BambuMachineGateway, MaterialRefreshResult, PrintProjectDispatchResult,
        PrinterRefreshResult, diagnostics::PrinterDiagnosticResult,
        discovery::PrinterDiscoveryResult,
    },
    protocol::agent::v1::PrintProjectFile,
};

#[derive(Debug, Clone, Default)]
pub(super) struct FakeArtifactReader {
    artifacts: Arc<HashMap<String, Vec<u8>>>,
    pub(super) reads: Arc<Mutex<Vec<String>>>,
}

impl FakeArtifactReader {
    pub(super) fn with_artifacts(
        artifacts: impl IntoIterator<Item = (&'static str, Vec<u8>)>,
    ) -> Self {
        Self {
            artifacts: Arc::new(
                artifacts
                    .into_iter()
                    .map(|(path, bytes)| (path.to_string(), bytes))
                    .collect(),
            ),
            reads: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl ArtifactReader for FakeArtifactReader {
    async fn read_artifact(&self, storage_path: &str) -> anyhow::Result<Vec<u8>> {
        self.reads.lock().await.push(storage_path.to_string());
        crate::commands::resolve_artifact_path(std::path::Path::new("."), storage_path)?;
        self.artifacts
            .get(storage_path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("fake artifact missing at {storage_path}"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RecordedPrint {
    pub(super) serial_number: String,
    pub(super) job_id: String,
    pub(super) artifact: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(super) struct FakePrintGateway {
    pub(super) prints: Arc<Mutex<Vec<RecordedPrint>>>,
    valid_serials: Vec<String>,
}

impl FakePrintGateway {
    pub(super) fn ok(serials: impl IntoIterator<Item = &'static str>) -> Self {
        Self {
            prints: Arc::new(Mutex::new(Vec::new())),
            valid_serials: serials.into_iter().map(str::to_string).collect(),
        }
    }
}

#[async_trait]
impl BambuMachineGateway for FakePrintGateway {
    fn redact_error(&self, message: &str) -> String {
        message.to_owned()
    }

    async fn discover_printers(
        &self,
        _timeout_seconds: u32,
    ) -> anyhow::Result<PrinterDiscoveryResult> {
        Ok(PrinterDiscoveryResult::new(Vec::new()))
    }

    async fn diagnose_printer(
        &self,
        serial_number: &str,
    ) -> anyhow::Result<PrinterDiagnosticResult> {
        Ok(PrinterDiagnosticResult {
            result_type: "printer_diagnostic",
            serial_number: serial_number.to_owned(),
            host: None,
            model: None,
            overall: crate::machine::diagnostics::DiagnosticStatus::Problem,
            checks: Vec::new(),
            compatibility: None,
        })
    }

    async fn refresh_printers(&self) -> anyhow::Result<Vec<PrinterRefreshResult>> {
        Ok(Vec::new())
    }

    async fn refresh_printer_materials(
        &self,
        serial_number: &str,
        _printer_id: Option<&str>,
    ) -> anyhow::Result<MaterialRefreshResult> {
        anyhow::bail!("no configured Bambu printer matches serial {serial_number}")
    }

    async fn validate_printer(&self, serial_number: &str) -> anyhow::Result<()> {
        if self
            .valid_serials
            .iter()
            .any(|serial| serial == serial_number)
        {
            return Ok(());
        }

        anyhow::bail!("no configured Bambu printer matches serial {serial_number}")
    }

    async fn print_project_file(
        &self,
        serial_number: &str,
        command: &PrintProjectFile,
        artifact: Vec<u8>,
    ) -> anyhow::Result<PrintProjectDispatchResult> {
        self.prints.lock().await.push(RecordedPrint {
            serial_number: serial_number.to_string(),
            job_id: command.job_id.clone(),
            artifact,
        });
        Ok(PrintProjectDispatchResult {
            topic: format!("device/{serial_number}/request"),
            payload: serde_json::to_value(TestProjectFilePayload {
                print: TestProjectFileCommand {
                    command: "project_file",
                },
            })
            .unwrap(),
            qos: 0,
            uploaded_path: "plate.gcode.3mf".to_string(),
            uploaded_url: "ftp://plate.gcode.3mf".to_string(),
            md5: "900150983CD24FB0D6963F7D28E17F72".to_string(),
        })
    }
}

#[derive(Serialize)]
struct TestProjectFilePayload {
    print: TestProjectFileCommand,
}

#[derive(Serialize)]
struct TestProjectFileCommand {
    command: &'static str,
}
