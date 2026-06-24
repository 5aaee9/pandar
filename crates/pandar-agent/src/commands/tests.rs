mod artifacts;
mod diagnostics;
mod print;

use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use tokio::{sync::Mutex, sync::mpsc};

use super::*;
use crate::{
    machine::{
        BambuMachineGateway, BambuPrinterEndpoint, MachineSnapshot, NoopMachineGateway,
        PrinterControl as MachinePrinterControl, diagnostics::PrinterDiagnosticResult,
        discovery::PrinterDiscoveryResult,
    },
    protocol::agent::v1::{
        DiagnosePrinter, DiscoverPrinters, HubCommand, PrinterControl as ProtoPrinterControl,
        RefreshPrinters,
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

    async fn control_printer(
        &self,
        _serial_number: &str,
        _control: MachinePrinterControl,
    ) -> anyhow::Result<()> {
        unreachable!("refresh tests do not dispatch printer control commands")
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
async fn printer_control_valid_emits_ack_and_success_with_result_json() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = ControlGateway::default();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        printer_control_command(command_id.clone(), "SERIAL1", "pause", 0),
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
            assert_eq!(json["type"], "printer_control");
            assert_eq!(json["action"], "pause");
            assert_eq!(json["serial_number"], "SERIAL1");
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert_eq!(
        gateway.controls().await,
        vec![("SERIAL1".to_string(), MachinePrinterControl::Pause)]
    );
}

#[tokio::test]
async fn printer_control_unknown_serial_rejects_ack_without_dispatch() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = ControlGateway::unknown_serial();
    let (sender, mut receiver) = mpsc::channel(1);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        printer_control_command(command_id.clone(), "UNKNOWN", "pause", 0),
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
    assert!(gateway.controls().await.is_empty());
}

#[tokio::test]
async fn printer_control_invalid_speed_rejects_ack_without_dispatch() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = ControlGateway::default();
    let (sender, mut receiver) = mpsc::channel(1);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        printer_control_command(command_id.clone(), "SERIAL1", "set_print_speed", 5),
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
    assert!(gateway.controls().await.is_empty());
}

#[tokio::test]
async fn printer_control_non_speed_action_with_speed_rejects_ack_without_dispatch() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = ControlGateway::default();
    let (sender, mut receiver) = mpsc::channel(1);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        printer_control_command(command_id.clone(), "SERIAL1", "pause", 2),
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
    assert!(gateway.controls().await.is_empty());
}

#[tokio::test]
async fn printer_control_publish_failure_emits_ack_then_failure_with_redacted_context() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = ControlGateway::publish_failure("ACCESS-CODE-UNIQUE");
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        printer_control_command(command_id.clone(), "SERIAL1", "resume", 0),
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
                    .contains("dispatch printer control resume to SERIAL1")
            );
            assert!(result.error.contains("[REDACTED_ACCESS_CODE]"));
            assert!(!result.error.contains("ACCESS-CODE-UNIQUE"));
            assert_eq!(result.result_json, "");
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert_eq!(
        gateway.controls().await,
        vec![("SERIAL1".to_string(), MachinePrinterControl::Resume)]
    );
}

#[tokio::test]
async fn printer_control_does_not_reject_missing_local_model() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = ControlGateway::default();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        printer_control_command(command_id.clone(), "SERIAL1", "set_print_speed", 4),
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
        gateway.controls().await,
        vec![(
            "SERIAL1".to_string(),
            MachinePrinterControl::SetPrintSpeed(4)
        )]
    );
}

fn printer_control_command(
    command_id: String,
    serial_number: &str,
    action: &str,
    speed_mode: u32,
) -> HubCommand {
    HubCommand {
        command_id,
        command: Some(hub_command::Command::PrinterControl(ProtoPrinterControl {
            serial_number: serial_number.to_owned(),
            action: action.to_owned(),
            speed_mode,
        })),
    }
}

#[derive(Debug, Clone, Default)]
struct ControlGateway {
    controls: Arc<Mutex<Vec<(String, MachinePrinterControl)>>>,
    validate_error: Option<String>,
    dispatch_error: Option<String>,
    access_code: Option<String>,
}

impl ControlGateway {
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

    async fn controls(&self) -> Vec<(String, MachinePrinterControl)> {
        self.controls.lock().await.clone()
    }
}

#[async_trait]
impl BambuMachineGateway for ControlGateway {
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
        unreachable!("printer control tests do not discover printers")
    }

    async fn diagnose_printer(
        &self,
        _serial_number: &str,
    ) -> anyhow::Result<PrinterDiagnosticResult> {
        unreachable!("printer control tests do not diagnose printers")
    }

    async fn refresh_printers(&self) -> anyhow::Result<Vec<MachineSnapshot>> {
        unreachable!("printer control tests do not refresh printers")
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
        unreachable!("printer control tests do not dispatch print commands")
    }

    async fn control_printer(
        &self,
        serial_number: &str,
        control: MachinePrinterControl,
    ) -> anyhow::Result<()> {
        self.controls
            .lock()
            .await
            .push((serial_number.to_string(), control));
        match &self.dispatch_error {
            Some(error) => Err(anyhow::anyhow!(error.clone())),
            None => Ok(()),
        }
    }
}
