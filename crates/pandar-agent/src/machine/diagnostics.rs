use std::{net::SocketAddr, time::Duration};

use anyhow::Context;
use serde::Serialize;
use tokio::net::TcpStream;

use crate::machine::{
    BambuPrinterEndpoint,
    compatibility::{Capability, DiagnosticCompatibility, compatibility_for_model},
    file_transfer::{
        BAMBU_FILE_TRANSFER_PORT, MachineFileTransfer, TransferModeCache, run_with_transfer_mode,
    },
    mqtt::{
        BAMBU_MQTT_PORT, BAMBU_MQTT_QOS, BambuMqttCommand, BambuMqttTopics, BambuMqttTransport,
        PublishedMqttCommand,
    },
};

pub(super) const DIAGNOSTIC_PROBE_PATH: &str = "Metadata/pandar-diagnostic.tmp";
const PORT_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticStatus {
    Ok,
    Warning,
    Problem,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagnosticCheck {
    pub id: &'static str,
    pub status: DiagnosticStatus,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrinterDiagnosticResult {
    #[serde(rename = "type")]
    pub result_type: &'static str,
    pub serial_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub overall: DiagnosticStatus,
    pub checks: Vec<DiagnosticCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<DiagnosticCompatibility>,
}

impl PrinterDiagnosticResult {
    fn new(
        serial_number: String,
        host: Option<String>,
        model: Option<String>,
        checks: Vec<DiagnosticCheck>,
        compatibility: Option<DiagnosticCompatibility>,
    ) -> Self {
        let overall = aggregate_status(&checks);
        Self {
            result_type: "printer_diagnostic",
            serial_number,
            host,
            model,
            overall,
            checks,
            compatibility,
        }
    }
}

pub async fn diagnose_printer<T, F>(
    printers: &[(BambuPrinterEndpoint, T, F)],
    transfer_cache: &TransferModeCache,
    report_timeout: Duration,
    serial_number: &str,
) -> PrinterDiagnosticResult
where
    T: BambuMqttTransport + Send + Sync,
    F: MachineFileTransfer + Send + Sync,
{
    let Some((endpoint, mqtt, transfer)) = printers
        .iter()
        .find(|(endpoint, _, _)| endpoint.serial == serial_number)
    else {
        return PrinterDiagnosticResult::new(
            serial_number.to_owned(),
            None,
            None,
            vec![check(
                "configured_printer",
                DiagnosticStatus::Problem,
                "No configured printer matches the requested serial number.",
                None,
            )],
            None,
        );
    };

    let compatibility = compatibility_for_model(endpoint.model.as_deref());
    let mut checks = vec![check(
        "configured_printer",
        DiagnosticStatus::Ok,
        "Printer is configured on this agent.",
        None,
    )];

    let mqtt_port = port_check(
        "mqtt_port",
        &endpoint.host,
        BAMBU_MQTT_PORT,
        &endpoint.access_code,
    )
    .await;
    let mqtt_port_ok = mqtt_port.status == DiagnosticStatus::Ok;
    checks.push(mqtt_port);

    if mqtt_port_ok {
        checks.push(mqtt_report_check(endpoint, mqtt, report_timeout).await);
    } else {
        checks.push(check(
            "mqtt_report",
            DiagnosticStatus::Skipped,
            "Skipped because MQTT port reachability failed first.",
            None,
        ));
    }

    let ftps_port = port_check(
        "ftps_port",
        &endpoint.host,
        BAMBU_FILE_TRANSFER_PORT,
        &endpoint.access_code,
    )
    .await;
    let ftps_port_ok = ftps_port.status == DiagnosticStatus::Ok;
    checks.push(ftps_port);

    if compatibility.external_storage == Capability::Unsupported {
        checks.push(check(
            "storage_writable",
            DiagnosticStatus::Skipped,
            "Skipped because this model does not use removable printer storage for LAN dispatch diagnostics.",
            None,
        ));
    } else if !ftps_port_ok {
        checks.push(check(
            "storage_writable",
            DiagnosticStatus::Skipped,
            "Skipped because FTPS port reachability failed first.",
            None,
        ));
    } else {
        checks.push(storage_writable_check(endpoint, transfer, transfer_cache).await);
    }

    checks.push(check(
        "compatibility",
        DiagnosticStatus::Ok,
        "Compatibility policy resolved for this printer.",
        None,
    ));

    PrinterDiagnosticResult::new(
        endpoint.serial.clone(),
        Some(endpoint.host.clone()),
        endpoint.model.clone(),
        checks,
        Some(compatibility),
    )
}

pub fn redact_access_code(message: &str, access_code: &str) -> String {
    if access_code.is_empty() {
        return message.to_owned();
    }
    message.replace(access_code, "[REDACTED_ACCESS_CODE]")
}

pub fn redact_known_access_codes(
    message: &str,
    access_codes: impl IntoIterator<Item = String>,
) -> String {
    access_codes
        .into_iter()
        .fold(message.to_owned(), |redacted, access_code| {
            redact_access_code(&redacted, &access_code)
        })
}

pub(super) fn aggregate_status(checks: &[DiagnosticCheck]) -> DiagnosticStatus {
    if checks
        .iter()
        .any(|check| check.status == DiagnosticStatus::Problem)
    {
        DiagnosticStatus::Problem
    } else if checks
        .iter()
        .any(|check| check.status == DiagnosticStatus::Warning)
    {
        DiagnosticStatus::Warning
    } else {
        DiagnosticStatus::Ok
    }
}

async fn port_check(id: &'static str, host: &str, port: u16, access_code: &str) -> DiagnosticCheck {
    let addr = format!("{host}:{port}");
    let result = async {
        let addr: SocketAddr = addr.parse().with_context(|| format!("parse {addr}"))?;
        tokio::time::timeout(PORT_TIMEOUT, TcpStream::connect(addr))
            .await
            .with_context(|| format!("connect timeout for {addr}"))?
            .with_context(|| format!("connect to {addr}"))?;
        Ok::<(), anyhow::Error>(())
    }
    .await;

    match result {
        Ok(()) => check(
            id,
            DiagnosticStatus::Ok,
            format!("{port} is reachable."),
            None,
        ),
        Err(err) => check(
            id,
            DiagnosticStatus::Problem,
            format!("{port} is not reachable."),
            Some(redact_access_code(&format!("{err:#}"), access_code)),
        ),
    }
}

pub(super) async fn mqtt_report_check<T>(
    endpoint: &BambuPrinterEndpoint,
    transport: &T,
    report_timeout: Duration,
) -> DiagnosticCheck
where
    T: BambuMqttTransport + Send + Sync,
{
    let result = async {
        let topics = BambuMqttTopics::for_serial(&endpoint.serial);
        transport
            .subscribe(&topics.report)
            .await
            .with_context(|| format!("subscribe to report topic {}", topics.report))?;
        transport
            .publish(PublishedMqttCommand {
                topic: topics.request.clone(),
                payload: BambuMqttCommand::RequestPushAll.payload(),
                qos: BAMBU_MQTT_QOS,
            })
            .await
            .with_context(|| format!("publish pushall to request topic {}", topics.request))?;
        transport
            .next_report(report_timeout)
            .await
            .context("wait for MQTT report")?;
        Ok::<(), anyhow::Error>(())
    }
    .await;

    match result {
        Ok(()) => check(
            "mqtt_report",
            DiagnosticStatus::Ok,
            "Authenticated MQTT report received.",
            None,
        ),
        Err(err) => {
            let details = redact_access_code(&format!("{err:#}"), &endpoint.access_code);
            tracing::warn!(
                serial = %endpoint.serial,
                error = %details,
                "printer diagnostic MQTT report check failed"
            );
            check(
                "mqtt_report",
                DiagnosticStatus::Problem,
                "Authenticated MQTT report was not received.",
                Some(details),
            )
        }
    }
}

pub(super) async fn storage_writable_check<F>(
    endpoint: &BambuPrinterEndpoint,
    transfer: &F,
    transfer_cache: &TransferModeCache,
) -> DiagnosticCheck
where
    F: MachineFileTransfer + Send + Sync,
{
    let bytes = b"pandar diagnostic\n";
    let upload = run_with_transfer_mode(endpoint, transfer_cache, false, |mode| async move {
        transfer.upload(DIAGNOSTIC_PROBE_PATH, bytes, mode).await
    })
    .await;

    if let Err(err) = upload {
        let details = redact_access_code(&format!("{err:#}"), &endpoint.access_code);
        tracing::warn!(
            serial = %endpoint.serial,
            error = %details,
            "printer diagnostic storage upload check failed"
        );
        return check(
            "storage_writable",
            DiagnosticStatus::Problem,
            "Diagnostic storage probe upload failed.",
            Some(details),
        );
    }

    match run_with_transfer_mode(endpoint, transfer_cache, false, |mode| async move {
        transfer.delete(DIAGNOSTIC_PROBE_PATH, mode).await
    })
    .await
    {
        Ok(()) => check(
            "storage_writable",
            DiagnosticStatus::Ok,
            "Diagnostic storage probe upload and delete succeeded.",
            None,
        ),
        Err(err) => {
            let details = redact_access_code(&format!("{err:#}"), &endpoint.access_code);
            tracing::warn!(
                serial = %endpoint.serial,
                error = %details,
                "printer diagnostic storage probe delete failed"
            );
            check(
                "storage_writable",
                DiagnosticStatus::Warning,
                "Diagnostic storage probe uploaded, but best-effort delete failed.",
                Some(details),
            )
        }
    }
}

pub(super) fn check(
    id: &'static str,
    status: DiagnosticStatus,
    message: impl Into<String>,
    details: Option<String>,
) -> DiagnosticCheck {
    DiagnosticCheck {
        id,
        status,
        message: message.into(),
        details,
    }
}

#[cfg(test)]
mod tests;
