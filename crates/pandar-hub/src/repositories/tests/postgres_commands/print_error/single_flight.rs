use super::*;

#[tokio::test]
async fn postgres_web_and_plugin_recovery_share_single_flight_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());
    let tenant = tenants
        .create("postgres-web-recovery", "Postgres Web Recovery")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let session_id = "postgres-web-recovery-session";
    agents
        .claim_online_session(
            tenant.id,
            agent.id,
            session_id,
            "test",
            "2026-07-10T00:00:00Z",
        )
        .await
        .unwrap();
    let printer_id = additional_printer(&database, tenant.id, agent.id).await;
    seed_web_recovery_state(&database, &printer_id, session_id, "20P123456789").await;

    let (left, right) = tokio::join!(
        commands.create_web_print_error_sent_with_audit(
            tenant.id,
            &printer_id,
            web_recovery_input(PrintErrorAction::Resume, agent.id, session_id),
            native_audit_actor(),
        ),
        commands.create_web_print_error_sent_with_audit(
            tenant.id,
            &printer_id,
            web_recovery_input(PrintErrorAction::Ignore, agent.id, session_id),
            native_audit_actor(),
        ),
    );
    let first = match (left, right) {
        (Ok(first), Err(RepositoryError::PrinterControlUnavailable))
        | (Err(RepositoryError::PrinterControlUnavailable), Ok(first)) => first,
        (left, right) => panic!("expected one PostgreSQL recovery winner: {left:?}, {right:?}"),
    };
    assert_eq!(commands.count().await.unwrap(), 1);
    commands
        .mark_succeeded(first.command.id, tenant.id, agent.id)
        .await
        .unwrap();

    let plugin = commands
        .create_printer_operation_sent_with_audit(
            tenant.id,
            &printer_id,
            agent.id,
            native_operation(PrintErrorAction::Stop, 83_918_929, 20_060),
            native_audit_actor(),
        )
        .await
        .unwrap();
    let error = commands
        .create_web_print_error_sent_with_audit(
            tenant.id,
            &printer_id,
            web_recovery_input(PrintErrorAction::Stop, agent.id, session_id),
            native_audit_actor(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, RepositoryError::PrinterControlUnavailable));
    commands
        .mark_failed(plugin.id, tenant.id, agent.id, "terminal")
        .await
        .unwrap();
    let retry = commands
        .create_web_print_error_sent_with_audit(
            tenant.id,
            &printer_id,
            web_recovery_input(PrintErrorAction::Stop, agent.id, session_id),
            native_audit_actor(),
        )
        .await
        .unwrap();
    assert_eq!(retry.command.status, CommandStatus::Sent);
    commands
        .mark_succeeded(retry.command.id, tenant.id, agent.id)
        .await
        .unwrap();

    let web_first_printer = additional_printer(&database, tenant.id, agent.id).await;
    seed_web_recovery_state(&database, &web_first_printer, session_id, "20PWEB123456").await;
    let before_web_first = commands.count().await.unwrap();
    let web_first = commands
        .create_web_print_error_sent_with_audit(
            tenant.id,
            &web_first_printer,
            web_recovery_input(PrintErrorAction::Resume, agent.id, session_id),
            native_audit_actor(),
        )
        .await
        .unwrap();
    let error = commands
        .create_printer_operation_sent_with_audit(
            tenant.id,
            &web_first_printer,
            agent.id,
            native_operation(PrintErrorAction::Ignore, 83_918_929, 20_061),
            native_audit_actor(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, RepositoryError::PrinterControlUnavailable));
    assert_eq!(commands.count().await.unwrap(), before_web_first + 1);
    commands
        .mark_succeeded(web_first.command.id, tenant.id, agent.id)
        .await
        .unwrap();

    let mixed_printer = additional_printer(&database, tenant.id, agent.id).await;
    seed_web_recovery_state(&database, &mixed_printer, session_id, "20PMIX123456").await;
    let before_mixed = commands.count().await.unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let web = tokio::spawn({
        let barrier = barrier.clone();
        let commands = commands.clone();
        let printer_id = mixed_printer.clone();
        async move {
            barrier.wait().await;
            commands
                .create_web_print_error_sent_with_audit(
                    tenant.id,
                    &printer_id,
                    web_recovery_input(PrintErrorAction::Resume, agent.id, session_id),
                    native_audit_actor(),
                )
                .await
        }
    });
    let plugin = tokio::spawn({
        let barrier = barrier.clone();
        let commands = commands.clone();
        let printer_id = mixed_printer.clone();
        async move {
            barrier.wait().await;
            commands
                .create_printer_operation_sent_with_audit(
                    tenant.id,
                    &printer_id,
                    agent.id,
                    native_operation(PrintErrorAction::Stop, 83_918_929, 20_062),
                    native_audit_actor(),
                )
                .await
        }
    });
    barrier.wait().await;
    let web = web.await.unwrap();
    let plugin = plugin.await.unwrap();
    let winner_id = match (web, plugin) {
        (Ok(web), Err(RepositoryError::PrinterControlUnavailable)) => web.command.id,
        (Err(RepositoryError::PrinterControlUnavailable), Ok(plugin)) => plugin.id,
        (web, plugin) => {
            panic!("expected one mixed PostgreSQL recovery winner: {web:?}, {plugin:?}")
        }
    };
    assert_eq!(commands.count().await.unwrap(), before_mixed + 1);
    commands
        .mark_succeeded(winner_id, tenant.id, agent.id)
        .await
        .unwrap();
}
