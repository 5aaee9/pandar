use super::*;

#[tokio::test]
async fn command_create_link_printer_sent_persists_redacted_payload_and_audit() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let audit = AuditEventRepository::new(database.clone());
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let access_code = "SECRET-LINK-CODE";

    let command = commands
        .create_link_printer_sent_with_audit(
            tenant.id,
            agent.id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: access_code.to_owned(),
                name: Some("Office X1C".to_owned()),
            },
            test_audit_actor(),
        )
        .await
        .unwrap();

    assert_eq!(command.kind, "link_printer");
    assert_eq!(command.status, CommandStatus::Sent);
    assert_eq!(command.printer_id, None);
    assert!(!command.payload_json.contains(access_code));
    let payload: TestRedactedLinkPrinterPayload =
        serde_json::from_str(&command.payload_json).unwrap();
    assert_eq!(
        payload,
        TestRedactedLinkPrinterPayload {
            printer_type: "BambuLab".to_owned(),
            host: "192.0.2.10".to_owned(),
            access_code: "[redacted]".to_owned(),
            name: Some("Office X1C".to_owned()),
        }
    );

    let events = audit.list_for_tenant(tenant.id).await.unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "agent.link_printer")
        .expect("link printer audit event");
    assert_eq!(event.target_type, "agent");
    assert_eq!(
        event.target_id.as_deref(),
        Some(agent.id.to_string().as_str())
    );
    assert!(!event.metadata_json.contains(access_code));
    let metadata: TestLinkPrinterAuditMetadata =
        serde_json::from_str(&event.metadata_json).unwrap();
    assert_eq!(
        metadata,
        TestLinkPrinterAuditMetadata {
            printer_type: "BambuLab".to_owned(),
            host: "192.0.2.10".to_owned(),
            name: Some("Office X1C".to_owned()),
            audit: test_audit_metadata(),
        }
    );
}

#[tokio::test]
async fn stale_link_printer_cleanup_skips_owned_pending_commands() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let old_owned = commands
        .create_link_printer_sent_with_audit(
            tenant.id,
            agent.id,
            link_payload("OWNED"),
            test_audit_actor(),
        )
        .await
        .unwrap();
    let old_unowned = commands
        .create_link_printer_sent_with_audit(
            tenant.id,
            agent.id,
            link_payload("UNOWNED"),
            test_audit_actor(),
        )
        .await
        .unwrap();

    set_command_updated_at(&database, old_owned.id, "2026-07-01T00:00:00Z").await;
    set_command_updated_at(&database, old_unowned.id, "2026-07-01T00:00:00Z").await;

    let failed = commands
        .fail_stale_unowned_live_commands(
            "2026-07-01T00:06:00Z",
            std::time::Duration::from_secs(300),
            std::time::Duration::from_secs(45),
            uuid::Uuid::new_v4(),
            &[old_owned.id],
        )
        .await
        .unwrap();

    assert_eq!(failed, 1);
    assert_eq!(
        commands
            .get_for_tenant(tenant.id, old_owned.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        CommandStatus::Sent,
    );
    let failed_command = commands
        .get_for_tenant(tenant.id, old_unowned.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(failed_command.status, CommandStatus::Failed);
    assert_eq!(
        failed_command.error.as_deref(),
        Some("printer link dispatch expired before completion"),
    );
}
