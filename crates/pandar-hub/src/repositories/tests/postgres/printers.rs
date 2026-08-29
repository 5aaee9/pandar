use super::*;

#[tokio::test]
async fn postgres_printer_repository_upsert_list_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(database.clone());
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();

    let created = printers
        .upsert_snapshot(
            tenant.id,
            agent.id,
            PrinterSnapshotUpsert {
                serial_number: "SN-001".to_string(),
                host: Some("192.0.2.10".to_string()),
                access_code: Some("12345678".to_string()),
                name: "Garage A1".to_string(),
                model: Some("A1 Mini".to_string()),
                status: Some("idle".to_string()),
                observed_at: "2026-06-21T00:00:00Z".to_string(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: None,
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_target_temperature_celsius: None,
                chamber_light_on: None,
                cooling_system: None,
                nozzle_system: None,
                connection_authoritative: false,
                telemetry_authoritative: true,
            },
        )
        .await
        .unwrap();
    printers
        .update_details_with_audit(
            tenant.id,
            &created.id,
            "Garage A1".to_owned(),
            "192.0.2.11".to_owned(),
            "edited-access-code".to_owned(),
            AuditActor::no_auth(),
        )
        .await
        .unwrap();
    let updated = printers
        .upsert_snapshot(
            tenant.id,
            agent.id,
            PrinterSnapshotUpsert {
                serial_number: "SN-001".to_string(),
                host: Some("192.0.2.10".to_owned()),
                access_code: Some("12345678".to_owned()),
                name: "Ignored Snapshot Name".to_string(),
                model: Some("A1 Mini".to_string()),
                status: Some("printing".to_string()),
                observed_at: "2026-06-21T00:05:00Z".to_string(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: None,
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_target_temperature_celsius: None,
                chamber_light_on: None,
                cooling_system: None,
                nozzle_system: None,
                connection_authoritative: false,
                telemetry_authoritative: true,
            },
        )
        .await
        .unwrap();

    assert_eq!(updated.id, created.id);
    assert_eq!(updated.created_at, created.created_at);
    assert_eq!(updated.name, "Garage A1");
    assert_eq!(updated.host.as_deref(), Some("192.0.2.11"));
    assert_eq!(updated.access_code.as_deref(), Some("edited-access-code"));
    assert_eq!(updated.status, "printing");
    assert_eq!(updated.last_seen_at, "2026-06-21T00:05:00Z");
    let Database::Postgres(pool) = &database else {
        panic!("expected PostgreSQL database");
    };
    let (plaintext, encrypted): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT access_code, access_code_encrypted FROM printers WHERE id = $1")
            .bind(&created.id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(plaintext, None);
    assert!(
        encrypted
            .as_deref()
            .is_some_and(|value| value.starts_with("v1:"))
    );

    let authoritative = printers
        .upsert_snapshot(
            tenant.id,
            agent.id,
            PrinterSnapshotUpsert {
                serial_number: "SN-001".to_string(),
                host: Some("192.0.2.12".to_owned()),
                access_code: Some("reloaded-access-code".to_owned()),
                name: "Ignored Snapshot Name".to_string(),
                model: Some("A1 Mini".to_string()),
                status: Some("idle".to_string()),
                observed_at: "2026-06-21T00:10:00Z".to_string(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: Some("60".to_owned()),
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_target_temperature_celsius: None,
                chamber_light_on: None,
                cooling_system: None,
                nozzle_system: None,
                connection_authoritative: true,
                telemetry_authoritative: true,
            },
        )
        .await
        .unwrap();
    assert_eq!(authoritative.host.as_deref(), Some("192.0.2.12"));
    assert_eq!(
        authoritative.access_code.as_deref(),
        Some("reloaded-access-code")
    );
    let partial = printers
        .upsert_snapshot(
            tenant.id,
            agent.id,
            PrinterSnapshotUpsert {
                serial_number: "SN-001".to_string(),
                host: None,
                access_code: None,
                name: "Ignored Snapshot Name".to_string(),
                model: None,
                status: None,
                observed_at: "2026-06-21T00:15:00Z".to_string(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: None,
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_target_temperature_celsius: None,
                chamber_light_on: None,
                cooling_system: None,
                nozzle_system: None,
                connection_authoritative: false,
                telemetry_authoritative: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(partial.status, "idle");
    assert_eq!(partial.bed_temperature_celsius.as_deref(), Some("60"));
    assert_eq!(
        printers.list_for_tenant(tenant.id).await.unwrap(),
        vec![partial]
    );
}
