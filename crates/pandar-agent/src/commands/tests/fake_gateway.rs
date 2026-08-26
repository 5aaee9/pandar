use super::*;

impl FakeGateway {
    pub(super) fn ok(snapshots: impl IntoIterator<Item = MachineSnapshot>) -> Self {
        Self {
            result: Arc::new(Mutex::new(Ok(snapshots
                .into_iter()
                .map(|snapshot| PrinterRefreshResult {
                    snapshot,
                    materials: None,
                })
                .collect()))),
            material_result: Arc::new(Mutex::new(Err(anyhow::anyhow!(
                "unexpected material refresh"
            )))),
            access_code: None,
        }
    }

    pub(super) fn ok_with_materials(
        results: impl IntoIterator<Item = PrinterRefreshResult>,
    ) -> Self {
        let results = results.into_iter().collect::<Vec<_>>();
        let material_result = results
            .iter()
            .find_map(|result| result.materials.clone())
            .ok_or_else(|| anyhow::anyhow!("unexpected material refresh"));
        Self {
            result: Arc::new(Mutex::new(Ok(results))),
            material_result: Arc::new(Mutex::new(material_result)),
            access_code: None,
        }
    }

    pub(super) fn fail() -> Self {
        Self {
            result: Arc::new(Mutex::new(
                Err(anyhow::anyhow!("transport unavailable")).context("refresh failed"),
            )),
            material_result: Arc::new(Mutex::new(Err(anyhow::anyhow!(
                "unexpected material refresh"
            )))),
            access_code: None,
        }
    }

    pub(super) fn fail_with_access_code(access_code: &str) -> Self {
        Self {
            result: Arc::new(Mutex::new(
                Err(anyhow::anyhow!("bad access code {access_code}")).context("refresh failed"),
            )),
            material_result: Arc::new(Mutex::new(Err(anyhow::anyhow!(
                "unexpected material refresh"
            )))),
            access_code: Some(access_code.to_owned()),
        }
    }

    pub(super) fn material_fail_with_access_code(access_code: &str, error: anyhow::Error) -> Self {
        Self {
            result: Arc::new(Mutex::new(Ok(Vec::new()))),
            material_result: Arc::new(Mutex::new(Err(error))),
            access_code: Some(access_code.to_owned()),
        }
    }
}

#[async_trait]
impl BambuMachineGateway for FakeGateway {
    fn redact_error(&self, message: &str) -> String {
        match &self.access_code {
            Some(access_code) => message.replace(access_code, "[REDACTED_ACCESS_CODE]"),
            None => message.to_owned(),
        }
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
        let mut result = self.result.lock().await;
        std::mem::replace(&mut *result, Ok(Vec::new()))
    }

    async fn refresh_printer_materials(
        &self,
        _serial_number: &str,
        _printer_id: Option<&str>,
    ) -> anyhow::Result<MaterialRefreshResult> {
        let mut result = self.material_result.lock().await;
        std::mem::replace(
            &mut *result,
            Err(anyhow::anyhow!("unexpected material refresh")),
        )
    }

    async fn validate_printer(&self, _serial_number: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn print_project_file(
        &self,
        _serial_number: &str,
        _command: &pandar_protocol::agent::v1::PrintProjectFile,
        _artifact: Vec<u8>,
    ) -> anyhow::Result<PrintProjectDispatchResult> {
        unreachable!("refresh tests do not dispatch print commands")
    }

    async fn operate_printer(
        &self,
        _serial_number: &str,
        _operation: MachinePrinterOperation,
    ) -> anyhow::Result<crate::machine::PrinterOperationDispatchResult> {
        unreachable!("refresh tests do not dispatch printer operation commands")
    }
}
