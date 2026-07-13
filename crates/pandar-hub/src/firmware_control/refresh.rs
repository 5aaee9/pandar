use pandar_core::TenantId;
use tokio::sync::oneshot;

use super::{
    FirmwareRefreshResult, FirmwareServiceError, begin_dispatch_ownership_fence,
    commit_current_session_fence, resolve_target, target_identity,
};
use crate::{
    AppState,
    protocol::agent::v1::{HubCommand, RefreshFirmwareVersion, hub_command},
    repositories::{AuditActor, FirmwareCommandOwner},
};

impl AppState {
    pub async fn refresh_version(
        &self,
        tenant_id: TenantId,
        printer_id: &str,
        sequence_id: String,
        actor: AuditActor,
    ) -> Result<FirmwareRefreshResult, FirmwareServiceError> {
        let target = resolve_target(self, tenant_id, printer_id).await?;
        let command = self
            .commands()
            .create_firmware_refresh_sent_with_audit(
                tenant_id,
                printer_id,
                target.agent_id,
                FirmwareCommandOwner {
                    session_id: target.token.persisted_id(),
                    instance_id: self.instance_id(),
                },
                sequence_id.clone(),
                actor,
            )
            .await
            .map_err(|error| {
                super::repository_error(error, "failed to persist firmware refresh command")
            })?;
        let identity = target_identity(&target, command.id);
        #[cfg(test)]
        super::dispatch_ownership_pause::wait("refresh", &identity.printer_id).await;
        let fence = match begin_dispatch_ownership_fence(self, &identity).await {
            Ok(Some(fence)) => fence,
            Ok(None) => {
                drop(target);
                super::finish_unclaimed_pre_publish_failure(
                    self,
                    identity.command_id,
                    identity.tenant_id,
                    identity.agent_id,
                    "firmware refresh ownership changed before dispatch",
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
                    "firmware refresh session fence could not be acquired",
                )
                .await
                {
                    tracing::error!(
                        command_id = %identity.command_id,
                        error = %format!("{cleanup_error:#}"),
                        "failed to persist firmware refresh fence failure"
                    );
                }
                return Err(error);
            }
        };
        let (waiter, receiver) = oneshot::channel();
        self.sessions()
            .begin_firmware_refresh_under_transition(identity.clone(), waiter);
        let outbound = HubCommand {
            command_id: command.id.to_string(),
            command: Some(hub_command::Command::RefreshFirmwareVersion(
                RefreshFirmwareVersion {
                    serial: target.serial.clone(),
                    sequence_id,
                    expected_generation: target.generation,
                },
            )),
        };
        let dispatch_failure = if target
            .dispatch
            .command_sender
            .try_send(Ok(outbound))
            .is_err()
        {
            Some(
                self.sessions()
                    .claim_firmware_result_under_transition(&identity, None)
                    .expect("new firmware refresh must remain pending until dispatch"),
            )
        } else {
            None
        };
        if let Err(error) = commit_current_session_fence(
            fence,
            target.token.persisted_id(),
            "failed to release firmware refresh dispatch session fence",
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
                    "firmware refresh session fence failed after dispatch",
                )
                .await;
            }
            return Err(error);
        }
        drop(target);
        if let Some(claimed) = dispatch_failure {
            super::finish_pre_publish_failure(
                self,
                claimed,
                "firmware refresh could not be sent to the current agent session",
            )
            .await;
        }
        receiver.await.map_err(|error| {
            FirmwareServiceError::internal(
                anyhow::Error::new(error)
                    .context("firmware refresh waiter closed before receiving an exact result"),
            )
        })?
    }
}
