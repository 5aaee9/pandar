use super::store::firmware_url;
use super::*;

impl SessionRegistry {
    pub(crate) fn begin_firmware_execute_under_transition(
        &self,
        prepared_token: &str,
        command: &FirmwareCommand,
        waiter: ExecuteWaiter,
    ) -> Result<FirmwareCommandIdentity, FirmwareServiceError> {
        let mut state = self.firmware_commands.lock();
        let Some(command_id) = state.prepared_tokens.get(prepared_token).copied() else {
            return Err(FirmwareServiceError::InvalidPreparedToken);
        };
        let metadata_matches = {
            let pending = state
                .commands
                .get(&command_id)
                .expect("prepared token must reference pending firmware command");
            if pending.phase != PendingFirmwarePhase::Prepared || prepare_expired(pending) {
                return Err(FirmwareServiceError::InvalidPreparedToken);
            }
            let PendingFirmwareKind::Control(metadata) = &pending.kind else {
                return Err(FirmwareServiceError::InvalidPreparedToken);
            };
            *metadata == FirmwareControlMetadata::from(command)
        };
        if !metadata_matches {
            store::take_prepared_token(&mut state, prepared_token);
            let pending = state
                .commands
                .get_mut(&command_id)
                .expect("mismatched prepared token must reference pending firmware command");
            drop(pending.prepared_token.take());
            pending.expires_at = None;
            return Err(FirmwareServiceError::MetadataMismatch);
        }
        let identity = state
            .commands
            .get(&command_id)
            .expect("validated prepared token must reference pending firmware command")
            .identity
            .clone();
        if let Some(url) = firmware_url(command) {
            store::reserve_firmware_redaction_url(&mut state, &identity, &url)?;
            state
                .commands
                .get_mut(&command_id)
                .expect("validated prepared token must reference pending firmware command")
                .transient_url = Some(url);
        }
        store::take_prepared_token(&mut state, prepared_token);
        let pending = state
            .commands
            .get_mut(&command_id)
            .expect("validated prepared token must reference pending firmware command");
        drop(pending.prepared_token.take());
        pending.expires_at = None;
        pending.execute_waiter = Some(waiter);
        pending.phase = PendingFirmwarePhase::ExecuteSent;
        Ok(pending.identity.clone())
    }

    pub(crate) fn validate_firmware_execute_under_transition(
        &self,
        prepared_token: &str,
        command: &FirmwareCommand,
    ) -> Result<FirmwareCommandIdentity, FirmwareServiceError> {
        let mut state = self.firmware_commands.lock();
        let Some(command_id) = state.prepared_tokens.get(prepared_token).copied() else {
            return Err(FirmwareServiceError::InvalidPreparedToken);
        };
        let identity = {
            let pending = state
                .commands
                .get(&command_id)
                .expect("prepared token must reference pending firmware command");
            if pending.phase != PendingFirmwarePhase::Prepared || prepare_expired(pending) {
                return Err(FirmwareServiceError::InvalidPreparedToken);
            }
            let PendingFirmwareKind::Control(metadata) = &pending.kind else {
                return Err(FirmwareServiceError::InvalidPreparedToken);
            };
            if *metadata != FirmwareControlMetadata::from(command) {
                return Err(FirmwareServiceError::MetadataMismatch);
            }
            pending.identity.clone()
        };
        if let Some(url) = firmware_url(command) {
            store::reserve_firmware_redaction_url(&mut state, &identity, &url)?;
            state
                .commands
                .get_mut(&command_id)
                .expect("validated prepared token must reference pending firmware command")
                .transient_url = Some(url);
        }
        Ok(identity)
    }

    pub(crate) fn mark_firmware_published_under_transition(
        &self,
        identity: &FirmwareCommandIdentity,
    ) -> bool {
        let mut state = self.firmware_commands.lock();
        let Some(command) = state.commands.get_mut(&identity.command_id) else {
            return false;
        };
        if command.identity != *identity || command.phase != PendingFirmwarePhase::ExecuteSent {
            return false;
        }
        command.phase = PendingFirmwarePhase::Published;
        true
    }
}
