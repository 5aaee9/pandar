use pandar_core::{AgentId, BambuDeviceFeature, CommandRecord, CommandStatus, TenantId};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tonic::Status;

use super::{
    CommandConversionOptions, agent_capabilities, conversion::persisted_printer_operation_payload,
    hub_command_from_record_with_options, mark_sent_and_job, repository_status,
};
use crate::{
    AppState,
    protocol::agent::v1::{AgentCapability, DeviceFeature, HubCommand},
    repositories::PrinterOperationPayload,
    sessions::SessionToken,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequiredDeviceFeature {
    BambuMqttHoming,
    BambuMqttAxisControl,
}

impl RequiredDeviceFeature {
    pub(crate) const fn bambu_feature(self) -> BambuDeviceFeature {
        match self {
            Self::BambuMqttHoming => BambuDeviceFeature::MqttHoming,
            Self::BambuMqttAxisControl => BambuDeviceFeature::MqttAxisControl,
        }
    }

    pub(crate) const fn proto_value(self) -> i32 {
        match self {
            Self::BambuMqttHoming => DeviceFeature::BambuMqttHoming as i32,
            Self::BambuMqttAxisControl => DeviceFeature::BambuMqttAxisControl as i32,
        }
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::BambuMqttHoming => "bambu_mqtt_homing",
            Self::BambuMqttAxisControl => "bambu_mqtt_axis_control",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionQueuedDispatch {
    Sent,
    FailedAndContinue,
    Empty,
    SessionEnded,
    ChannelClosed,
}

pub(crate) async fn finalize_required_features_for_closing_session(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
) -> Result<(), Status> {
    let _lease = state
        .sessions()
        .transition_lease_for_session(agent_id, token)
        .await;
    if state
        .sessions()
        .current_token(tenant_id, agent_id)
        .await
        .is_some()
    {
        return Ok(());
    }

    let commands = state
        .commands()
        .queued_for_agent_in_order(tenant_id, agent_id)
        .await
        .map_err(repository_status)?;
    for command in commands {
        let operation = match persisted_printer_operation_payload(&command) {
            Ok(Some(operation)) => operation,
            Ok(None) => continue,
            Err(err) => {
                fail_queued_command(
                    state,
                    tenant_id,
                    agent_id,
                    &command,
                    format!(
                        "required device feature gate failed: persisted printer operation payload is invalid: {err:#}"
                    ),
                )
                .await?;
                continue;
            }
        };
        if operation.operation.required_device_features().is_empty() {
            continue;
        }
        fail_queued_command(
            state,
            tenant_id,
            agent_id,
            &command,
            "required device feature gate failed: exact agent session is no longer current"
                .to_owned(),
        )
        .await?;
    }
    Ok(())
}

pub(crate) async fn dispatch_next_queued_for_session(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    command_sender: &mpsc::Sender<Result<HubCommand, Status>>,
    options: CommandConversionOptions,
) -> Result<SessionQueuedDispatch, Status> {
    let _lease = state
        .sessions()
        .transition_lease_for_session(agent_id, token)
        .await;
    let current = state.sessions().is_current(agent_id, token).await;
    let Some(command) = state
        .commands()
        .next_queued_for_agent(tenant_id, agent_id)
        .await
        .map_err(repository_status)?
    else {
        return Ok(if current {
            SessionQueuedDispatch::Empty
        } else {
            SessionQueuedDispatch::SessionEnded
        });
    };

    #[cfg(test)]
    pause::wait(token, pause::Phase::AfterQueuedRowRead).await;

    let operation = match persisted_printer_operation_payload(&command) {
        Ok(operation) => operation,
        Err(err) => {
            tracing::error!(
                command_id = %command.id,
                error = %format!("{err:#}"),
                "failed to deserialize queued printer operation command payload"
            );
            fail_queued_command(
                state,
                tenant_id,
                agent_id,
                &command,
                format!(
                    "required device feature gate failed: persisted printer operation payload is invalid: {err:#}"
                ),
            )
            .await?;
            return Ok(SessionQueuedDispatch::FailedAndContinue);
        }
    };
    let failure = agent_capabilities::queued_command_gate_failure(
        state,
        tenant_id,
        agent_id,
        token,
        current,
        operation.as_ref(),
    )
    .await;
    let failure = match failure {
        Some(failure) => Some(failure),
        None => required_feature_gate_failure(
            state,
            tenant_id,
            agent_id,
            token,
            current,
            &command,
            operation.as_ref(),
        )
        .await?
        .map(|failure| format!("required device feature gate failed: {failure}")),
    };

    #[cfg(test)]
    pause::wait(token, pause::Phase::AfterFeatureValidation).await;

    if let Some(failure) = failure {
        fail_queued_command(state, tenant_id, agent_id, &command, failure).await?;
        return Ok(SessionQueuedDispatch::FailedAndContinue);
    }
    if !current {
        return Ok(SessionQueuedDispatch::SessionEnded);
    }

    let hub_command = hub_command_from_record_with_options(command.clone(), options)?;
    if let Err(status) = mark_sent_and_job(state, command.clone(), tenant_id, agent_id).await {
        if print_command_was_cancelled(state, tenant_id, agent_id, &command).await? {
            tracing::debug!(
                command_id = %command.id,
                "skipped queued print command cancelled before dispatch CAS"
            );
            return Ok(SessionQueuedDispatch::FailedAndContinue);
        }
        return Err(status);
    }

    #[cfg(test)]
    pause::wait(token, pause::Phase::BeforeChannelSend).await;

    if command_sender.send(Ok(hub_command)).await.is_err() {
        return Ok(SessionQueuedDispatch::ChannelClosed);
    }
    Ok(SessionQueuedDispatch::Sent)
}

async fn print_command_was_cancelled(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    command: &CommandRecord,
) -> Result<bool, Status> {
    if command.kind != "print_project_file" {
        return Ok(false);
    }
    let persisted = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .map_err(repository_status)?;
    Ok(persisted.is_some_and(|persisted| {
        persisted.agent_id == agent_id && persisted.status == CommandStatus::Cancelled
    }))
}

async fn required_feature_gate_failure(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    current: bool,
    command: &CommandRecord,
    operation: Option<&PrinterOperationPayload>,
) -> Result<Option<String>, Status> {
    let Some(operation) = operation else {
        return Ok(None);
    };
    let required = operation.operation.required_device_features();
    if required.is_empty() {
        return Ok(None);
    }
    if !operation.operation.has_valid_required_device_features() {
        return Ok(Some(
            "persisted printer operation has invalid required-device-feature semantics".to_owned(),
        ));
    }
    if !current {
        return Ok(Some("exact agent session is no longer current".to_owned()));
    }
    if state
        .sessions()
        .current_token_for_capability(tenant_id, agent_id, AgentCapability::RequiredDeviceFeatures)
        .await
        != Some(token)
    {
        return Ok(Some(
            "current agent session does not advertise required-device-features capability"
                .to_owned(),
        ));
    }
    let Some(printer_id) = command.printer_id.as_deref() else {
        return Ok(Some(
            "printer operation is missing its owned printer".to_owned(),
        ));
    };
    if printer_id != operation.printer_id {
        return Ok(Some(
            "printer operation payload does not match its owned printer".to_owned(),
        ));
    }
    let Some(printer) = state
        .printers()
        .get_for_tenant(tenant_id, printer_id)
        .await
        .map_err(repository_status)?
    else {
        return Ok(Some("owned printer no longer exists".to_owned()));
    };
    if printer.agent_id != agent_id {
        return Ok(Some(
            "owned printer belongs to a different agent".to_owned(),
        ));
    }
    let persisted_id = token.persisted_id();
    match printer.bambu_device_features_session_id.as_deref() {
        None => {
            return Ok(Some(
                "printer feature observation has no agent-session marker".to_owned(),
            ));
        }
        Some(marker) if marker != persisted_id => {
            return Ok(Some(
                "printer feature observation belongs to a different agent session".to_owned(),
            ));
        }
        Some(_) => {}
    }
    let Some(features) = printer.bambu_device_features else {
        return Ok(Some("printer feature bitmap is missing".to_owned()));
    };
    if let Some(missing) = required
        .iter()
        .copied()
        .find(|required| !features.contains(required.bambu_feature()))
    {
        return Ok(Some(format!(
            "printer feature bitmap is missing {}",
            missing.as_str()
        )));
    }
    Ok(None)
}

async fn fail_queued_command(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    command: &CommandRecord,
    failure: String,
) -> Result<(), Status> {
    state
        .commands()
        .fail_queued_printer_operation(command.id, tenant_id, agent_id, failure)
        .await
        .map_err(repository_status)?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod pause;
