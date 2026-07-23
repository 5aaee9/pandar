use super::*;

#[derive(Debug, Clone)]
pub(super) struct LinkGateway {
    discovery: Arc<Mutex<anyhow::Result<PrinterDiscoveryResult>>>,
    direct_host_discovery: Arc<Mutex<anyhow::Result<Option<DiscoveredPrinter>>>>,
    result: Arc<Mutex<anyhow::Result<MachineSnapshot>>>,
    linked_endpoints: Arc<Mutex<Vec<BambuPrinterEndpoint>>>,
    firmware: FirmwareObservationCache,
    emit_firmware: bool,
    access_code: Option<String>,
}

impl LinkGateway {
    pub(super) fn success(snapshot: MachineSnapshot) -> Self {
        Self {
            discovery: Arc::new(Mutex::new(Ok(PrinterDiscoveryResult::new(vec![
                discovered_printer("192.0.2.10", Some("SERIAL123"), Some("X1 Carbon")),
            ])))),
            direct_host_discovery: Arc::new(Mutex::new(Ok(None))),
            result: Arc::new(Mutex::new(Ok(snapshot))),
            linked_endpoints: Arc::new(Mutex::new(Vec::new())),
            firmware: FirmwareObservationCache::default(),
            emit_firmware: false,
            access_code: None,
        }
    }

    pub(super) fn success_with_firmware(snapshot: MachineSnapshot) -> Self {
        Self {
            emit_firmware: true,
            ..Self::success(snapshot)
        }
    }

    pub(super) fn discovery_result(printers: Vec<DiscoveredPrinter>) -> Self {
        Self {
            discovery: Arc::new(Mutex::new(Ok(PrinterDiscoveryResult::new(printers)))),
            direct_host_discovery: Arc::new(Mutex::new(Ok(None))),
            result: Arc::new(Mutex::new(Ok(snapshot(
                "SERIAL123",
                "Office X1C",
                Some("X1 Carbon"),
                "READY",
            )))),
            linked_endpoints: Arc::new(Mutex::new(Vec::new())),
            firmware: FirmwareObservationCache::default(),
            emit_firmware: false,
            access_code: None,
        }
    }

    pub(super) fn discovery_result_with_direct_host(
        printers: Vec<DiscoveredPrinter>,
        direct_host: Option<DiscoveredPrinter>,
    ) -> Self {
        Self {
            discovery: Arc::new(Mutex::new(Ok(PrinterDiscoveryResult::new(printers)))),
            direct_host_discovery: Arc::new(Mutex::new(Ok(direct_host))),
            result: Arc::new(Mutex::new(Ok(snapshot(
                "SERIAL123",
                "Office X1C",
                Some("X1 Carbon"),
                "READY",
            )))),
            linked_endpoints: Arc::new(Mutex::new(Vec::new())),
            firmware: FirmwareObservationCache::default(),
            emit_firmware: false,
            access_code: None,
        }
    }

    pub(super) fn failure(access_code: &str) -> Self {
        Self {
            discovery: Arc::new(Mutex::new(Ok(PrinterDiscoveryResult::new(vec![
                discovered_printer("192.0.2.10", Some("SERIAL123"), Some("X1 Carbon")),
            ])))),
            direct_host_discovery: Arc::new(Mutex::new(Ok(None))),
            result: Arc::new(Mutex::new(
                Err(anyhow::anyhow!("bad access code {access_code}"))
                    .context("validate runtime printer SERIAL123"),
            )),
            linked_endpoints: Arc::new(Mutex::new(Vec::new())),
            firmware: FirmwareObservationCache::default(),
            emit_firmware: false,
            access_code: Some(access_code.to_owned()),
        }
    }

    pub(super) async fn linked_endpoints(&self) -> Vec<BambuPrinterEndpoint> {
        self.linked_endpoints.lock().await.clone()
    }
}

pub(super) fn discovered_printer(
    host: &str,
    serial: Option<&str>,
    model: Option<&str>,
) -> DiscoveredPrinter {
    DiscoveredPrinter {
        serial_number: serial.map(str::to_owned),
        host: host.to_owned(),
        name: Some("Discovered Office X1C".to_owned()),
        model: model.map(str::to_owned),
        source: "ssdp",
    }
}

pub(super) fn link_printer_command_with_type(
    command_id: String,
    access_code: &str,
    printer_type: &str,
) -> HubCommand {
    HubCommand {
        command_id,
        command: Some(hub_command::Command::LinkPrinter(LinkPrinter {
            host: "192.0.2.10".to_owned(),
            access_code: access_code.to_owned(),
            name: "Office X1C".to_owned(),
            printer_type: printer_type.to_owned(),
        })),
    }
}

#[async_trait]
impl BambuMachineGateway for LinkGateway {
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
        let mut discovery = self.discovery.lock().await;
        std::mem::replace(&mut *discovery, Ok(PrinterDiscoveryResult::new(Vec::new())))
    }

    async fn discover_printer_at_host(
        &self,
        host: &str,
        _timeout_seconds: u32,
    ) -> anyhow::Result<Option<DiscoveredPrinter>> {
        assert_eq!(host, "192.0.2.10");
        let mut direct_host_discovery = self.direct_host_discovery.lock().await;
        std::mem::replace(&mut *direct_host_discovery, Ok(None))
    }

    async fn diagnose_printer(
        &self,
        _serial_number: &str,
    ) -> anyhow::Result<PrinterDiagnosticResult> {
        unreachable!("link printer tests do not diagnose printers")
    }

    async fn refresh_printers(&self) -> anyhow::Result<Vec<PrinterRefreshResult>> {
        unreachable!("link printer tests do not refresh printers")
    }

    async fn refresh_printer_materials(
        &self,
        _serial_number: &str,
        _printer_id: Option<&str>,
    ) -> anyhow::Result<MaterialRefreshResult> {
        unreachable!("link printer tests do not refresh printer materials")
    }

    async fn validate_printer(&self, _serial_number: &str) -> anyhow::Result<()> {
        unreachable!("link printer tests do not validate by serial")
    }

    async fn validate_printer_endpoint_identity(
        &self,
        endpoint: &BambuPrinterEndpoint,
    ) -> anyhow::Result<()> {
        assert_eq!(endpoint.host, "192.0.2.10");
        assert_eq!(endpoint.serial, "SERIAL123");
        assert_eq!(endpoint.access_code, "SECRET-LINK-CODE");
        Ok(())
    }

    async fn print_project_file(
        &self,
        _serial_number: &str,
        _command: &crate::protocol::agent::v1::PrintProjectFile,
        _artifact: Vec<u8>,
    ) -> anyhow::Result<PrintProjectDispatchResult> {
        unreachable!("link printer tests do not dispatch print commands")
    }

    async fn operate_printer(
        &self,
        _serial_number: &str,
        _operation: MachinePrinterOperation,
    ) -> anyhow::Result<crate::machine::PrinterOperationDispatchResult> {
        unreachable!("link printer tests do not dispatch printer operation commands")
    }

    async fn link_printer(
        &self,
        endpoint: BambuPrinterEndpoint,
        config: &AgentConfig,
        sender: &mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<MachineSnapshot> {
        assert_eq!(endpoint.host, "192.0.2.10");
        assert_eq!(endpoint.serial, "SERIAL123");
        assert_eq!(endpoint.access_code, "SECRET-LINK-CODE");
        assert_eq!(endpoint.name.as_deref(), Some("Office X1C"));
        assert_eq!(endpoint.model.as_deref(), Some("X1 Carbon"));
        self.linked_endpoints.lock().await.push(endpoint.clone());
        let mut result = self.result.lock().await;
        let snapshot = std::mem::replace(
            &mut *result,
            Ok(snapshot("SERIAL123", "unused", None, "unused")),
        )?;
        drop(result);
        sender
            .send(responses::authoritative_printer_snapshot_event(
                config,
                snapshot.clone(),
            ))
            .await?;
        if self.emit_firmware {
            let transition = self
                .firmware
                .begin_generation(config, endpoint.clone(), sender, None)
                .await?
                .expect("link test generation is unconditional");
            let generation = transition.generation();
            let modules = transition.commit_modules(
                &endpoint.serial,
                vec![PrinterFirmwareModule {
                    name: "ota".to_owned(),
                    software_version: Some("01.00.00.00".to_owned()),
                    software_new_version: None,
                    new_version: None,
                    visible: None,
                    product_name: Some("X1 Carbon".to_owned()),
                    serial_number: None,
                    hardware_version: None,
                    firmware_flag: None,
                }],
            )?;
            assert_eq!(modules.generation, generation);
            sender
                .send(crate::machine::firmware_modules_event(config, modules))
                .await?;
        }
        Ok(snapshot)
    }
}
