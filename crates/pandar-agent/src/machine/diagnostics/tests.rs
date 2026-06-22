use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::anyhow;

use super::*;
use crate::machine::{
    file_transfer::{FileTransferRequest, TransferProtectionMode},
    mqtt::FakeMqttTransport,
};

#[test]
fn aggregate_ignores_skipped_and_prefers_problem() {
    assert_eq!(
        aggregate_status(&[
            check("a", DiagnosticStatus::Skipped, "", None),
            check("b", DiagnosticStatus::Warning, "", None)
        ]),
        DiagnosticStatus::Warning
    );
    assert_eq!(
        aggregate_status(&[
            check("a", DiagnosticStatus::Warning, "", None),
            check("b", DiagnosticStatus::Problem, "", None)
        ]),
        DiagnosticStatus::Problem
    );
}

#[tokio::test]
async fn absent_configured_printer_skips_network_and_omits_compatibility() {
    let result = diagnose_printer::<FakeMqttTransport, DiagnosticFakeTransfer>(
        &[],
        &TransferModeCache::default(),
        Duration::from_millis(1),
        "MISSING",
    )
    .await;

    assert_eq!(result.overall, DiagnosticStatus::Problem);
    assert_eq!(result.host, None);
    assert_eq!(result.model, None);
    assert_eq!(result.compatibility, None);
    assert_eq!(result.checks.len(), 1);
    assert_eq!(result.checks[0].id, "configured_printer");
}

#[tokio::test]
async fn configured_without_model_reports_unknown_compatibility() {
    let endpoint = endpoint(None);
    let mqtt = FakeMqttTransport::with_timeout();
    let transfer = DiagnosticFakeTransfer::default();
    let result = diagnose_printer(
        &[(endpoint, mqtt, transfer)],
        &TransferModeCache::default(),
        Duration::from_millis(1),
        "SERIAL1",
    )
    .await;

    let compatibility = result.compatibility.unwrap();
    assert_eq!(compatibility.normalized_model, None);
    assert_eq!(compatibility.external_storage, Capability::Unknown);
    assert_eq!(
        compatibility.features.chamber_temperature,
        Capability::Unknown
    );
    assert_eq!(compatibility.features.drying, Capability::Unknown);
    assert_eq!(compatibility.features.flow_calibration, Capability::Unknown);
    assert_eq!(
        compatibility.features.vibration_calibration,
        Capability::Unknown
    );
    assert_eq!(compatibility.features.dual_nozzle, Capability::Unknown);
    assert_eq!(
        compatibility.features.nozzle_offset_calibration,
        Capability::Unknown
    );
}

#[tokio::test]
async fn no_mqtt_report_is_problem_and_redacts_access_code() {
    let access_code = "ACCESS-CODE-UNIQUE";
    let mut endpoint = endpoint(Some("P1S"));
    endpoint.access_code = access_code.to_owned();
    let mqtt = FailingMqttTransport {
        access_code: access_code.to_owned(),
    };

    let check = mqtt_report_check(&endpoint, &mqtt, Duration::from_millis(1)).await;

    assert_eq!(check.status, DiagnosticStatus::Problem);
    let details = check.details.unwrap();
    assert!(!details.contains(access_code));
    assert!(details.contains("[REDACTED_ACCESS_CODE]"));
}

#[tokio::test]
async fn no_ftps_listener_skips_storage_probe() {
    let endpoint = endpoint(Some("P1S"));
    let mqtt = FakeMqttTransport::with_timeout();
    let transfer = DiagnosticFakeTransfer::default();

    let result = diagnose_printer(
        &[(endpoint, mqtt, transfer.clone())],
        &TransferModeCache::default(),
        Duration::from_millis(1),
        "SERIAL1",
    )
    .await;

    let ftps = result
        .checks
        .iter()
        .find(|check| check.id == "ftps_port")
        .unwrap();
    let storage = result
        .checks
        .iter()
        .find(|check| check.id == "storage_writable")
        .unwrap();
    assert_eq!(ftps.status, DiagnosticStatus::Problem);
    assert_eq!(storage.status, DiagnosticStatus::Skipped);
    assert!(transfer.recorded().is_empty());
}

#[tokio::test]
async fn unsupported_external_storage_skips_probe() {
    let endpoint = endpoint(Some("A1 Mini"));
    let mqtt = FakeMqttTransport::with_timeout();
    let transfer = DiagnosticFakeTransfer::default();

    let result = diagnose_printer(
        &[(endpoint, mqtt, transfer.clone())],
        &TransferModeCache::default(),
        Duration::from_millis(1),
        "SERIAL1",
    )
    .await;

    let storage = result
        .checks
        .iter()
        .find(|check| check.id == "storage_writable")
        .unwrap();
    assert_eq!(storage.status, DiagnosticStatus::Skipped);
    assert!(transfer.recorded().is_empty());
}

#[tokio::test]
async fn storage_probe_surfaces_upload_failure_and_redacts_access_code() {
    let access_code = "ACCESS-CODE-UNIQUE";
    let mut endpoint = endpoint(Some("P1S"));
    endpoint.access_code = access_code.to_owned();
    let transfer = DiagnosticFakeTransfer::upload_error(anyhow!("disk full {access_code}"));

    let check = storage_writable_check(&endpoint, &transfer, &TransferModeCache::default()).await;

    assert_eq!(check.status, DiagnosticStatus::Problem);
    let details = check.details.unwrap();
    assert!(!details.contains(access_code));
    assert!(details.contains("[REDACTED_ACCESS_CODE]"));
}

#[tokio::test]
async fn storage_probe_delete_failure_is_warning() {
    let endpoint = endpoint(Some("P1S"));
    let transfer = DiagnosticFakeTransfer::delete_error(anyhow!("delete failed"));

    let check = storage_writable_check(&endpoint, &transfer, &TransferModeCache::default()).await;

    assert_eq!(check.status, DiagnosticStatus::Warning);
    assert_eq!(
        transfer.recorded(),
        vec![
            (
                TransferProtectionMode::ProtectedData,
                FileTransferRequest::upload(DIAGNOSTIC_PROBE_PATH, 18)
            ),
            (
                TransferProtectionMode::ProtectedData,
                FileTransferRequest::delete(DIAGNOSTIC_PROBE_PATH)
            ),
        ]
    );
}

#[test]
fn redacts_distinctive_access_code_from_error_chain() {
    let access_code = "ACCESS-CODE-UNIQUE";
    let message = format!("outer: inner password {access_code}");

    assert_eq!(
        redact_access_code(&message, access_code),
        "outer: inner password [REDACTED_ACCESS_CODE]"
    );
}

fn endpoint(model: Option<&str>) -> BambuPrinterEndpoint {
    BambuPrinterEndpoint {
        host: "127.0.0.1".to_owned(),
        serial: "SERIAL1".to_owned(),
        access_code: "12345678".to_owned(),
        model: model.map(str::to_owned),
        name: Some("garage".to_owned()),
    }
}

#[derive(Debug, Clone, Default)]
struct DiagnosticFakeTransfer {
    state: Arc<Mutex<DiagnosticFakeTransferState>>,
}

#[derive(Debug, Default)]
struct DiagnosticFakeTransferState {
    recorded: Vec<(TransferProtectionMode, FileTransferRequest)>,
    upload_error: Option<anyhow::Error>,
    delete_error: Option<anyhow::Error>,
}

impl DiagnosticFakeTransfer {
    fn upload_error(err: anyhow::Error) -> Self {
        Self {
            state: Arc::new(Mutex::new(DiagnosticFakeTransferState {
                upload_error: Some(err),
                ..Default::default()
            })),
        }
    }

    fn delete_error(err: anyhow::Error) -> Self {
        Self {
            state: Arc::new(Mutex::new(DiagnosticFakeTransferState {
                delete_error: Some(err),
                ..Default::default()
            })),
        }
    }

    fn recorded(&self) -> Vec<(TransferProtectionMode, FileTransferRequest)> {
        self.state.lock().unwrap().recorded.clone()
    }
}

#[async_trait::async_trait]
impl MachineFileTransfer for DiagnosticFakeTransfer {
    async fn list(
        &self,
        _path: &str,
        _mode: TransferProtectionMode,
    ) -> anyhow::Result<Vec<String>> {
        Ok(Vec::new())
    }

    async fn download(
        &self,
        _path: &str,
        _mode: TransferProtectionMode,
    ) -> anyhow::Result<Vec<u8>> {
        Ok(Vec::new())
    }

    async fn upload(
        &self,
        path: &str,
        bytes: &[u8],
        mode: TransferProtectionMode,
    ) -> anyhow::Result<()> {
        let mut state = self.state.lock().unwrap();
        state
            .recorded
            .push((mode, FileTransferRequest::upload(path, bytes.len() as u64)));
        if let Some(err) = state.upload_error.take() {
            Err(err)
        } else {
            Ok(())
        }
    }

    async fn delete(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<()> {
        let mut state = self.state.lock().unwrap();
        state
            .recorded
            .push((mode, FileTransferRequest::delete(path)));
        if let Some(err) = state.delete_error.take() {
            Err(err)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug)]
struct FailingMqttTransport {
    access_code: String,
}

#[async_trait::async_trait]
impl BambuMqttTransport for FailingMqttTransport {
    async fn subscribe(&self, _topic: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn publish(&self, _command: PublishedMqttCommand) -> anyhow::Result<()> {
        Ok(())
    }

    async fn next_report(&self, _timeout: Duration) -> anyhow::Result<serde_json::Value> {
        Err(anyhow!("wrong access code {}", self.access_code))
    }
}
