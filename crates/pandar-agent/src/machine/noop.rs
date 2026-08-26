use anyhow::bail;
use async_trait::async_trait;

use crate::machine::{
    BambuMachineGateway, MaterialRefreshResult, PrintProjectDispatchResult, PrinterDiscoveryResult,
    PrinterOperation, PrinterOperationDispatchResult, PrinterRefreshResult,
    diagnostics::{DiagnosticCheck, DiagnosticStatus, PrinterDiagnosticResult},
    discovery,
};
use pandar_protocol::agent::v1::PrintProjectFile;

#[derive(Debug, Clone, Copy, Default)]
pub struct NoopMachineGateway;

#[async_trait]
impl BambuMachineGateway for NoopMachineGateway {
    fn redact_error(&self, message: &str) -> String {
        message.to_owned()
    }

    async fn discover_printers(
        &self,
        timeout_seconds: u32,
    ) -> anyhow::Result<PrinterDiscoveryResult> {
        discovery::discover_printers(timeout_seconds).await
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
            overall: DiagnosticStatus::Problem,
            checks: vec![DiagnosticCheck {
                id: "configured_printer",
                status: DiagnosticStatus::Problem,
                message: "No configured printer matches the requested serial number.".to_owned(),
                details: None,
            }],
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
        bail!("no Bambu printer configured for serial {serial_number}")
    }

    async fn validate_printer(&self, serial_number: &str) -> anyhow::Result<()> {
        bail!("no Bambu printer configured for serial {serial_number}")
    }

    async fn print_project_file(
        &self,
        serial_number: &str,
        _command: &PrintProjectFile,
        _artifact: Vec<u8>,
    ) -> anyhow::Result<PrintProjectDispatchResult> {
        bail!("no Bambu printer configured for serial {serial_number}")
    }

    async fn operate_printer(
        &self,
        serial_number: &str,
        _operation: PrinterOperation,
    ) -> anyhow::Result<PrinterOperationDispatchResult> {
        bail!("no Bambu printer configured for serial {serial_number}")
    }
}
