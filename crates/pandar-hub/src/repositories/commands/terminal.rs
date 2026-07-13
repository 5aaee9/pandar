use super::*;

impl CommandRepository {
    pub(super) async fn guard_generic_terminal_transition(
        &self,
        transition: TerminalCommandTransition,
    ) -> RepositoryResult<CommandRecord> {
        let command = self
            .load_owned(
                transition.command_id,
                transition.tenant_id,
                transition.agent_id,
            )
            .await?;
        if matches!(
            command.kind.as_str(),
            "firmware_refresh" | "firmware_control"
        ) {
            return Err(RepositoryError::InvalidCommandTransition {
                from: command.status.as_str().to_owned(),
                action: "finish firmware command through generic transition",
            });
        }
        self.guard_terminal_transition(transition).await
    }

    pub(super) async fn guard_terminal_transition(
        &self,
        transition: TerminalCommandTransition,
    ) -> RepositoryResult<CommandRecord> {
        let updated = transitions::update_status_if_current(
            &self.database,
            transitions::StatusTransition {
                command_id: transition.command_id,
                tenant_id: transition.tenant_id,
                agent_id: transition.agent_id,
                status: transition.terminal_status.clone(),
                error: transition.error,
                result_json: transition.result_json,
                allowed_statuses: &[CommandStatus::Sent, CommandStatus::Acknowledged],
            },
        )
        .await?;
        let command = self
            .load_owned(
                transition.command_id,
                transition.tenant_id,
                transition.agent_id,
            )
            .await?;

        if updated || command.status == transition.terminal_status {
            return Ok(command);
        }

        Err(invalid_transition(command.status, transition.action))
    }
}
