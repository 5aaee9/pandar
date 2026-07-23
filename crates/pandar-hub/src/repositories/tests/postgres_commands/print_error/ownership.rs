use super::*;

#[tokio::test]
async fn postgres_web_recovery_revalidates_ownership_and_session_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());
    let tenant = tenants
        .create("postgres-web-revalidation", "Postgres Web Revalidation")
        .await
        .unwrap();
    let original = agents.create(tenant.id, "original").await.unwrap();
    let replacement = agents.create(tenant.id, "replacement").await.unwrap();
    let session_id = "postgres-web-revalidation-session";
    agents
        .claim_online_session(
            tenant.id,
            original.id,
            session_id,
            "test",
            "2026-07-10T00:00:00Z",
        )
        .await
        .unwrap();

    let ownership_printer = additional_printer(&database, tenant.id, original.id).await;
    let ownership_serial = "20POWN123456";
    seed_web_recovery_state(&database, &ownership_printer, session_id, ownership_serial).await;
    let pause = printer_operation_ownership_pause::install(&ownership_printer);
    let recovery = tokio::spawn({
        let commands = commands.clone();
        let printer_id = ownership_printer.clone();
        async move {
            commands
                .create_web_print_error_sent_with_audit(
                    tenant.id,
                    &printer_id,
                    web_recovery_input(PrintErrorAction::Resume, original.id, session_id),
                    native_audit_actor(),
                )
                .await
        }
    });
    let resume = pause.wait_until_reached().await.unwrap();
    printers
        .upsert_snapshot(
            tenant.id,
            replacement.id,
            reassigned_snapshot(ownership_serial.to_owned()),
        )
        .await
        .unwrap();
    resume.send(()).unwrap();
    assert!(matches!(
        recovery.await.unwrap().unwrap_err(),
        RepositoryError::PrinterControlUnavailable
    ));
    assert_eq!(commands.count().await.unwrap(), 0);

    let session_printer = additional_printer(&database, tenant.id, original.id).await;
    seed_web_recovery_state(&database, &session_printer, session_id, "20PSES123456").await;
    let pause = printer_operation_ownership_pause::install(&session_printer);
    let recovery = tokio::spawn({
        let commands = commands.clone();
        let printer_id = session_printer.clone();
        async move {
            commands
                .create_web_print_error_sent_with_audit(
                    tenant.id,
                    &printer_id,
                    web_recovery_input(PrintErrorAction::Stop, original.id, session_id),
                    native_audit_actor(),
                )
                .await
        }
    });
    let resume = pause.wait_until_reached().await.unwrap();
    agents
        .claim_online_session(
            tenant.id,
            original.id,
            "postgres-web-replacement-session",
            "replacement",
            "2026-07-10T00:00:01Z",
        )
        .await
        .unwrap();
    resume.send(()).unwrap();
    assert!(matches!(
        recovery.await.unwrap().unwrap_err(),
        RepositoryError::PrinterControlUnavailable
    ));
    assert_eq!(commands.count().await.unwrap(), 0);
}
