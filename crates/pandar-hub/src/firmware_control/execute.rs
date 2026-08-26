use pandar_core::{FirmwareCommand, TenantId};
use tokio::sync::{mpsc::error::TrySendError, oneshot};
use zeroize::Zeroize;

use super::execute_cancellation::{ExecuteCancellationOwner, wait_for_cleanup};
use super::{
    FirmwareExecuteResult, FirmwareServiceError, begin_dispatch_ownership_fence,
    commit_current_session_fence, identity_is_current, proto_firmware_command,
};
use crate::AppState;
use pandar_protocol::agent::v1::{
    ExecuteFirmwareControl, HubCommand, firmware_command, hub_command,
};

impl AppState {
    pub async fn execute_control(
        &self,
        tenant_id: TenantId,
        prepared_token: &str,
        command: FirmwareCommand,
    ) -> Result<FirmwareExecuteResult, FirmwareServiceError> {
        let identity = self
            .sessions()
            .firmware_token_locator(prepared_token)
            .filter(|identity| identity.tenant_id == tenant_id)
            .ok_or(FirmwareServiceError::InvalidPreparedToken)?;
        let lease = self
            .sessions()
            .transition_lease_for_session(identity.agent_id, identity.session_token)
            .await;
        if identity_is_current(self, &identity)
            .await
            .map_err(FirmwareServiceError::into_pre_publish)?
            .is_none()
        {
            return Err(FirmwareServiceError::Unavailable);
        }
        #[cfg(test)]
        super::execute_ownership_gap_pause::wait(identity.command_id).await;
        #[cfg(test)]
        super::dispatch_ownership_pause::wait("execute", &identity.printer_id).await;
        let fence = match begin_dispatch_ownership_fence(self, &identity).await {
            Ok(Some(fence)) => fence,
            Ok(None) => {
                drop(lease);
                return Err(FirmwareServiceError::Unavailable);
            }
            Err(error) => {
                let mut cancellation = ExecuteCancellationOwner::new(self, identity.clone());
                let cleanup = cancellation
                    .schedule_pre_publish("firmware execute ownership fence could not be acquired");
                drop(lease);
                wait_for_cleanup(cleanup).await;
                return Err(error.into_pre_publish());
            }
        };
        if let Err(error) = self
            .sessions()
            .validate_firmware_execute_under_transition(prepared_token, &command)
        {
            let reason = match &error {
                FirmwareServiceError::MetadataMismatch => {
                    let (waiter, _receiver) = oneshot::channel();
                    let mismatch = self.sessions().begin_firmware_execute_under_transition(
                        prepared_token,
                        &command,
                        waiter,
                    );
                    debug_assert!(matches!(
                        mismatch,
                        Err(FirmwareServiceError::MetadataMismatch)
                    ));
                    "prepared firmware metadata did not match execute command"
                }
                FirmwareServiceError::CommandFailed { .. } => {
                    "firmware URL redaction capacity exhausted"
                }
                _ => {
                    let release = commit_current_session_fence(
                        fence,
                        identity.session_token.persisted_id(),
                        "failed to release firmware execute validation fence",
                    )
                    .await;
                    drop(lease);
                    release?;
                    return Err(error);
                }
            };
            let mut cancellation = ExecuteCancellationOwner::new(self, identity.clone());
            let release = commit_current_session_fence(
                fence,
                identity.session_token.persisted_id(),
                "failed to release rejected firmware execute validation fence",
            )
            .await;
            drop(lease);
            let cleanup = cancellation.schedule_pre_publish(reason);
            wait_for_cleanup(cleanup).await;
            release?;
            return Err(error);
        }
        let (waiter, receiver) = oneshot::channel();
        self.sessions().begin_firmware_execute_under_transition(
            prepared_token,
            &command,
            waiter,
        )?;
        let mut cancellation = ExecuteCancellationOwner::new(self, identity.clone());
        if let Err(error) = self
            .commands()
            .mark_firmware_execute_sent_on(
                &fence,
                identity.command_id,
                identity.tenant_id,
                identity.agent_id,
            )
            .await
        {
            if let Err(rollback_error) = fence.rollback().await {
                tracing::error!(
                    command_id = %identity.command_id,
                    error = %format!("{:#}", anyhow::Error::new(rollback_error).context("failed to roll back firmware ExecuteSent phase transaction")),
                    "failed to release firmware execute session fence after persistence failure"
                );
            }
            tracing::error!(
                command_id = %identity.command_id,
                error = %format!("{:#}", anyhow::Error::new(error).context("failed to record firmware ExecuteSent phase")),
                "failed to persist firmware execute phase before dispatch"
            );
            let cleanup = cancellation.schedule_pre_publish(
                "firmware execute phase could not be recorded before dispatch",
            );
            drop(lease);
            wait_for_cleanup(cleanup).await;
            return receiver.await.map_err(|error| {
                FirmwareServiceError::internal_pre_publish(
                    anyhow::Error::new(error)
                        .context("firmware execute waiter closed after persistence failure"),
                )
            });
        }
        if let Err(error) = commit_current_session_fence(
            fence,
            identity.session_token.persisted_id(),
            "failed to commit durable firmware ExecuteSent phase",
        )
        .await
        {
            tracing::error!(
                command_id = %identity.command_id,
                error = %format!("{:#}", anyhow::Error::new(error).context("failed to commit firmware ExecuteSent phase before dispatch")),
                "failed to commit firmware execute phase before dispatch"
            );
            let cleanup = cancellation.schedule_pre_publish(
                "firmware execute phase could not be committed before dispatch",
            );
            drop(lease);
            wait_for_cleanup(cleanup).await;
            return receiver.await.map_err(|waiter_error| {
                FirmwareServiceError::internal_pre_publish(
                    anyhow::Error::new(waiter_error)
                        .context("firmware execute waiter closed after phase commit failure"),
                )
            });
        }
        #[cfg(test)]
        super::dispatch_ownership_pause::wait("execute-durable", &identity.printer_id).await;
        let Some(dispatch) = self
            .sessions()
            .current_firmware_dispatch(
                identity.tenant_id,
                identity.agent_id,
                identity.session_token,
            )
            .await
        else {
            let cleanup =
                cancellation.schedule_pre_publish("firmware execute owner changed before dispatch");
            drop(lease);
            wait_for_cleanup(cleanup).await;
            return receiver.await.map_err(|waiter_error| {
                FirmwareServiceError::internal_pre_publish(
                    anyhow::Error::new(waiter_error)
                        .context("firmware execute waiter closed after owner changed"),
                )
            });
        };
        let dispatch_fence = match begin_dispatch_ownership_fence(self, &identity).await {
            Ok(Some(fence)) => fence,
            Ok(None) => {
                let cleanup = cancellation.schedule_pre_publish(
                    "firmware execute printer ownership changed before dispatch",
                );
                drop(lease);
                wait_for_cleanup(cleanup).await;
                return receiver.await.map_err(|waiter_error| {
                    FirmwareServiceError::internal_pre_publish(
                        anyhow::Error::new(waiter_error)
                            .context("firmware execute waiter closed after printer reassignment"),
                    )
                });
            }
            Err(error) => {
                let cleanup = cancellation.schedule_pre_publish(
                    "firmware execute ownership fence could not be reacquired",
                );
                drop(lease);
                wait_for_cleanup(cleanup).await;
                return Err(error.into_pre_publish());
            }
        };
        let outbound = HubCommand {
            command_id: identity.command_id.to_string(),
            command: Some(hub_command::Command::ExecuteFirmwareControl(
                ExecuteFirmwareControl {
                    command_id: identity.command_id.to_string(),
                    serial: identity.serial.clone(),
                    expected_generation: identity.generation,
                    command: Some(proto_firmware_command(command)),
                },
            )),
        };
        let dispatch_failure = match dispatch.command_sender.try_send(Ok(outbound)) {
            Ok(()) => {
                cancellation.mark_dispatch_attempted();
                None
            }
            Err(error) => Some(DispatchFailure::from_send_error(error)),
        };
        if let Err(error) = commit_current_session_fence(
            dispatch_fence,
            identity.session_token.persisted_id(),
            "failed to release firmware execute second dispatch fence",
        )
        .await
        {
            let cleanup = if let Some(failure) = &dispatch_failure {
                let error = failure.message();
                cancellation.schedule_pre_publish_error(&error)
            } else {
                cancellation.schedule_outcome_unknown(
                    "firmware execute session fence failed after dispatch",
                )
            };
            drop(lease);
            wait_for_cleanup(cleanup).await;
            if dispatch_failure.is_none() {
                tracing::error!(
                    command_id = %identity.command_id,
                    error = %format!("{:#}", anyhow::Error::new(error).context("failed to commit firmware execute ownership fence after dispatch")),
                    "firmware execute dispatch fence failed after dispatch"
                );
                return receiver.await.map_err(|waiter_error| {
                    FirmwareServiceError::internal(
                        anyhow::Error::new(waiter_error)
                            .context("firmware execute waiter closed after session fence failure"),
                    )
                });
            }
            return Err(error.into_pre_publish());
        }
        if let Some(failure) = dispatch_failure {
            let error = failure.message();
            let cleanup = cancellation.schedule_pre_publish_error(&error);
            drop(lease);
            #[cfg(test)]
            super::dispatch_ownership_pause::wait("execute-rejected-claim", &identity.printer_id)
                .await;
            wait_for_cleanup(cleanup).await;
        } else {
            drop(lease);
        }
        match receiver.await {
            Ok(result) => {
                cancellation.disarm();
                Ok(result)
            }
            Err(error) => {
                let error = anyhow::Error::new(error)
                    .context("firmware execute waiter closed after execute was attempted");
                tracing::error!(
                    command_id = %identity.command_id,
                    error = %format!("{error:#}"),
                    "firmware execute result waiter closed after dispatch"
                );
                let cleanup = cancellation.schedule_outcome_unknown(
                    "firmware execute result unavailable; outcome unknown",
                );
                wait_for_cleanup(cleanup).await;
                Ok(FirmwareExecuteResult {
                    command_id: identity.command_id,
                    phase: crate::firmware_control::FirmwareExecutePhase::OutcomeUnknown,
                    outcome: None,
                    transient_status: None,
                    error: Some("firmware execute result unavailable; outcome unknown".to_owned()),
                })
            }
        }
    }
}

enum DispatchFailure {
    Full,
    Closed,
}

impl DispatchFailure {
    fn from_send_error(error: TrySendError<Result<HubCommand, tonic::Status>>) -> Self {
        match error {
            TrySendError::Full(mut outbound) => {
                zeroize_rejected_firmware_url(&mut outbound);
                Self::Full
            }
            TrySendError::Closed(mut outbound) => {
                zeroize_rejected_firmware_url(&mut outbound);
                Self::Closed
            }
        }
    }

    fn message(&self) -> String {
        let cause = match self {
            Self::Full => "current agent command queue is full",
            Self::Closed => "current agent command queue is closed",
        };
        format!(
            "{:#}",
            anyhow::Error::msg(cause)
                .context("firmware execute could not be sent to the current agent session")
        )
    }
}

fn zeroize_rejected_firmware_url(outbound: &mut Result<HubCommand, tonic::Status>) {
    let Ok(outbound) = outbound else {
        unreachable!("firmware execute dispatch always sends a command")
    };
    let Some(hub_command::Command::ExecuteFirmwareControl(execute)) = &mut outbound.command else {
        unreachable!("firmware execute dispatch always sends ExecuteFirmwareControl")
    };
    let Some(command) = &mut execute.command else {
        unreachable!("firmware execute dispatch always includes its typed command")
    };
    if let Some(firmware_command::Command::Start(start)) = &mut command.command {
        start.url.zeroize();
    }
}
