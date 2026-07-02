mod artifacts;
mod diagnostics;
mod print;

use std::{
    io::{self, Write},
    sync::{Arc, Mutex as StdMutex},
};

use anyhow::Context;
use async_trait::async_trait;
use tokio::{sync::Mutex, sync::mpsc};

use super::*;
use crate::{
    machine::{
        BambuMachineGateway, BambuPrinterEndpoint, MachineSnapshot, NoopMachineGateway,
        PrinterOperation as MachinePrinterOperation,
        diagnostics::PrinterDiagnosticResult,
        discovery::{DiscoveredPrinter, PrinterDiscoveryResult},
    },
    protocol::agent::v1::{
        Axis, AxisMovement, DiagnosePrinter, DiscoverPrinters, HomeOperation, HubCommand,
        LinkPrinter, MoveAxesOperation, PauseOperation, PrinterOperation as ProtoPrinterOperation,
        RefreshPrinters, SetHotendTemperatureOperation, SetPrintSpeedOperation, printer_operation,
    },
};

#[test]
fn parses_printer_config_json() {
    let printers = parse_printer_config(
        r#"[{"host":"192.0.2.10","serial":"01S00EXAMPLE","access_code":"12345678","model":"A1 Mini","name":"garage-a1"}]"#,
    )
    .unwrap();

    assert_eq!(
        printers,
        vec![BambuPrinterEndpoint {
            host: "192.0.2.10".to_owned(),
            serial: "01S00EXAMPLE".to_owned(),
            access_code: "12345678".to_owned(),
            model: Some("A1 Mini".to_owned()),
            name: Some("garage-a1".to_owned()),
        }]
    );
}

#[test]
fn parses_printer_config_empty_config_means_no_printers() {
    assert_eq!(parse_printer_config("").unwrap(), Vec::new());
    assert_eq!(parse_printer_config("   ").unwrap(), Vec::new());
    assert_eq!(parse_printer_config("[]").unwrap(), Vec::new());
    assert_eq!(parse_printer_config("  []  ").unwrap(), Vec::new());
}

#[test]
fn invalid_printer_config_malformed_json_preserves_context() {
    let err = parse_printer_config("not json").unwrap_err();

    assert!(format!("{err:#}").contains("PANDAR_PRINTERS"));
}

#[test]
fn invalid_printer_config_missing_required_field_preserves_context() {
    let err =
        parse_printer_config(r#"[{"host":"192.0.2.10","serial":"","access_code":"12345678"}]"#)
            .unwrap_err();

    let error = format!("{err:#}");
    assert!(error.contains("PANDAR_PRINTERS"));
    assert!(error.contains("serial"));
}

#[tokio::test]
async fn refresh_printers_no_configured_noop_emits_ack_and_success_only() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &NoopMachineGateway,
        &sender,
        refresh_command(command_id.clone()),
    )
    .await
    .unwrap();
    drop(sender);

    let ack = receiver.recv().await.unwrap();
    let success = receiver.recv().await.unwrap();
    assert!(receiver.recv().await.is_none());
    assert_eq!(ack, ack_event(&config, &command_id));
    assert_eq!(success, success_event(&config, &command_id));
}

#[tokio::test]
async fn refresh_printers_one_snapshot_emits_ack_snapshot_success() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = FakeGateway::ok([snapshot("SERIAL1", "garage", Some("A1 Mini"), "READY")]);
    let (sender, mut receiver) = mpsc::channel(3);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        refresh_command(command_id.clone()),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_snapshot(
        receiver.recv().await.unwrap(),
        "SERIAL1",
        "garage",
        "A1 Mini",
        "READY",
    );
    assert_eq!(
        receiver.recv().await.unwrap(),
        success_event(&config, &command_id)
    );
    assert!(receiver.recv().await.is_none());
}

#[tokio::test]
async fn refresh_printers_multiple_snapshots_stay_ordered() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = FakeGateway::ok([
        snapshot("SERIAL1", "first", Some("A1 Mini"), "READY"),
        snapshot("SERIAL2", "second", None, "RUNNING"),
    ]);
    let (sender, mut receiver) = mpsc::channel(4);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        refresh_command(command_id.clone()),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_snapshot(
        receiver.recv().await.unwrap(),
        "SERIAL1",
        "first",
        "A1 Mini",
        "READY",
    );
    assert_snapshot(
        receiver.recv().await.unwrap(),
        "SERIAL2",
        "second",
        "",
        "RUNNING",
    );
    assert_eq!(
        receiver.recv().await.unwrap(),
        success_event(&config, &command_id)
    );
    assert!(receiver.recv().await.is_none());
}

#[tokio::test]
async fn refresh_printers_gateway_failure_emits_ack_and_failed_result_with_context() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = FakeGateway::fail();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        refresh_command(command_id.clone()),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    let failure = receiver.recv().await.unwrap();
    assert!(receiver.recv().await.is_none());

    match failure.event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert_eq!(result.command_id, command_id);
            assert!(!result.success);
            assert!(result.error.contains("refresh failed"));
            assert!(result.error.contains("transport unavailable"));
        }
        other => panic!("expected command result, got {other:?}"),
    }
}

fn refresh_command(command_id: String) -> HubCommand {
    HubCommand {
        command_id,
        command: Some(hub_command::Command::RefreshPrinters(RefreshPrinters {})),
    }
}

fn link_printer_command(command_id: String, access_code: &str) -> HubCommand {
    HubCommand {
        command_id,
        command: Some(hub_command::Command::LinkPrinter(LinkPrinter {
            host: "192.0.2.10".to_owned(),
            access_code: access_code.to_owned(),
            name: "Office X1C".to_owned(),
            printer_type: "BambuLab".to_owned(),
        })),
    }
}

pub(super) fn discover_command(command_id: String) -> HubCommand {
    HubCommand {
        command_id,
        command: Some(hub_command::Command::DiscoverPrinters(DiscoverPrinters {
            timeout_seconds: 1,
        })),
    }
}

pub(super) fn diagnose_command(command_id: String, serial_number: &str) -> HubCommand {
    HubCommand {
        command_id,
        command: Some(hub_command::Command::DiagnosePrinter(DiagnosePrinter {
            serial_number: serial_number.to_owned(),
        })),
    }
}

pub(super) fn test_config() -> AgentConfig {
    AgentConfig {
        hub_grpc_url: "http://hub.internal:50051".to_owned(),
        hub_api_url: None,
        agent_name: "garage".to_owned(),
        agent_id: "agent-id".to_owned(),
        tenant_id: "tenant-id".to_owned(),
        agent_credential: "pandar_ac_test".to_owned(),
        agent_version: "9.8.7".to_owned(),
        printers: "[]".to_owned(),
        artifact_root: ".".into(),
    }
}

fn snapshot(serial: &str, name: &str, model: Option<&str>, state: &str) -> MachineSnapshot {
    MachineSnapshot {
        serial: serial.to_owned(),
        name: name.to_owned(),
        model: model.map(str::to_owned),
        state: state.to_owned(),
    }
}

fn assert_snapshot(event: AgentEvent, serial: &str, name: &str, model: &str, state: &str) {
    assert_eq!(event.agent_id, "agent-id");
    assert_eq!(event.tenant_id, "tenant-id");
    match event.event.unwrap() {
        agent_event::Event::PrinterSnapshot(snapshot) => {
            assert_eq!(snapshot.serial, serial);
            assert_eq!(snapshot.name, name);
            assert_eq!(snapshot.model, model);
            assert_eq!(snapshot.state, state);
        }
        other => panic!("expected printer snapshot, got {other:?}"),
    }
}

pub(super) fn assert_failure_contains(event: AgentEvent, command_id: &str, needle: &str) {
    match event.event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert_eq!(result.command_id, command_id);
            assert!(!result.success);
            assert!(result.error.contains(needle), "{}", result.error);
        }
        other => panic!("expected command result, got {other:?}"),
    }
}

#[derive(Debug, Clone)]
pub(super) struct FakeGateway {
    result: Arc<Mutex<anyhow::Result<Vec<MachineSnapshot>>>>,
    access_code: Option<String>,
}

impl FakeGateway {
    pub(super) fn ok(snapshots: impl IntoIterator<Item = MachineSnapshot>) -> Self {
        Self {
            result: Arc::new(Mutex::new(Ok(snapshots.into_iter().collect()))),
            access_code: None,
        }
    }

    fn fail() -> Self {
        Self {
            result: Arc::new(Mutex::new(
                Err(anyhow::anyhow!("transport unavailable")).context("refresh failed"),
            )),
            access_code: None,
        }
    }

    fn fail_with_access_code(access_code: &str) -> Self {
        Self {
            result: Arc::new(Mutex::new(
                Err(anyhow::anyhow!("bad access code {access_code}")).context("refresh failed"),
            )),
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

    async fn refresh_printers(&self) -> anyhow::Result<Vec<MachineSnapshot>> {
        let mut result = self.result.lock().await;
        std::mem::replace(&mut *result, Ok(Vec::new()))
    }

    async fn validate_printer(&self, _serial_number: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn print_project_file(
        &self,
        _serial_number: &str,
        _command: &crate::protocol::agent::v1::PrintProjectFile,
        _artifact: Vec<u8>,
    ) -> anyhow::Result<()> {
        unreachable!("refresh tests do not dispatch print commands")
    }

    async fn operate_printer(
        &self,
        _serial_number: &str,
        _operation: MachinePrinterOperation,
    ) -> anyhow::Result<()> {
        unreachable!("refresh tests do not dispatch printer operation commands")
    }
}

#[tokio::test]
async fn command_failure_redacts_access_code() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let access_code = "ACCESS-CODE-UNIQUE";
    let gateway = FakeGateway::fail_with_access_code(access_code);
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        refresh_command(command_id.clone()),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert!(!result.success);
            assert!(!result.error.contains(access_code));
            assert!(result.error.contains("[REDACTED_ACCESS_CODE]"));
            assert_eq!(result.result_json, "");
        }
        other => panic!("expected command result, got {other:?}"),
    }
}

#[tokio::test]
async fn link_printer_emits_ack_snapshot_and_success_without_access_code() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = LinkGateway::success(snapshot(
        "SERIAL123",
        "Office X1C",
        Some("X1 Carbon"),
        "READY",
    ));
    let (sender, mut receiver) = mpsc::channel(3);
    let access_code = "SECRET-LINK-CODE";

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        link_printer_command(command_id.clone(), access_code),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_snapshot(
        receiver.recv().await.unwrap(),
        "SERIAL123",
        "Office X1C",
        "X1 Carbon",
        "READY",
    );
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert!(result.success);
            assert!(!result.result_json.contains(access_code));
            let json: serde_json::Value = serde_json::from_str(&result.result_json).unwrap();
            assert_eq!(json["type"], "printer_link");
            assert_eq!(json["serial_number"], "SERIAL123");
            assert_eq!(json["host"], "192.0.2.10");
            assert_eq!(json["name"], "Office X1C");
            assert_eq!(json["model"], "X1 Carbon");
            assert_eq!(json["status"], "READY");
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert_eq!(gateway.linked_endpoints().await.len(), 1);
    assert!(receiver.recv().await.is_none());
}

#[tokio::test]
async fn link_printer_fails_when_discovery_does_not_find_host() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = LinkGateway::discovery_result(vec![discovered_printer(
        "192.0.2.11",
        Some("OTHER"),
        Some("A1 Mini"),
    )]);
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        link_printer_command(command_id.clone(), "SECRET-LINK-CODE"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_failure_contains(
        receiver.recv().await.unwrap(),
        &command_id,
        "could not discover printer at 192.0.2.10",
    );
    assert!(receiver.recv().await.is_none());
    assert!(gateway.linked_endpoints().await.is_empty());
}

#[tokio::test]
async fn link_printer_uses_direct_host_discovery_when_multicast_misses_host() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = LinkGateway::discovery_result_with_direct_host(
        Vec::new(),
        Some(discovered_printer(
            "192.0.2.10",
            Some("SERIAL123"),
            Some("X1 Carbon"),
        )),
    );
    let (sender, mut receiver) = mpsc::channel(3);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        link_printer_command(command_id.clone(), "SECRET-LINK-CODE"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_snapshot(
        receiver.recv().await.unwrap(),
        "SERIAL123",
        "Office X1C",
        "X1 Carbon",
        "READY",
    );
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert_eq!(result.command_id, command_id);
            assert!(result.success);
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert_eq!(gateway.linked_endpoints().await.len(), 1);
    assert!(receiver.recv().await.is_none());
}

#[tokio::test]
async fn link_printer_fails_when_discovered_printer_has_no_serial() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = LinkGateway::discovery_result(vec![discovered_printer(
        "192.0.2.10",
        None,
        Some("X1 Carbon"),
    )]);
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        link_printer_command(command_id.clone(), "SECRET-LINK-CODE"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_failure_contains(
        receiver.recv().await.unwrap(),
        &command_id,
        "printer serial could not be discovered for 192.0.2.10",
    );
    assert!(receiver.recv().await.is_none());
    assert!(gateway.linked_endpoints().await.is_empty());
}

#[tokio::test]
async fn link_printer_rejects_unsupported_type_without_discovery() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = LinkGateway::discovery_result(vec![discovered_printer(
        "192.0.2.10",
        Some("SERIAL123"),
        Some("X1 Carbon"),
    )]);
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        link_printer_command_with_type(command_id.clone(), "SECRET-LINK-CODE", "Other"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_failure_contains(
        receiver.recv().await.unwrap(),
        &command_id,
        "unsupported printer type Other",
    );
    assert!(receiver.recv().await.is_none());
    assert!(gateway.linked_endpoints().await.is_empty());
}

#[tokio::test]
async fn link_printer_failure_redacts_access_code_from_result_error() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let access_code = "SECRET-LINK-CODE";
    let gateway = LinkGateway::failure(access_code);
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        link_printer_command(command_id.clone(), access_code),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert!(!result.success);
            assert!(result.error.contains("validate runtime printer"));
            assert!(result.error.contains("[REDACTED_ACCESS_CODE]"));
            assert!(!result.error.contains(access_code));
            assert_eq!(result.result_json, "");
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert!(receiver.recv().await.is_none());
}

#[test]
fn link_printer_failure_log_redacts_access_code() {
    let _capture_guard = crate::TRACING_CAPTURE_LOCK.lock().unwrap();
    let logs = CapturedLogs::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.clone())
        .with_ansi(false)
        .without_time()
        .finish();
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let access_code = "SECRET-LINK-CODE";
    let gateway = LinkGateway::failure(access_code);
    let (sender, _receiver) = mpsc::channel(2);

    tracing::subscriber::with_default(subscriber, || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                handle_command_with_gateway(
                    &config,
                    &gateway,
                    &sender,
                    link_printer_command(command_id, access_code),
                )
                .await
            })
            .unwrap();
    });

    let captured = logs.contents();
    assert!(captured.contains("runtime printer link failed"));
    assert!(captured.contains("[REDACTED_ACCESS_CODE]"));
    assert!(!captured.contains(access_code));
}

#[derive(Debug, Clone)]
struct LinkGateway {
    discovery: Arc<Mutex<anyhow::Result<PrinterDiscoveryResult>>>,
    direct_host_discovery: Arc<Mutex<anyhow::Result<Option<DiscoveredPrinter>>>>,
    result: Arc<Mutex<anyhow::Result<MachineSnapshot>>>,
    linked_endpoints: Arc<Mutex<Vec<BambuPrinterEndpoint>>>,
    access_code: Option<String>,
}

impl LinkGateway {
    fn success(snapshot: MachineSnapshot) -> Self {
        Self {
            discovery: Arc::new(Mutex::new(Ok(PrinterDiscoveryResult::new(vec![
                discovered_printer("192.0.2.10", Some("SERIAL123"), Some("X1 Carbon")),
            ])))),
            direct_host_discovery: Arc::new(Mutex::new(Ok(None))),
            result: Arc::new(Mutex::new(Ok(snapshot))),
            linked_endpoints: Arc::new(Mutex::new(Vec::new())),
            access_code: None,
        }
    }

    fn discovery_result(printers: Vec<DiscoveredPrinter>) -> Self {
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
            access_code: None,
        }
    }

    fn discovery_result_with_direct_host(
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
            access_code: None,
        }
    }

    fn failure(access_code: &str) -> Self {
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
            access_code: Some(access_code.to_owned()),
        }
    }

    async fn linked_endpoints(&self) -> Vec<BambuPrinterEndpoint> {
        self.linked_endpoints.lock().await.clone()
    }
}

fn discovered_printer(host: &str, serial: Option<&str>, model: Option<&str>) -> DiscoveredPrinter {
    DiscoveredPrinter {
        serial_number: serial.map(str::to_owned),
        host: host.to_owned(),
        name: Some("Discovered Office X1C".to_owned()),
        model: model.map(str::to_owned),
        source: "ssdp",
    }
}

fn link_printer_command_with_type(
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

    async fn refresh_printers(&self) -> anyhow::Result<Vec<MachineSnapshot>> {
        unreachable!("link printer tests do not refresh printers")
    }

    async fn validate_printer(&self, _serial_number: &str) -> anyhow::Result<()> {
        unreachable!("link printer tests do not validate by serial")
    }

    async fn print_project_file(
        &self,
        _serial_number: &str,
        _command: &crate::protocol::agent::v1::PrintProjectFile,
        _artifact: Vec<u8>,
    ) -> anyhow::Result<()> {
        unreachable!("link printer tests do not dispatch print commands")
    }

    async fn operate_printer(
        &self,
        _serial_number: &str,
        _operation: MachinePrinterOperation,
    ) -> anyhow::Result<()> {
        unreachable!("link printer tests do not dispatch printer operation commands")
    }

    async fn link_printer(
        &self,
        endpoint: BambuPrinterEndpoint,
        _config: &AgentConfig,
        _sender: &mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<MachineSnapshot> {
        assert_eq!(endpoint.host, "192.0.2.10");
        assert_eq!(endpoint.serial, "SERIAL123");
        assert_eq!(endpoint.access_code, "SECRET-LINK-CODE");
        assert_eq!(endpoint.name.as_deref(), Some("Office X1C"));
        assert_eq!(endpoint.model.as_deref(), Some("X1 Carbon"));
        self.linked_endpoints.lock().await.push(endpoint);
        let mut result = self.result.lock().await;
        std::mem::replace(
            &mut *result,
            Ok(snapshot("SERIAL123", "unused", None, "unused")),
        )
    }
}

#[tokio::test]
async fn printer_operation_valid_emits_ack_and_success_with_result_json() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        pause_operation_command(command_id.clone(), "SERIAL1"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert_eq!(result.command_id, command_id);
            assert!(result.success);
            let json: serde_json::Value = serde_json::from_str(&result.result_json).unwrap();
            assert_eq!(json["type"], "printer_operation");
            assert_eq!(json["action"], "pause");
            assert_eq!(json["serial_number"], "SERIAL1");
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert_eq!(
        gateway.operations().await,
        vec![("SERIAL1".to_string(), MachinePrinterOperation::Pause)]
    );
}

#[tokio::test]
async fn printer_operation_unknown_serial_rejects_ack_without_dispatch() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::unknown_serial();
    let (sender, mut receiver) = mpsc::channel(1);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        pause_operation_command(command_id.clone(), "UNKNOWN"),
    )
    .await
    .unwrap();
    drop(sender);

    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandAck(ack) => {
            assert_eq!(ack.command_id, command_id);
            assert!(!ack.accepted);
            assert!(ack.error.contains("UNKNOWN"));
        }
        other => panic!("expected command ack, got {other:?}"),
    }
    assert!(receiver.recv().await.is_none());
    assert!(gateway.operations().await.is_empty());
}

#[tokio::test]
async fn printer_operation_invalid_speed_rejects_ack_without_dispatch() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(1);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        set_print_speed_operation_command(command_id.clone(), "SERIAL1", 5),
    )
    .await
    .unwrap();
    drop(sender);

    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandAck(ack) => {
            assert_eq!(ack.command_id, command_id);
            assert!(!ack.accepted);
            assert!(ack.error.contains("speed_mode"));
        }
        other => panic!("expected command ack, got {other:?}"),
    }
    assert!(receiver.recv().await.is_none());
    assert!(gateway.operations().await.is_empty());
}

#[tokio::test]
async fn printer_operation_unspecified_axis_rejects_ack_without_dispatch() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(1);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        home_operation_command(
            command_id.clone(),
            "SERIAL1",
            vec![Axis::Unspecified as i32],
        ),
    )
    .await
    .unwrap();
    drop(sender);

    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandAck(ack) => {
            assert_eq!(ack.command_id, command_id);
            assert!(!ack.accepted);
            assert!(ack.error.contains("axis"));
        }
        other => panic!("expected command ack, got {other:?}"),
    }
    assert!(receiver.recv().await.is_none());
    assert!(gateway.operations().await.is_empty());
}

#[tokio::test]
async fn printer_operation_duplicate_move_axis_rejects_ack_without_dispatch() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(1);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        move_axes_operation_command_with_movements(
            command_id.clone(),
            "SERIAL1",
            vec![
                AxisMovement {
                    axis: Axis::X as i32,
                    delta_mm: 10.0,
                },
                AxisMovement {
                    axis: Axis::X as i32,
                    delta_mm: 12.0,
                },
            ],
            3000,
        ),
    )
    .await
    .unwrap();
    drop(sender);

    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandAck(ack) => {
            assert_eq!(ack.command_id, command_id);
            assert!(!ack.accepted);
            assert!(ack.error.contains("duplicate axis"));
        }
        other => panic!("expected command ack, got {other:?}"),
    }
    assert!(receiver.recv().await.is_none());
    assert!(gateway.operations().await.is_empty());
}

#[tokio::test]
async fn printer_operation_invalid_move_bounds_reject_ack_without_dispatch() {
    for (command, expected_error) in [
        (
            move_axes_operation_command_with_movements(
                uuid::Uuid::new_v4().to_string(),
                "SERIAL1",
                vec![AxisMovement {
                    axis: Axis::X as i32,
                    delta_mm: 0.0,
                }],
                3000,
            ),
            "delta_mm",
        ),
        (
            move_axes_operation_command_with_movements(
                uuid::Uuid::new_v4().to_string(),
                "SERIAL1",
                vec![AxisMovement {
                    axis: Axis::X as i32,
                    delta_mm: 51.0,
                }],
                3000,
            ),
            "delta_mm",
        ),
        (
            move_axes_operation_command_with_movements(
                uuid::Uuid::new_v4().to_string(),
                "SERIAL1",
                vec![AxisMovement {
                    axis: Axis::X as i32,
                    delta_mm: f64::NAN,
                }],
                3000,
            ),
            "delta_mm",
        ),
        (
            move_axes_operation_command_with_movements(
                uuid::Uuid::new_v4().to_string(),
                "SERIAL1",
                vec![AxisMovement {
                    axis: Axis::X as i32,
                    delta_mm: 5.0,
                }],
                12_001,
            ),
            "feedrate",
        ),
    ] {
        let config = test_config();
        let command_id = command.command_id.clone();
        let gateway = OperationGateway::default();
        let (sender, mut receiver) = mpsc::channel(1);

        handle_command_with_gateway(&config, &gateway, &sender, command)
            .await
            .unwrap();
        drop(sender);

        match receiver.recv().await.unwrap().event.unwrap() {
            agent_event::Event::CommandAck(ack) => {
                assert_eq!(ack.command_id, command_id);
                assert!(!ack.accepted);
                assert!(ack.error.contains(expected_error), "{}", ack.error);
            }
            other => panic!("expected command ack, got {other:?}"),
        }
        assert!(receiver.recv().await.is_none());
        assert!(gateway.operations().await.is_empty());
    }
}

#[tokio::test]
async fn printer_operation_invalid_hotend_temperature_rejects_ack_without_dispatch() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(1);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        hotend_operation_command(command_id.clone(), "SERIAL1", 301, false),
    )
    .await
    .unwrap();
    drop(sender);

    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandAck(ack) => {
            assert_eq!(ack.command_id, command_id);
            assert!(!ack.accepted);
            assert!(ack.error.contains("temperature"));
        }
        other => panic!("expected command ack, got {other:?}"),
    }
    assert!(receiver.recv().await.is_none());
    assert!(gateway.operations().await.is_empty());
}

#[tokio::test]
async fn printer_operation_publish_failure_emits_ack_then_failure_with_redacted_context() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::publish_failure("ACCESS-CODE-UNIQUE");
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        resume_operation_command(command_id.clone(), "SERIAL1"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert_eq!(result.command_id, command_id);
            assert!(!result.success);
            assert!(
                result
                    .error
                    .contains("dispatch printer operation resume to SERIAL1")
            );
            assert!(result.error.contains("[REDACTED_ACCESS_CODE]"));
            assert!(!result.error.contains("ACCESS-CODE-UNIQUE"));
            assert_eq!(result.result_json, "");
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert_eq!(
        gateway.operations().await,
        vec![("SERIAL1".to_string(), MachinePrinterOperation::Resume)]
    );
}

#[tokio::test]
async fn printer_operation_does_not_reject_missing_local_model() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        set_print_speed_operation_command(command_id.clone(), "SERIAL1", 4),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert_eq!(result.command_id, command_id);
            assert!(result.success);
            let json: serde_json::Value = serde_json::from_str(&result.result_json).unwrap();
            assert_eq!(json["action"], "set_print_speed");
            assert_eq!(json["speed_mode"], 4);
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert_eq!(
        gateway.operations().await,
        vec![(
            "SERIAL1".to_string(),
            MachinePrinterOperation::SetPrintSpeed(4)
        )]
    );
}

#[tokio::test]
async fn printer_operation_move_axes_dispatches_typed_details() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        move_axes_operation_command(command_id.clone(), "SERIAL1"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert!(result.success);
            let json: serde_json::Value = serde_json::from_str(&result.result_json).unwrap();
            assert_eq!(json["type"], "printer_operation");
            assert_eq!(json["action"], "move_axes");
            assert_eq!(json["x_mm"], 10.0);
            assert_eq!(json["z_mm"], -0.5);
            assert_eq!(json["feedrate_mm_per_min"], 3000.0);
            assert!(json.get("y_mm").is_none());
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert_eq!(
        gateway.operations().await,
        vec![(
            "SERIAL1".to_string(),
            MachinePrinterOperation::MoveAxes {
                x_mm: Some(10.0),
                y_mm: None,
                z_mm: Some(-0.5),
                feedrate_mm_per_min: Some(3000.0),
            }
        )]
    );
}

#[tokio::test]
async fn printer_operation_hotend_dispatches_typed_details() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        hotend_operation_command(command_id.clone(), "SERIAL1", 215, true),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert!(result.success);
            let json: serde_json::Value = serde_json::from_str(&result.result_json).unwrap();
            assert_eq!(json["type"], "printer_operation");
            assert_eq!(json["action"], "set_hotend_temperature");
            assert_eq!(json["temperature_celsius"], 215);
            assert_eq!(json["wait"], true);
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert_eq!(
        gateway.operations().await,
        vec![(
            "SERIAL1".to_string(),
            MachinePrinterOperation::SetHotendTemperature {
                temperature_celsius: 215,
                wait: true,
            }
        )]
    );
}

#[tokio::test]
async fn printer_operation_missing_operation_rejects_ack_without_dispatch() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(1);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        printer_operation_command(command_id.clone(), "SERIAL1", None),
    )
    .await
    .unwrap();
    drop(sender);

    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandAck(ack) => {
            assert_eq!(ack.command_id, command_id);
            assert!(!ack.accepted);
            assert!(ack.error.contains("missing printer operation"));
        }
        other => panic!("expected command ack, got {other:?}"),
    }
    assert!(receiver.recv().await.is_none());
    assert!(gateway.operations().await.is_empty());
}

fn pause_operation_command(command_id: String, serial_number: &str) -> HubCommand {
    printer_operation_command(
        command_id,
        serial_number,
        Some(printer_operation::Operation::Pause(PauseOperation {})),
    )
}

fn resume_operation_command(command_id: String, serial_number: &str) -> HubCommand {
    printer_operation_command(
        command_id,
        serial_number,
        Some(printer_operation::Operation::Resume(
            crate::protocol::agent::v1::ResumeOperation {},
        )),
    )
}

fn set_print_speed_operation_command(
    command_id: String,
    serial_number: &str,
    speed_mode: u32,
) -> HubCommand {
    printer_operation_command(
        command_id,
        serial_number,
        Some(printer_operation::Operation::SetPrintSpeed(
            SetPrintSpeedOperation { speed_mode },
        )),
    )
}

fn home_operation_command(command_id: String, serial_number: &str, axes: Vec<i32>) -> HubCommand {
    printer_operation_command(
        command_id,
        serial_number,
        Some(printer_operation::Operation::Home(HomeOperation { axes })),
    )
}

fn move_axes_operation_command(command_id: String, serial_number: &str) -> HubCommand {
    move_axes_operation_command_with_movements(
        command_id,
        serial_number,
        vec![
            AxisMovement {
                axis: Axis::X as i32,
                delta_mm: 10.0,
            },
            AxisMovement {
                axis: Axis::Z as i32,
                delta_mm: -0.5,
            },
        ],
        3000,
    )
}

fn move_axes_operation_command_with_movements(
    command_id: String,
    serial_number: &str,
    movements: Vec<AxisMovement>,
    feedrate_mm_per_min: u32,
) -> HubCommand {
    printer_operation_command(
        command_id,
        serial_number,
        Some(printer_operation::Operation::MoveAxes(MoveAxesOperation {
            movements,
            feedrate_mm_per_min,
        })),
    )
}

fn hotend_operation_command(
    command_id: String,
    serial_number: &str,
    temperature_celsius: u32,
    wait: bool,
) -> HubCommand {
    printer_operation_command(
        command_id,
        serial_number,
        Some(printer_operation::Operation::SetHotendTemperature(
            SetHotendTemperatureOperation {
                temperature_celsius,
                wait,
            },
        )),
    )
}

fn printer_operation_command(
    command_id: String,
    serial_number: &str,
    operation: Option<printer_operation::Operation>,
) -> HubCommand {
    HubCommand {
        command_id,
        command: Some(hub_command::Command::PrinterOperation(
            ProtoPrinterOperation {
                serial_number: serial_number.to_owned(),
                operation,
            },
        )),
    }
}

#[derive(Debug, Clone, Default)]
struct OperationGateway {
    operations: Arc<Mutex<Vec<(String, MachinePrinterOperation)>>>,
    validate_error: Option<String>,
    dispatch_error: Option<String>,
    access_code: Option<String>,
}

impl OperationGateway {
    fn unknown_serial() -> Self {
        Self {
            validate_error: Some("no configured Bambu printer matches serial UNKNOWN".to_string()),
            ..Self::default()
        }
    }

    fn publish_failure(access_code: &str) -> Self {
        Self {
            dispatch_error: Some(format!(
                "fake publish failure with access code {access_code}"
            )),
            access_code: Some(access_code.to_string()),
            ..Self::default()
        }
    }

    async fn operations(&self) -> Vec<(String, MachinePrinterOperation)> {
        self.operations.lock().await.clone()
    }
}

#[async_trait]
impl BambuMachineGateway for OperationGateway {
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
        unreachable!("printer operation tests do not discover printers")
    }

    async fn diagnose_printer(
        &self,
        _serial_number: &str,
    ) -> anyhow::Result<PrinterDiagnosticResult> {
        unreachable!("printer operation tests do not diagnose printers")
    }

    async fn refresh_printers(&self) -> anyhow::Result<Vec<MachineSnapshot>> {
        unreachable!("printer operation tests do not refresh printers")
    }

    async fn validate_printer(&self, _serial_number: &str) -> anyhow::Result<()> {
        match &self.validate_error {
            Some(error) => Err(anyhow::anyhow!(error.clone())),
            None => Ok(()),
        }
    }

    async fn print_project_file(
        &self,
        _serial_number: &str,
        _command: &crate::protocol::agent::v1::PrintProjectFile,
        _artifact: Vec<u8>,
    ) -> anyhow::Result<()> {
        unreachable!("printer operation tests do not dispatch print commands")
    }

    async fn operate_printer(
        &self,
        serial_number: &str,
        operation: MachinePrinterOperation,
    ) -> anyhow::Result<()> {
        self.operations
            .lock()
            .await
            .push((serial_number.to_string(), operation));
        match &self.dispatch_error {
            Some(error) => Err(anyhow::anyhow!(error.clone())),
            None => Ok(()),
        }
    }
}

#[derive(Clone, Default)]
struct CapturedLogs {
    buffer: Arc<StdMutex<Vec<u8>>>,
}

impl CapturedLogs {
    fn contents(&self) -> String {
        String::from_utf8(self.buffer.lock().unwrap().clone()).unwrap()
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturedLogs {
    type Writer = CapturedLogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        CapturedLogWriter {
            buffer: Arc::clone(&self.buffer),
        }
    }
}

struct CapturedLogWriter {
    buffer: Arc<StdMutex<Vec<u8>>>,
}

impl Write for CapturedLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
