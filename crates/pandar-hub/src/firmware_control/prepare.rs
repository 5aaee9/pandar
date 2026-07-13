use std::time::Duration;

use pandar_core::{CommandStatus, FirmwareControlMetadata, TenantId};
use tokio::sync::oneshot;

use super::{
    FirmwareServiceError, PreparedFirmwareControl, begin_dispatch_ownership_fence,
    commit_current_session_fence, resolve_target, target_identity,
};
use crate::{
    AppState,
    protocol::agent::v1::{HubCommand, PrepareFirmwareControl, hub_command},
    repositories::{AuditActor, FirmwareCommandOwner},
};

const PREPARE_LIFETIME: Duration = Duration::from_secs(1);
const PREPARE_EXPIRY_REASON: &str = "firmware prepare expired before execute";

impl AppState {
    pub async fn prepare_control(
        &self,
        tenant_id: TenantId,
        printer_id: &str,
        metadata: FirmwareControlMetadata,
        actor: AuditActor,
    ) -> Result<PreparedFirmwareControl, FirmwareServiceError> {
        let target = resolve_target(self, tenant_id, printer_id).await?;
        let command = self
            .commands()
            .create_firmware_control_sent_with_audit(
                tenant_id,
                printer_id,
                target.agent_id,
                FirmwareCommandOwner {
                    session_id: target.token.persisted_id(),
                    instance_id: self.instance_id(),
                },
                metadata.clone(),
                actor,
            )
            .await
            .map_err(|error| {
                super::repository_error(error, "failed to persist firmware prepare command")
            })?;
        let identity = target_identity(&target, command.id);
        let expires_at = tokio::time::Instant::now() + PREPARE_LIFETIME;
        let expiry_state = self.clone();
        let expiry_identity = identity.clone();
        tokio::spawn(async move {
            tokio::time::sleep_until(expires_at).await;
            expiry_state
                .expire_firmware_prepare(expiry_identity, expires_at)
                .await;
        });
        #[cfg(test)]
        super::dispatch_ownership_pause::wait("prepare", &identity.printer_id).await;
        let fence = match begin_dispatch_ownership_fence(self, &identity).await {
            Ok(Some(fence)) => fence,
            Ok(None) => {
                drop(target);
                super::finish_unclaimed_pre_publish_failure(
                    self,
                    identity.command_id,
                    identity.tenant_id,
                    identity.agent_id,
                    "firmware prepare ownership changed before dispatch",
                )
                .await?;
                return Err(FirmwareServiceError::Unavailable);
            }
            Err(error) => {
                drop(target);
                if let Err(cleanup_error) = super::finish_unclaimed_pre_publish_failure(
                    self,
                    identity.command_id,
                    identity.tenant_id,
                    identity.agent_id,
                    "firmware prepare session fence could not be acquired",
                )
                .await
                {
                    tracing::error!(
                        command_id = %identity.command_id,
                        error = %format!("{cleanup_error:#}"),
                        "failed to persist firmware prepare fence failure"
                    );
                }
                return Err(error);
            }
        };
        if tokio::time::Instant::now() >= expires_at {
            let release = commit_current_session_fence(
                fence,
                target.token.persisted_id(),
                "failed to release expired firmware prepare session fence",
            )
            .await;
            drop(target);
            super::finish_unclaimed_pre_publish_failure(
                self,
                identity.command_id,
                identity.tenant_id,
                identity.agent_id,
                PREPARE_EXPIRY_REASON,
            )
            .await?;
            release?;
            return Err(FirmwareServiceError::CommandFailed {
                message: PREPARE_EXPIRY_REASON.to_owned(),
            });
        }
        let (waiter, receiver) = oneshot::channel();
        let prepared_token = self.sessions().begin_firmware_prepare_under_transition(
            identity.clone(),
            metadata,
            expires_at,
            waiter,
        );
        let outbound = HubCommand {
            command_id: command.id.to_string(),
            command: Some(hub_command::Command::PrepareFirmwareControl(
                PrepareFirmwareControl {
                    command_id: command.id.to_string(),
                    serial: target.serial.clone(),
                    expected_generation: target.generation,
                },
            )),
        };
        let dispatch_failed = target
            .dispatch
            .command_sender
            .try_send(Ok(outbound))
            .is_err();
        let dispatch_failure = dispatch_failed.then(|| {
            self.sessions()
                .claim_firmware_result_under_transition(&identity, None)
                .expect("new firmware prepare must remain pending until dispatch")
        });
        if let Err(error) = commit_current_session_fence(
            fence,
            target.token.persisted_id(),
            "failed to release firmware prepare dispatch session fence",
        )
        .await
        {
            let claimed = dispatch_failure.or_else(|| {
                self.sessions()
                    .claim_firmware_result_under_transition(&identity, None)
            });
            drop(target);
            if let Some(claimed) = claimed {
                super::finish_agent_failure(
                    self,
                    claimed,
                    "firmware prepare session fence failed after dispatch",
                )
                .await;
            }
            return Err(error);
        }
        if let Some(claimed) = dispatch_failure {
            drop(target);
            super::finish_pre_publish_failure(
                self,
                claimed,
                "firmware prepare could not be sent to the current agent session",
            )
            .await;
            return Err(FirmwareServiceError::CommandFailed {
                message: "firmware prepare could not be sent to the current agent session"
                    .to_owned(),
            });
        }
        drop(target);
        tokio::time::timeout_at(expires_at, receiver)
            .await
            .map_err(|_| FirmwareServiceError::CommandFailed {
                message: "firmware prepare expired before execute".to_owned(),
            })?
            .map_err(|error| {
                FirmwareServiceError::internal(
                    anyhow::Error::new(error)
                        .context("firmware prepare waiter closed before receiving an exact result"),
                )
            })??;
        Ok(PreparedFirmwareControl {
            command_id: command.id,
            prepared_token,
        })
    }

    async fn expire_firmware_prepare(
        &self,
        identity: crate::sessions::FirmwareCommandIdentity,
        expires_at: tokio::time::Instant,
    ) {
        let lease = self
            .sessions()
            .transition_lease_for_session(identity.agent_id, identity.session_token)
            .await;
        let Some(claimed) = self
            .sessions()
            .expire_firmware_prepare_under_transition(&identity, expires_at)
        else {
            if self
                .sessions()
                .pending_live_command_ids()
                .await
                .contains(&identity.command_id)
            {
                return;
            }
            drop(lease);
            match self
                .commands()
                .get_for_tenant(identity.tenant_id, identity.command_id)
                .await
            {
                Ok(Some(command)) if command.status == CommandStatus::Sent => {
                    if let Err(error) = super::finish_unclaimed_pre_publish_failure(
                        self,
                        identity.command_id,
                        identity.tenant_id,
                        identity.agent_id,
                        PREPARE_EXPIRY_REASON,
                    )
                    .await
                    {
                        tracing::error!(
                            command_id = %identity.command_id,
                            error = %format!("{error:#}"),
                            "failed to persist unregistered firmware prepare expiry"
                        );
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::error!(
                        command_id = %identity.command_id,
                        error = %format!("{:#}", anyhow::Error::new(error).context("failed to load unregistered firmware prepare during expiry")),
                        "failed to inspect unregistered firmware prepare expiry"
                    );
                }
            }
            return;
        };
        drop(lease);
        super::finish_pre_publish_failure(self, claimed, PREPARE_EXPIRY_REASON).await;
    }
}
