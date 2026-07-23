use super::*;

#[tokio::test]
async fn command_enqueue_rejects_missing_agent() {
    let (_, tenants, _, _, commands, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();

    let err = commands
        .enqueue_refresh_printers(tenant.id, AgentId::new())
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::MissingAgent));
}

#[tokio::test]
async fn command_enqueue_rejects_wrong_tenant() {
    let (_, tenants, agents, _, commands, _) = repositories().await;
    let acme = tenants.create("acme", "Acme Labs").await.unwrap();
    let beta = tenants.create("beta", "Beta Labs").await.unwrap();
    let agent = agents.create(acme.id, "agent").await.unwrap();

    let err = commands
        .enqueue_refresh_printers(beta.id, agent.id)
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::CommandOwnershipMismatch));
}

#[tokio::test]
async fn command_queue_filters_by_tenant_and_agent() {
    let (_, tenants, agents, _, commands, _) = repositories().await;
    let acme = tenants.create("acme", "Acme Labs").await.unwrap();
    let beta = tenants.create("beta", "Beta Labs").await.unwrap();
    let acme_agent = agents.create(acme.id, "agent").await.unwrap();
    let other_acme_agent = agents.create(acme.id, "other").await.unwrap();
    let beta_agent = agents.create(beta.id, "agent").await.unwrap();

    let expected = commands
        .enqueue_refresh_printers(acme.id, acme_agent.id)
        .await
        .unwrap();
    commands
        .enqueue_refresh_printers(acme.id, other_acme_agent.id)
        .await
        .unwrap();
    commands
        .enqueue_refresh_printers(beta.id, beta_agent.id)
        .await
        .unwrap();

    assert_eq!(
        commands
            .next_queued_for_agent(acme.id, acme_agent.id)
            .await
            .unwrap()
            .unwrap()
            .id,
        expected.id
    );
}

#[tokio::test]
async fn command_enqueue_print_project_file_persists_payload_and_printer() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();

    let command = commands
        .enqueue_print_project_file(
            tenant.id,
            agent.id,
            &printer_id,
            print_payload(&printer_id, "serial-explicit"),
        )
        .await
        .unwrap();
    let payload: PrintProjectFilePayload = serde_json::from_str(&command.payload_json).unwrap();

    assert_eq!(command.kind, "print_project_file");
    assert_eq!(command.status, CommandStatus::Queued);
    assert_eq!(command.printer_id.as_deref(), Some(printer_id.as_str()));
    assert_eq!(payload, print_payload(&printer_id, "serial-explicit"));
}

#[tokio::test]
async fn command_enqueue_print_project_file_rejects_missing_printer() {
    let (_, tenants, agents, _, commands, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = uuid::Uuid::new_v4().to_string();

    let err = commands
        .enqueue_print_project_file(
            tenant.id,
            agent.id,
            &printer_id,
            print_payload(&printer_id, "SERIAL1"),
        )
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::MissingPrinter));
}

#[tokio::test]
async fn command_enqueue_print_project_file_rejects_wrong_printer_owner() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let acme = tenants.create("acme", "Acme Labs").await.unwrap();
    let beta = tenants.create("beta", "Beta Labs").await.unwrap();
    let acme_agent = agents.create(acme.id, "agent").await.unwrap();
    let beta_agent = agents.create(beta.id, "agent").await.unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture(
        &database,
        beta.id,
        beta_agent.id,
    )
    .await
    .unwrap();

    let err = commands
        .enqueue_print_project_file(
            acme.id,
            acme_agent.id,
            &printer_id,
            print_payload(&printer_id, "SERIAL1"),
        )
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::MissingPrinter));
}
