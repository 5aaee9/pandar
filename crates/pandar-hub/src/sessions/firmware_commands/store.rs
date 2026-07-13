use super::*;

impl PendingFirmwareCommands {
    pub(super) fn lock(&self) -> std::sync::MutexGuard<'_, PendingFirmwareState> {
        self.inner
            .lock()
            .expect("pending firmware commands mutex should not be poisoned")
    }

    pub(super) fn insert(&self, command: PendingFirmwareCommand) {
        let mut state = self.lock();
        if let Some(token) = &command.prepared_token {
            state
                .prepared_tokens
                .insert(token.clone(), command.identity.command_id);
        }
        state.commands.insert(command.identity.command_id, command);
    }

    pub(super) fn remove_exact(
        &self,
        identity: &FirmwareCommandIdentity,
        error: Option<&str>,
    ) -> Option<ClaimedFirmwareCommand> {
        let mut state = self.lock();
        if state
            .commands
            .get(&identity.command_id)
            .is_none_or(|command| command.identity != *identity)
        {
            return None;
        }
        let mut claimed = remove_command(&mut state, identity.command_id, error);
        claimed._completion_ownership = Some(self.begin_completion(identity.command_id));
        Some(claimed)
    }

    pub(super) fn remove_matching(
        &self,
        predicate: impl Fn(&FirmwareCommandIdentity) -> bool,
    ) -> Vec<ClaimedFirmwareCommand> {
        let mut state = self.lock();
        let ids = state
            .commands
            .values()
            .filter(|command| predicate(&command.identity))
            .map(|command| command.identity.command_id)
            .collect::<Vec<_>>();
        ids.into_iter()
            .map(|command_id| {
                let mut claimed = remove_command(&mut state, command_id, None);
                claimed._completion_ownership = Some(self.begin_completion(command_id));
                claimed
            })
            .collect()
    }

    pub(super) fn begin_completion(&self, command_id: CommandId) -> FirmwareCompletionOwnership {
        assert!(
            self.completing
                .lock()
                .expect("completing firmware commands mutex should not be poisoned")
                .insert(command_id),
            "firmware command completion must have one exclusive owner"
        );
        FirmwareCompletionOwnership {
            command_id,
            completing: self.completing.clone(),
        }
    }
}

pub(super) fn remove_command(
    state: &mut PendingFirmwareState,
    command_id: CommandId,
    error: Option<&str>,
) -> ClaimedFirmwareCommand {
    let identity = &state
        .commands
        .get(&command_id)
        .expect("pending firmware command must exist")
        .identity;
    let redacted_error = error.map(|error| {
        redact_firmware_text_for_scope(state, identity.tenant_id, &identity.serial, error)
    });
    let mut command = state
        .commands
        .remove(&command_id)
        .expect("pending firmware command must exist");
    if let Some(token) = command.prepared_token.take() {
        take_prepared_token(state, token.as_str());
    }
    drop(command.transient_url.take());
    ClaimedFirmwareCommand {
        identity: command.identity,
        kind: match command.kind {
            PendingFirmwareKind::Refresh => ClaimedFirmwareKind::Refresh,
            PendingFirmwareKind::Control(_) => ClaimedFirmwareKind::Control,
        },
        phase: command.phase,
        prepare_waiter: command.prepare_waiter,
        refresh_waiter: command.refresh_waiter,
        execute_waiter: command.execute_waiter,
        redacted_error,
        _completion_ownership: None,
    }
}

pub(super) fn take_prepared_token(
    state: &mut PendingFirmwareState,
    prepared_token: &str,
) -> Option<CommandId> {
    state
        .prepared_tokens
        .remove_entry(prepared_token)
        .map(|(_, command_id)| command_id)
}

pub(super) fn firmware_url(command: &FirmwareCommand) -> Option<FirmwareSecret> {
    match command {
        FirmwareCommand::Start { url, .. } => Some(FirmwareSecret::from(url.clone())),
        _ => None,
    }
}

pub(super) fn redact_firmware_text_for_scope(
    state: &PendingFirmwareState,
    tenant_id: TenantId,
    serial: &str,
    value: &str,
) -> String {
    let urls = state.commands.values().filter_map(|command| {
        let identity = &command.identity;
        (identity.tenant_id == tenant_id && identity.serial == serial)
            .then_some(command.transient_url.as_deref())
            .flatten()
    });
    let retained_urls = state.retained_redaction_urls.iter().filter_map(|retained| {
        (retained.tenant_id == tenant_id && retained.serial == serial)
            .then_some(retained.url.as_str())
    });
    redact_firmware_text_with_urls(value, urls.chain(retained_urls))
}

pub(super) fn reserve_firmware_redaction_url(
    state: &mut PendingFirmwareState,
    identity: &FirmwareCommandIdentity,
    url: &str,
) -> Result<(), FirmwareServiceError> {
    if state.retained_redaction_urls.iter().any(|retained| {
        retained.tenant_id == identity.tenant_id
            && retained.serial == identity.serial
            && retained.url.as_str() == url
    }) {
        return Ok(());
    }
    state.retained_redaction_urls.push(RetainedFirmwareUrl {
        tenant_id: identity.tenant_id,
        serial: identity.serial.clone(),
        url: FirmwareSecret::from(url.to_owned()),
    });
    Ok(())
}

pub(super) fn redact_firmware_text_with_urls<'a>(
    value: &str,
    urls: impl IntoIterator<Item = &'a str>,
) -> String {
    let mut secrets = Vec::<FirmwareSecret>::new();
    for url in urls.into_iter().filter(|url| !url.is_empty()) {
        secrets.push(FirmwareSecret::from(url.to_owned()));
        collect_firmware_url_components(url, &mut secrets);
    }
    secrets.sort_unstable_by(|left, right| {
        right
            .len()
            .cmp(&left.len())
            .then_with(|| left.as_str().cmp(right.as_str()))
    });
    secrets.dedup();
    let mut redacted = FirmwareSecret::from(value.to_owned());
    for secret in secrets {
        redacted = FirmwareSecret::from(redacted.replace(secret.as_str(), "[redacted]"));
    }
    crate::redaction::redact_secrets(redacted.as_str())
}

fn collect_firmware_url_components(url: &str, secrets: &mut Vec<FirmwareSecret>) {
    if let Ok(parsed) = reqwest::Url::parse(url) {
        push_firmware_secret(secrets, parsed.username());
        if let Some(password) = parsed.password() {
            push_firmware_secret(secrets, password);
        }
        if parsed.path() != "/" {
            push_firmware_secret(secrets, parsed.path());
        }
        if let Some(query) = parsed.query() {
            push_firmware_secret(secrets, query);
            for parameter in query.split('&') {
                if let Some((_, value)) = parameter.split_once('=') {
                    push_firmware_secret(secrets, value);
                }
            }
        }
        for (key, value) in parsed.query_pairs() {
            if let std::borrow::Cow::Owned(key) = key {
                drop(FirmwareSecret::from(key));
            }
            match value {
                std::borrow::Cow::Borrowed(value) => push_firmware_secret(secrets, value),
                std::borrow::Cow::Owned(value) => push_owned_firmware_secret(secrets, value),
            }
        }
        if let Some(fragment) = parsed.fragment() {
            push_firmware_secret(secrets, fragment);
        }
        drop(FirmwareSecret::from(String::from(parsed)));
    }
}

fn push_firmware_secret(secrets: &mut Vec<FirmwareSecret>, secret: &str) {
    if !secret.is_empty() {
        secrets.push(FirmwareSecret::from(secret.to_owned()));
    }
}

fn push_owned_firmware_secret(secrets: &mut Vec<FirmwareSecret>, secret: String) {
    if secret.is_empty() {
        drop(FirmwareSecret::from(secret));
    } else {
        secrets.push(FirmwareSecret::from(secret));
    }
}
