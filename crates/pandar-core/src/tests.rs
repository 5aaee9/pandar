use super::*;

#[test]
fn id_parse_rejects_invalid_uuid() {
    assert_eq!(
        TenantId::parse("not-a-uuid").unwrap_err(),
        CoreError::InvalidTenantId
    );
    assert_eq!(
        AgentId::parse("not-a-uuid").unwrap_err(),
        CoreError::InvalidAgentId
    );
}

#[test]
fn tenant_validates_required_fields() {
    assert_eq!(
        Tenant::new(" ", "Acme").unwrap_err(),
        CoreError::EmptyTenantSlug
    );
    assert_eq!(
        Tenant::new("acme", " ").unwrap_err(),
        CoreError::EmptyTenantDisplayName
    );
    assert_eq!(
        Tenant::from_parts(TenantId::new(), "acme", " ", "2026-06-20T00:00:00Z").unwrap_err(),
        CoreError::EmptyTenantDisplayName
    );
}

#[test]
fn tenant_new_sets_iso_utc_created_at() {
    let tenant = Tenant::new("acme", "Acme").unwrap();

    assert!(tenant.created_at.ends_with('Z'));
    assert!(OffsetDateTime::parse(&tenant.created_at, &Rfc3339).is_ok());
}

#[test]
fn agent_validates_name_and_starts_offline_for_tenant() {
    assert_eq!(
        Agent::new(TenantId::new(), " ").unwrap_err(),
        CoreError::EmptyAgentName
    );
    let tenant = Tenant::new("acme", "Acme").unwrap();
    let agent = Agent::new(tenant.id, "garage").unwrap();
    assert_eq!(agent.tenant_id, tenant.id);
    assert_eq!(agent.status, AgentStatus::Offline);
}

#[test]
fn agent_status_round_trips_persisted_strings() {
    assert_eq!(AgentStatus::Offline.as_str(), "offline");
    assert_eq!(AgentStatus::Connecting.as_str(), "connecting");
    assert_eq!(AgentStatus::Online.as_str(), "online");
    assert_eq!("offline".parse(), Ok(AgentStatus::Offline));
    assert_eq!("connecting".parse(), Ok(AgentStatus::Connecting));
    assert_eq!("online".parse(), Ok(AgentStatus::Online));
    assert_eq!(
        "retired".parse::<AgentStatus>(),
        Err(CoreError::InvalidAgentStatus("retired".to_string()))
    );
}

#[test]
fn command_id_parse_round_trips_uuid_strings() {
    let id = CommandId::new();

    assert_eq!(CommandId::parse(&id.to_string()), Ok(id));
    assert_eq!(
        CommandId::parse("not-a-uuid"),
        Err(CoreError::InvalidCommandId)
    );
}

#[test]
fn command_status_round_trips_persisted_strings() {
    for (status, value) in [
        (CommandStatus::Queued, "queued"),
        (CommandStatus::Sent, "sent"),
        (CommandStatus::Acknowledged, "acknowledged"),
        (CommandStatus::Succeeded, "succeeded"),
        (CommandStatus::Failed, "failed"),
    ] {
        assert_eq!(status.as_str(), value);
        assert_eq!(value.parse::<CommandStatus>(), Ok(status));
    }
    assert_eq!(
        "lost".parse::<CommandStatus>(),
        Err(CoreError::InvalidCommandStatus("lost".to_string()))
    );
}

#[test]
fn command_record_from_parts_validates_kind_and_status() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let created_at = "2026-06-20T00:00:00Z";
    let build = |kind: &str, status: &str| {
        CommandRecord::from_parts(CommandRecordParts {
            id: CommandId::new(),
            tenant_id,
            agent_id,
            printer_id: None,
            kind: kind.to_owned(),
            status: status.to_owned(),
            payload_json: "{}".to_owned(),
            error: None,
            created_at: created_at.to_owned(),
            updated_at: created_at.to_owned(),
        })
    };

    let record = build("refresh_printers", "queued").unwrap();
    assert_eq!(record.status, CommandStatus::Queued);
    assert_eq!(
        build(" ", "queued").unwrap_err(),
        CoreError::EmptyCommandKind
    );
    assert_eq!(
        build("refresh_printers", "lost").unwrap_err(),
        CoreError::InvalidCommandStatus("lost".to_string())
    );
}
