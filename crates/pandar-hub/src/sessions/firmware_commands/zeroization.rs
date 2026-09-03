use super::*;
use zeroize::ZeroizeOnDrop;

fn assert_zeroize_on_drop<T: ZeroizeOnDrop>() {}

#[test]
fn firmware_secret_storage_types_zeroize_on_drop() {
    assert_zeroize_on_drop::<FirmwareSecret>();

    let state = PendingFirmwareState::default();
    assert_key_type(state.prepared_tokens.keys().next());
    assert_pending_types(&firmware_command_with_secrets(firmware_identity()));
    assert_reference_type(&FirmwareSecret::from(
        "https://firmware.invalid/retained".to_owned(),
    ));
}

#[test]
fn firmware_secret_hash_matches_borrowed_str_lookup() {
    let command_id = CommandId::new();
    let mut prepared_tokens = HashMap::new();
    prepared_tokens.insert(
        FirmwareSecret::from("prepared-secret".to_owned()),
        command_id,
    );

    assert_eq!(prepared_tokens.get("prepared-secret"), Some(&command_id));
    assert_eq!(prepared_tokens.remove("prepared-secret"), Some(command_id));
}

fn assert_key_type<T: ZeroizeOnDrop>(_: Option<&T>) {}

fn assert_pending_types(command: &PendingFirmwareCommand) {
    assert_optional_type(command.prepared_token.as_ref());
    assert_optional_type(command.transient_url.as_ref());
}

fn assert_optional_type<T: ZeroizeOnDrop>(_: Option<&T>) {}

fn assert_reference_type<T: ZeroizeOnDrop>(_: &T) {}

#[test]
fn firmware_secret_removal_releases_token_and_transient_url_storage() {
    let storage = PendingFirmwareCommands::default();
    let identity = firmware_identity();
    storage.insert(firmware_command_with_secrets(identity.clone()));

    {
        let state = storage.lock();
        assert_eq!(state.commands.len(), 1);
        assert_eq!(state.prepared_tokens.len(), 1);
    }

    let claimed = storage
        .remove_exact(&identity, None)
        .expect("firmware command should be removable");
    {
        let state = storage.lock();
        assert!(state.commands.is_empty());
        assert!(state.prepared_tokens.is_empty());
    }
    drop(claimed);
}

#[test]
fn firmware_registry_drop_releases_zeroizing_storage() {
    let storage = PendingFirmwareCommands::default();
    let weak_inner = Arc::downgrade(&storage.inner);
    let identity = firmware_identity();
    storage.insert(firmware_command_with_secrets(identity.clone()));
    store::reserve_firmware_redaction_url(
        &mut storage.lock(),
        &identity,
        "https://firmware.invalid/path?token=retained-secret",
    )
    .expect("redaction URL should fit in the retained set");

    drop(storage);

    assert!(weak_inner.upgrade().is_none());
}

fn firmware_command_with_secrets(identity: FirmwareCommandIdentity) -> PendingFirmwareCommand {
    PendingFirmwareCommand {
        identity,
        kind: PendingFirmwareKind::Control(FirmwareControlMetadata::UpgradeConfirm {
            sequence_id: "sequence".to_owned(),
            src_id: 1,
        }),
        phase: PendingFirmwarePhase::Prepared,
        prepared_token: Some(FirmwareSecret::from("prepared-secret".to_owned())),
        expires_at: None,
        transient_url: Some(FirmwareSecret::from(
            "https://firmware.invalid/path?token=transient-secret".to_owned(),
        )),
        prepare_waiter: None,
        refresh_waiter: None,
        execute_waiter: None,
    }
}

fn firmware_identity() -> FirmwareCommandIdentity {
    FirmwareCommandIdentity {
        command_id: CommandId::new(),
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        session_token: SessionToken::new(),
        printer_id: "printer".to_owned(),
        serial: "serial".to_owned(),
        generation: 1,
    }
}
