use super::*;

mod command_job;

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
    assert_eq!(
        JobId::parse("not-a-uuid").unwrap_err(),
        CoreError::InvalidJobId
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
fn printer_from_parts_builds_valid_record() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer = Printer::from_parts(PrinterParts {
        id: "printer-1".to_owned(),
        tenant_id,
        agent_id,
        serial_number: "SERIAL1".to_owned(),
        name: "garage".to_owned(),
        model: Some("A1 Mini".to_owned()),
        status: "online".to_owned(),
        last_seen_at: "2026-06-20T00:00:00Z".to_owned(),
        created_at: "2026-06-19T00:00:00Z".to_owned(),
    })
    .unwrap();

    assert_eq!(printer.id, "printer-1");
    assert_eq!(printer.tenant_id, tenant_id);
    assert_eq!(printer.agent_id, agent_id);
    assert_eq!(printer.serial_number, "SERIAL1");
    assert_eq!(printer.name, "garage");
    assert_eq!(printer.model, Some("A1 Mini".to_owned()));
    assert_eq!(printer.status, "online");
}

#[test]
fn printer_from_parts_validates_required_fields() {
    let build = |id: &str, serial_number: &str, name: &str, status: &str| {
        Printer::from_parts(PrinterParts {
            id: id.to_owned(),
            tenant_id: TenantId::new(),
            agent_id: AgentId::new(),
            serial_number: serial_number.to_owned(),
            name: name.to_owned(),
            model: None,
            status: status.to_owned(),
            last_seen_at: "2026-06-20T00:00:00Z".to_owned(),
            created_at: "2026-06-19T00:00:00Z".to_owned(),
        })
    };

    assert_eq!(
        build(" ", "SERIAL1", "garage", "online").unwrap_err(),
        CoreError::EmptyPrinterId
    );
    assert_eq!(
        build("printer-1", " ", "garage", "online").unwrap_err(),
        CoreError::EmptyPrinterSerialNumber
    );
    assert_eq!(
        build("printer-1", "SERIAL1", " ", "online").unwrap_err(),
        CoreError::EmptyPrinterName
    );
    assert_eq!(
        build("printer-1", "SERIAL1", "garage", " ").unwrap_err(),
        CoreError::EmptyPrinterStatus
    );
}

#[test]
fn printer_from_parts_normalizes_blank_model() {
    let printer = Printer::from_parts(PrinterParts {
        id: "printer-1".to_owned(),
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        serial_number: "SERIAL1".to_owned(),
        name: "garage".to_owned(),
        model: Some("  ".to_owned()),
        status: "online".to_owned(),
        last_seen_at: "2026-06-20T00:00:00Z".to_owned(),
        created_at: "2026-06-19T00:00:00Z".to_owned(),
    })
    .unwrap();

    assert_eq!(printer.model, None);
}
