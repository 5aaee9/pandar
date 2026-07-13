use std::time::Duration;

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use pandar_core::{
    FirmwareAcknowledgement, FirmwareTerminalOutcome, PrinterFirmwareStatus, PrinterUpgradeState,
};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::machine::{
    FirmwareControlOutcome, FirmwareControlPhase, FirmwareExecuteRequest, FirmwareMachineGateway,
    FirmwareModulesDelivery, FirmwarePrepareRequest, FirmwarePreparedObservation,
    FirmwarePublishTransition, FirmwareRefreshRequest,
    mqtt::{
        FirmwareMqttCommand, FirmwareMqttSession, firmware_command_payload, firmware_mqtt_failure,
        firmware_mqtt_failure_phase, parse_firmware_acknowledgement,
    },
};

use super::RuntimeBambuMachineGateway;
use cancellation::FirmwarePumpCancellationGuard;

mod cancellation;

const CONTROL_ACK_TIMEOUT: Duration = Duration::from_secs(2);

#[async_trait]
impl FirmwareMachineGateway for RuntimeBambuMachineGateway {
    async fn refresh_firmware_version(
        &self,
        request: FirmwareRefreshRequest,
    ) -> anyhow::Result<FirmwareModulesDelivery> {
        super::firmware_refresh::refresh_firmware_version_with_connector(
            &self.firmware,
            request,
            self.report_timeout,
            &super::firmware_refresh::ProductionFirmwareSessionConnector {
                task_set: self.firmware_mqtt_tasks.clone(),
            },
        )
        .await
    }

    async fn prepare_firmware_control(
        &self,
        request: FirmwarePrepareRequest,
    ) -> anyhow::Result<FirmwarePreparedObservation> {
        self.firmware.prepare_firmware_control(request).await
    }

    async fn execute_firmware_control(
        &self,
        request: FirmwareExecuteRequest,
        phases: mpsc::UnboundedSender<FirmwareControlPhase>,
    ) -> anyhow::Result<FirmwareControlOutcome> {
        let pending_url = match &request.command {
            pandar_core::FirmwareCommand::Start { url, .. } => Some(url.clone()),
            _ => None,
        };
        let result = self.execute_firmware_control_inner(request, phases).await;
        result.map_err(|error| redact_pending_url(error, pending_url.as_deref()))
    }

    async fn cancel_firmware_session(&self, session_epoch: u64) -> anyhow::Result<()> {
        let teardown = self
            .firmware_mqtt_tasks
            .abort_and_join_all()
            .await
            .context("teardown firmware MQTT pumps for reverse session");
        self.firmware.cancel_firmware_session(session_epoch).await;
        teardown
    }
}

impl RuntimeBambuMachineGateway {
    async fn execute_firmware_control_inner(
        &self,
        request: FirmwareExecuteRequest,
        phases: mpsc::UnboundedSender<FirmwareControlPhase>,
    ) -> anyhow::Result<FirmwareControlOutcome> {
        let execution = self.firmware.claim_firmware_execute(&request).await?;
        let snapshot = self
            .firmware
            .snapshot(&request.serial)
            .await
            .ok_or_else(|| anyhow!("no firmware endpoint for printer {}", request.serial))?;
        let command = firmware_command_payload(&request.command);
        let pending_url = match &request.command {
            pandar_core::FirmwareCommand::Start { url, .. } => Some(url.as_str()),
            _ => None,
        };
        let mut session =
            FirmwareMqttSession::connect(&snapshot.endpoint, self.firmware_mqtt_tasks.clone())
                .await?;
        let transition =
            firmware_publish_transition_with_cleanup(&mut session, &execution, &snapshot.endpoint)
                .await?;
        complete_firmware_control_operation(
            &mut session,
            command,
            phases,
            pending_url,
            Some(transition),
        )
        .await
    }
}

pub(super) async fn firmware_publish_transition_with_cleanup(
    session: &mut FirmwareMqttSession,
    execution: &crate::machine::FirmwareExecutionLease,
    expected_endpoint: &crate::machine::BambuPrinterEndpoint,
) -> anyhow::Result<FirmwarePublishTransition> {
    let transition = match execution.publish_transition().await {
        Ok(transition) => transition,
        Err(error) => return shutdown_before_publish(session, error).await,
    };
    if transition.endpoint() != expected_endpoint {
        drop(transition);
        return shutdown_before_publish(
            session,
            anyhow!("printer firmware endpoint changed before publish"),
        )
        .await;
    }
    Ok(transition)
}

async fn shutdown_before_publish<T>(
    session: &mut FirmwareMqttSession,
    error: anyhow::Error,
) -> anyhow::Result<T> {
    match session.shutdown().await {
        Ok(()) => Err(error),
        Err(shutdown_error) => Err(error.context(format!(
            "shutdown fresh firmware MQTT session after pre-publish failure: {shutdown_error:#}"
        ))),
    }
}

#[cfg(test)]
pub(super) async fn complete_firmware_control_with_session(
    session: &mut FirmwareMqttSession,
    command: FirmwareMqttCommand,
    phases: mpsc::UnboundedSender<FirmwareControlPhase>,
    pending_url: Option<&str>,
) -> anyhow::Result<FirmwareControlOutcome> {
    complete_firmware_control_operation(session, command, phases, pending_url, None).await
}

#[cfg(test)]
pub(super) async fn complete_firmware_control_with_transition_for_test(
    session: &mut FirmwareMqttSession,
    command: FirmwareMqttCommand,
    phases: mpsc::UnboundedSender<FirmwareControlPhase>,
    transition: FirmwarePublishTransition,
) -> anyhow::Result<FirmwareControlOutcome> {
    complete_firmware_control_operation(session, command, phases, None, Some(transition)).await
}

async fn complete_firmware_control_operation(
    session: &mut FirmwareMqttSession,
    command: FirmwareMqttCommand,
    phases: mpsc::UnboundedSender<FirmwareControlPhase>,
    pending_url: Option<&str>,
    transition: Option<FirmwarePublishTransition>,
) -> anyhow::Result<FirmwareControlOutcome> {
    let mut cancellation_guard = transition
        .as_ref()
        .map(|_| FirmwarePumpCancellationGuard::new(session.pump_abort_handle()));
    let expected_command = command.command().to_owned();
    let expected_sequence_id = command.sequence_id().to_owned();
    let attempt = match transition {
        Some(transition) => session.publish_with_transition(command, transition),
        None => session.publish(command).await,
    };
    let mut attempt = match attempt {
        Ok(attempt) => attempt,
        Err(error) => {
            if let Some(guard) = cancellation_guard.take() {
                guard.disarm();
            }
            log_shutdown_failure(session.shutdown().await, pending_url);
            return Err(error);
        }
    };
    if let Err(error) = attempt.wait_published().await {
        if let Some(guard) = cancellation_guard.take() {
            guard.disarm();
        }
        log_shutdown_failure(session.shutdown().await, pending_url);
        return Err(error);
    }
    if let Some(guard) = cancellation_guard.take() {
        guard.disarm();
    }
    let outcome = if phases.send(FirmwareControlPhase::Published).is_err() {
        published_without_acknowledgement(
            anyhow!("firmware phase receiver closed after publish"),
            pending_url,
        )
    } else {
        match attempt.wait_matching_report(CONTROL_ACK_TIMEOUT).await {
            Ok(report) => match parse_firmware_acknowledgement(
                &report.payload,
                &expected_command,
                &expected_sequence_id,
            ) {
                Ok(Some(acknowledgement)) => {
                    let acknowledgement = redact_acknowledgement(acknowledgement, pending_url);
                    let transient_status = match parse_transient_status(&report.payload) {
                        Ok(status) => {
                            status.map(|status| redact_transient_status(status, pending_url))
                        }
                        Err(error) => {
                            tracing::warn!(
                                error = %redacted_error_text(&error, pending_url),
                                "ignore malformed transient firmware status after acknowledgement"
                            );
                            None
                        }
                    };
                    FirmwareControlOutcome {
                        terminal: FirmwareTerminalOutcome::Acknowledged { acknowledgement },
                        transient_status,
                    }
                }
                Ok(None) => published_without_acknowledgement(
                    anyhow!("matching firmware MQTT report had no acknowledgement"),
                    pending_url,
                ),
                Err(error) => published_without_acknowledgement(error, pending_url),
            },
            Err(error) => published_without_acknowledgement(error, pending_url),
        }
    };
    log_shutdown_failure(session.shutdown().await, pending_url);
    Ok(outcome)
}

fn published_without_acknowledgement(
    error: anyhow::Error,
    pending_url: Option<&str>,
) -> FirmwareControlOutcome {
    tracing::warn!(
        error = %redacted_error_text(&error, pending_url),
        "firmware MQTT control ended after publish; outcome unknown"
    );
    FirmwareControlOutcome {
        terminal: FirmwareTerminalOutcome::PublishedWithoutAcknowledgement,
        transient_status: None,
    }
}

fn log_shutdown_failure(result: anyhow::Result<()>, pending_url: Option<&str>) {
    if let Err(error) = result {
        tracing::warn!(
            error = %redacted_error_text(&error, pending_url),
            "firmware MQTT session shutdown was ambiguous"
        );
    }
}

fn redacted_error_text(error: &anyhow::Error, pending_url: Option<&str>) -> String {
    let message = format!("{error:#}");
    match pending_url.filter(|url| !url.is_empty()) {
        Some(url) => message.replace(url, "[redacted]"),
        None => message,
    }
}

fn redact_acknowledgement(
    mut acknowledgement: FirmwareAcknowledgement,
    pending_url: Option<&str>,
) -> FirmwareAcknowledgement {
    let Some(url) = pending_url.filter(|url| !url.is_empty()) else {
        return acknowledgement;
    };
    for text in [
        &mut acknowledgement.result,
        &mut acknowledgement.reason,
        &mut acknowledgement.message,
    ] {
        redact_optional_text(text, url);
    }
    acknowledgement
}

fn redact_transient_status(
    mut status: PrinterFirmwareStatus,
    pending_url: Option<&str>,
) -> PrinterFirmwareStatus {
    let Some(url) = pending_url.filter(|url| !url.is_empty()) else {
        return status;
    };
    redact_optional_text(&mut status.cfg, url);
    let Some(upgrade) = status.upgrade_state.as_mut() else {
        return status;
    };
    for text in [
        &mut upgrade.status,
        &mut upgrade.progress,
        &mut upgrade.message,
        &mut upgrade.module,
        &mut upgrade.ota_new_version_number,
        &mut upgrade.ams_new_version_number,
        &mut upgrade.ahb_new_version_number,
    ] {
        redact_optional_text(text, url);
    }
    if let Some(versions) = upgrade.new_versions.as_mut() {
        for version in versions {
            redact_text(&mut version.name, url);
            redact_optional_text(&mut version.current_version, url);
            redact_optional_text(&mut version.new_version, url);
        }
    }
    if let Some(ams) = upgrade.ams_firmware.as_mut() {
        redact_optional_text(&mut ams.status, url);
        if let Some(firmware) = ams.firmware.as_mut() {
            for descriptor in firmware {
                redact_text(&mut descriptor.name, url);
                redact_text(&mut descriptor.version, url);
            }
        }
    }
    status
}

fn redact_optional_text(text: &mut Option<String>, pending_url: &str) {
    if let Some(text) = text {
        redact_text(text, pending_url);
    }
}

fn redact_text(text: &mut String, pending_url: &str) {
    if text.contains(pending_url) {
        *text = text.replace(pending_url, "[redacted]");
    }
}

pub(super) fn redact_pending_url(error: anyhow::Error, pending_url: Option<&str>) -> anyhow::Error {
    let message = format!("{error:#}");
    let Some(url) = pending_url.filter(|url| !url.is_empty() && message.contains(url)) else {
        return error;
    };
    let message = message.replace(url, "[redacted]");
    match firmware_mqtt_failure_phase(&error) {
        Some(after_publish) => firmware_mqtt_failure(after_publish, message),
        None => anyhow!(message),
    }
}

fn parse_transient_status(
    report: &serde_json::Value,
) -> anyhow::Result<Option<PrinterFirmwareStatus>> {
    let envelope = serde_json::from_value::<TransientStatusEnvelope>(report.clone())
        .context("parse transient firmware command status")?;
    Ok(envelope.print.and_then(|print| {
        (print.upgrade_state.is_some() || print.cfg.is_some()).then_some(PrinterFirmwareStatus {
            upgrade_state: print.upgrade_state,
            cfg: print.cfg,
        })
    }))
}

#[derive(Deserialize)]
struct TransientStatusEnvelope {
    #[serde(default)]
    print: Option<TransientStatusFields>,
}

#[derive(Deserialize)]
struct TransientStatusFields {
    #[serde(default)]
    upgrade_state: Option<PrinterUpgradeState>,
    #[serde(default)]
    cfg: Option<String>,
}
