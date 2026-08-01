use super::*;

pub(super) async fn exercise_exact_session_guards(database: Database, mutation_database: Database) {
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(mutation_database.clone());
    let persisted_printers = PrinterRepository::new(database.clone());
    let jobs = JobRepository::new(mutation_database.clone());
    let materials = MaterialRepository::new(mutation_database.clone());
    let mutation_agents = AgentRepository::new(mutation_database);
    let tenant = tenants
        .create("session-guards", "Session Guards")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let session_a = uuid::Uuid::new_v4().to_string();
    let session_b = uuid::Uuid::new_v4().to_string();
    agents
        .claim_online_session(
            tenant.id,
            agent.id,
            &session_a,
            "test-a",
            "2026-07-10T00:00:00Z",
        )
        .await
        .unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    let printer_before = persisted_printers
        .list_with_live_status_for_tenant(tenant.id)
        .await
        .unwrap()
        .into_iter()
        .find(|printer| printer.printer.id == printer_id)
        .unwrap();
    agents
        .claim_online_session(
            tenant.id,
            agent.id,
            &session_b,
            "test-b",
            "2026-07-10T00:01:00Z",
        )
        .await
        .unwrap();

    let heartbeat = mutation_agents
        .heartbeat_if_current(tenant.id, agent.id, &session_a, "2026-07-10T00:02:00Z")
        .await
        .unwrap_err();
    assert!(matches!(heartbeat, RepositoryError::AgentSessionNotCurrent));
    assert!(
        mutation_agents
            .mark_offline_if_current(tenant.id, agent.id, &session_a, "2026-07-10T00:02:00Z",)
            .await
            .unwrap()
            .is_none()
    );
    let snapshot = printers
        .upsert_snapshot_if_current(
            tenant.id,
            agent.id,
            &session_a,
            PrinterSnapshotUpsert {
                serial_number: format!("serial-{printer_id}"),
                host: None,
                access_code: None,
                name: "stale".to_string(),
                model: None,
                status: Some("stale".to_string()),
                observed_at: "2026-07-10T00:02:00Z".to_string(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: None,
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_target_temperature_celsius: None,
                chamber_light_on: None,
                nozzle_system: None,
                connection_authoritative: false,
                telemetry_authoritative: true,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(snapshot, RepositoryError::AgentSessionNotCurrent));
    let report = jobs
        .apply_current_print_report(
            &session_a,
            super::super::jobs::report_input(
                tenant.id,
                agent.id,
                &printer_id,
                None,
                None,
                "RUNNING",
            ),
        )
        .await
        .unwrap_err();
    assert!(matches!(report, RepositoryError::AgentSessionNotCurrent));
    let material = materials
        .apply_snapshot_if_current(
            &session_a,
            tenant.id,
            agent.id,
            &printer_id,
            format!("serial-{printer_id}"),
            "{}".to_string(),
        )
        .await
        .unwrap_err();
    assert!(matches!(material, RepositoryError::AgentSessionNotCurrent));

    let printer_after = persisted_printers
        .list_with_live_status_for_tenant(tenant.id)
        .await
        .unwrap()
        .into_iter()
        .find(|printer| printer.printer.id == printer_id)
        .unwrap();
    assert_eq!(printer_after, printer_before);
    assert!(
        MaterialRepository::new(database.clone())
            .latest_for_printer(tenant.id, &printer_id)
            .await
            .unwrap()
            .is_none()
    );

    let persisted = agent_entities::Entity::find_by_id(agent.id.to_string())
        .one(&database.sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        persisted.current_session_id.as_deref(),
        Some(session_b.as_str())
    );
    assert_eq!(persisted.status, "online");

    mutation_agents
        .heartbeat_if_current(tenant.id, agent.id, &session_b, "2026-07-10T00:03:00Z")
        .await
        .unwrap();
    let persisted = agent_entities::Entity::find_by_id(agent.id.to_string())
        .one(&database.sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        persisted.last_seen_at.as_deref(),
        Some("2026-07-10T00:03:00Z")
    );
    assert_eq!(
        persisted.current_session_id.as_deref(),
        Some(session_b.as_str())
    );
    assert!(
        mutation_agents
            .mark_offline_if_current(tenant.id, agent.id, &session_b, "2026-07-10T00:04:00Z",)
            .await
            .unwrap()
            .is_some()
    );
    let persisted = agent_entities::Entity::find_by_id(agent.id.to_string())
        .one(&database.sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.status, "offline");
    assert_eq!(persisted.current_session_id, None);
}
