use super::*;

pub(super) async fn complete_refresh(
    state: &AppState,
    mut claimed: ClaimedFirmwareCommand,
    revision: u64,
    modules: Vec<crate::protocol::agent::v1::PrinterFirmwareModule>,
) -> Result<(), Status> {
    storage_value(revision, "module_revision")?;
    if modules.is_empty() {
        finish_pre_publish_failure(state, claimed, "fresh firmware refresh returned no modules")
            .await;
        return Ok(());
    }
    if !module_names_are_valid(&modules) {
        finish_pre_publish_failure(
            state,
            claimed,
            "fresh firmware refresh returned an empty module name",
        )
        .await;
        return Ok(());
    }
    let modules = modules.into_iter().map(core_module).collect::<Vec<_>>();
    let applied = match state
        .printers()
        .replace_modules_if_current(
            claimed.identity.tenant_id,
            claimed.identity.agent_id,
            &claimed.identity.session_token.persisted_id(),
            &claimed.identity.serial,
            claimed.identity.generation,
            revision,
            modules.clone(),
        )
        .await
    {
        Ok(applied) => applied,
        Err(error) => {
            let status = repository_status(error);
            finish_pre_publish_failure(state, claimed, "firmware refresh state persistence failed")
                .await;
            return Err(status);
        }
    };
    if applied != PrinterFirmwareUpdateOutcome::Applied {
        finish_pre_publish_failure(
            state,
            claimed,
            "fresh firmware modules were stale before persistence",
        )
        .await;
        return Ok(());
    }
    state
        .publish_printer_projection_change_for_serial(
            claimed.identity.tenant_id,
            &claimed.identity.serial,
        )
        .await;
    if let Err(error) = state
        .commands()
        .mark_firmware_terminal(
            claimed.identity.command_id,
            claimed.identity.tenant_id,
            claimed.identity.agent_id,
            CommandStatus::Succeeded,
            None,
            FirmwarePersistedResult {
                phase: FirmwarePersistedPhase::Refreshed,
                outcome: None,
                transient_status: None,
            },
        )
        .await
    {
        let status = repository_status(error);
        finish_pre_publish_failure(
            state,
            claimed,
            "firmware refresh terminal persistence failed",
        )
        .await;
        return Err(status);
    }
    if let Some(waiter) = claimed.refresh_waiter.take() {
        let _ = waiter.send(Ok(FirmwareRefreshResult {
            command_id: claimed.identity.command_id,
            modules,
            module_revision: revision,
        }));
    }
    Ok(())
}

pub(super) async fn complete_control(
    state: &AppState,
    mut claimed: ClaimedFirmwareCommand,
    phase: FirmwareExecutePhase,
    outcome: FirmwareTerminalOutcome,
    transient_status: Option<CoreFirmwareStatus>,
) -> Result<(), Status> {
    let (status, persisted_phase, error) = match phase {
        FirmwareExecutePhase::Acknowledged => (
            CommandStatus::Succeeded,
            FirmwarePersistedPhase::Acknowledged,
            None,
        ),
        FirmwareExecutePhase::Rejected => (
            CommandStatus::Failed,
            FirmwarePersistedPhase::Rejected,
            Some("printer rejected firmware command".to_owned()),
        ),
        FirmwareExecutePhase::OutcomeUnknown => (
            CommandStatus::Failed,
            FirmwarePersistedPhase::OutcomeUnknown,
            Some("firmware command published without acknowledgement; outcome unknown".to_owned()),
        ),
        FirmwareExecutePhase::PrePublishFailure => unreachable!("terminal result is post execute"),
    };
    let persisted = state
        .commands()
        .mark_firmware_terminal(
            claimed.identity.command_id,
            claimed.identity.tenant_id,
            claimed.identity.agent_id,
            status,
            error.clone(),
            FirmwarePersistedResult {
                phase: persisted_phase,
                outcome: Some(outcome.clone()),
                transient_status: transient_status.clone(),
            },
        )
        .await;
    if let Err(error) = persisted {
        let error = anyhow::Error::new(error)
            .context("failed to persist typed firmware result after execute was attempted");
        tracing::error!(
            command_id = %claimed.identity.command_id,
            error = %format!("{error:#}"),
            "firmware execute result persistence failed"
        );
        if let Err(fallback_error) = state
            .commands()
            .mark_firmware_terminal(
                claimed.identity.command_id,
                claimed.identity.tenant_id,
                claimed.identity.agent_id,
                CommandStatus::Failed,
                Some("firmware result persistence failed; outcome unknown".to_owned()),
                FirmwarePersistedResult {
                    phase: FirmwarePersistedPhase::OutcomeUnknown,
                    outcome: None,
                    transient_status: None,
                },
            )
            .await
        {
            tracing::error!(
                command_id = %claimed.identity.command_id,
                error = %format!("{:#}", anyhow::Error::new(fallback_error).context("failed to persist firmware outcome-unknown fallback")),
                "firmware execute fallback persistence failed"
            );
        }
        if let Some(waiter) = claimed.execute_waiter.take() {
            let _ = waiter.send(FirmwareExecuteResult {
                command_id: claimed.identity.command_id,
                phase: FirmwareExecutePhase::OutcomeUnknown,
                outcome: None,
                transient_status: None,
                error: Some("firmware result persistence failed; outcome unknown".to_owned()),
            });
        }
        return Err(Status::internal(
            "failed to persist firmware command result",
        ));
    }
    if let Some(waiter) = claimed.execute_waiter.take() {
        let _ = waiter.send(FirmwareExecuteResult {
            command_id: claimed.identity.command_id,
            phase,
            outcome: Some(outcome),
            transient_status,
            error,
        });
    }
    Ok(())
}
