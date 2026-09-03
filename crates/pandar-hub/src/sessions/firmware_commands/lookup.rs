use super::*;

impl SessionRegistry {
    pub(crate) fn firmware_command_locator(
        &self,
        command_id: CommandId,
    ) -> Option<FirmwareCommandIdentity> {
        self.firmware_commands
            .lock()
            .commands
            .get(&command_id)
            .map(|command| command.identity.clone())
    }

    pub(crate) fn firmware_token_locator(
        &self,
        prepared_token: &str,
    ) -> Option<FirmwareCommandIdentity> {
        let state = self.firmware_commands.lock();
        let command_id = state.prepared_tokens.get(prepared_token)?;
        state
            .commands
            .get(command_id)
            .map(|command| command.identity.clone())
    }

    pub(crate) fn redact_firmware_text_under_transition(
        &self,
        identity: &FirmwareCommandIdentity,
        value: &str,
    ) -> String {
        let state = self.firmware_commands.lock();
        store::redact_firmware_text_for_scope(&state, identity.tenant_id, &identity.serial, value)
    }

    pub(crate) fn redact_firmware_snapshot_text_under_transition(
        &self,
        tenant_id: TenantId,
        serial: &str,
        value: &str,
    ) -> String {
        let state = self.firmware_commands.lock();
        store::redact_firmware_text_for_scope(&state, tenant_id, serial, value)
    }

    pub(crate) fn pending_firmware_command_ids(&self) -> Vec<CommandId> {
        let mut ids = self
            .firmware_commands
            .lock()
            .commands
            .keys()
            .copied()
            .collect::<Vec<_>>();
        ids.extend(
            self.firmware_commands
                .completing
                .lock()
                .expect("completing firmware commands mutex should not be poisoned")
                .iter()
                .copied(),
        );
        ids
    }

    #[cfg(test)]
    pub(crate) fn pending_firmware_command_ids_in_storage_order(&self) -> Vec<CommandId> {
        self.firmware_commands
            .lock()
            .commands
            .keys()
            .copied()
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn retain_firmware_redaction_url_for_tests(
        &self,
        identity: &FirmwareCommandIdentity,
        url: &str,
    ) -> Result<(), FirmwareServiceError> {
        store::reserve_firmware_redaction_url(&mut self.firmware_commands.lock(), identity, url)
    }

    #[cfg(test)]
    pub(crate) fn retained_firmware_redaction_url_count(
        &self,
        identity: &FirmwareCommandIdentity,
    ) -> usize {
        self.firmware_commands
            .lock()
            .retained_redaction_urls
            .get(&RetainedFirmwareScope {
                tenant_id: identity.tenant_id,
                serial: identity.serial.clone(),
            })
            .map_or(0, Vec::len)
    }
}
